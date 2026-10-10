use rustpython_parser::{
    Mode, Parse, ParseError, ParseErrorType, Tok,
    ast::{Constant, Expr, ExprConstant, Ranged, UnaryOp},
    lexer::{LexicalErrorType, lex_starts_at},
    text_size::TextSize,
};
use serde_json::{Map, Number, Value, value::RawValue};

pub const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;
const MAX_DEPTH: usize = 64;

/// Convert Python dictionaries or valid JSON with one space after object commas and key colons.
pub fn convert(input: &str) -> Result<String, String> {
    Ok(space_object_entries(&convert_exact(input)?))
}

/// Preserve valid JSON byte for byte, or convert Python dictionaries to compact JSON.
pub fn convert_exact(input: &str) -> Result<String, String> {
    if input.len() > MAX_INPUT_BYTES {
        return Err(
            "input exceeds the 16 MiB limit\nhint: reduce the UTF-8 input to at most 16 MiB".into(),
        );
    }
    let json_error = match serde_json::from_str::<&RawValue>(input) {
        Ok(_) => return Ok(input.to_owned()),
        Err(error) => error,
    };
    let literal = input.trim_matches([' ', '\t', '\r', '\n']);
    let start = input.len() - input.trim_start_matches([' ', '\t', '\r', '\n']).len();
    if literal.is_empty() {
        return Err(error_at(
            input,
            start,
            "input is empty",
            "provide a Python dictionary or a valid JSON value",
        ));
    }
    if literal.contains('\0') || literal.starts_with('\u{feff}') {
        return Err(error_at(
            input,
            input.find('\0').unwrap_or(start),
            "Python source cannot contain NUL characters or start with a BOM",
            "remove NUL characters and save the source as UTF-8 without a BOM",
        ));
    }
    check_tokens(input, literal, TextSize::from(start as u32), &json_error)?;
    let expression = Expr::parse_starts_at(literal, "<input>", TextSize::from(start as u32))
        .map_err(|error| syntax_error(input, error))?;
    if !matches!(expression, Expr::Dict(_)) {
        return Err(error_at(
            input,
            usize::from(expression.start()),
            "Python input must be a dictionary with string keys",
            "wrap Python values in a dictionary, for example {'key': value}, or provide valid JSON",
        ));
    }
    serde_json::to_string(&to_json(expression, input)?).map_err(|error| error.to_string())
}

fn error_at(input: &str, offset: usize, message: &str, hint: &str) -> String {
    let mut offset = offset.min(input.len());
    while !input.is_char_boundary(offset) {
        offset -= 1;
    }
    let mut line = 1usize;
    let mut line_start = 0;
    for (index, character) in input[..offset].char_indices() {
        if character == '\r' || character == '\n' {
            if character == '\r' || !input[..index].ends_with('\r') {
                line += 1;
            }
            line_start = index + 1;
        }
    }
    let column = input[line_start..offset].chars().count();
    let source_line = input[line_start..].split(['\r', '\n']).next().unwrap_or("");
    let window_start = column.saturating_sub(40);
    let mut excerpt = if window_start > 0 {
        "...".to_owned()
    } else {
        String::new()
    };
    let mut caret = excerpt.len();
    let mut characters = source_line.chars().enumerate().skip(window_start);
    // shortcut: caret spacing counts characters, use display widths if wide-glyph alignment is needed.
    for (index, character) in characters.by_ref().take(100) {
        let displayed = match character {
            '\'' | '"' | '\\' => character.to_string(),
            _ => character.escape_debug().to_string(),
        };
        if index < column {
            caret += displayed.chars().count();
        }
        excerpt.push_str(&displayed);
    }
    if characters.next().is_some() {
        excerpt.push_str("...");
    }
    format!(
        "{message} at line {line}, column {}\n{line} | {excerpt}\n{} | {}^\nhint: {hint}",
        column + 1,
        " ".repeat(line.to_string().len()),
        " ".repeat(caret),
    )
}

