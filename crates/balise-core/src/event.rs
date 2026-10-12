//! Events: everything Balise reports, in one shape, and its two renderings.
//!
//! Both renderings are pure functions of the event: time and request ID come from it, so the
//! reference files of `mockup/outputs/` can be compared byte for byte. Writing events is the
//! job of [`log`](crate::log).

use std::fmt::Write as _;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error_codes::ErrorCode;
use crate::json::{self, Value};

/// How serious an event is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl Level {
    /// As JSON writes it: `error`.
    pub fn name(self) -> &'static str {
        match self {
            Level::Trace => "trace",
            Level::Debug => "debug",
            Level::Info => "info",
            Level::Warn => "warn",
            Level::Error => "error",
        }
    }

    /// As the terminal shows it; `info` shows nothing, so ordinary lines stay quiet.
    fn label(self) -> &'static str {
        match self {
            Level::Trace => "TRACE",
            Level::Debug => "DEBUG",
            Level::Info => "",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
        }
    }
}

/// One thing that happened, with a registered code when it is an error or a warning.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    /// Milliseconds since the Unix epoch, UTC.
    pub time_ms: u64,
    pub level: Level,
    pub code: Option<&'static ErrorCode>,
    pub message: String,
    /// Set on the summary of a request, the one line each request produces.
    pub request: Option<RequestLine>,
    /// What the logs may say beyond the response: `… it belongs to another user`.
    pub detail: Option<String>,
    /// The handler, as logs name it: `orders::create`.
    pub handler: Option<String>,
    /// Where in application code: `src/orders/mod.rs:31`.
    pub location: Option<String>,
    pub fix: Option<String>,
    /// The original error, kept in full.
    pub cause: Option<String>,
    /// Other structured fields.
    pub fields: Vec<(String, Value)>,
    pub request_id: Option<String>,
}

/// What a request's summary says about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestLine {
    pub method: String,
    /// The full path, API base included, without the query string.
    pub path: String,
    pub status: u16,
    /// Microseconds: most requests take less than a millisecond.
    pub duration_us: u64,
    /// Database queries, when the `orm` module counts them.
    pub queries: Option<u32>,
}

impl Event {
    /// An event at this level, now.
    pub fn new(level: Level, message: impl Into<String>) -> Event {
        Event {
            time_ms: now_ms(),
            level,
            code: None,
            message: message.into(),
            request: None,
            detail: None,
            handler: None,
            location: None,
            fix: None,
            cause: None,
            fields: Vec::new(),
            request_id: None,
        }
    }

    /// An event for a registered code, its message and fix filled with `args`.
    pub fn coded(level: Level, code: &'static ErrorCode, args: &[(&str, &str)]) -> Event {
        Event {
            code: Some(code),
            fix: Some(code.format_fix(args)),
            ..Event::new(level, code.format_message(args))
        }
    }

    /// The summary of a request: `GET /api/orders 200`.
    pub fn request(line: RequestLine, request_id: &str) -> Event {
        Event {
            request_id: Some(request_id.to_string()),
            ..Event::new(
                Level::Info,
                format!("{} {} {}", line.method, line.path, line.status),
            )
        }
        .with_request(line)
    }

    pub fn with_code(mut self, code: &'static ErrorCode) -> Event {
        self.code = Some(code);
        self
    }

    fn with_request(mut self, line: RequestLine) -> Event {
        self.request = Some(line);
        self
    }

    pub fn with_cause(mut self, cause: impl ToString) -> Event {
        self.cause = Some(cause.to_string());
        self
    }

    /// Writes the event where the application's logs go.
    pub fn emit(self) {
        crate::log::write(self);
    }

    /// The terminal rendering, with ANSI colors when `color` is set.
    pub fn render_text(&self, color: bool) -> String {
        let paint = |text: &str, style: &str| match color {
            true => format!("\x1b[{style}m{text}\x1b[0m"),
            false => text.to_string(),
        };
        let mut out = time_of_day(self.time_ms);
        let label = self.level.label();
        // Continuation lines start under the first word after the time.
        let indent = match label {
            "" => " ".repeat(out.len() + 2),
            _ => " ".repeat(out.len() + 1),
        };
        if label.is_empty() {
            out.push_str("  ");
        } else {
            let style = if self.level >= Level::Warn {
                if self.level == Level::Error {
                    RED
                } else {
                    YELLOW
                }
            } else {
                DIM
            };
            let _ = write!(out, " {} ", paint(&format!("{label:<5}"), style));
        }

        match &self.request {
            Some(line) => {
                let style = match line.status {
                    500.. => RED,
                    400.. => YELLOW,
                    _ => DIM,
                };
                out.push_str(&paint(&request_sentence(line, self), style));
                // A `4xx` carries its detail; a `5xx` was reported by its own event.
                if (400..500).contains(&line.status) && self.code.is_some() {
                    let detail = self.detail.as_deref().unwrap_or(&self.message);
                    self.write_details(&mut out, &indent, detail, false, &paint);
                }
            }
            None => {
                if let Some(code) = self.code {
                    let _ = write!(out, "{}  ", paint(code.code(), BOLD));
                }
                out.push_str(&self.message);
                self.write_continuation(&mut out, &indent, true, &paint);
            }
        }
        out
    }

