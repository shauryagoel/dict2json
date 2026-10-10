use dict2json::{MAX_INPUT_BYTES, convert, convert_exact};
use std::{
    env,
    fs::File,
    io::{self, IsTerminal, Read, Write},
    process::ExitCode,
};

const HELP: &str = "Usage: dict2json [--exact] [INPUT]\n       dict2json [--exact] --file PATH\n       dict2json [--exact] < input.txt\n\nConvert a Python dictionary literal or valid JSON to JSON.\nBy default, use one space after object commas and key colons.\nPass INPUT as one quoted argument, or read stdin with no argument or '-'.\nUse -- before INPUT to stop option parsing. Choose only one input source.\nDictionary keys must be strings. Input limit: 16 MiB; Python nesting: 64.\n\nOptions:\n  --file PATH   Read UTF-8 input from a file\n  --exact       Preserve valid JSON unchanged; convert Python to compact JSON\n  -h, --help     Print help\n  -V, --version  Print version\n";

fn run() -> Result<(), String> {
    let arguments: Vec<String> = env::args_os()
        .skip(1)
        .map(|argument| {
            argument
                .into_string()
                .map_err(|_| "input must be valid UTF-8".to_owned())
        })
        .collect::<Result<_, _>>()?;
    let mut arguments = arguments.into_iter();
    let mut input = None;
    let mut file = None;
    let mut exact = false;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--help" | "-h" => {
                return io::stdout()
                    .lock()
                    .write_all(HELP.as_bytes())
                    .map_err(|error| error.to_string());
            }
            "--version" | "-V" => {
                return writeln!(
                    io::stdout().lock(),
                    "dict2json {}",
                    env!("CARGO_PKG_VERSION")
                )
                .map_err(|error| error.to_string());
            }
            "--exact" => exact = true,
            "--file" => {
                if file.is_some() {
                    return Err("--file can only be specified once; see --help".into());
                }
                file = Some(
                    arguments
                        .next()
                        .ok_or("--file requires a path; see --help")?,
                );
            }
            "--" => {
                let argument = arguments
                    .next()
                    .ok_or("expected an input after '--'; see --help")?;
                if input.replace(argument).is_some() || arguments.next().is_some() {
                    return Err("expected one input argument; see --help".into());
                }
                break;
            }
            option if option.starts_with("--") => {
                return Err(format!("unknown option {option:?}; see --help"));
            }
            _ => {
                if input.replace(argument).is_some() {
                    return Err("expected one input argument; see --help".into());
                }
            }
        }
    }
    let input = match (input, file) {
        (Some(_), Some(_)) => {
            return Err("choose an input argument or --file, not both; see --help".into());
        }
        (None, Some(path)) => {
            let file = File::open(&path)
                .map_err(|error| format!("could not open input file {path:?}: {error}"))?;
            read_input(file).map_err(|error| format!("{path:?}: {error}"))?
        }
        (Some(input), None) if input != "-" => input,
        _ => read_stdin()?,
    };
    let output = if exact {
        convert_exact(&input)?
    } else {
        convert(&input)?
    };
    io::stdout()
        .lock()
        .write_all(output.as_bytes())
        .map_err(|error| error.to_string())
}

fn read_stdin() -> Result<String, String> {
    let stdin = io::stdin();
    if stdin.is_terminal() {
        return Err(
            "provide an input argument, use --file, or pipe input on stdin; see --help".into(),
        );
    }
    read_input(stdin.lock())
}

fn read_input(reader: impl Read) -> Result<String, String> {
    let mut input = String::new();
    reader
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
