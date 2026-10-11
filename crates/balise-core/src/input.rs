//! What the `Input` derive builds on: reading fields from a JSON object or a query string,
//! and the validation rules.
//!
//! The generated code stays small: for each field, one call to [`Fields::read`] (or
//! [`Fields::read_or`]) and one call per rule. Everything else lives here, compiled once.

use std::fmt::Display;

use crate::extract::{FieldError, FieldErrors};
use crate::json::{Number, Value};

/// A struct that derives `Input`: built from a JSON body or a query string, with its rules.
pub trait Input: Sized {
    fn from_fields(fields: Fields) -> Result<Self, FieldErrors>;
}

/// The fields received, before they are read into an `Input`.
pub struct Fields {
    entries: Vec<(String, Raw)>,
    errors: FieldErrors,
}

/// One value received: JSON, or text from a query string.
#[doc(hidden)]
pub enum Raw {
    Json(Value),
    Text(String),
}

impl Fields {
    /// The fields of a JSON object; any other JSON value is described in the error.
    pub fn from_json(value: Value) -> Result<Fields, &'static str> {
        match value {
            Value::Object(entries) => Ok(Fields {
                entries: entries
                    .into_iter()
                    .map(|(name, value)| (name, Raw::Json(value)))
                    .collect(),
                errors: Vec::new(),
            }),
            other => Err(other.kind()),
        }
    }

    /// The fields of a query string. A repeated name keeps its first value.
    pub fn from_query(pairs: Vec<(String, String)>) -> Fields {
        let mut entries: Vec<(String, Raw)> = Vec::with_capacity(pairs.len());
        for (name, value) in pairs {
            if !entries.iter().any(|(known, _)| *known == name) {
                entries.push((name, Raw::Text(value)));
            }
        }
        Fields {
            entries,
            errors: Vec::new(),
        }
    }

    /// Reads a field. `None` means it did not pass, and its error is recorded.
    pub fn read<T: InputField>(&mut self, name: &str) -> Option<T> {
        let raw = self.take(name);
        match T::read(raw) {
            Ok(value) => Some(value),
            Err(error) => {
                self.errors.push(error.into_field_error(name));
                None
            }
        }
    }

    /// Reads a field with a default: absent or `null` gives `default()`.
    pub fn read_or<T: InputField>(&mut self, name: &str, default: impl FnOnce() -> T) -> Option<T> {
        match self.take(name) {
            None | Some(Raw::Json(Value::Null)) => Some(default()),
            raw => match T::read(raw) {
                Ok(value) => Some(value),
                Err(error) => {
                    self.errors.push(error.into_field_error(name));
                    None
                }
            },
        }
    }

    /// Records the result of a rule: `Err` holds what the rule requires.
    pub fn check(&mut self, name: &str, result: Result<(), String>) {
        if let Err(rule) = result
            && !self.errors.iter().any(|error| error.field == name)
        {
            self.errors.push(FieldError::rule(name, &rule));
        }
    }

    /// Ends reading: what remains was not declared, and is an error unless the `Input`
    /// accepts unknown fields. `known` lists the declared names, for suggestions.
    pub fn finish(mut self, known: &[&str], allow_unknown: bool) -> FieldErrors {
        if !allow_unknown {
            for (name, _) in std::mem::take(&mut self.entries) {
                self.errors
                    .push(FieldError::unknown(&name, closest(&name, known)));
            }
        }
        self.errors
    }

    fn take(&mut self, name: &str) -> Option<Raw> {
        let index = self.entries.iter().position(|(known, _)| known == name)?;
        Some(self.entries.remove(index).1)
    }
}

/// The declared name closest to a typo, if one is close enough: `quantiy` gives `quantity`.
fn closest<'a>(name: &str, known: &[&'a str]) -> Option<&'a str> {
    known
        .iter()
        .map(|candidate| (distance(name, candidate), *candidate))
        .filter(|(distance, candidate)| *distance <= (candidate.len() / 3).max(1))
        .min_by_key(|(distance, _)| *distance)
        .map(|(_, candidate)| candidate)
}

/// The number of single-character edits from `a` to `b`.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut current = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let substitution = previous[j] + usize::from(ca != *cb);
            current.push(substitution.min(previous[j + 1] + 1).min(current[j] + 1));
        }
        previous = current;
    }
    previous[b.len()]
}

