//! What the `Output` derive builds on: writing a value as JSON.
//!
//! The generated code writes each field with [`OutputValue::write_json`]; the field names
//! are written as declared, in declaration order.

use std::fmt::Write as _;

use crate::handler::{IntoResponse, Request, Response};
use crate::json;

/// A struct that derives `Output`: returned by a handler, sent as JSON.
pub trait Output {
    fn write_json(&self, out: &mut String);
}

/// A value an `Output` field can hold: text, numbers, booleans, lists, other outputs.
#[diagnostic::on_unimplemented(
    message = "CMP0010: `{Self}` cannot be a field of an `Output`",
    label = "Balise cannot write this type",
    note = "fix: Use text, numbers, booleans and lists of them; `Option<T>` and `Patch<T>` in an `Input`; another `Output` in an `Output`.",
    note = "more: balise explain CMP0010"
)]
pub trait OutputValue {
    fn write_json(&self, out: &mut String);
}

impl<T: Output> OutputValue for T {
    fn write_json(&self, out: &mut String) {
        Output::write_json(self, out);
    }
}

impl<T: Output> IntoResponse for T {
    fn into_response(self, _: &Request) -> Response {
        let mut body = String::new();
        Output::write_json(&self, &mut body);
        Response::json(200, body)
    }
}

/// Writes one field, `"name":value`, preceded by a comma unless it is the first.
pub fn write_field<T: OutputValue>(out: &mut String, first: bool, name: &str, value: &T) {
    if !first {
        out.push(',');
    }
    json::write_string(out, name);
    out.push(':');
    value.write_json(out);
}

impl OutputValue for String {
    fn write_json(&self, out: &mut String) {
        json::write_string(out, self);
    }
}

impl OutputValue for bool {
    fn write_json(&self, out: &mut String) {
        out.push_str(if *self { "true" } else { "false" });
    }
}

macro_rules! integer_output {
    ($($t:ty),*) => {$(
        impl OutputValue for $t {
            fn write_json(&self, out: &mut String) {
                let _ = write!(out, "{self}");
            }
        }
    )*};
}

integer_output!(i8, i16, i32, i64, u8, u16, u32, u64);

impl OutputValue for f64 {
    /// JSON has no infinity or NaN: they are written `null`.
    fn write_json(&self, out: &mut String) {
        if self.is_finite() {
            let _ = write!(out, "{self}");
        } else {
            out.push_str("null");
        }
    }
}

impl<T: OutputValue> OutputValue for Option<T> {
    fn write_json(&self, out: &mut String) {
        match self {
            Some(value) => value.write_json(out),
            None => out.push_str("null"),
        }
    }
}

impl<T: OutputValue> OutputValue for Vec<T> {
    fn write_json(&self, out: &mut String) {
        out.push('[');
        for (i, item) in self.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            item.write_json(out);
        }
        out.push(']');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Line {
        sku: String,
        quantity: u32,
    }

    impl Output for Line {
        fn write_json(&self, out: &mut String) {
            out.push('{');
            write_field(out, true, "sku", &self.sku);
            write_field(out, false, "quantity", &self.quantity);
            out.push('}');
        }
    }

    #[test]
    fn values_are_written_as_json() {
        let mut out = String::new();
        let lines = vec![Line {
            sku: "a\"b".into(),
            quantity: 2,
        }];
        out.push('{');
        write_field(&mut out, true, "lines", &lines);
        write_field(&mut out, false, "note", &None::<String>);
        write_field(&mut out, false, "ratio", &f64::NAN);
        write_field(&mut out, false, "paid", &true);
        out.push('}');
        assert_eq!(
            out,
            r#"{"lines":[{"sku":"a\"b","quantity":2}],"note":null,"ratio":null,"paid":true}"#
        );
    }
}
