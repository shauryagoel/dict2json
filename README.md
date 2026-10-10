# dict2json

A Rust CLI that safely converts Python dictionary literals into JSON, with one
space after object commas and key colons by default and no added trailing
space. Already-valid JSON uses the same spacing. All other input bytes are preserved,
including string contents, array separators, key order, duplicate keys, and
number formatting. Use `--exact` to preserve already-valid JSON byte for byte.
Python is not needed to run it.

## Build and install

Install a Rust toolchain, then run:

```sh
cargo build --locked --release
```

The executable is `target/release/dict2json`. To install it on your PATH:

```sh
cargo install --locked --path .
```

## Usage

Pass the dictionary as one shell argument:

```sh
dict2json "{'project_name': 'xyz', 'start_time': '2026-10-07 13:00:00+05:30', 'end_time': '2026-10-07 19:00:00+05:30'}"
```

Output:

```json
{"project_name": "xyz", "start_time": "2026-10-07 13:00:00+05:30", "end_time": "2026-10-07 19:00:00+05:30"}
```

Stdin is useful for files, multiline input, or strings with shell characters:

```sh
dict2json <<'EOF'
{'name': 'Alice', 'active': True, 'missing': None, 'items': [1, 2, 3]}
EOF

dict2json < input.txt
```

Read input directly from a file with `--file <path>`:

```sh
dict2json --file input.txt
```

Add `--exact` to preserve valid JSON exactly, including whitespace and newlines:

```sh
dict2json --exact --file input.json
dict2json --exact '{"a":1,"b":2}'
dict2json --exact < input.json
```

When `--exact` receives a Python dictionary, it still converts the literal to
JSON, using compact output without added separator spaces. `--exact` may appear
before or after the input argument or file option. File input uses the same
UTF-8 requirement and 16 MiB size limit as stdin; the source file is never changed.
Choose one input source: an argument, a file, or stdin.

No argument, or `-`, reads the entire stdin stream as one value. Use `--` before
an input argument to stop option parsing. `--help` and `--version` print usage and
version information. Output has no added newline.
Object separator spacing also applies to nested objects; array separators and
commas or colons inside strings keep their existing formatting. For valid JSON,
whitespace immediately after an object separator comma or key colon is replaced
by one space unless `--exact` is used.

## Supported input

Python input must be a dictionary with **string keys**. Values may contain:

- Dictionaries, lists, and tuples; tuples become JSON arrays.
- Single/double/triple quoted strings, raw and Unicode string prefixes,
  adjacent string literals, Unicode characters, and Python escape sequences.
- Integers, including arbitrary precision, binary/octal/hexadecimal literals,
  and digit separators; finite floats, including scientific notation.
- `True`, `False`, and `None`, converted to `true`, `false`, and `null`.
- Trailing commas, comments, and whitespace. Duplicate string keys keep their
  original position and take their last value, following Python behavior.

Numeric literals may have a single sign and grouping parentheses. Function calls,
variables, arithmetic, comprehensions, dictionary unpacking, f-strings, bytes,
sets, complex values, non-string keys, and non-finite floats are rejected.
Python strings containing escaped Unicode surrogates are rejected to prevent
data loss. Already-valid JSON containing escaped surrogates is preserved.

Any valid JSON value is accepted, including objects, arrays, strings,
numbers, booleans, and `null`. The size limit is 16 MiB of UTF-8 input; Python
delimiter nesting is limited to 64, including the outer dictionary. Valid JSON
does not have this Python nesting limit.

Invalid or unsupported input exits with status `1`, writes a diagnostic to
stderr, and leaves stdout empty. Syntax and literal validation errors show a
one-based line and column in the original input, a source excerpt with a caret,
and a hint about the likely correction. For example, `dict2json "{'x':}"` reports:

```text
dict2json: missing dictionary value at line 1, column 6
1 | {'x':}
  |      ^
hint: add a value after ':' before closing the dictionary
```

File conversion errors also include the source filename. Diagnostics work with
arguments, stdin, files, and `--exact`. Columns count Unicode characters;
leading whitespace and LF, CRLF, or CR line endings keep their original positions.
Excerpts are limited to 100 source characters around the error, with `...` marking
cropped text. Tabs and control characters are escaped for safe terminal display.
Carets use character spacing, so wide Unicode glyphs can affect visual alignment.
The location marks where the problem was detected; an unclosed string or
delimiter can point to the end of the input. Hints describe probable causes, and
one error is reported at a time. Input size and read failures report the relevant
limit or I/O error.

Input is parsed using
[RustPython](https://docs.rs/rustpython-parser/0.4.0/rustpython_parser/), never
executed. JSON validation and encoding use
[serde_json](https://docs.rs/serde_json/latest/serde_json/).

## Tests

```sh
cargo test --locked
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked --release
python3 tests/differential.py target/release/dict2json
```

Rust tests cover conversion, 2,000 generated dictionaries, input limits,
malformed source, diagnostic locations and hints, bounded terminal-safe excerpts,
files/stdin/arguments, object spacing, exact JSON preservation, negative numeric
arguments, and CLI error paths. The optional Python
standard-library suite runs over 3,000 additional CLI cases, comparing conversions
against `ast.literal_eval` and
`json.dumps`, and checking JSON token preservation and clean rejection of
adversarial inputs. Python is needed only for that extra test suite. Both
generators use fixed inputs/seeds so failures are reproducible.