/// Why one field did not pass.
pub enum Problem {
    Missing,
    /// The received value, as messages show it: `a string`, `` `abc` ``.
    WrongType {
        expected: String,
        found: String,
    },
}

impl Problem {
    fn into_field_error(self, name: &str) -> FieldError {
        match self {
            Problem::Missing => FieldError::missing(name),
            Problem::WrongType { expected, found } => {
                FieldError::wrong_type(name, &expected, &found)
            }
        }
    }
}

/// A value an `Input` field can hold: text, numbers, booleans, lists.
pub trait InputValue: Sized {
    /// What the value must be, as messages say it: `a whole number`.
    fn expected() -> String;
    /// Reads a JSON value other than `null`; `None` if it has another type.
    fn from_json(value: Value) -> Option<Self>;
    /// Reads a query string value; `None` if it is not readable as this type.
    fn from_text(text: &str) -> Option<Self>;

    /// Like `from_json`, with what was received when it does not fit: `a string`.
    fn read_json(value: Value) -> Result<Self, String> {
        let found = describe(&value);
        Self::from_json(value).ok_or(found)
    }
}

/// A received value as messages show it: numbers as written, other values by kind.
fn describe(value: &Value) -> String {
    match value {
        Value::Number(number) => format!("`{}`", number_text(number)),
        other => other.kind().to_string(),
    }
}

/// How a field reads what was received, absent and `null` included: `T` requires a value,
/// `Option<T>` accepts none, `Patch<T>` tells absent from `null`.
#[diagnostic::on_unimplemented(
    message = "CMP0010: `{Self}` cannot be a field of an `Input`",
    label = "Balise cannot read this type",
    note = "fix: Use text, numbers, booleans and lists of them; `Option<T>` and `Patch<T>` in an `Input`; another `Output` in an `Output`.",
    note = "more: balise explain CMP0010"
)]
pub trait InputField: Sized {
    /// The value rules apply to.
    type Inner;
    #[doc(hidden)]
    fn read(raw: Option<Raw>) -> Result<Self, Problem>;
    /// The value to check, when there is one.
    fn inner(&self) -> Option<&Self::Inner>;
}

impl<T: InputValue> InputField for T {
    type Inner = T;

    fn read(raw: Option<Raw>) -> Result<T, Problem> {
        match raw {
            None | Some(Raw::Json(Value::Null)) => Err(Problem::Missing),
            Some(raw) => read_value(raw),
        }
    }

    fn inner(&self) -> Option<&T> {
        Some(self)
    }
}

impl<T: InputValue> InputField for Option<T> {
    type Inner = T;

    fn read(raw: Option<Raw>) -> Result<Option<T>, Problem> {
        match raw {
            None | Some(Raw::Json(Value::Null)) => Ok(None),
            Some(raw) => read_value(raw).map(Some),
        }
    }

    fn inner(&self) -> Option<&T> {
        self.as_ref()
    }
}

/// A field of a partial update: absent leaves it unchanged, a value sets it. On a nullable
/// field (`Patch<Option<T>>`), `null` sets it to `None`; elsewhere, `null` is an error.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Patch<T> {
    #[default]
    Unchanged,
    Set(T),
}

impl<T: InputField> InputField for Patch<T> {
    type Inner = T::Inner;

    fn read(raw: Option<Raw>) -> Result<Patch<T>, Problem> {
        match raw {
            None => Ok(Patch::Unchanged),
            Some(Raw::Json(Value::Null)) => match T::read(Some(Raw::Json(Value::Null))) {
                Ok(value) => Ok(Patch::Set(value)),
                Err(_) => Err(Problem::WrongType {
                    expected: "a value (this field cannot be cleared)".to_string(),
                    found: "null".to_string(),
                }),
            },
            raw => T::read(raw).map(Patch::Set),
        }
    }

    fn inner(&self) -> Option<&T::Inner> {
        match self {
            Patch::Unchanged => None,
            Patch::Set(value) => value.inner(),
        }
    }
}

