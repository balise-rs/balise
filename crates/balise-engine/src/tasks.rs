//! Running work outside a request's own path: shared mutable state, blocking work,
//! background tasks, and, in dev, the watch on tasks that hold the event loop.

use std::future::Future;
use std::ops::{Deref, DerefMut};
use std::panic::{AssertUnwindSafe, Location};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use balise_core::error::Error;
use balise_core::error_codes::{EXT0001, PERF0002};
use balise_core::event::{Event, Level};
use balise_core::task::Task;
use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::{REQUEST, current_request, panic_message};

/// Shared state that changes: `#[balise::state] fn counter() -> Shared<Counter>`, read with
/// `State<Shared<Counter>>`. The lock waits without blocking the server, and a panic while
/// it is held does not break the requests that follow.
pub struct Shared<T>(RwLock<T>);

impl<T> Shared<T> {
    pub fn new(value: T) -> Shared<T> {
        Shared(RwLock::new(value))
    }

    /// Reads the value; several readers at once, never while it is written.
    pub async fn read(&self) -> SharedRead<'_, T> {
        SharedRead(self.0.read().await)
    }

    /// Changes the value; one writer at a time. Keep the guard for the change only.
    pub async fn write(&self) -> SharedWrite<'_, T> {
        SharedWrite(self.0.write().await)
    }
}

/// A read of a [`Shared`] value, used like a `&T`.
pub struct SharedRead<'a, T>(RwLockReadGuard<'a, T>);

/// A write to a [`Shared`] value, used like a `&mut T`.
pub struct SharedWrite<'a, T>(RwLockWriteGuard<'a, T>);

impl<T> Deref for SharedRead<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> Deref for SharedWrite<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> DerefMut for SharedWrite<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

/// Runs CPU-heavy or blocking work on a pool made for it, so the server keeps answering.
/// Logs from inside keep the request they belong to.
pub async fn blocking<F, T>(work: F) -> T
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let context = REQUEST.try_with(Clone::clone).ok();
    let task = tokio::task::spawn_blocking(move || match context {
        Some(context) => REQUEST.sync_scope(context, work),
        None => work(),
    });
    match task.await {
        Ok(value) => value,
        // The panic continues in the caller, as if the work had run there.
        Err(error) => std::panic::resume_unwind(error.into_panic()),
    }
}

/// Starts a background task: an `async move` closure whose parameters are services.
///
/// It runs to the end even after the request that started it has answered. Its logs carry
/// that request's ID. An error or a panic is logged with the line of this call.
#[track_caller]
pub fn spawn<Params>(task: impl Task<Params>) {
    let spawned_at = Location::caller();
    let context = REQUEST.try_with(Clone::clone).ok();
    let future = CatchPanic(watch(task.into_future(), "a task"));
    let run = async move {
        let outcome = match future.await {
            Ok(result) => result,
            Err(panic) => Err(Error::coded(500, EXT0001, &[("cause", &panic)]).with_cause(panic)),
        };
        if let Err(error) = outcome {
            let mut event = error.event();
            let at = format!("{}:{}", spawned_at.file(), spawned_at.line());
            event
                .fields
                .push(("spawned_at".into(), balise_core::json::Value::String(at)));
            if let Some(context) = current_request() {
                event.request_id = Some(context.request_id);
                event.handler = Some(context.handler);
            }
            event.emit();
        }
    };
    match context {
        Some(context) => tokio::spawn(REQUEST.scope(context, run)),
        None => tokio::spawn(run),
    };
}

/// A future whose panic is caught and returned as its message.
struct CatchPanic<F>(F);

impl<F: Future + Unpin> Future for CatchPanic<F> {
    type Output = Result<F::Output, String>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let inner = &mut self.0;
        match std::panic::catch_unwind(AssertUnwindSafe(|| Pin::new(inner).poll(cx))) {
            Ok(Poll::Ready(value)) => Poll::Ready(Ok(value)),
            Ok(Poll::Pending) => Poll::Pending,
            Err(panic) => Poll::Ready(Err(panic_message("the task panicked", &*panic))),
        }
    }
}

/// Past this, a step of a task holds the event loop for too long.
pub const BLOCKING_THRESHOLD: Duration = Duration::from_millis(10);

/// In dev, times every step of `future` and warns when one holds the event loop. Elsewhere,
/// the future as it is.
pub(crate) fn watch<F: Future + Send + 'static>(
    future: F,
    what: &'static str,
) -> Pin<Box<dyn Future<Output = F::Output> + Send>> {
    if cfg!(feature = "dev") {
        Box::pin(Watched {
            future: Box::pin(future),
            what,
            report: |what, elapsed| blocked_event(what, elapsed).emit(),
        })
    } else {
        Box::pin(future)
    }
}

/// A future timed at every poll.
pub(crate) struct Watched<F> {
    pub(crate) future: Pin<Box<F>>,
    pub(crate) what: &'static str,
    pub(crate) report: fn(&'static str, Duration),
}

impl<F: Future> Future for Watched<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
        let started = Instant::now();
        let poll = self.future.as_mut().poll(cx);
        let elapsed = started.elapsed();
        if elapsed > BLOCKING_THRESHOLD {
            (self.report)(self.what, elapsed);
        }
        poll
    }
}

/// `PERF0002`, for the current request's handler or task.
fn blocked_event(what: &'static str, elapsed: Duration) -> Event {
    let context = current_request();
    let task = match (&context, what) {
        (Some(context), "a handler") => format!("`{}`", context.handler),
        (Some(context), _) => format!("A task started by `{}`", context.handler),
        (None, _) => "A task".to_string(),
    };
    let duration = format!("{} ms", elapsed.as_millis());
    let mut event = Event::coded(
        Level::Warn,
        PERF0002,
        &[("task", &capitalized(&task)), ("duration", &duration)],
    );
    if let Some(context) = context {
        event.request_id = Some(context.request_id);
        event.handler = Some(context.handler);
    }
    event
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static REPORTED_MS: AtomicU64 = AtomicU64::new(0);

    #[tokio::test]
    async fn a_step_longer_than_the_threshold_is_reported() {
        let slow = async {
            std::thread::sleep(Duration::from_millis(30));
        };
        Watched {
            future: Box::pin(slow),
            what: "a handler",
            report: |_, elapsed| REPORTED_MS.store(elapsed.as_millis() as u64, Ordering::SeqCst),
        }
        .await;
        assert!(REPORTED_MS.load(Ordering::SeqCst) >= 30);
    }

    #[tokio::test]
    async fn shared_values_are_read_and_written() {
        let counter = Shared::new(0);
        *counter.write().await += 2;
        assert_eq!(*counter.read().await, 2);
    }

    #[tokio::test]
    async fn blocking_work_returns_its_value() {
        assert_eq!(blocking(|| 6 * 7).await, 42);
    }
}
