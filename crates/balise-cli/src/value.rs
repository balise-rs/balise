//! Reading JSON values: the compiler's diagnostics, and what the app answers.

use balise_core::json::{Number, Value};

pub fn get<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    match value {
        Value::Object(fields) => fields.iter().find(|(k, _)| k == key).map(|(_, v)| v),
        _ => None,
    }
}

pub fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    match get(value, key)? {
        Value::String(text) => Some(text),
        _ => None,
    }
}

pub fn number(value: &Value, key: &str) -> usize {
    match get(value, key) {
        Some(Value::Number(Number::Integer(n))) => (*n).max(0) as usize,
        Some(Value::Number(Number::Unsigned(n))) => *n as usize,
        _ => 0,
    }
}

pub fn array<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    match get(value, key) {
        Some(Value::Array(items)) => items,
        _ => &[],
    }
}
