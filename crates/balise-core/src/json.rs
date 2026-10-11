//! JSON as Balise reads and writes it.
//!
//! Parsing text is delegated to `serde_json`; what comes out is Balise's own [`Value`], so
//! that the code generated for inputs and outputs never names a delegated crate.

use std::fmt::Write as _;

/// A JSON value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(Number),
    String(String),
    Array(Vec<Value>),
    /// Keys in alphabetical order: the order sent by the client is not kept.
    Object(Vec<(String, Value)>),
}

/// A JSON number, kept in the most precise form it fits in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Number {
    Integer(i64),
    Unsigned(u64),
    Float(f64),
}

impl Value {
    /// The kind of value, as messages name it: `a string`, `a number`…
    pub fn kind(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Bool(_) => "a boolean",
            Value::Number(_) => "a number",
            Value::String(_) => "a string",
            Value::Array(_) => "a list",
            Value::Object(_) => "an object",
        }
    }
}

/// Parses JSON text, or explains in one line why it is not valid JSON.
pub fn parse(text: &[u8]) -> Result<Value, String> {
    let value: serde_json::Value =
        serde_json::from_slice(text).map_err(|error| error.to_string())?;
    Ok(from_serde(value))
}

fn from_serde(value: serde_json::Value) -> Value {
    match value {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(b),
        serde_json::Value::Number(n) => Value::Number(match (n.as_i64(), n.as_u64()) {
            (Some(i), _) => Number::Integer(i),
            (None, Some(u)) => Number::Unsigned(u),
            _ => Number::Float(n.as_f64().unwrap_or(f64::NAN)),
        }),
        serde_json::Value::String(s) => Value::String(s),
        serde_json::Value::Array(items) => {
            Value::Array(items.into_iter().map(from_serde).collect())
        }
        serde_json::Value::Object(map) => {
            Value::Object(map.into_iter().map(|(k, v)| (k, from_serde(v))).collect())
        }
    }
}

/// Appends `text` to `out` as a JSON string, quotes included.
pub fn write_string(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < ' ' => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_into_balise_values() {
        assert_eq!(
            parse(br#"{"quantity": 2, "note": null, "tags": ["a"], "price": -1.5}"#),
            Ok(Value::Object(vec![
                ("note".into(), Value::Null),
                ("price".into(), Value::Number(Number::Float(-1.5))),
                ("quantity".into(), Value::Number(Number::Integer(2))),
                ("tags".into(), Value::Array(vec![Value::String("a".into())])),
            ]))
        );
        assert!(parse(b"{\"quantity\": }").unwrap_err().contains("line 1"));
    }

    #[test]
    fn writes_escaped_strings() {
        let mut out = String::new();
        write_string(&mut out, "a\"b\\c\nd\u{1}");
        assert_eq!(out, r#""a\"b\\c\nd\u0001""#);
    }
}
