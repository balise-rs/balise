//! Shared state: values created once at startup, read by handlers and tasks with `State<T>`.
//!
//! Each `#[balise::state]` function registers how to create one value, next to the resource
//! that uses it. At startup, every state is created, then checked against what the routes
//! ask for: a missing or duplicated state stops the startup, never a request.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::future::Future;
use std::ops::Deref;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};

use crate::error::Error;
use crate::error_codes::{HTTP0014, HTTP0015, HTTP0016};
use crate::event::{Event, Level};
use crate::handler::{FromRequest, Request};
use crate::registry::Route;

/// A value of shared state, read by a handler or a task: `cache: State<PriceCache>`. It is
/// used like a `&PriceCache`.
pub struct State<T>(Arc<T>);

impl<T> Deref for State<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> Clone for State<T> {
    fn clone(&self) -> State<T> {
        State(self.0.clone())
    }
}

impl<T: Send + Sync + 'static> State<T> {
    /// The value created at startup. The startup check guarantees it exists for every route;
    /// a task asking for a state nobody declared gets `HTTP0014`.
    fn get() -> Result<State<T>, Error> {
        let name = short_name(std::any::type_name::<T>());
        let missing = || Error::coded(500, HTTP0014, &[("type", name), ("reader", "a task")]);
        let value = STORE.get().and_then(|store| store.get(&TypeId::of::<T>()));
        let value = value.ok_or_else(missing)?.clone();
        value.downcast::<T>().map(State).map_err(|_| missing())
    }
}

impl<T: Send + Sync + 'static> FromRequest for State<T> {
    fn from_request(_: &mut Request) -> impl Future<Output = Result<Self, Error>> + Send {
        let value = State::get();
        async move { value }
    }
}

impl<T: Send + Sync + 'static> crate::task::FromTask for State<T> {
    fn from_task() -> impl Future<Output = Result<Self, Error>> + Send {
        let value = State::get();
        async move { value }
    }
}

/// The type a route asks for with `State<T>`, as the route macro records it.
#[derive(Debug, Clone, Copy)]
pub struct StateKey {
    pub id: TypeId,
    /// As messages show it: `PriceCache`.
    pub name: &'static str,
}

impl StateKey {
    pub fn of<T: 'static>() -> StateKey {
        StateKey {
            id: TypeId::of::<T>(),
            name: short_name(std::any::type_name::<T>()),
        }
    }
}

/// A value being created, type-erased.
pub type StateFuture = Pin<Box<dyn Future<Output = Result<StateValue, Error>> + Send>>;

/// A created value, type-erased.
pub type StateValue = Arc<dyn Any + Send + Sync>;

/// One `#[balise::state]` function, as its attribute registers it.
pub struct StateInit {
    pub key: fn() -> StateKey,
    /// `prices::price_cache`, as messages show it.
    pub function: &'static str,
    pub file: &'static str,
    pub line: u32,
    pub create: fn() -> StateFuture,
}

#[doc(hidden)]
#[linkme::distributed_slice]
pub static STATES: [StateInit];

/// Registers a state. Used by `#[balise::state]`, never written by hand.
#[doc(hidden)]
#[macro_export]
macro_rules! __register_state {
    ($name:ident, $init:expr) => {
        #[$crate::registry::__linkme::distributed_slice($crate::state::STATES)]
        #[linkme(crate = $crate::registry::__linkme)]
        static $name: $crate::state::StateInit = $init;
    };
}

/// Erases a created value. Called by the code `#[balise::state]` generates.
#[doc(hidden)]
pub fn erase<T: Send + Sync + 'static>(value: T) -> StateValue {
    Arc::new(value)
}

static STORE: OnceLock<HashMap<TypeId, StateValue>> = OnceLock::new();

