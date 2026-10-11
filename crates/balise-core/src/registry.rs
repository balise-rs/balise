//! The routes of the application, collected at compile time.
//!
//! Each route attribute adds one [`Route`] through [`__register_route!`](crate::__register_route).
//! The mechanism is delegated to `linkme` (a static array filled by the linker, nothing runs
//! before `main`), and only this module knows it: replacing it changes nothing elsewhere.

use crate::handler::{HandlerFn, Method};

#[doc(hidden)]
pub use linkme as __linkme;

/// One route, as declared by its attribute.
#[derive(Debug)]
pub struct Route {
    pub method: Method,
    /// The path as written in the attribute, without the API base: `/orders/{id}`.
    pub path: &'static str,
    /// `module_path!()` where the handler is defined.
    pub module: &'static str,
    /// The handler function's name.
    pub function: &'static str,
    pub file: &'static str,
    pub line: u32,
    pub invoke: HandlerFn,
}

impl Route {
    /// The handler's name as logs and errors show it: `orders::create`.
    pub fn handler_name(&self) -> String {
        let module = self.module.rsplit("::").next().unwrap_or(self.module);
        format!("{module}::{}", self.function)
    }

    /// Where the handler is: `src/orders/mod.rs:29`.
    pub fn location(&self) -> String {
        format!("{}:{}", self.file, self.line)
    }
}

#[doc(hidden)]
#[linkme::distributed_slice]
pub static ROUTES: [Route];

/// Every route of the application.
pub fn routes() -> &'static [Route] {
    &ROUTES
}

/// Registers a route. Used by the route attributes, never written by hand.
#[doc(hidden)]
#[macro_export]
macro_rules! __register_route {
    ($name:ident, $route:expr) => {
        #[$crate::registry::__linkme::distributed_slice($crate::registry::ROUTES)]
        #[linkme(crate = $crate::registry::__linkme)]
        static $name: $crate::registry::Route = $route;
    };
}