    /// The lines under a `4xx` request line: code and detail, then where and how to fix it.
    fn write_details(
        &self,
        out: &mut String,
        indent: &str,
        detail: &str,
        with_request_id: bool,
        paint: &dyn Fn(&str, &str) -> String,
    ) {
        if let Some(code) = self.code {
            let _ = write!(out, "\n{indent}{}  {detail}", paint(code.code(), BOLD));
        }
        self.write_continuation(out, indent, with_request_id, paint);
    }

    /// `at …`, `cause:`, other fields, `fix:` and `more:`, each on its own line.
    fn write_continuation(
        &self,
        out: &mut String,
        indent: &str,
        with_request_id: bool,
        paint: &dyn Fn(&str, &str) -> String,
    ) {
        let mut at = Vec::new();
        if let Some(location) = &self.location {
            at.push(format!("at {location}"));
        }
        if let Some(handler) = &self.handler {
            at.push(format!("in {handler}"));
        }
        let mut at = at.join(" ");
        if with_request_id && let Some(request_id) = &self.request_id {
            if !at.is_empty() {
                at.push(',');
                at.push(' ');
            }
            let _ = write!(at, "request {request_id}");
        }
        if !at.is_empty() {
            let _ = write!(out, "\n{indent}{}", paint(&at, CYAN));
        }
        if let Some(cause) = &self.cause {
            let _ = write!(out, "\n{indent}cause: {cause}");
        }
        for (name, value) in &self.fields {
            match value {
                // One line per entry: `fields:`, then `  contact: VAL0006 must be an email`.
                Value::Object(entries) => {
                    let _ = write!(out, "\n{indent}{name}:");
                    for (key, value) in entries {
                        let _ = write!(out, "\n{indent}  {key}: {}", text_of(value));
                    }
                }
                value => {
                    let _ = write!(out, "\n{indent}{name}: {}", text_of(value));
                }
            }
        }
        if let Some(fix) = &self.fix {
            let _ = write!(out, "\n{indent}{} {fix}", paint("fix:", GREEN));
        }
        if let Some(code) = self.code {
            let more = format!("more: balise explain {}", code.code());
            let _ = write!(out, "\n{indent}{}", paint(&more, DIM));
        }
    }

    /// One JSON object, on one line.
    pub fn render_json(&self) -> String {
        let mut out = String::from("{");
        let mut first = true;
        let mut field = |out: &mut String, name: &str, write: &dyn Fn(&mut String)| {
            if !first {
                out.push(',');
            }
            first = false;
            json::write_string(out, name);
            out.push(':');
            write(out);
        };
        let text = |value: &str| {
            let value = value.to_string();
            move |out: &mut String| json::write_string(out, &value)
        };
        let number = |value: u64| move |out: &mut String| out.push_str(&value.to_string());

        field(&mut out, "time", &text(&rfc3339(self.time_ms)));
        field(&mut out, "level", &text(self.level.name()));
        match &self.request {
            // A summary: what was asked and what came back, then the error's code if any.
            Some(line) => {
                field(&mut out, "message", &text(&self.message));
                field(&mut out, "method", &text(&line.method));
                field(&mut out, "path", &text(&line.path));
                field(&mut out, "status", &number(u64::from(line.status)));
                if let Some(handler) = &self.handler {
                    field(&mut out, "handler", &text(handler));
                }
                let duration = milliseconds(line.duration_us);
                field(&mut out, "duration_ms", &move |out: &mut String| {
                    out.push_str(&duration)
                });
                if let Some(queries) = line.queries {
                    field(&mut out, "queries", &number(u64::from(queries)));
                }
                if let Some(code) = self.code {
                    field(&mut out, "code", &text(code.code()));
                }
                if (400..500).contains(&line.status) {
                    if let Some(detail) = &self.detail {
                        field(&mut out, "detail", &text(detail));
                    }
                    if let Some(location) = &self.location {
                        field(&mut out, "location", &text(location));
                    }
                }
            }
            None => {
                if let Some(code) = self.code {
                    field(&mut out, "code", &text(code.code()));
                }
                field(&mut out, "message", &text(&self.message));
                if let Some(handler) = &self.handler {
                    field(&mut out, "handler", &text(handler));
                }
                if let Some(location) = &self.location {
                    field(&mut out, "location", &text(location));
                }
                if let Some(fix) = &self.fix {
                    field(&mut out, "fix", &text(fix));
                }
                if let Some(cause) = &self.cause {
                    field(&mut out, "cause", &text(cause));
                }
            }
        }
        for (name, value) in &self.fields {
            let value = value.clone();
            field(&mut out, name, &move |out: &mut String| {
                json::write_value(out, &value)
            });
        }
        if let Some(request_id) = &self.request_id {
            field(&mut out, "request_id", &text(request_id));
        }
        out.push('}');
        out
    }
}

