//! The registry of every error code Balise can raise.
//!
//! Each area has its own file, named after its prefix, where codes are declared
//! with [`codes!`]. A code is never reused or renumbered.
//!
//! To add an area: create its file, then add it to [`AREAS`].

/// Declares the codes of one area.
///
/// Each code becomes a constant of type [`ErrorCode`], and the area's list is
/// collected in `ALL` for [`AREAS`].
macro_rules! codes {
    ($(
        $code:ident {
            title: $title:literal,
            message: $message:literal,
            fix: $fix:literal,
            explanation: $explanation:literal $(,)?
        }
    )*) => {
        $(
            pub const $code: &crate::error_codes::ErrorCode = &crate::error_codes::ErrorCode {
                code: stringify!($code),
                title: $title,
                message: $message,
                fix: $fix,
                explanation: $explanation,
            };
        )*

        pub(super) const ALL: &[&crate::error_codes::ErrorCode] = &[$($code),*];
    };
}

mod cfg;
mod cmp;
mod ext;
mod http;
mod val;

pub use cfg::*;
pub use cmp::*;
pub use ext::*;
pub use http::*;
pub use val::*;

/// Every area, with its prefix and its codes.
pub const AREAS: &[(&str, &[&ErrorCode])] = &[
    ("CFG", cfg::ALL),
    ("CMP", cmp::ALL),
    ("EXT", ext::ALL),
    ("HTTP", http::ALL),
    ("VAL", val::ALL),
];

/// Prefixes that applications cannot use: Balise's areas, present and planned.
pub const RESERVED_PREFIXES: &[&str] = &[
    "AUTH", "CFG", "CMP", "DB", "DEV", "EXT", "HTTP", "MAIL", "PERF", "VAL",
];

/// A registered error code. Only the registry and `AppError` can create one.
#[derive(Debug, PartialEq, Eq)]
pub struct ErrorCode {
    code: &'static str,
    title: &'static str,
    message: &'static str,
    fix: &'static str,
    explanation: &'static str,
}

impl ErrorCode {
    /// An application's code, declared by `#[derive(AppError)]`.
    #[doc(hidden)]
    pub const fn app(
        code: &'static str,
        title: &'static str,
        message: &'static str,
        fix: &'static str,
        explanation: &'static str,
    ) -> ErrorCode {
        ErrorCode {
            code,
            title,
            message,
            fix,
            explanation,
        }
    }

    /// The code itself, such as `EXT0001`.
    pub fn code(&self) -> &'static str {
        self.code
    }

    /// A short title, the same for every occurrence.
    pub fn title(&self) -> &'static str {
        self.title
    }

    /// The message template, with `{name}` placeholders.
    pub fn message(&self) -> &'static str {
        self.message
    }

    /// The suggested fix.
    pub fn fix(&self) -> &'static str {
        self.fix
    }

    /// The full explanation, in Markdown, shown by `balise explain`.
    pub fn explanation(&self) -> &'static str {
        self.explanation
    }

    /// The message with its placeholders filled: `{address}` takes the value of `address`.
    /// A placeholder without a value is left as is.
    pub fn format_message(&self, args: &[(&str, &str)]) -> String {
        let mut message = self.message.to_string();
        for (name, value) in args {
            message = message.replace(&format!("{{{name}}}"), value);
        }
        message
    }

    /// The fix with its placeholders filled, like [`format_message`](Self::format_message).
    pub fn format_fix(&self, args: &[(&str, &str)]) -> String {
        let mut fix = self.fix.to_string();
        for (name, value) in args {
            fix = fix.replace(&format!("{{{name}}}"), value);
        }
        fix
    }

    /// The text printed by `balise explain`.
    pub fn explain(&self) -> String {
        format!(
            "{}  {}\n\nmessage: {}\nfix: {}\n\n{}\n",
            self.code,
            self.title,
            self.message,
            self.fix,
            self.explanation.trim()
        )
    }
}

/// Every registered code.
pub fn all() -> impl Iterator<Item = &'static ErrorCode> {
    AREAS.iter().flat_map(|(_, codes)| codes.iter().copied())
}

/// Looks up a code, such as `EXT0001` or an application's `ORD0001`.
pub fn find(code: &str) -> Option<&'static ErrorCode> {
    all()
        .chain(app_codes().iter().copied())
        .find(|c| c.code == code)
}

/// Every code declared by the application's `AppError` enums.
pub fn app_codes() -> &'static [&'static ErrorCode] {
    &APP_CODES
}

#[doc(hidden)]
#[linkme::distributed_slice]
pub static APP_CODES: [&'static ErrorCode];

/// Registers an application's code. Used by `#[derive(AppError)]`, never written by hand.
#[doc(hidden)]
#[macro_export]
macro_rules! __register_app_code {
    ($name:ident, $code:expr) => {
        #[$crate::registry::__linkme::distributed_slice($crate::error_codes::APP_CODES)]
        #[linkme(crate = $crate::registry::__linkme)]
        static $name: &$crate::error_codes::ErrorCode = $code;
    };
}

/// Checked at startup: two `AppError` variants of different enums with the same code
/// (`HTTP0013`). Within one enum, the derive rejects it at compile time.
#[allow(clippy::result_large_err)] // startup only: the size costs nothing
pub fn check_app_codes() -> Result<(), crate::event::Event> {
    let codes = app_codes();
    for (i, code) in codes.iter().enumerate() {
        if let Some(other) = codes[i + 1..].iter().find(|c| c.code == code.code) {
            return Err(crate::event::Event::coded(
                crate::event::Level::Error,
                HTTP0013,
                &[
                    ("code", code.code),
                    ("first", &format!("`{}`", code.title)),
                    ("second", &format!("`{}`", other.title)),
                ],
            ));
        }
    }
    Ok(())
}
