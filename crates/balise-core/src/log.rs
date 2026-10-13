//! Where events go: one logger per process, installed at startup from the configuration.
//!
//! Application code logs through `balise::log` (`info!`, `warn!`…), which ends here with the
//! request's ID and handler when it runs inside a request.

use std::io::{IsTerminal, Write as _};
use std::sync::{Arc, Mutex, OnceLock};

use crate::event::{Event, Level};
use crate::json::{Number, Value};

/// How events are written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// For a person at a terminal: one sentence per request, details indented below.
    Text,
    /// One JSON object per line, for log collectors.
    Json,
}

/// Where rendered events go.
#[derive(Debug, Clone)]
pub enum Sink {
    Stdout,
    /// Kept in memory, for tests that read what was logged.
    Memory(Arc<Mutex<Vec<String>>>),
}

/// The process's logger.
#[derive(Debug, Clone)]
pub struct Logger {
    pub format: Format,
    /// Events below this level are dropped.
    pub min_level: Level,
    /// ANSI colors, for text on a terminal only.
    pub color: bool,
    pub sink: Sink,
}

impl Logger {
    /// Text or JSON on stdout. Colors only on a terminal, and never with `NO_COLOR`.
    pub fn stdout(format: Format, min_level: Level) -> Logger {
        let color = format == Format::Text
            && std::io::stdout().is_terminal()
            && std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty());
        Logger {
            format,
            min_level,
            color,
            sink: Sink::Stdout,
        }
    }

    fn write(&self, event: &Event) {
        if event.level < self.min_level {
            return;
        }
        let line = match self.format {
            Format::Text => event.render_text(self.color),
            Format::Json => event.render_json(),
        };
        match &self.sink {
            Sink::Stdout => {
                let _ = writeln!(std::io::stdout().lock(), "{line}");
            }
            Sink::Memory(lines) => lines.lock().unwrap_or_else(|e| e.into_inner()).push(line),
        }
    }
}

static LOGGER: OnceLock<Logger> = OnceLock::new();

/// Installs the process's logger. The first call wins; it returns whether this one did.
pub fn install(logger: Logger) -> bool {
    LOGGER.set(logger).is_ok()
}

/// Writes an event. Before a logger is installed (a failure while starting), text on stdout.
pub(crate) fn write(event: Event) {
    #[cfg(feature = "dev")]
    crate::journal::event(&event);
    match LOGGER.get() {
        Some(logger) => logger.write(&event),
        None => Logger::stdout(Format::Text, Level::Info).write(&event),
    }
}

/// The request the current code runs for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestContext {
    pub request_id: String,
    pub handler: String,
}

static CONTEXT: OnceLock<fn() -> Option<RequestContext>> = OnceLock::new();

/// Sets how to find the current request. The engine keeps it per task; the core does not
/// know how.
pub fn set_context(current: fn() -> Option<RequestContext>) {
    let _ = CONTEXT.set(current);
}

fn context() -> Option<RequestContext> {
    CONTEXT.get().and_then(|current| current())
}

/// Whether a field's value is never written: its name contains `password`, `token` or
/// `secret`.
pub fn is_secret(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    ["password", "token", "secret"]
        .iter()
        .any(|word| name.contains(word))
}

/// The text written instead of a secret value.
pub const REDACTED: &str = "***";

/// A log event from application code, built by `balise::log`'s macros.
#[doc(hidden)]
pub fn app(
    level: Level,
    file: &'static str,
    line: u32,
    message: String,
    fields: Vec<(&'static str, Value)>,
) {
    let mut event = Event::new(level, message);
    event.location = Some(format!("{file}:{line}"));
    if let Some(context) = context() {
        event.request_id = Some(context.request_id);
        event.handler = Some(context.handler);
    }
    event.fields = fields
        .into_iter()
        .map(|(name, value)| match is_secret(name) {
            true => (name.to_string(), Value::String(REDACTED.to_string())),
            false => (name.to_string(), value),
        })
        .collect();
    write(event);
}

/// A value a log field can hold: text, numbers, booleans, and `Option`s of them.
#[diagnostic::on_unimplemented(
    message = "CMP0012: `{Self}` cannot be the value of a log field",
    label = "not text, a number or a boolean",
    note = "fix: Log text, numbers or booleans: `order_id = id`, or `status = status.to_string()`.",
    note = "more: balise explain CMP0012"
)]
pub trait LogValue {
    fn to_log_value(&self) -> Value;
}

impl LogValue for str {
    fn to_log_value(&self) -> Value {
        Value::String(self.to_string())
    }
}

impl LogValue for String {
    fn to_log_value(&self) -> Value {
        Value::String(self.clone())
    }
}

impl LogValue for bool {
    fn to_log_value(&self) -> Value {
        Value::Bool(*self)
    }
}

macro_rules! number_value {
    ($variant:ident as $target:ty: $($t:ty),*) => {$(
        impl LogValue for $t {
            fn to_log_value(&self) -> Value {
                Value::Number(Number::$variant(<$target>::from(*self)))
            }
        }
    )*};
}

number_value!(Integer as i64: i8, i16, i32, i64);
number_value!(Unsigned as u64: u8, u16, u32, u64);
number_value!(Float as f64: f32, f64);

impl LogValue for usize {
    fn to_log_value(&self) -> Value {
        Value::Number(Number::Unsigned(*self as u64))
    }
}

impl<T: LogValue + ?Sized> LogValue for &T {
    fn to_log_value(&self) -> Value {
        (**self).to_log_value()
    }
}

impl<T: LogValue> LogValue for Option<T> {
    fn to_log_value(&self) -> Value {
        match self {
            Some(value) => value.to_log_value(),
            None => Value::Null,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_are_recognized_by_name() {
        for name in ["password", "reset_token", "client_secret", "Password"] {
            assert!(is_secret(name), "{name}");
        }
        assert!(!is_secret("email"));
    }
}
