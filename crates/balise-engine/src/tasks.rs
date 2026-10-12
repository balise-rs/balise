//! Running work outside a request's own path: shared mutable state, blocking work,
//! background tasks, what a shutdown waits for, and, in dev, the watch on tasks that hold
//! the event loop.

use std::future::Future;
use std::ops::{Deref, DerefMut};
use std::panic::Location;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use balise_core::error::Error;
use balise_core::error_codes::PERF0002;
use balise_core::event::{Event, Level};
use balise_core::panic::{self, Catch};
use balise_core::task::Task;
use tokio::sync::{Notify, RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::{REQUEST, current_request};

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
    let task = tokio::task::spawn_blocking(move || {
        panic::catching(move || match context {
            Some(context) => REQUEST.sync_scope(context, work),
            None => work(),
        })
    });
    match task.await {
        Ok(Ok(value)) => value,
        // The panic continues in the caller, with its report, as if the work had run there.
        Ok(Err((report, payload))) => panic::rethrow(report, payload),
        Err(error) => std::panic::resume_unwind(error.into_panic()),
    }
}

/// Starts a background task: an `async move` closure whose parameters are services.
///
/// It runs to the end even after the request that started it has answered, and a shutdown
/// waits for it. Its logs carry that request's ID. An error or a panic is logged with the
/// line of this call.
#[track_caller]
pub fn spawn<Params>(task: impl Task<Params>) {
    let spawned_at = Location::caller();
    let context = REQUEST.try_with(Clone::clone).ok();
    let running = TASKS.enter();
    let future = Catch(watch(task.into_future(), "a task"));
    let run = async move {
        let _running = running;
        let outcome = match future.await {
            Ok(result) => result,
            Err(report) => {
                let what = match current_request() {
                    Some(context) => format!("A task started by `{}`", context.handler),
                    None => "A task".to_string(),
                };
                Err(Error::panicked(&what, report))
            }
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

/// What is in progress, for a shutdown to wait for it.
pub(crate) struct Counter {
    count: AtomicUsize,
    idle: Notify,
}

/// Requests being handled, and background tasks running.
pub(crate) static REQUESTS: Counter = Counter::new();
pub(crate) static TASKS: Counter = Counter::new();

impl Counter {
    const fn new() -> Counter {
        Counter {
            count: AtomicUsize::new(0),
            idle: Notify::const_new(),
        }
    }

    /// Counts one more until the guard is dropped, cancelled or panicked included.
    pub(crate) fn enter(&'static self) -> Entered {
        self.count.fetch_add(1, Ordering::SeqCst);
        Entered(self)
    }

    pub(crate) fn get(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }

    /// Returns once nothing is in progress.
    pub(crate) async fn idle(&self) {
        loop {
            let notified = self.idle.notified();
            if self.get() == 0 {
                return;
            }
            notified.await;
        }
    }
}

pub(crate) struct Entered(&'static Counter);

impl Drop for Entered {
    fn drop(&mut self) {
        if self.0.count.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.0.idle.notify_waiters();
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

    #[tokio::test]
    async fn a_panic_in_blocking_work_keeps_its_report_in_the_caller() {
        let caught = Catch(Box::pin(blocking(|| -> u32 { panic!("heavy") }))).await;
        assert_eq!(caught.unwrap_err().message, "heavy");
    }

    #[tokio::test]
    async fn a_counter_is_idle_once_every_guard_is_dropped() {
        static COUNTER: Counter = Counter::new();
        let entered = COUNTER.enter();
        let idle = tokio::spawn(COUNTER.idle());
        tokio::task::yield_now().await;
        assert!(!idle.is_finished());
        drop(entered);
        idle.await.unwrap();
    }
}
