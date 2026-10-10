#!/usr/bin/env python3
"""Compare dict2json with Python's literal and JSON parsers (stdlib only)."""

import ast
import json
import math
from pathlib import Path
import random
import subprocess
import sys


SEED = 20261007
MAX_INPUT = 16 * 1024 * 1024
STRINGS = [
    "", "plain", "single ' quote", 'double " quote', "back\\slash", "\n\r\t\b\f\0",
    "हिन्दी", "中文", "😀🚀", "é", "e\u0301", "\u2028\u2029", "True None False",
    "{[(:)]}", "a/b", "line\nline", "# comment", "\\u1234", "\x01\x1f\x7f",
]
FLOATS = [0.0, -0.0, 0.1, -0.1, 1.2345678901234567, 5e-324, -5e-324,
          2.2250738585072014e-308, 1.7976931348623157e308, -1.7976931348623157e308]


def reject_constant(value):
    raise ValueError(f"non-JSON constant: {value}")


def load_json(value):
    return json.loads(value, parse_constant=reject_constant)


def expected_literal(source):
    # Python 3.10+ strips leading spaces and tabs in literal_eval; support 3.9 too.
    return load_json(json.dumps(ast.literal_eval(source.lstrip(" \t")), allow_nan=False))


def canonical(value):
    # Plain Python equality treats False == 0 and 1 == 1.0, masking type mistakes.
    return json.dumps(value, sort_keys=True, allow_nan=False)


def run(binary, source):
    if isinstance(source, str):
        source = source.encode("utf-8")
    try:
        result = subprocess.run([str(binary)], input=source, capture_output=True, timeout=5)
    except subprocess.TimeoutExpired as error:
        raise AssertionError(f"CLI timed out for {source[:160]!r}") from error
    assert result.returncode >= 0, f"CLI crashed ({result.returncode}) for {source[:160]!r}"
    return result


def accepted(binary, source, *, passthrough=False):
    result = run(binary, source)
    preview = repr(source[:160])
    assert result.returncode == 0, f"rejected {preview}: {result.stderr[:500]!r}"
    assert not result.stderr, f"successful input wrote stderr for {preview}: {result.stderr!r}"
    try:
        actual = load_json(result.stdout.decode("utf-8"))
    except (UnicodeError, ValueError) as error:
        raise AssertionError(f"invalid JSON output for {preview}: {result.stdout[:500]!r}") from error
    if passthrough:
        assert result.stdout == source.encode("utf-8"), f"changed valid JSON {preview}"
    else:
        assert canonical(actual) == canonical(expected_literal(source)), \
            f"wrong conversion for {preview}: {actual!r}"


def rejected(binary, source):
    result = run(binary, source)
    assert result.returncode > 0, f"accepted unsupported input {source[:160]!r}"
    assert not result.stdout, f"failed input wrote stdout: {result.stdout[:500]!r}"
    assert result.stderr, f"failed input had no explanation: {source[:160]!r}"


def random_value(rng, depth=0):
    choices = 5 if depth >= 4 else 8
    kind = rng.randrange(choices)
    if kind == 0:
        return rng.choice(STRINGS) + rng.choice(["", str(rng.randrange(1000)), "✨"])
    if kind == 1:
        return rng.choice([None, True, False])
    if kind == 2:
        return rng.choice([-1, 1]) * rng.getrandbits(rng.choice([0, 8, 32, 64, 128, 512, 2048]))
    if kind == 3:
        return rng.choice(FLOATS)
    if kind == 4:
        return math.ldexp(rng.uniform(-1, 1), rng.randrange(-1000, 1000))
    if kind == 5:
        return {rng.choice(STRINGS) + str(i): random_value(rng, depth + 1)
                for i in range(rng.randrange(5))}
    values = [random_value(rng, depth + 1) for _ in range(rng.randrange(5))]
    return values if kind == 6 else tuple(values)