fn syntax_error(input: &str, error: ParseError) -> String {
    let offset = usize::from(error.offset);
    let before = input.get(..offset).unwrap_or(input).trim_end();
    if let ParseErrorType::Lexical(error) = error.error {
        return lexical_error(input, offset, error);
    }
    let (message, hint) = if before.ends_with(':') {
        (
            "missing dictionary value",
            "add a value after ':' before closing the dictionary",
        )
    } else {
        match error.error {
            ParseErrorType::UnrecognizedToken(_, Some(expected)) if expected.contains(':') => (
                "missing ':' after a dictionary key",
                "add ':' between each dictionary key and its value",
            ),
            ParseErrorType::ExtraToken(_) => (
                "unexpected content after the input value",
                "provide one dictionary or JSON value; remove trailing content",
            ),
            ParseErrorType::Eof => (
                "incomplete input",
                "close any open quotes and delimiters ('}', ']', or ')') and supply missing values",
            ),
            _ => (
                "invalid syntax",
                "check for a missing ',' between entries, ':' between a key and value, or a mismatched delimiter",
            ),
        }
    };
    error_at(input, offset, message, hint)
}

fn json_syntax_error(
    input: &str,
    error: &serde_json::Error,
    keyword_offset: usize,
) -> Option<String> {
    // Serde's columns count bytes; the shared formatter counts Unicode characters.
    let offset = input
        .split_inclusive('\n')
        .take(error.line().saturating_sub(1))
        .map(str::len)
        .sum::<usize>()
        + error.column().saturating_sub(usize::from(!error.is_eof()));
    if offset <= keyword_offset {
        return None;
    }
    let description = error.to_string();
    let cause = description
        .rsplit_once(" at line ")
        .map_or(description.as_str(), |(cause, _)| cause);
    let before = input.get(..offset).unwrap_or(input).trim_end();
    let hint = if error.is_eof() {
        "close any open quotes, objects, or arrays and supply missing values"
    } else if cause == "trailing comma"
        || (before.ends_with(',')
            && input
                .get(offset..)
                .is_some_and(|after| after.starts_with(['}', ']'])))
    {
        "remove the trailing comma before '}' or ']'; JSON does not allow trailing commas"
    } else if before.ends_with(':') {
        "add a value after ':' before closing the JSON object"
    } else {
        "check JSON separators and delimiters; use double-quoted strings and true, false, or null"
    };
    Some(error_at(
        input,
        offset,
        &format!("invalid JSON: {cause}"),
        hint,
    ))
}

fn lexical_error(input: &str, offset: usize, error: LexicalErrorType) -> String {
    let (message, hint) = match &error {
        LexicalErrorType::StringError => (
            "unterminated string literal",
            "close the string with a matching quote; escape quotes inside the string",
        ),
        LexicalErrorType::OtherError(message) if message.contains("scanning string literal") => (
            "unterminated string literal",
            "close the quote before the newline, or use triple quotes for a multiline string",
        ),
        LexicalErrorType::Eof => (
            "unexpected end of input",
            "close any open quotes and delimiters ('}', ']', or ')') and supply missing values",
        ),
        LexicalErrorType::NestingError => (
            "unmatched closing delimiter",
            "remove the extra closing delimiter or add its matching opening delimiter",
        ),
        LexicalErrorType::UnicodeError => (
            "invalid Unicode escape",
            "use a valid Unicode escape with four hex digits after \\u or eight after \\U",
        ),
        LexicalErrorType::UnrecognizedToken { .. } => (
            "unexpected character in input",
            "remove the character or put it inside a quoted string",
        ),
        _ => {
            return error_at(
                input,
                offset,
                &error.to_string(),
                "check the literal's spelling, escapes, and indentation",
            );
        }
    };
    error_at(input, offset, message, hint)
}

fn space_object_entries(json: &str) -> String {
    // JSON is already validated; adjust separators without re-encoding its data.
    let mut output = String::with_capacity(json.len());
    let mut containers = Vec::new();
    let mut in_string = false;
    let mut escaped = false;
    let mut chars = json.chars().peekable();
    while let Some(character) = chars.next() {
        output.push(character);
        if in_string {
            if character == '"' && !escaped {
                in_string = false;
            }
            escaped = character == '\\' && !escaped;
            continue;
        }
        match character {
            '"' => in_string = true,
            '{' | '[' => containers.push(character),
            '}' | ']' => {
                containers.pop();
            }
            ',' | ':' if containers.last() == Some(&'{') => {
                output.push(' ');
                while chars.next_if(|c| c.is_ascii_whitespace()).is_some() {}
            }
            _ => {}
        }
    }
    output
}

