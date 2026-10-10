use dict2json::{MAX_INPUT_BYTES, convert};
use std::{
    env,
    io::{self, IsTerminal, Read, Write},
    process::ExitCode,
};

const HELP: &str = "Usage: dict2json [INPUT]\n       dict2json < input.txt\n\nConvert a Python dictionary literal to JSON. Valid JSON is preserved exactly.\nPass INPUT as one quoted argument, or read stdin with no argument or '-'.\nDictionary keys must be strings. Input limit: 16 MiB; Python nesting: 64.\n\nOptions:\n  -h, --help     Print help\n  -V, --version  Print version\n";

fn run() -> Result<(), String> {
    let arguments: Vec<String> = env::args_os()
        .skip(1)
        .map(|argument| {
            argument
                .into_string()
                .map_err(|_| "input must be valid UTF-8".to_owned())
        })
        .collect::<Result<_, _>>()?;
    let input = match arguments.as_slice() {
        [option] if option == "--help" || option == "-h" => {
            return io::stdout()
                .lock()
                .write_all(HELP.as_bytes())
                .map_err(|error| error.to_string());
        }
        [option] if option == "--version" || option == "-V" => {
            return writeln!(
                io::stdout().lock(),
                "dict2json {}",
                env!("CARGO_PKG_VERSION")
            )
            .map_err(|error| error.to_string());
        }
        [] => read_stdin()?,
        [input] if input == "-" => read_stdin()?,
        [input] => input.clone(),
        [separator, input] if separator == "--" => input.clone(),
        _ => return Err("expected one input argument, or stdin; see --help".into()),
    };
    let output = convert(&input)?;
    io::stdout()
        .lock()
        .write_all(output.as_bytes())
        .map_err(|error| error.to_string())
}

fn read_stdin() -> Result<String, String> {
    let stdin = io::stdin();
    if stdin.is_terminal() {
        return Err("provide an input argument or pipe input on stdin; see --help".into());
    }
    let mut input = String::new();
    stdin
        .lock()
        .take((MAX_INPUT_BYTES + 1) as u64)
        .read_to_string(&mut input)
        .map_err(|error| format!("could not read UTF-8 input: {error}"))?;
    Ok(input)
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = writeln!(io::stderr().lock(), "dict2json: {error}");
            ExitCode::FAILURE
        }
    }
}