fn read_value<T: InputValue>(raw: Raw) -> Result<T, Problem> {
    match raw {
        Raw::Json(value) => T::read_json(value).map_err(|found| Problem::WrongType {
            expected: T::expected(),
            found,
        }),
        Raw::Text(text) => T::from_text(&text).ok_or_else(|| Problem::WrongType {
            expected: T::expected(),
            found: format!("`{text}`"),
        }),
    }
}

fn number_text(number: &Number) -> String {
    match number {
        Number::Integer(n) => n.to_string(),
        Number::Unsigned(n) => n.to_string(),
        Number::Float(n) => n.to_string(),
    }
}

impl InputValue for String {
    fn expected() -> String {
        "text".to_string()
    }

    fn from_json(value: Value) -> Option<String> {
        match value {
            Value::String(text) => Some(text),
            _ => None,
        }
    }

    fn from_text(text: &str) -> Option<String> {
        Some(text.to_string())
    }
}

impl InputValue for bool {
    fn expected() -> String {
        "`true` or `false`".to_string()
    }

    fn from_json(value: Value) -> Option<bool> {
        match value {
            Value::Bool(b) => Some(b),
            _ => None,
        }
    }

    fn from_text(text: &str) -> Option<bool> {
        text.parse().ok()
    }
}

macro_rules! integer_value {
    ($($t:ty => $expected:literal),* $(,)?) => {$(
        impl InputValue for $t {
            fn expected() -> String {
                $expected.to_string()
            }

            fn from_json(value: Value) -> Option<$t> {
                match value {
                    Value::Number(Number::Integer(n)) => n.try_into().ok(),
                    Value::Number(Number::Unsigned(n)) => n.try_into().ok(),
                    _ => None,
                }
            }

            fn from_text(text: &str) -> Option<$t> {
                text.parse().ok()
            }
        }
    )*};
}

integer_value! {
    i8 => "a whole number from -128 to 127",
    i16 => "a whole number from -32768 to 32767",
    i32 => "a whole number",
    i64 => "a whole number",
    u8 => "a whole number from 0 to 255",
    u16 => "a whole number from 0 to 65535",
    u32 => "a whole number, 0 or more",
    u64 => "a whole number, 0 or more",
}

impl InputValue for f64 {
    fn expected() -> String {
        "a number".to_string()
    }

    fn from_json(value: Value) -> Option<f64> {
        match value {
            Value::Number(Number::Integer(n)) => Some(n as f64),
            Value::Number(Number::Unsigned(n)) => Some(n as f64),
            Value::Number(Number::Float(n)) => Some(n),
            _ => None,
        }
    }

    fn from_text(text: &str) -> Option<f64> {
        text.parse().ok().filter(|n: &f64| n.is_finite())
    }
}

impl<T: InputValue> InputValue for Vec<T> {
    fn expected() -> String {
        format!("a list where each item is {}", T::expected())
    }

    fn from_json(value: Value) -> Option<Vec<T>> {
        Self::read_json(value).ok()
    }

    /// One wrong item makes the whole list wrong; the message names it, counting from 1.
    fn read_json(value: Value) -> Result<Vec<T>, String> {
        let Value::Array(items) = value else {
            return Err(describe(&value));
        };
        items
            .into_iter()
            .enumerate()
            .map(|(i, item)| {
                T::read_json(item)
                    .map_err(|found| format!("a list where item {} is {found}", i + 1))
            })
            .collect()
    }

    /// `?tag=a,b`: comma-separated items.
    fn from_text(text: &str) -> Option<Vec<T>> {
        if text.is_empty() {
            return Some(Vec::new());
        }
        text.split(',').map(T::from_text).collect()
    }
}

/// The validation rules, called by the code the derive generates. Each returns what the
/// value must satisfy when it does not.
pub mod rules {
    use super::*;

