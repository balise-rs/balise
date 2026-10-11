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

mod cmp;
mod ext;
mod http;
mod val;

pub use cmp::*;
pub use ext::*;
pub use http::*;
pub use val::*;

/// Every area, with its prefix and its codes.
pub const AREAS: &[(&str, &[&ErrorCode])] = &[
    ("CMP", cmp::ALL),
    ("EXT", ext::ALL),
    ("HTTP", http::ALL),
    ("VAL", val::ALL),
];

/// A registered error code. Only the registry can create one.
#[derive(Debug, PartialEq, Eq)]
pub struct ErrorCode {
    code: &'static str,
    title: &'static str,
    message: &'static str,
    fix: &'static str,
    explanation: &'static str,
}

impl ErrorCode {
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

/// Looks up a code, such as `EXT0001`.
pub fn find(code: &str) -> Option<&'static ErrorCode> {
    all().find(|c| c.code == code)
}
