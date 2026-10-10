use std::{
    fs,
    io::Write,
    path::PathBuf,
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

struct InputFile(PathBuf);

impl InputFile {
    fn new(name: &str, contents: &[u8]) -> Self {
        let path = std::env::temp_dir().join(format!("dict2json-{}-{name}", std::process::id()));
        fs::write(&path, contents).unwrap();
        Self(path)
    }

    fn path(&self) -> &str {
        self.0.to_str().unwrap()
    }
}

impl Drop for InputFile {
    fn drop(&mut self) {
        fs::remove_file(&self.0).unwrap();
    }
}

#[test]
fn reads_a_file_without_changing_it() {
    let source = b"{'a': True, 'b': None}";
    let file = InputFile::new("input with spaces.txt", source);
    let output = run(&["--file", file.path()], b"ignored stdin");
    assert!(output.status.success(), "{:?}", output.stderr);
    assert_eq!(output.stdout, br#"{"a":true, "b":null}"#);
    assert!(output.stderr.is_empty());
    assert_eq!(fs::read(&file.0).unwrap(), source);
}

#[test]
fn exact_preserves_json_bytes_from_every_input_path() {
    let source = " \r\n{\"b\":1e400,\"a\":-0,\"b\":2,\"u\":\"\\ud800\"}\t ";
    let file = InputFile::new("exact.json", source.as_bytes());
    for output in [
        run(&["--exact", source], b""),
        run(&[source, "--exact"], b""),
        run(&["--exact"], source.as_bytes()),
        run(&["--exact", "-"], source.as_bytes()),
        run(&["--exact", "--", source], b""),
        run(&["--exact", "--file", file.path()], b""),
        run(&["--file", file.path(), "--exact"], b""),
    ] {
        assert!(output.status.success(), "{:?}", output.stderr);
        assert_eq!(output.stdout, source.as_bytes());
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn exact_still_converts_python_literals() {
    let output = run(&["--exact", "{'a': True, 'b': None}"], b"");
    assert!(output.status.success(), "{:?}", output.stderr);
    assert_eq!(output.stdout, br#"{"a":true,"b":null}"#);
    assert!(output.stderr.is_empty());
}

#[test]
fn accepts_negative_json_numbers_as_arguments() {
    for input in ["-0", "-1", "-1.00", "-2.3E-10", "-1e400"] {
        for arguments in [vec![input], vec!["--exact", input]] {
            let output = run(&arguments, b"");
            assert!(
                output.status.success(),
                "{arguments:?}: {:?}",
                output.stderr
            );
            assert_eq!(output.stdout, input.as_bytes());
            assert!(output.stderr.is_empty());
        }
    }
}

#[test]
fn spaces_valid_json_from_a_file() {
    let source = b" \n{\"b\":1e400,\"a\":-0,\"b\":2}\t ";
    let file = InputFile::new("spacing.json", source);
    let output = run(&["--file", file.path()], b"");
    assert!(output.status.success(), "{:?}", output.stderr);
    assert_eq!(output.stdout, b" \n{\"b\":1e400, \"a\":-0, \"b\":2}\t ");
    assert!(output.stderr.is_empty());
}

#[test]
fn rejects_invalid_file_input_and_option_combinations() {
    let invalid = InputFile::new("invalid.txt", b"{'x':}");
    let non_utf8 = InputFile::new("non-utf8.txt", &[0xff, 0xfe]);
    let mut oversized_source = vec![b' '; dict2json::MAX_INPUT_BYTES + 1];
    oversized_source[..2].copy_from_slice(b"{}");
    let oversized = InputFile::new("oversized.txt", &oversized_source);
    let missing =
        std::env::temp_dir().join(format!("dict2json-{}-missing.txt", std::process::id()));
    assert!(!missing.exists());
    for arguments in [
        vec!["--file"],
        vec!["--file", missing.to_str().unwrap()],
        vec!["--file", invalid.path()],
        vec!["--exact", "--file", invalid.path()],
        vec!["--file", non_utf8.path()],
        vec!["--file", oversized.path()],
        vec!["--file", invalid.path(), "{}"],
        vec!["{}", "--file", invalid.path()],
        vec!["--file", invalid.path(), "-"],
        vec!["--file", invalid.path(), "--file", invalid.path()],
        vec!["--unknown"],
        vec!["--"],
        vec!["--exact", "{'x':}"],
    ] {
        let output = run(&arguments, b"");
        assert_eq!(output.status.code(), Some(1), "{arguments:?}");
        assert!(output.stdout.is_empty(), "{arguments:?}");
        assert!(String::from_utf8_lossy(&output.stderr).starts_with("dict2json: "));
    }
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
        if argument == "-h" || argument == "--help" {
            let help = String::from_utf8_lossy(&output.stdout);
            assert!(help.contains("--file PATH"));
            assert!(help.contains("--exact"));
        }
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
