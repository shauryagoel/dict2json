use dict2json::convert;
use serde_json::{Value, json};

#[test]
fn converts_the_supplied_example_in_order() {
    let input = "{'project_name': 'xyz', 'start_time': '2026-10-07 13:00:00+05:30', 'end_time': '2026-10-07 19:00:00+05:30'}";
    let output = convert(input).unwrap();
    let expected = r#"{"project_name":"xyz", "start_time":"2026-10-07 13:00:00+05:30", "end_time":"2026-10-07 19:00:00+05:30"}"#;
    assert_eq!(output, expected);
    assert_eq!(
        convert(r#"{"project_name":"xyz","start_time":"2026-10-07 13:00:00+05:30","end_time":"2026-10-07 19:00:00+05:30"}"#).unwrap(),
        expected
    );
}

#[test]
fn preserves_valid_json_with_no_object_spacing_changes_needed() {
    for input in [
        "{}",
        "  {\"x\": 1, \"x\": 2}\n",
        "[true, null]",
        "1e400",
        "-0",
        "true",
        "null",
        "\"hello\"",
        "{\"x\":\"\\ud800\"}",
    ] {
        assert_eq!(convert(input).unwrap(), input);
    }
}

#[test]
fn normalizes_valid_json_object_separator_spacing() {
    for (input, expected) in [
        (
            "\n{ \"a\" :1,\t \r\n  \"b\":2}\n",
            "\n{ \"a\" :1, \"b\":2}\n",
        ),
        (
            r#"[{"a":1,"b":2},{"c":3,"d":4},1,2]"#,
            r#"[{"a":1, "b":2},{"c":3, "d":4},1,2]"#,
        ),
        (
            r#"{"a":1e400,"a":-0,"u":"\ud800","n":1.00}"#,
            r#"{"a":1e400, "a":-0, "u":"\ud800", "n":1.00}"#,
        ),
        (r#"{"only":1}"#, r#"{"only":1}"#),
        ("{}", "{}"),
    ] {
        let output = convert(input).unwrap();
        assert_eq!(output, expected, "{input:?}");
        assert_eq!(convert(&output).unwrap(), expected);
    }
}

#[test]
fn spaces_nested_objects_without_changing_arrays_or_string_contents() {
    let expected = r#"{"a,b":"a,b {\"x\":1,\"y\":2}", "slash\\":"ends with \\", "items":[1,{"a":2, "b":3},[4,5]]}"#;
    for input in [
        r#"{'a,b': 'a,b {"x":1,"y":2}', 'slash\\': 'ends with \\', 'items': [1, {'a': 2, 'b': 3}, (4, 5)]}"#,
        r#"{"a,b":"a,b {\"x\":1,\"y\":2}","slash\\":"ends with \\","items":[1,{"a":2,"b":3},[4,5]]}"#,
    ] {
        assert_eq!(convert(input).unwrap(), expected, "{input:?}");
    }
}

#[test]
fn converts_nested_literals_and_escapes() {
    let input = r#"{'x': [True, False, None, {'y': (1, -2, +3.5,)}], 's': 'it\'s "fine"\n\u263a', 'raw': r'\n', 'joined': 'a' 'b'}"#;
    assert_eq!(
        serde_json::from_str::<Value>(&convert(input).unwrap()).unwrap(),
        json!({"x": [true, false, null, {"y": [1, -2, 3.5]}], "s": "it's \"fine\"\n☺", "raw": "\\n", "joined": "ab"})
    );
}

#[test]
fn rejects_unsupported_or_malformed_input() {
    for input in [
        "",
        "{'x':}",
        "{'x': NaN}",
        "{'x': 1e400}",
        "{1: 'x'}",
        "{'x': b'bytes'}",
        "{'x': {1, 2}}",
        "{'x': f'{1}'}",
        "{'x': 1 + 2}",
        "{'x': --1}",
        "{'x': print('bad')}",
        "{'x': [i for i in []]}",
        "{'x': None} junk",
        "{'x': 1, **{}}",
        "{'x': ...}",
        "{'x': 1j}",
        "{'x': '\\ud800'}",
    ] {
        assert!(convert(input).is_err(), "accepted {input:?}");
    }
}

#[test]
fn preserves_numeric_values_without_truncating_integers() {
    let cases = [
        ("{'n': 0xff}", "255"),
        ("{'n': -0b101}", "-5"),
        ("{'n': 0o755}", "493"),
        ("{'n': 1_000_000}", "1000000"),
        (
            "{'n': 999999999999999999999999999999999999999999}",
            "999999999999999999999999999999999999999999",
        ),
        (
            "{'n': -999999999999999999999999999999999999999999}",
            "-999999999999999999999999999999999999999999",
        ),
        ("{'n': -0.0}", "-0.0"),
        ("{'n': .5}", "0.5"),
        ("{'n': 2.}", "2.0"),
        ("{'n': 1e-400}", "0.0"),
        ("{'n': -(1)}", "-1"),
        ("{'n': +(.5)}", "0.5"),
        ("{'n': -(((2.0)))}", "-2.0"),
        ("{'n': -(0xFF)}", "-255"),
    ];
    for (input, expected) in cases {
        assert_eq!(convert(input).unwrap(), format!("{{\"n\":{expected}}}"));
    }
}

#[test]
fn duplicate_keys_keep_the_last_value_and_original_position() {
    assert_eq!(
        convert("{'x': 1, 'y': 2, 'x': 3}").unwrap(),
        r#"{"x":3, "y":2}"#
    );
}

#[test]
fn handles_python_string_forms_without_rewriting_their_contents() {
    let cases = [
        (r#"{'s': '\x41\101\N{SNOWMAN}'}"#, "AA☃"),
        (r#"{'s': u'\U0001f680'}"#, "🚀"),
        (r#"{'s': r'\ud800'}"#, "\\ud800"),
        (r#"{'s': '\\ud800'}"#, "\\ud800"),
        ("{'s': '''line 1\nline 2'''}", "line 1\nline 2"),
        (
            r#"{'s': 'True False None {"a": null}'}"#,
            "True False None {\"a\": null}",
        ),
        ("{'s': 'a\\\nb'}", "ab"),
        (r#"{'s': '\a\b\f\n\r\t\v\0'}"#, "\x07\x08\x0c\n\r\t\x0b\0"),
    ];
    for (input, expected) in cases {
        let output: Value = serde_json::from_str(&convert(input).unwrap()).unwrap();
        assert_eq!(output["s"], expected, "{input:?}");
    }
}

#[test]
fn guards_resource_limits_and_long_expressions() {
    let at_limit = format!("{{'x': {}0{}}}", "[".repeat(63), "]".repeat(63));
    assert!(convert(&at_limit).is_ok());
    let too_deep = format!("{{'x': {}0{}}}", "[".repeat(64), "]".repeat(64));
    assert!(convert(&too_deep).is_err());
    for expression in ["-".repeat(10_000) + "1", "1+".repeat(10_000) + "1"] {
        assert!(convert(&format!("{{'x': {expression}}}")).is_err());
    }
    assert!(convert(&" ".repeat(dict2json::MAX_INPUT_BYTES + 1)).is_err());
    let deep_json = "[".repeat(10_000) + "0" + &"]".repeat(10_000);
    assert_eq!(convert(&deep_json).unwrap(), deep_json);
    let deep_json = "[".repeat(10_000) + r#"{"x":1,"y":2}"# + &"]".repeat(10_000);
    let expected = "[".repeat(10_000) + r#"{"x":1, "y":2}"# + &"]".repeat(10_000);
    assert_eq!(convert(&deep_json).unwrap(), expected);
}

#[test]
fn generated_dictionaries_always_produce_the_expected_json() {
    for index in 0..2_000 {
        let string = format!("case {index}: quotes '\" slashes \\ \n🚀 हिंदी True None");
        let quoted = serde_json::to_string(&string).unwrap();
        let input = format!(
            "{{'s': {quoted}, 'n': -{index}, 'items': (True, None, {{'x': [{index}, False]}}),}}"
        );
        let output: Value = serde_json::from_str(&convert(&input).unwrap()).unwrap();
        assert_eq!(
            output,
            json!({"s": string, "n": -index, "items": [true, null, {"x": [index, false]}]})
        );
    }
}

#[test]
fn rejects_invalid_python_source_characters() {
    for input in [
        "{'s': '\0'}",
        "{'x': 1} #\0",
        "\u{a0}{'x':1}\u{a0}",
        "\u{feff}{'x':1}",
    ] {
        assert!(convert(input).is_err(), "accepted {input:?}");
    }
}

#[test]
fn rejects_signed_non_numeric_operands() {
    for input in [
        "{'x': +(-1)}",
        "{'x': -(True)}",
        "{'x': -(1,)}",
        "{'x': -()}",
    ] {
        assert!(convert(input).is_err(), "accepted {input:?}");
    }
}
