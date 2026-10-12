//! `Error`: every error a request can end with, and the single error body.
//!
//! An error carries its status, code, message, fix, where it was raised in application code,
//! and its cause. How much of it reaches the client depends on the environment: everything in
//! dev; in prod, a `4xx` keeps its code and message, and a `5xx` becomes a generic `HTTP0500`.

use std::fmt;
use std::panic::Location;

use crate::error_codes::{EXT0001, ErrorCode, HTTP0500};
use crate::event::{Event, Level};
use crate::extract::FieldError;
use crate::handler::{Request, Response};
use crate::json::{self, Value};

/// What a handler returns when it can fail: `Result<OrderView>`.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// An error with a status and a code: from Balise, from an application's `AppError`, or any
/// other error converted by `?`.
///
/// It does not implement `std::error::Error`, so that `?` converts every error into it. Boxed,
/// so that a `Result` stays as small as its value.
pub struct Error(Box<Details>);

struct Details {
    status: u16,
    code: &'static ErrorCode,
    message: String,
    fix: String,
    fields: Vec<FieldError>,
    /// Where it was raised in application code: file and line.
    location: Option<(&'static str, u32)>,
    detail: Option<String>,
    cause: Option<String>,
}

impl Error {
    /// The HTTP status: `409`.
    pub fn status(&self) -> u16 {
        self.0.status
    }

    /// The code: `ORD0001`.
    pub fn code(&self) -> &'static str {
        self.0.code.code()
    }

    pub fn message(&self) -> &str {
        &self.0.message
    }

    /// An error for a registered code, its message and fix filled with `args`.
    #[doc(hidden)]
    pub fn coded(status: u16, code: &'static ErrorCode, args: &[(&str, &str)]) -> Error {
        Error(Box::new(Details {
            status,
            code,
            message: code.format_message(args),
            fix: code.format_fix(args),
            fields: Vec::new(),
            location: None,
            detail: None,
            cause: None,
        }))
    }

    /// An application error, built by the code `AppError` generates.
    #[doc(hidden)]
    #[track_caller]
    pub fn app(status: u16, code: &'static ErrorCode, message: String, fix: String) -> Error {
        Error(Box::new(Details {
            status,
            code,
            message,
            fix,
            fields: Vec::new(),
            location: Some(caller()),
            detail: None,
            cause: None,
        }))
    }

    #[doc(hidden)]
    pub fn with_fields(mut self, fields: Vec<FieldError>) -> Error {
        self.0.fields = fields;
        self
    }

    /// Sets where the error was raised, for tests that need a fixed location.
    #[doc(hidden)]
    pub fn at(mut self, file: &'static str, line: u32) -> Error {
        self.0.location = Some((file, line));
        self
    }

    #[doc(hidden)]
    pub fn with_cause(mut self, cause: impl ToString) -> Error {
        self.0.cause = Some(cause.to_string());
        self
    }

    /// The response to a request. The error travels with it, outside the body, so that the
    /// engine logs it with the request's summary.
    pub fn respond(self, request: &Request) -> Response {
        self.into_response(&request.request_id, request.error_details)
    }

    /// Like [`respond`](Self::respond), outside a handler: for the router's own errors.
    pub fn into_response(self, request_id: &str, details: bool) -> Response {
        let mut response = self.render(request_id, details);
        response.error = Some(self);
        response
    }

    /// What the logs may say beyond the response: `No Order with id … for usr_…: it belongs
    /// to another user`. The response keeps the message.
    #[doc(hidden)]
    pub fn with_detail(mut self, detail: impl Into<String>) -> Error {
        self.0.detail = Some(detail.into());
        self
    }

