use std::{
    io::Write,
    process::{Command, Output, Stdio},
};

fn run(arguments: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_dict2json"))
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn accepts_an_argument_stdin_and_explicit_stdin() {
    for (arguments, input) in [
        (vec!["{'a': True, 'b': None}"], b"".as_slice()),
        (vec![], b"{'a': True, 'b': None}".as_slice()),
        (vec!["-"], b"{'a': True, 'b': None}".as_slice()),
        (vec!["--", "{'a': True, 'b': None}"], b"".as_slice()),
    ] {
        let output = run(&arguments, input);
        assert!(output.status.success(), "{:?}", output.stderr);
        assert_eq!(output.stdout, br#"{"a":true, "b":null}"#);
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn spaces_valid_json_through_both_input_paths() {
    let json = " \n{\"b\":1e400,\"a\":-0,\"b\":2}\t ";
    let expected = b" \n{\"b\":1e400, \"a\":-0, \"b\":2}\t ";
    for output in [run(&[json], b""), run(&[], json.as_bytes())] {
        assert!(output.status.success());
        assert_eq!(output.stdout, expected);
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn errors_have_no_stdout_and_a_nonzero_status() {
    for output in [
        run(&["{'x':}"], b""),
        run(&["{'x': 1e400}"], b""),
        run(&["one", "two"], b""),
        run(&[], b""),
        run(&[], &[0xff, 0xfe]),
    ] {
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).starts_with("dict2json: "));
    }
}

#[test]
fn prints_help_and_version() {
    for argument in ["-h", "--help", "-V", "--version"] {
        let output = run(&[argument], b"");
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("dict2json"));
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn rejects_long_call_and_subscript_chains_without_aborting() {
    for suffix in ["()", "[0]"] {
        let input = format!("{{'x': 1{}}}", suffix.repeat(100_000));
        let output = run(&[], input.as_bytes());
        assert_eq!(output.status.code(), Some(1), "{:?}", output.stderr);
        assert!(output.stdout.is_empty());
    }
}