    /// A type `min` and `max` apply to.
    #[diagnostic::on_unimplemented(
        message = "CMP0009: `min` and `max` do not apply to `{Self}`",
        label = "not a number",
        note = "fix: Use `min` and `max` on numbers, and `length(min = …, max = …)` on text and lists.",
        note = "more: balise explain CMP0009"
    )]
    pub trait Numeric: PartialOrd + Display {}

    impl Numeric for i8 {}
    impl Numeric for i16 {}
    impl Numeric for i32 {}
    impl Numeric for i64 {}
    impl Numeric for u8 {}
    impl Numeric for u16 {}
    impl Numeric for u32 {}
    impl Numeric for u64 {}
    impl Numeric for f64 {}

    /// A type `length` applies to.
    #[diagnostic::on_unimplemented(
        message = "CMP0009: `length` does not apply to `{Self}`",
        label = "not text or a list",
        note = "fix: Use `length` on `String` and `Vec` fields, and `min` / `max` on numbers.",
        note = "more: balise explain CMP0009"
    )]
    pub trait Measured {
        /// Characters for text, items for a list.
        fn measure(&self) -> (usize, &'static str);
    }

    impl Measured for String {
        fn measure(&self) -> (usize, &'static str) {
            (self.chars().count(), "characters")
        }
    }

    impl<T> Measured for Vec<T> {
        fn measure(&self) -> (usize, &'static str) {
            (self.len(), "items")
        }
    }

    /// A type `email`, `url` and `one_of` apply to.
    #[diagnostic::on_unimplemented(
        message = "CMP0009: `email`, `url` and `one_of` do not apply to `{Self}`",
        label = "not text",
        note = "fix: Use these rules on `String` fields; for a fixed set of values, an enum is clearer.",
        note = "more: balise explain CMP0009"
    )]
    pub trait Text {
        fn text(&self) -> &str;
    }

    impl Text for String {
        fn text(&self) -> &str {
            self
        }
    }

    pub fn min<T: Numeric>(value: &T, min: T) -> Result<(), String> {
        if *value >= min {
            Ok(())
        } else {
            Err(format!("must be at least {min}"))
        }
    }

    pub fn max<T: Numeric>(value: &T, max: T) -> Result<(), String> {
        if *value <= max {
            Ok(())
        } else {
            Err(format!("must be at most {max}"))
        }
    }

    pub fn length<T: Measured>(
        value: &T,
        min: Option<usize>,
        max: Option<usize>,
    ) -> Result<(), String> {
        let (length, unit) = value.measure();
        let fits = min.is_none_or(|min| length >= min) && max.is_none_or(|max| length <= max);
        if fits {
            return Ok(());
        }
        Err(match (min, max) {
            (Some(min), Some(max)) => format!("must have from {min} to {max} {unit}"),
            (Some(min), None) => format!("must have at least {min} {unit}"),
            (None, Some(max)) => format!("must have at most {max} {unit}"),
            (None, None) => unreachable!("a length without bounds always fits"),
        })
    }

    /// One `@` with text on both sides, and a dot in the domain. Whether the address exists
    /// is only known by sending to it.
    pub fn email<T: Text>(value: &T) -> Result<(), String> {
        let text = value.text();
        let valid = match text.split_once('@') {
            Some((local, domain)) => {
                !local.is_empty()
                    && !domain.contains('@')
                    && domain.contains('.')
                    && !domain.starts_with('.')
                    && !domain.ends_with('.')
                    && !text.contains(char::is_whitespace)
            }
            None => false,
        };
        if valid {
            Ok(())
        } else {
            Err("must be an email address".to_string())
        }
    }

    /// An `http` or `https` URL with a host.
    pub fn url<T: Text>(value: &T) -> Result<(), String> {
        let text = value.text();
        let rest = text
            .strip_prefix("https://")
            .or_else(|| text.strip_prefix("http://"));
        let valid = rest.is_some_and(|rest| {
            let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
            !host.is_empty() && !text.contains(char::is_whitespace)
        });
        if valid {
            Ok(())
        } else {
            Err("must be an http or https URL".to_string())
        }
    }

    pub fn one_of<T: Text>(value: &T, allowed: &[&str]) -> Result<(), String> {
        if allowed.contains(&value.text()) {
            Ok(())
        } else {
            let list: Vec<String> = allowed.iter().map(|a| format!("`{a}`")).collect();
            Err(format!("must be one of {}", list.join(", ")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::rules::*;
    use super::*;

    fn json(text: &str) -> Fields {
        Fields::from_json(crate::json::parse(text.as_bytes()).unwrap()).unwrap()
    }

    #[test]
    fn required_optional_and_patch_fields_read_absent_and_null_differently() {
        let mut fields = json(r#"{"a": null, "b": null, "c": null}"#);
        assert_eq!(fields.read::<u32>("a"), None);
        assert_eq!(fields.read::<Option<u32>>("b"), Some(None));
        assert_eq!(
            fields.read::<Patch<Option<u32>>>("c"),
            Some(Patch::Set(None))
        );
        assert_eq!(fields.read::<Patch<u32>>("d"), Some(Patch::Unchanged));
        let errors = fields.finish(&[], false);
        assert_eq!(errors, [FieldError::missing("a")]);
    }

    #[test]
    fn null_on_a_patch_that_cannot_be_cleared_is_a_wrong_type() {
        let mut fields = json(r#"{"quantity": null}"#);
        assert_eq!(fields.read::<Patch<u32>>("quantity"), None);
        assert_eq!(fields.finish(&[], false)[0].code.code(), "VAL0005");
    }

    #[test]
    fn wrong_types_name_what_was_expected_and_received() {
        let mut fields = json(r#"{"a": "2", "b": 300, "c": [1, "x"]}"#);
        fields.read::<u32>("a");
        fields.read::<u8>("b");
        fields.read::<Vec<u32>>("c");
        let messages: Vec<String> = fields
            .finish(&[], false)
            .into_iter()
            .map(|e| e.message)
            .collect();
        assert_eq!(
            messages,
            [
                "must be a whole number, 0 or more, not a string",
                "must be a whole number from 0 to 255, not `300`",
                "must be a list where each item is a whole number, 0 or more, not a list where item 2 is a string",
            ]
        );
    }

    #[test]
    fn query_values_are_read_from_text() {
        let pairs = vec![
            ("page".to_string(), "2".to_string()),
            ("tags".to_string(), "a,b".to_string()),
            ("page".to_string(), "3".to_string()),
        ];
        let mut fields = Fields::from_query(pairs);
        assert_eq!(fields.read::<u32>("page"), Some(2));
        assert_eq!(
            fields.read::<Vec<String>>("tags"),
            Some(vec!["a".into(), "b".into()])
        );
        assert_eq!(fields.read_or::<bool>("archived", || false), Some(false));
        assert!(fields.finish(&[], false).is_empty());
    }

    #[test]
    fn unknown_fields_are_rejected_with_the_closest_name() {
        let fields = json(r#"{"quantiy": 2, "zzz": 1}"#);
        let errors = fields.finish(&["quantity", "note"], false);
        let messages: Vec<(&str, &str)> = errors
            .iter()
            .map(|e| (e.field.as_str(), e.message.as_str()))
            .collect();
        assert_eq!(
            messages,
            [
                (
                    "quantiy",
                    "is not a field of this input; did you mean `quantity`?"
                ),
                ("zzz", "is not a field of this input"),
            ]
        );
        assert!(json(r#"{"zzz": 1}"#).finish(&[], true).is_empty());
    }

    #[test]
    fn rules_say_what_the_value_must_satisfy() {
        assert_eq!(min(&0u32, 1), Err("must be at least 1".into()));
        assert_eq!(max(&5u32, 10), Ok(()));
        assert_eq!(
            length(&"short".to_string(), Some(12), Some(128)),
            Err("must have from 12 to 128 characters".into())
        );
        assert_eq!(length(&vec![1], None, Some(1)), Ok(()));
        assert_eq!(email(&"a@b.co".to_string()), Ok(()));
        for bad in ["a@b", "@b.co", "a@@b.co", "a b@c.co", "a@.co"] {
            assert!(email(&bad.to_string()).is_err(), "{bad}");
        }
        assert_eq!(url(&"https://balise.rs/docs".to_string()), Ok(()));
        assert!(url(&"ftp://x".to_string()).is_err());
        assert_eq!(
            one_of(&"red".to_string(), &["green", "blue"]),
            Err("must be one of `green`, `blue`".into())
        );
    }

    #[test]
    fn a_rule_is_reported_once_per_field_after_its_type() {
        let mut fields = json(r#"{"quantity": 0}"#);
        let quantity = fields.read::<u32>("quantity");
        if let Some(value) = quantity.as_ref().and_then(InputField::inner) {
            fields.check("quantity", min(value, 1));
            fields.check("quantity", max(value, 100));
        }
        let errors = fields.finish(&["quantity"], false);
        assert_eq!(errors, [FieldError::rule("quantity", "must be at least 1")]);
    }
}
