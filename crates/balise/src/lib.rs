//! Balise, a Rust web framework designed for AI coding agents.

/// Everything a resource needs: `use balise::prelude::*;`.
pub mod prelude {
    pub use balise_core::extract::{Body, Path, Query, RequestId};
    pub use balise_core::input::Patch;
    pub use balise_macros::{Input, Output, delete, get, patch, post, put};
}

pub use balise_core::extract::{Body, Path, Query, RequestId};
pub use balise_core::input::Patch;
pub use balise_macros::{Input, Output, delete, get, patch, post, put};

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

/// Used by the code that Balise's macros generate. Not part of the API: never use it directly.
#[doc(hidden)]
pub mod __private {
    pub use balise_core::__register_route;
    pub use balise_core::extract::{FieldError, FieldErrors, FromParam};
    pub use balise_core::handler::{
        FromRequest, IntoResponse, Method, Request, Response, ResponseFuture, extract, respond,
    };
    pub use balise_core::input::{Fields, Input, InputField, InputValue, rules};
    pub use balise_core::json::{Number, Value};
    pub use balise_core::output::{Output, OutputValue, write_field};
    pub use balise_core::registry::{Route, routes};
}
