//! Balise, a Rust web framework designed for AI coding agents.

/// Everything a resource needs: `use balise::prelude::*;`.
pub mod prelude {
    pub use balise_core::error::{Error, Result};
    pub use balise_core::extract::{Body, Path, Query, RequestId};
    pub use balise_core::input::Patch;
    pub use balise_core::response::{Created, NoContent, Redirect};
    pub use balise_macros::{AppError, Input, Output, delete, get, patch, post, put};
}

pub use balise_core::error::{Error, Result};
pub use balise_core::extract::{Body, Path, Query, RequestId};
pub use balise_core::input::Patch;
pub use balise_core::response::{Created, NoContent, Redirect};
pub use balise_macros::{AppError, Input, Output, delete, get, patch, post, put};

/// Starts the app: the server listens and serves until the process stops.
///
/// ```no_run
/// fn main() {
///     balise::run();
/// }
/// ```
///
/// It never returns. If the app cannot start (the port is taken, two handlers claim the same
/// route…), the error is reported with its code and the process exits.
#[allow(clippy::needless_doctest_main)] // the example is the application's whole main.rs
pub fn run() -> ! {
    let config = match balise_core::config::Config::load() {
        Ok(config) => config,
        Err(event) => {
            event.emit();
            std::process::exit(1)
        }
    };
    balise_engine::run(config)
}

/// Logs from application code, in the same format as Balise's own events, with the
/// request's ID and handler when it runs inside a request.
///
/// ```no_run
/// # let id = 7;
/// balise::log::info!(order_id = id, "Order {id} cancelled");
/// balise::log::warn!("Stock is low");
/// ```
///
/// Fields come first, as `name = value`, then the message, written like `format!`. Fields
/// whose name contains `password`, `token` or `secret` are written `***`.
pub mod log {
    pub use crate::{
        __log_debug as debug, __log_error as error, __log_info as info, __log_warn as warn,
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __log {
    ($level:ident, [$($fields:tt)*] $name:ident = $value:expr, $($rest:tt)+) => {
        $crate::__log!($level, [$($fields)* ($name, $value)] $($rest)+)
    };
    ($level:ident, [$(($name:ident, $value:expr))*] $($message:tt)+) => {
        $crate::__private::log_app(
            $crate::__private::Level::$level,
            ::core::file!(),
            ::core::line!(),
            ::std::format!($($message)+),
            ::std::vec![$((
                ::core::stringify!($name),
                $crate::__private::LogValue::to_log_value(&$value),
            )),*],
        )
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __log_debug {
    ($($args:tt)+) => { $crate::__log!(Debug, [] $($args)+) };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __log_info {
    ($($args:tt)+) => { $crate::__log!(Info, [] $($args)+) };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __log_warn {
    ($($args:tt)+) => { $crate::__log!(Warn, [] $($args)+) };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __log_error {
    ($($args:tt)+) => { $crate::__log!(Error, [] $($args)+) };
}

/// Used by the code that Balise's macros generate. Not part of the API: never use it directly.
#[doc(hidden)]
pub mod __private {
    pub use balise_core::error::Error;
    pub use balise_core::error_codes::ErrorCode;
    pub use balise_core::event::Level;
    pub use balise_core::extract::{FieldError, FieldErrors, FromParam};
    pub use balise_core::handler::{
        FromRequest, IntoResponse, Method, Request, Response, ResponseFuture, extract, respond,
    };
    pub use balise_core::input::{Fields, Input, InputField, InputValue, rules};
    pub use balise_core::json::{Number, Value};
    pub use balise_core::log::{LogValue, app as log_app};
    pub use balise_core::output::{Output, OutputValue, write_field};
    pub use balise_core::registry::{Route, routes};
    pub use balise_core::{__register_app_code, __register_route};
}