fn check_tokens(
    source: &str,
    input: &str,
    start: TextSize,
    json_error: &serde_json::Error,
) -> Result<(), String> {
    let mut depth = 0usize;
    let mut value_start = true;
    let mut needs_number = false;
    for token in lex_starts_at(input, Mode::Expression, start) {
        let (token, range) = token
            .map_err(|error| lexical_error(source, usize::from(error.location), error.error))?;
        let offset = usize::from(range.start());
        if matches!(token, Tok::Newline | Tok::EndOfFile) {
            continue;
        }
        if needs_number {
            match token {
                Tok::Lpar => value_start = true,
                Tok::Int { .. } | Tok::Float { .. } => needs_number = false,
                _ => {
                    return Err(error_at(
                        source,
                        offset,
                        "a sign must be followed by a numeric literal",
                        "use a single '+' or '-' before an integer or float",
                    ));
                }
            }
        }
        value_start = match token {
            Tok::Lbrace | Tok::Lsqb | Tok::Lpar if value_start => {
                depth += 1;
                if depth > MAX_DEPTH {
                    return Err(error_at(
                        source,
                        offset,
                        "Python nesting exceeds the limit of 64",
                        "reduce nesting to at most 64 opening delimiters, including the outer dictionary",
                    ));
                }
                true
            }
            Tok::Rbrace | Tok::Rsqb | Tok::Rpar => {
                depth = depth.checked_sub(1).ok_or_else(|| {
                    error_at(
                        source,
                        offset,
                        "unmatched closing delimiter",
                        "remove the extra closing delimiter",
                    )
                })?;
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
                    check_unicode_escapes(&value).map_err(|error| error_at(source, offset, &error,
                        "use a valid Unicode character or escape; lone Unicode surrogates would lose data"))?;
                }
                false
            }
            _ => {
                let hint = match token {
                    Tok::String { .. } => {
                        "use an ordinary quoted string; bytes and f-strings are unsupported"
                    }
                    Tok::Name { name } if matches!(name.as_str(), "true" | "false" | "null") => {
                        if let Some(error) = json_syntax_error(source, json_error, offset) {
                            return Err(error);
                        }
                        "Python dictionaries use True, False, and None; JSON uses true, false, and null with double-quoted keys"
                    }
                    _ => {
                        "use literal values; variables, function calls, arithmetic, and comprehensions are unsupported"
                    }
                };
                return Err(error_at(
                    source,
                    offset,
                    "unsupported Python expression",
                    hint,
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

fn to_json(expression: Expr, input: &str) -> Result<Value, String> {
    let offset = usize::from(expression.start());
    let number_error = |error: String| {
        error_at(
            input,
            offset,
            &error,
            "use a finite integer or float; '+' and '-' can only apply to numeric literals",
        )
    };
    match expression {
        Expr::Dict(dictionary) => {
            let mut object = Map::new();
            for (key, value) in dictionary.keys.into_iter().zip(dictionary.values) {
                let key_offset = key.as_ref().map_or(offset, |key| usize::from(key.start()));
                let Some(Expr::Constant(ExprConstant {
                    value: Constant::Str(key),
                    ..
                })) = key
                else {
                    return Err(error_at(
                        input,
                        key_offset,
                        "dictionary keys must be strings",
                        "quote the dictionary key, for example {'key': value}",
                    ));
                };
                object.insert(key, to_json(value, input)?);
            }
            Ok(Value::Object(object))
        }
        Expr::List(list) => list
            .elts
            .into_iter()
            .map(|value| to_json(value, input))
            .collect(),
        Expr::Tuple(tuple) => tuple
            .elts
            .into_iter()
            .map(|value| to_json(value, input))
            .collect(),
        Expr::Constant(constant) => match constant.value {
            Constant::None => Ok(Value::Null),
            Constant::Bool(value) => Ok(Value::Bool(value)),
            Constant::Str(value) => Ok(Value::String(value)),
            value => number(value, false).map_err(number_error),
        },
        Expr::UnaryOp(unary) => {
            let Expr::Constant(constant) = *unary.operand else {
                return Err(error_at(
                    input,
                    offset,
                    "a sign must apply to a numeric literal",
                    "use '+' or '-' only with an integer or float",
                ));
            };
            match unary.op {
                UnaryOp::UAdd => number(constant.value, false).map_err(number_error),
                UnaryOp::USub => number(constant.value, true).map_err(number_error),
                _ => Err(error_at(
                    input,
                    offset,
                    "unsupported numeric operator",
                    "use a numeric literal with an optional '+' or '-' sign",
                )),
            }
        }
        _ => Err(error_at(
            input,
            offset,
            "only JSON-compatible Python literals are supported",
            "use dictionaries, lists, tuples, strings, finite numbers, True, False, or None; sets are unsupported",
        )),
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