def main():
    binary = Path(sys.argv[1] if len(sys.argv) > 1 else "target/debug/dict2json").resolve()
    if not binary.is_file():
        sys.exit(f"Binary not found: {binary}; build with cargo build first.")
    rng = random.Random(SEED)
    counts = {"literal fixtures": 0, "generated dictionaries": 0, "JSON passthrough": 0,
              "rejections": 0, "adversarial inputs": 0}

    literals = [
        "{}", "{'project_name': 'xyz', 'start_time': '2026-10-07 13:00:00+05:30', "
        "'end_time': '2026-10-07 19:00:00+05:30'}",
        "{'a': True, 'b': False, 'c': None}",
        "{'nested': {'list': [1, None, True, {'x': 'y'}]}, 'tuple': (1, 2)}",
        "{'empty': (), 'single': (1,), 'parenthesized': (1), 'empty_list': []}",
        "{'a': 1, 'a': 2}", "{'a': 'x',}",
        "{'x': +1, 'y': -1, 'z': -0.0, 'p': +0.0}",
        "{'x': -(1)}", "{'x': +(.5)}", "{'x': -(((2.0)))}", "{'x': -(0xFF)}",
        "{'binary': 0b1010, 'octal': 0o177, 'hex': 0xDEAD_BEEF, 'decimal': 1_000_000}",
        "{'x': 1_0.2_5e+0_2, 'y': .5, 'z': 1., 'underflow': 1e-999}",
        "{'x': 'first' 'second', 'y': \"a\" 'b'}",
        "{'x': r'raw\\n\\t', 'y': u'Unicode \\u2603'}",
        "{'x': '''one\ntwo''', 'y': \"\"\"double ' quote\"\"\"}",
        "{'x': '\\x41\\101\\u2603\\U0001f680', 'y': '\\N{SNOWMAN}'}",
        "{'x': 'continued\\\nline'}",
        "  {\n # leading comment\n 'x': [1, # middle comment\n 2,],\n} # trailing comment\n",
        "({'x': True})", "\t{'x': None}\n",
        "{'x': '\\a\\v\\b\\f\\r\\t\\n\\\\\\\"\\\''}",
        "{'x': '\\q'}",
    ]
    literals.extend("{'number': " + str(value) + "}" for value in [
        2**53 + 1, -(2**53 + 1), 2**63, -(2**63) - 1, 2**64,
        10**100, -(10**200), 10**600 + 123,
    ])
    literals.append("{'x': " + "[" * 63 + "None" + "]" * 63 + "}")
    for source in literals:
        accepted(binary, source)
        counts["literal fixtures"] += 1

    for index in range(2200):
        value = {rng.choice(STRINGS) + str(i): random_value(rng)
                 for i in range(rng.randrange(1, 7))}
        # A Python-only literal also exercises conversion on otherwise JSON-looking values.
        value["case"] = (index, None, rng.choice([True, False]))
        accepted(binary, repr(value))
        counts["generated dictionaries"] += 1

    json_inputs = [
        "{}", "[]", "null", "true", "false", "0", "-0", "1.00", "1e+02", "-2.3E-10",
        '"text"', '"\\u2603"', '"\\ud83d\\ude00"', '"\\ud800"', '"\\udfff"',
        '"escaped \\/ slash"',
        " \n\t{ \"x\" : [true, null, false, 1.0] }\r\n ",
        '{"duplicate":1,"duplicate":2}', '[{}, [], "", -0.0]',
        '{"large":1234567890123456789012345678901234567890}', "1e400", "1e-999",
        "[" * 200 + "0" + "]" * 200,
        " " * (MAX_INPUT - 2) + "{}",
    ]
    for _ in range(120):
        value = {rng.choice(STRINGS) + str(i): random_value(rng)
                 for i in range(rng.randrange(5))}
        json_inputs.append(json.dumps(value, ensure_ascii=rng.choice([True, False]),
                                     indent=rng.choice([None, 2]), allow_nan=False))
    for source in json_inputs:
        accepted(binary, source, passthrough=True)
        counts["JSON passthrough"] += 1

    unsupported = [
        "", "   ", "{'x':}", "{'x': 1", "{'x': 1} trailing", "{'x' 'y':}",
        "{1: 'numeric key'}", "{None: 'none key'}", "{True: 1}", "{(1,): 2}",
        "{'x': {1: 2}}", "{'x': b'bytes'}", "{'x': bytearray(b'x')}", "{'x': {1, 2}}",
        "{'x': set()}", "{'x': ...}", "{'x': 1j}", "{'x': 1+2j}", "{'x': float('nan')}",
        "{'x': NaN}", "{'x': Infinity}", "{'x': -Infinity}", "{'x': 1e999}",
        "{'x': -1e999}", "{'x': (lambda: 1)()}", "{'x': len('abc')}",
        "{'x': [x for x in []]}", "{**{'x': 1}}", "dict(x=1)",
        "('x',)", "(1,2)", "[True]", "None", "True", "False", "'text'", "0x10",
        "{'x': 01}", "{'x': 1__0}", "{'x': 0x}", "{'x': '\\xGG'}", "{'x': '\\uXYZW'}",
        "{'x': '\\U00110000'}", "{'x': 1; 'y': 2}", "{'x': 1} {'y': 2}",
        "{'x': ---1}", "{'x': 1 + 2}", "{'x': f'hello'}", "{'x': 'unterminated}",
        "{'x': +(-1)}", "{'x': -(True)}", "{'x': -(1,)}", "{'x': -()}",
        "{'x': 'raw\0nul'}", "{'x': 1} # raw\0nul", "\u00a0{'x': 1}", "\ufeff{'x': 1}",
        "{'x': " + "[" * 65 + "None" + "]" * 65 + "}",
        "{'x': " + "(" * 10000 + "1" + ")" * 10000 + "}",
        "{'x': " + "-" * 100000 + "1}",
        "{'x': " + "1+" * 10000 + "1}",
        "{'x': 1" + "()" * 100000 + "}",
        "{'x': 1" + "[0]" * 100000 + "}",
        " " * (MAX_INPUT - 1) + "{}",
        b"{'x': '\xff'}", b"\x00{}", b"{\xff}",
    ]
    for source in unsupported:
        rejected(binary, source)
        counts["rejections"] += 1

    alphabet = "{}[]():,+'\"\\# \n\t0123456789abcdefghijklmnopqrstuvwxyz-😀"
    seeds = ["{'a': [1, True, None, {'b': 'text'}]}", "{'quote': 'it\\'s fine'}",
             '{"x": [true, null, 1e2]}', "{'x': (1, 2, 3)}"]
    for index in range(600):
        if index < 400:
            source = rng.choice(seeds)
            for _ in range(rng.randrange(1, 5)):
                position = rng.randrange(len(source) + 1)
                action = rng.randrange(3)
                if action == 0:
                    source = source[:position] + rng.choice(alphabet) + source[position:]
                elif action == 1:
                    source = source[:position] + source[position + 1:]
                else:
                    source = source[:position] + rng.choice(alphabet) + source[position + 1:]
        else:
            source = "".join(rng.choice(alphabet) for _ in range(rng.randrange(1, 180)))
        result = run(binary, source)
        if result.returncode == 0:
            assert not result.stderr, f"successful fuzz input wrote stderr: {source!r}"
            actual = load_json(result.stdout.decode("utf-8"))
            try:
                load_json(source)
            except ValueError:
                assert canonical(actual) == canonical(expected_literal(source)), \
                    f"wrong fuzz conversion: {source!r}"
            else:
                assert result.stdout == source.encode("utf-8"), f"changed fuzz JSON: {source!r}"
        else:
            assert not result.stdout, f"failed fuzz input wrote stdout: {source!r}"
            assert result.stderr, f"failed fuzz input had no explanation: {source!r}"
        counts["adversarial inputs"] += 1

    print(f"Passed {sum(counts.values()):,} differential CLI cases (seed {SEED}):")
    for category, count in counts.items():
        print(f"  {category}: {count:,}")


if __name__ == "__main__":
    main()
