//! Events: everything Balise reports, in one shape.
//!
//! This is the minimal model the engine needs to report its errors. The full model (request
//! fields, JSON rendering, redaction, the per-request summary) builds on it.

use std::fmt::Write as _;
use std::io::Write as _;

use crate::error_codes::ErrorCode;

/// How serious an event is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

impl Level {
    fn label(self) -> &'static str {
        match self {
            Level::Debug => "DEBUG",
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
        }
    }
}

/// Events below this level are not printed.
const MIN_LEVEL: Level = Level::Info;

/// One thing that happened, with a registered code when it is an error or a warning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub level: Level,
    pub code: Option<&'static ErrorCode>,
    pub message: String,
    /// The original error, kept in full.
    pub cause: Option<String>,
}

impl Event {
    /// An event for a registered code, its message filled with `args`.
    pub fn coded(level: Level, code: &'static ErrorCode, args: &[(&str, &str)]) -> Event {
        Event {
            level,
            code: Some(code),
            message: code.format_message(args),
            cause: None,
        }
    }

    pub fn with_cause(mut self, cause: impl ToString) -> Event {
        self.cause = Some(cause.to_string());
        self
    }

    /// The dev rendering: code, message, cause, fix, and where to learn more.
    pub fn render(&self) -> String {
        let mut text = format!("{:<5}", self.level.label());
        match self.code {
            Some(code) => {
                let _ = write!(text, " {}  {}", code.code(), self.message);
            }
            None => {
                let _ = write!(text, " {}", self.message);
            }
        }
        if let Some(cause) = &self.cause {
            let _ = write!(text, "\n      cause: {cause}");
        }
        if let Some(code) = self.code {
            let _ = write!(text, "\n      fix: {}", code.fix());
            let _ = write!(text, "\n      more: balise explain {}", code.code());
        }
        text
    }

    /// Prints the event on stderr, if its level is high enough.
    pub fn emit(&self) {
        if self.level >= MIN_LEVEL {
            let _ = writeln!(std::io::stderr(), "{}", self.render());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error_codes::HTTP0001;

    #[test]
    fn renders_code_message_cause_fix_and_more() {
        let event = Event::coded(Level::Error, HTTP0001, &[("address", "0.0.0.0:3000")])
            .with_cause("Address already in use (os error 98)");
        assert_eq!(
            event.render(),
            "ERROR HTTP0001  Cannot listen on 0.0.0.0:3000: the port is already in use\n      \
             cause: Address already in use (os error 98)\n      \
             fix: Stop the other process that listens on this port, or run the app on another port.\n      \
             more: balise explain HTTP0001"
        );
    }
}
