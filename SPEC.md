# dict2json

Build a standalone Rust CLI named `dict2json` that converts a Python dictionary
literal into one JSON string. Python is not required at runtime.

- Accept exactly one input argument, or read the entire UTF-8 stdin stream.
- Preserve valid JSON byte for byte, including whitespace, numbers, and key order.
  Any JSON value is accepted, including arrays and scalars.
- Otherwise accept a Python dictionary with string keys. Support nested
  dictionaries, lists, tuples (JSON arrays), strings, integers, finite floats,
  `True`, `False`, and `None`. Keep dictionary insertion order and use the last
  value for a repeated string key, as Python does.
- Parse literals only; never evaluate functions, variables, comprehensions,
  interpolation, arithmetic, or dictionary unpacking. Reject values without a
  JSON representation rather than guessing or emitting invalid JSON.
- Limit input to 16 MiB and Python delimiter nesting to 64 for predictable
  resource use. Invalid input produces a nonzero exit status, an explanatory
  message on stderr, and no stdout. Write no extra newline to preserve JSON.
- Provide `--help` and `--version`, using Rust's standard argument/I/O APIs.

Use RustPython's lexer/parser for Python syntax and string decoding, and
`serde_json` for JSON validation and encoding. Preflight Python tokens before
AST parsing to reject expressions and limit nesting. No hand-written string
replacement or Python execution. Keep source in `src/`, Rust integration tests
in `tests/cli.rs`, and an optional Python differential suite in
`tests/differential.py`.

Implementation order: establish failing conversion and CLI tests; implement
the JSON pass-through path and guarded literal conversion; add CLI I/O and
usage docs; run the broad differential suite and review error/resource paths.

Acceptance checks:

```sh
cargo test --locked
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked --release
python3 tests/differential.py target/release/dict2json
```

Use standard Rust formatting and explicit `Result` errors at input boundaries.
Every successful conversion must decode as JSON and retain the expected data;
JSON input must remain exactly unchanged. Test the supplied timestamp example,
Unicode and escape forms, numeric boundaries, nesting, malformed/unsupported
inputs, stdin and arguments, exit codes, and deterministic generated cases.
Do not change existing agent configuration, publish, or merge as part of this
task.
