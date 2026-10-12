//! Graceful shutdown: on `SIGTERM` or Ctrl-C, stop accepting, then wait for the requests and
//! tasks in progress, for `shutdown.timeout` at most.

use std::future::{Future, poll_fn};
use std::pin::pin;
use std::task::Poll;
use std::time::Duration;

use balise_core::error_codes::RUN0008;
use balise_core::event::{Event, Level};
use balise_core::text::format_duration;
use hyper_util::server::graceful::GracefulShutdown;

use crate::tasks::{REQUESTS, TASKS};

/// Resolves on `SIGTERM`, what orchestrators send, or on Ctrl-C. A signal that cannot be
/// listened to is ignored: the other one still works, and `SIGKILL` always does.
pub async fn signal() {
    let mut ctrl_c = pin!(tokio::signal::ctrl_c());
    let mut ctrl_c_works = true;
    #[cfg(unix)]
    let mut terminate =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).ok();
    poll_fn(|cx| {
        if ctrl_c_works && let Poll::Ready(result) = ctrl_c.as_mut().poll(cx) {
            match result {
                Ok(()) => return Poll::Ready(()),
                Err(_) => ctrl_c_works = false,
            }
        }
        #[cfg(unix)]
        if let Some(terminate) = terminate.as_mut()
            && terminate.poll_recv(cx).is_ready()
        {
            return Poll::Ready(());
        }
        Poll::Pending
    })
    .await
}

/// Waits for the connections, requests and tasks in progress, for `timeout` at most.
pub(crate) async fn drain(timeout: Duration, connections: GracefulShutdown) {
    let (requests, tasks) = (REQUESTS.get(), TASKS.get());
    let message = match (requests, tasks) {
        (0, 0) => "Shutting down".to_string(),
        _ => format!(
            "Shutting down: {} and {} in progress",
            count(requests, "request"),
            count(tasks, "task")
        ),
    };
    Event::new(Level::Info, message).emit();
    let finished = async {
        connections.shutdown().await;
        REQUESTS.idle().await;
        TASKS.idle().await;
    };
    match tokio::time::timeout(timeout, finished).await {
        Ok(()) => Event::new(Level::Info, "Stopped").emit(),
        Err(_) => Event::coded(
            Level::Warn,
            RUN0008,
            &[
                ("timeout", &format_duration(timeout)),
                ("requests", &REQUESTS.get().to_string()),
                ("tasks", &TASKS.get().to_string()),
            ],
        )
        .emit(),
    }
}

/// `1 request`, `3 tasks`.
fn count(n: usize, noun: &str) -> String {
    match n {
        1 => format!("1 {noun}"),
        n => format!("{n} {noun}s"),
    }
}
