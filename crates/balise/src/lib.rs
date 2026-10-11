//! Balise, a Rust web framework designed for AI coding agents.

/// Starts the app: the server listens and serves until the process stops.
///
/// ```no_run
/// fn main() {
///     balise::run();
/// }
/// ```
///
/// It never returns. If the app cannot start (the port is taken, for instance), the error is
/// reported with its code and the process exits.
#[allow(clippy::needless_doctest_main)] // the example is the application's whole main.rs
pub fn run() -> ! {
    balise_engine::run(balise_engine::DEFAULT_ADDRESS)
}