/// `GET /api/orders → request req_51e07a (orders::list returned 200 in 4 ms with 1 query)`.
fn request_sentence(line: &RequestLine, event: &Event) -> String {
    let request_id = event.request_id.as_deref().unwrap_or("-");
    let mut outcome = match &event.handler {
        Some(handler) => format!("{handler} returned {}", line.status),
        None => format!("no handler, {}", line.status),
    };
    let _ = write!(outcome, " in {}", readable_duration(line.duration_us));
    if let Some(queries) = line.queries {
        let noun = if queries == 1 { "query" } else { "queries" };
        let _ = write!(outcome, " with {queries} {noun}");
    }
    format!(
        "{} {} → request {request_id} ({outcome})",
        line.method, line.path
    )
}

/// Milliseconds for JSON, to the microsecond: `4`, `0.312`, `12.5`.
fn milliseconds(us: u64) -> String {
    let text = format!("{}.{:03}", us / 1000, us % 1000);
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// A duration for a person: `312 µs` below a millisecond, `4.6 ms` below ten, `52 ms` above.
fn readable_duration(us: u64) -> String {
    match us {
        0..1_000 => format!("{us} µs"),
        1_000..10_000 => {
            let tenths = (us + 50) / 100;
            match tenths % 10 {
                0 => format!("{} ms", tenths / 10),
                rest => format!("{}.{rest} ms", tenths / 10),
            }
        }
        _ => format!("{} ms", (us + 500) / 1000),
    }
}

/// A field's value in the terminal: text without quotes, an object's values side by side.
fn text_of(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Object(entries) => entries
            .iter()
            .map(|(_, value)| text_of(value))
            .collect::<Vec<_>>()
            .join(" "),
        other => {
            let mut out = String::new();
            json::write_value(&mut out, other);
            out
        }
    }
}

const RED: &str = "31";
const YELLOW: &str = "33";
const GREEN: &str = "32";
const CYAN: &str = "36";
const BOLD: &str = "1";
const DIM: &str = "2";

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// `12:02:11`, UTC.
fn time_of_day(time_ms: u64) -> String {
    let seconds = (time_ms / 1000) % 86_400;
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    )
}

/// `2026-10-11T12:02:11.478Z`.
fn rfc3339(time_ms: u64) -> String {
    let days = (time_ms / 86_400_000) as i64;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{}.{:03}Z",
        time_of_day(time_ms),
        time_ms % 1000
    )
}

/// The calendar date of a day counted from 1970-01-01 (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error_codes::HTTP0001;

    #[test]
    fn times_are_utc() {
        // 2026-10-11T12:02:11.478Z
        assert_eq!(rfc3339(1_791_720_131_478), "2026-10-11T12:02:11.478Z");
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(rfc3339(951_782_400_000), "2000-02-29T00:00:00.000Z");
        assert_eq!(time_of_day(1_791_720_131_478), "12:02:11");
    }

    #[test]
    fn durations_keep_sub_millisecond_precision() {
        assert_eq!(readable_duration(312), "312 µs");
        assert_eq!(readable_duration(4_000), "4 ms");
        assert_eq!(readable_duration(4_640), "4.6 ms");
        assert_eq!(readable_duration(5_004_200), "5004 ms");
        assert_eq!(milliseconds(4_000), "4");
        assert_eq!(milliseconds(312), "0.312");
        assert_eq!(milliseconds(12_500), "12.5");
    }

    #[test]
    fn renders_code_message_cause_fix_and_more() {
        let event = Event {
            time_ms: 1_791_720_131_478,
            ..Event::coded(Level::Error, HTTP0001, &[("address", "0.0.0.0:3000")])
                .with_cause("Address already in use (os error 98)")
        };
        assert_eq!(
            event.render_text(false),
            "12:02:11 ERROR HTTP0001  Cannot listen on 0.0.0.0:3000: the port is already in use\n         \
             cause: Address already in use (os error 98)\n         \
             fix: Stop the other process that listens on this port, or run the app on another port.\n         \
             more: balise explain HTTP0001"
        );
    }

    #[test]
    fn colors_carry_the_level_and_the_status() {
        let event = Event {
            time_ms: 0,
            ..Event::coded(Level::Error, HTTP0001, &[("address", "x")])
        };
        let text = event.render_text(true);
        assert!(text.contains("\x1b[31mERROR\x1b[0m"), "{text}");
        assert!(text.contains("\x1b[1mHTTP0001\x1b[0m"), "{text}");
        assert!(text.contains("\x1b[32mfix:\x1b[0m"), "{text}");
    }
}