    /// The response: `{"error": {"code", "message", "fields"?, "location"?, "fix"?, "cause"?,
    /// "request_id"}}`. `details` adds location, fix and cause, and keeps a `5xx` as it is;
    /// without it, a `5xx` is a generic `HTTP0500`.
    pub fn render(&self, request_id: &str, details: bool) -> Response {
        let hidden = self.0.status >= 500 && !details;
        let (code, message) = match hidden {
            true => (HTTP0500, HTTP0500.message()),
            false => (self.0.code, self.0.message.as_str()),
        };
        let mut body = String::from(r#"{"error":{"code":"#);
        json::write_string(&mut body, code.code());
        body.push_str(r#","message":"#);
        json::write_string(&mut body, message);
        if !self.0.fields.is_empty() && !hidden {
            body.push_str(r#","fields":{"#);
            for (i, field) in self.0.fields.iter().enumerate() {
                if i > 0 {
                    body.push(',');
                }
                json::write_string(&mut body, &field.field);
                body.push_str(r#":{"code":"#);
                json::write_string(&mut body, field.code.code());
                body.push_str(r#","message":"#);
                json::write_string(&mut body, &field.message);
                body.push('}');
            }
            body.push('}');
        }
        if details {
            if let Some(location) = self.0.location {
                body.push_str(r#","location":"#);
                json::write_string(&mut body, &format!("{}:{}", location.0, location.1));
            }
            body.push_str(r#","fix":"#);
            json::write_string(&mut body, &self.0.fix);
            if let Some(cause) = &self.0.cause {
                body.push_str(r#","cause":"#);
                json::write_string(&mut body, cause);
            }
        }
        body.push_str(r#","request_id":"#);
        json::write_string(&mut body, request_id);
        body.push_str("}}");
        Response::json(if hidden { 500 } else { self.0.status }, body)
    }

    /// The event logged for a `5xx`, before the request's summary.
    pub fn event(&self) -> Event {
        Event {
            location: self.location_text(),
            fix: Some(self.0.fix.clone()),
            cause: self.0.cause.clone(),
            detail: self.0.detail.clone(),
            ..Event::new(Level::Error, &self.0.message).with_code(self.0.code)
        }
    }

    /// Adds the error to its request's summary: its code always; for a `4xx`, which has no
    /// event of its own, also what happened, where, and the fix.
    pub fn summarize(&self, summary: &mut Event) {
        summary.code = Some(self.0.code);
        if self.0.status < 500 {
            summary.detail = Some(self.0.detail.clone().unwrap_or(self.0.message.clone()));
            summary.location = self.location_text();
            summary.fix = Some(self.0.fix.clone());
            if !self.0.fields.is_empty() {
                let fields = self
                    .0
                    .fields
                    .iter()
                    .map(|field| {
                        let entry = Value::Object(vec![
                            (
                                "code".to_string(),
                                Value::String(field.code.code().to_string()),
                            ),
                            ("message".to_string(), Value::String(field.message.clone())),
                        ]);
                        (field.field.clone(), entry)
                    })
                    .collect();
                summary
                    .fields
                    .push(("fields".to_string(), Value::Object(fields)));
            }
        }
    }

    fn location_text(&self) -> Option<String> {
        self.0.location.map(|(file, line)| format!("{file}:{line}"))
    }
}

#[track_caller]
fn caller() -> (&'static str, u32) {
    let location = Location::caller();
    (location.file(), location.line())
}

/// Any other error becomes a `500` (`EXT0001`) that keeps it as its cause, raised where `?`
/// converted it.
impl<E: std::error::Error + Send + Sync + 'static> From<E> for Error {
    #[track_caller]
    fn from(error: E) -> Error {
        let cause = error.to_string();
        let mut error = Error::coded(500, EXT0001, &[("cause", &cause)]).with_cause(cause);
        error.0.location = Some(caller());
        error
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {}: {}",
            self.0.status,
            self.0.code.code(),
            self.0.message
        )?;
        if let Some(location) = self.0.location {
            write!(f, " (at {}:{})", location.0, location.1)?;
        }
        if let Some(cause) = &self.0.cause {
            write!(f, ", caused by: {cause}")?;
        }
        Ok(())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.0.code.code(), self.0.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error_codes::HTTP0003;

    fn body(response: Response) -> String {
        String::from_utf8(response.body).unwrap()
    }

    fn failing() -> Result<u32> {
        let n: u32 = "x".parse()?;
        Ok(n)
    }

    #[test]
    fn question_mark_keeps_the_cause_and_where_it_was_raised() {
        let error = failing().unwrap_err();
        assert_eq!((error.status(), error.code()), (500, "EXT0001"));
        let line = line!() - 8;
        assert_eq!(
            format!("{error:?}"),
            format!(
                "500 EXT0001: invalid digit found in string (at crates/balise-core/src/error.rs:{line}), \
                 caused by: invalid digit found in string"
            )
        );
    }

    #[test]
    fn a_5xx_is_generic_without_details() {
        let error = failing().unwrap_err();
        assert_eq!(
            body(error.render("req_1", false)),
            r#"{"error":{"code":"HTTP0500","message":"Internal error","request_id":"req_1"}}"#
        );
        assert!(body(error.render("req_1", true)).contains(r#""cause":"invalid digit"#));
    }

    #[test]
    fn a_4xx_keeps_its_code_and_message_and_adds_the_fix_in_dev() {
        let error = Error::coded(404, HTTP0003, &[("method", "GET"), ("path", "/api/x")]);
        assert_eq!(
            body(error.render("req_1", false)),
            r#"{"error":{"code":"HTTP0003","message":"No route for GET /api/x","request_id":"req_1"}}"#
        );
        assert!(body(error.render("req_1", true)).contains(r#","fix":"#));
    }
}
