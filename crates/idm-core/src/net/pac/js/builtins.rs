//! The built-in properties and methods of a value.

use super::*;

/// The property of a value.
pub(super) fn member(object: &Value, property: &str) -> Result<Value, PacError> {
    match object {
        Value::Str(text) => match property {
            "length" => Ok(Value::Num(text.chars().count() as f64)),
            _ => Err(PacError::unsupported(format!(
                "a string has no property '{property}'"
            ))),
        },
        Value::Array(items) => match property {
            "length" => Ok(Value::Num(items.len() as f64)),
            _ => Err(PacError::unsupported(format!(
                "a list has no property '{property}'"
            ))),
        },
        Value::Null => Err(PacError::runtime(
            "the script reads a property of null",
        )),
        Value::Undefined => Err(PacError::runtime(
            "the script reads a property of undefined",
        )),
        _ => Err(PacError::unsupported(format!(
            "the script reads the property '{property}' of something else"
        ))),
    }
}

/// The item of a list.
pub(super) fn indexed(object: &Value, index: &Value) -> Result<Value, PacError> {
    let Value::Array(items) = object else {
        return Err(PacError::runtime("the script indexes something that is not a list"));
    };
    let at = to_int(index).unwrap_or(0);
    if at < 0 || at as usize >= items.len() {
        return Ok(Value::Undefined);
    }
    Ok(items[at as usize].clone())
}

/// A method of a string or a list.
pub(super) fn method(object: &Value, name: &str, args: &[Value]) -> Result<Value, PacError> {
    match object {
        Value::Str(text) => string_method(text, name, args),
        Value::Array(items) => array_method(items, name, args),
        Value::Null | Value::Undefined => {
            Err(PacError::runtime("the script calls a method of nothing"))
        }
        _ => Err(PacError::unsupported(format!(
            "the script calls the unknown method '{name}'"
        ))),
    }
}

/// The number an argument holds.
fn argument_number(args: &[Value], at: usize) -> i64 {
    to_int(&args.get(at).cloned().unwrap_or(Value::Undefined)).unwrap_or(0)
}

/// The text an argument holds.
fn argument_text(args: &[Value], at: usize) -> String {
    args.get(at).map(as_text).unwrap_or_default()
}

/// A method of a string.
fn string_method(text: &str, name: &str, args: &[Value]) -> Result<Value, PacError> {
    let characters: Vec<char> = text.chars().collect();
    let number = |at: usize| argument_number(args, at);
    Ok(match name {
        "length" => Value::Num(characters.len() as f64),
        "charAt" => {
            let at = number(0);
            match characters.get(at.max(0) as usize) {
                Some(c) => Value::Str(c.to_string()),
                None => Value::Str(String::new()),
            }
        }
        "indexOf" => {
            let needle = argument_text(args, 0);
            match text.find(&needle) {
                Some(at) => Value::Num(text[..at].chars().count() as f64),
                None => Value::Num(-1.0),
            }
        }
        "lastIndexOf" => {
            let needle = argument_text(args, 0);
            match text.rfind(&needle) {
                Some(at) => Value::Num(text[..at].chars().count() as f64),
                None => Value::Num(-1.0),
            }
        }
        "startsWith" => Value::Bool(text.starts_with(&argument_text(args, 0))),
        "endsWith" => Value::Bool(text.ends_with(&argument_text(args, 0))),
        "includes" => Value::Bool(text.contains(&argument_text(args, 0))),
        "toLowerCase" => Value::Str(text.to_lowercase()),
        "toUpperCase" => Value::Str(text.to_uppercase()),
        "trim" => Value::Str(text.trim().to_owned()),
        "toString" => Value::Str(text.to_owned()),
        "substring" => {
            let mut from = number(0).max(0) as usize;
            let mut to = if args.len() > 1 {
                number(1).max(0) as usize
            } else {
                characters.len()
            };
            if from > to {
                std::mem::swap(&mut from, &mut to);
            }
            let from = from.min(characters.len());
            let to = to.min(characters.len());
            Value::Str(characters[from..to].iter().collect())
        }
        "slice" => {
            let from = slice_start(number(0), characters.len());
            let to = if args.len() > 1 {
                slice_start(number(1), characters.len())
            } else {
                characters.len()
            };
            let (from, to) = (from.min(characters.len()), to.min(characters.len()));
            if from >= to {
                Value::Str(String::new())
            } else {
                Value::Str(characters[from..to].iter().collect())
            }
        }
        "split" => {
            let separator = argument_text(args, 0);
            let parts: Vec<Value> = if separator.is_empty() {
                characters.iter().map(|c| Value::Str(c.to_string())).collect()
            } else {
                text.split(&separator as &str)
                    .map(|part| Value::Str(part.to_owned()))
                    .collect()
            };
            Value::Array(Rc::new(parts))
        }
        "replace" => {
            let from = argument_text(args, 0);
            let to = argument_text(args, 1);
            Value::Str(text.replacen(&from as &str, &to, 1))
        }
        "concat" => Value::Str(format!("{text}{}", argument_text(args, 0))),
        other => {
            return Err(PacError::unsupported(format!(
                "the script calls the unknown method '{other}' of a string"
            )))
        }
    })
}

/// Where a slice begins or ends, counting from the end when it is negative.
fn slice_start(at: i64, len: usize) -> usize {
    let at = if at < 0 {
        len as i64 + at
    } else {
        at
    };
    at.clamp(0, len as i64) as usize
}

/// A method of a list.
fn array_method(items: &Rc<Vec<Value>>, name: &str, args: &[Value]) -> Result<Value, PacError> {
    Ok(match name {
        "length" => Value::Num(items.len() as f64),
        "indexOf" => {
            let wanted = args.first().cloned().unwrap_or(Value::Undefined);
            match items
                .iter()
                .position(|item| loose_eq(item, &wanted))
            {
                Some(at) => Value::Num(at as f64),
                None => Value::Num(-1.0),
            }
        }
        "includes" => {
            let wanted = args.first().cloned().unwrap_or(Value::Undefined);
            Value::Bool(items.iter().any(|item| loose_eq(item, &wanted)))
        }
        "join" => {
            let separator = args
                .first()
                .map(as_text)
                .unwrap_or_else(|| ",".to_owned());
            Value::Str(
                items
                    .iter()
                    .map(as_text)
                    .collect::<Vec<String>>()
                    .join(&separator),
            )
        }
        "slice" => {
            let from = slice_start(argument_number(args, 0), items.len());
            let to = if args.len() > 1 {
                slice_start(argument_number(args, 1), items.len())
            } else {
                items.len()
            };
            let (from, to) = (from.min(items.len()), to.min(items.len()));
            if from >= to {
                Value::Array(Rc::new(Vec::new()))
            } else {
                Value::Array(Rc::new(items[from..to].to_vec()))
            }
        }
        "concat" => {
            let mut all = items.as_ref().clone();
            if let Some(Value::Array(other)) = args.first() {
                all.extend(other.iter().cloned());
            }
            Value::Array(Rc::new(all))
        }
        "toString" => Value::Str(
            items
                .iter()
                .map(as_text)
                .collect::<Vec<String>>()
                .join(","),
        ),
        other => {
            return Err(PacError::unsupported(format!(
                "the script calls the unknown method '{other}' of a list"
            )))
        }
    })
}
