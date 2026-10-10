use rustpython_parser::{
    Mode, Parse, Tok,
    ast::{Constant, Expr, UnaryOp},
    lexer::lex,
};
use serde_json::{Map, Number, Value, value::RawValue};

pub const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;
const MAX_DEPTH: usize = 64;

/// Preserve JSON exactly, or safely convert a Python dictionary literal.
pub fn convert(input: &str) -> Result<String, String> {
    if input.len() > MAX_INPUT_BYTES {
        return Err("input exceeds the 16 MiB limit".into());
    }
    if serde_json::from_str::<&RawValue>(input).is_ok() {
        return Ok(input.to_owned());
    }
    let literal = input.trim_matches([' ', '\t', '\r', '\n']);
    if literal.contains('\0') || literal.starts_with('\u{feff}') {
        return Err("Python source cannot contain NUL characters or start with a BOM".into());
    }
    check_tokens(literal)?;
    let expression = Expr::parse(literal, "<input>").map_err(|error| error.to_string())?;
    if !matches!(expression, Expr::Dict(_)) {
        return Err("Python input must be a dictionary with string keys".into());
    }
    serde_json::to_string(&to_json(expression)?).map_err(|error| error.to_string())
}

fn check_tokens(input: &str) -> Result<(), String> {
    let mut depth = 0usize;
    let mut value_start = true;
    let mut needs_number = false;
    for token in lex(input, Mode::Expression) {
        let (token, range) = token
            .map_err(|error| format!("{} at byte {}", error.error, u32::from(error.location)))?;
        if matches!(token, Tok::Newline | Tok::EndOfFile) {
            continue;
        }
        if needs_number {
            match token {
                Tok::Lpar => value_start = true,
                Tok::Int { .. } | Tok::Float { .. } => needs_number = false,
                _ => return Err("a sign must be followed by a numeric literal".into()),
            }
        }
        value_start = match token {
            Tok::Lbrace | Tok::Lsqb | Tok::Lpar if value_start => {
                depth += 1;
                if depth > MAX_DEPTH {
                    return Err("Python nesting exceeds the limit of 64".into());
                }
                true
            }
            Tok::Rbrace | Tok::Rsqb | Tok::Rpar => {
                depth = depth.checked_sub(1).ok_or("unmatched closing delimiter")?;
                false
            }
            Tok::Colon | Tok::Comma => true,
            Tok::Plus | Tok::Minus if value_start => {
                needs_number = true;
                false
            }
            Tok::Int { .. } | Tok::Float { .. } | Tok::True | Tok::False | Tok::None => false,
            Tok::String { value, kind, .. } if !kind.is_any_bytes() && !kind.is_any_fstring() => {
                if !kind.is_raw() {
                    check_unicode_escapes(&value)?;
                }
                false
            }
            _ => {
                return Err(format!(
                    "unsupported Python expression at byte {}",
                    u32::from(range.start())
                ));
            }
        };
    }
    Ok(())
}

fn check_unicode_escapes(value: &str) -> Result<(), String> {
    let mut chars = value.chars();
    while let Some(character) = chars.next() {
        if character != '\\' {
            continue;
        }
        let digits = match chars.next() {
            Some('u') => 4,
            Some('U') => 8,
            _ => continue,
        };
        let escape: String = chars.by_ref().take(digits).collect();
        // RustPython replaces surrogates with U+FFFD; reject rather than lose data.
        if u32::from_str_radix(&escape, 16).is_ok_and(|code| (0xd800..=0xdfff).contains(&code)) {
            return Err("Python strings containing Unicode surrogates are unsupported".into());
        }
    }
    Ok(())
}

fn to_json(expression: Expr) -> Result<Value, String> {
    match expression {
        Expr::Dict(dictionary) => {
            let mut object = Map::new();
            for (key, value) in dictionary.keys.into_iter().zip(dictionary.values) {
                let Some(Expr::Constant(key)) = key else {
                    return Err("dictionary keys must be strings".into());
                };
                let Constant::Str(key) = key.value else {
                    return Err("dictionary keys must be strings".into());
                };
                object.insert(key, to_json(value)?);
            }
            Ok(Value::Object(object))
        }
        Expr::List(list) => list.elts.into_iter().map(to_json).collect(),
        Expr::Tuple(tuple) => tuple.elts.into_iter().map(to_json).collect(),
        Expr::Constant(constant) => match constant.value {
            Constant::None => Ok(Value::Null),
            Constant::Bool(value) => Ok(Value::Bool(value)),
            Constant::Str(value) => Ok(Value::String(value)),
            value => number(value, false),
        },
        Expr::UnaryOp(unary) => {
            let Expr::Constant(constant) = *unary.operand else {
                return Err("a sign must apply to a numeric literal".into());
            };
            match unary.op {
                UnaryOp::UAdd => number(constant.value, false),
                UnaryOp::USub => number(constant.value, true),
                _ => Err("unsupported numeric operator".into()),
            }
        }
        _ => Err("only JSON-compatible Python literals are supported".into()),
    }
}

fn number(value: Constant, negative: bool) -> Result<Value, String> {
    let number = match value {
        Constant::Int(value) => {
            let value = if negative { -value } else { value };
            value
                .to_string()
                .parse::<Number>()
                .map_err(|error| error.to_string())?
        }
        Constant::Float(value) => Number::from_f64(if negative { -value } else { value })
            .ok_or("non-finite numbers cannot be represented in JSON")?,
        _ => return Err("unsupported Python value".into()),
    };
    Ok(Value::Number(number))
}