/// Creates every declared state, then checks that each route finds the states it reads.
/// Called once at startup.
#[allow(clippy::result_large_err)] // startup only: the size costs nothing
pub async fn create(inits: &'static [StateInit], routes: &'static [Route]) -> Result<(), Event> {
    let mut store: HashMap<TypeId, StateValue> = HashMap::new();
    let mut declared: Vec<(StateKey, &StateInit)> = Vec::new();
    for init in inits {
        let key = (init.key)();
        if let Some((_, first)) = declared.iter().find(|(k, _)| k.id == key.id) {
            return Err(Event::coded(
                Level::Error,
                HTTP0015,
                &[
                    ("type", key.name),
                    (
                        "first",
                        &format!("{} ({}:{})", first.function, first.file, first.line),
                    ),
                    (
                        "second",
                        &format!("{} ({}:{})", init.function, init.file, init.line),
                    ),
                ],
            ));
        }
        declared.push((key, init));
    }
    for (key, init) in &declared {
        match (init.create)().await {
            Ok(value) => {
                store.insert(key.id, value);
            }
            Err(error) => {
                let mut event = Event::coded(
                    Level::Error,
                    HTTP0016,
                    &[("type", key.name), ("message", error.message())],
                );
                let from = error.event();
                event.location = from
                    .location
                    .or(Some(format!("{}:{}", init.file, init.line)));
                event.cause = from.cause;
                return Err(event);
            }
        }
    }
    for route in routes {
        for key in route.states.iter().map(|key| key()) {
            if !store.contains_key(&key.id) {
                let reader = format!("`{}`", route.handler_name());
                let args = [("type", key.name), ("reader", reader.as_str())];
                let mut event = Event::coded(Level::Error, HTTP0014, &args);
                event.location = Some(route.location());
                return Err(event);
            }
        }
    }
    let _ = STORE.set(store);
    Ok(())
}

/// `prices::PriceCache` as `PriceCache`, `balise::Shared<app::Counter>` as `Shared<Counter>`.
/// Called at startup, and on errors only: the text is kept for the process's life.
fn short_name(full: &'static str) -> &'static str {
    let mut out = String::new();
    let mut word = String::new();
    for c in full.chars() {
        if c.is_alphanumeric() || c == '_' || c == ':' {
            word.push(c);
        } else {
            out.push_str(word.rsplit("::").next().unwrap_or(&word));
            word.clear();
            out.push(c);
        }
    }
    out.push_str(word.rsplit("::").next().unwrap_or(&word));
    Box::leak(out.into_boxed_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handler::{Method, ResponseFuture};

    struct Cache;

    fn invoke(_: Request) -> ResponseFuture {
        unreachable!("never called")
    }

    static ROUTES: [Route; 1] = [Route {
        method: Method::Get,
        path: "/prices",
        module: "app::prices",
        function: "list",
        file: "src/prices/mod.rs",
        line: 8,
        invoke,
        states: &[StateKey::of::<Cache>],
        files: &[],
        page: None,
    }];

    fn cache() -> StateFuture {
        Box::pin(async { Ok(erase(Cache)) })
    }

    fn failing() -> StateFuture {
        Box::pin(async { Err(Error::from("x".parse::<u32>().unwrap_err())) })
    }

    fn init(create: fn() -> StateFuture, line: u32) -> StateInit {
        StateInit {
            key: StateKey::of::<Cache>,
            function: "app::prices::cache",
            file: "src/prices/mod.rs",
            line,
            create,
        }
    }

    fn message(event: Event) -> String {
        format!("{} {}", event.code.unwrap().code(), event.message)
    }

    /// Runs a future that never waits, which is all these tests need.
    fn block_on<F: Future>(future: F) -> F::Output {
        let mut future = std::pin::pin!(future);
        let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
        loop {
            if let std::task::Poll::Ready(value) = future.as_mut().poll(&mut cx) {
                return value;
            }
        }
    }

    #[allow(clippy::result_large_err)] // startup only, as in `create`
    fn start(inits: Vec<StateInit>) -> Result<(), Event> {
        block_on(create(Box::leak(inits.into_boxed_slice()), &ROUTES))
    }

    #[test]
    fn a_state_a_route_reads_must_be_declared() {
        assert_eq!(
            message(start(vec![]).unwrap_err()),
            "HTTP0014 No `#[balise::state]` creates `Cache`, which `prices::list` reads with `State<Cache>`"
        );
    }

    #[test]
    fn a_state_is_declared_once() {
        let error = start(vec![init(cache, 3), init(cache, 9)]).unwrap_err();
        assert_eq!(
            message(error),
            "HTTP0015 `Cache` is created by two `#[balise::state]` functions: \
             app::prices::cache (src/prices/mod.rs:3) and app::prices::cache (src/prices/mod.rs:9)"
        );
    }

    #[test]
    fn a_state_that_fails_stops_the_startup_with_its_cause() {
        let error = start(vec![init(failing, 3)]).unwrap_err();
        assert_eq!(
            error.cause.as_deref(),
            Some("invalid digit found in string")
        );
        assert_eq!(
            message(error),
            "HTTP0016 Cannot create the state `Cache`: invalid digit found in string"
        );
    }

    #[test]
    fn type_names_drop_their_paths() {
        assert_eq!(short_name("app::prices::PriceCache"), "PriceCache");
        assert_eq!(
            short_name("balise_engine::shared::Shared<app::Counter>"),
            "Shared<Counter>"
        );
    }
}
