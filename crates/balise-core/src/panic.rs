//! Panics: caught in the request or task that raised them, with where they were raised and
//! the application's frames that led there.
//!
//! Balise's panic hook replaces Rust's message on stderr. When the panic will be caught (the
//! code runs inside [`Catch`] or [`catching`]), the hook leaves its report for the catcher, on
//! the same thread. Otherwise the process is about to stop: the hook logs the panic, writes a
//! crash report, which the next start publishes (`RUN0007`), and lets Rust print its message.

use std::any::Any;
use std::backtrace::Backtrace;
use std::cell::{Cell, RefCell};
use std::future::Future;
use std::panic::{AssertUnwindSafe, PanicHookInfo};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Mutex, OnceLock};
use std::task::{Context, Poll};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error_codes::{RUN0006, RUN0007};
use crate::event::{Event, Level, rfc3339};
use crate::json::{self, Value};

/// What a panic leaves: its message, where it was raised, and the application's frames.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanicReport {
    pub message: String,
    /// `src/orders/mod.rs:31`, where `panic!` or `unwrap()` was called.
    pub location: Option<(&'static str, u32)>,
    /// The application's frames, framework frames folded: see [`fold`].
    pub backtrace: Vec<String>,
}

impl PanicReport {
    /// The report of a panic the hook did not see: its message only.
    fn from_payload(payload: &(dyn Any + Send)) -> PanicReport {
        let message = match (
            payload.downcast_ref::<&str>(),
            payload.downcast_ref::<String>(),
        ) {
            (Some(text), _) => text.to_string(),
            (_, Some(text)) => text.clone(),
            _ => "a panic without a message".to_string(),
        };
        PanicReport {
            message,
            location: None,
            backtrace: Vec::new(),
        }
    }

    /// The `RUN0006` event: `what` reads like `` `orders::get` `` or `A task`.
    pub fn event(&self, what: &str) -> Event {
        let mut event = Event::coded(
            Level::Error,
            RUN0006,
            &[("what", what), ("message", &self.message)],
        );
        event.location = self.location.map(|(file, line)| format!("{file}:{line}"));
        if !self.backtrace.is_empty() {
            event
                .fields
                .push(("backtrace".into(), backtrace_value(&self.backtrace)));
        }
        event
    }
}

pub(crate) fn backtrace_value(lines: &[String]) -> Value {
    Value::Array(lines.iter().cloned().map(Value::String).collect())
}

thread_local! {
    /// How many catchers the current thread runs inside.
    static CATCHING: Cell<u32> = const { Cell::new(0) };
    /// The report of a panic raised inside a catcher, not caught yet.
    static PENDING: RefCell<Option<PanicReport>> = const { RefCell::new(None) };
}

/// The application, to tell its frames from the others.
struct App {
    /// `web_template`: symbols of the application start with it.
    crate_name: String,
    /// The folder of its `Cargo.toml`, removed from paths.
    root: String,
}

static APP: OnceLock<App> = OnceLock::new();

/// Installs Balise's panic hook, once at startup.
pub fn install(crate_name: &str, root: &str) {
    let app = App {
        crate_name: crate_name.to_string(),
        root: format!("{}/", root.trim_end_matches('/')),
    };
    if APP.set(app).is_ok() {
        // Rust's own message stays for panics that stop a thread: it is the last thing a
        // supervisor, or a test harness, can show.
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if !hook(info) {
                previous(info);
            }
        }));
    }
}

/// Returns whether the panic will be caught.
fn hook(info: &PanicHookInfo<'_>) -> bool {
    let mut report = PanicReport::from_payload(info.payload());
    report.location = info
        .location()
        .map(|location| (intern(location.file()), location.line()));
    let frames = Backtrace::force_capture().to_string();
    report.backtrace = match APP.get() {
        Some(app) => fold(&frames, &app.crate_name, &app.root),
        None => Vec::new(),
    };
    // Inside a catcher, unless a panic is already unwinding: a second one aborts.
    let caught = CATCHING.get() > 0 && PENDING.with_borrow(Option::is_none);
    if caught {
        PENDING.set(Some(report));
        return true;
    }
    let thread = std::thread::current();
    let what = format!("Thread `{}`", thread.name().unwrap_or("unnamed"));
    report.event(&what).emit();
    write_crash_report(&report);
    false
}

/// A file name of the panic's location, kept for the life of the process. Each file is kept
/// once, however many panics it raises.
fn intern(file: &str) -> &'static str {
    static FILES: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
    let mut files = FILES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    match files.iter().find(|known| **known == file) {
        Some(known) => known,
        None => {
            let leaked: &'static str = Box::leak(file.to_string().into_boxed_str());
            files.push(leaked);
            leaked
        }
    }
}

/// The report of the panic that `payload` comes from, raised on this thread.
fn take(payload: &(dyn Any + Send)) -> PanicReport {
    PENDING
        .take()
        .unwrap_or_else(|| PanicReport::from_payload(payload))
}

/// Runs `work`, catching its panic.
pub fn catching<T>(work: impl FnOnce() -> T) -> Result<T, (PanicReport, Box<dyn Any + Send>)> {
    CATCHING.set(CATCHING.get() + 1);
    let result = std::panic::catch_unwind(AssertUnwindSafe(work));
    CATCHING.set(CATCHING.get() - 1);
    result.map_err(|payload| (take(&*payload), payload))
}

/// Raises again, on this thread, a panic caught on another one: its report goes with it.
pub fn rethrow(report: PanicReport, payload: Box<dyn Any + Send>) -> ! {
    if CATCHING.get() > 0 {
        PENDING.set(Some(report));
    }
    std::panic::resume_unwind(payload)
}

/// A future whose panic is caught and returned as its report.
pub struct Catch<F>(pub F);

impl<F: Future + Unpin> Future for Catch<F> {
    type Output = Result<F::Output, PanicReport>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let inner = &mut self.0;
        match catching(|| Pin::new(inner).poll(cx)) {
            Ok(Poll::Ready(value)) => Poll::Ready(Ok(value)),
            Ok(Poll::Pending) => Poll::Pending,
            Err((report, _)) => Poll::Ready(Err(report)),
        }
    }
}

/// The frames of a backtrace as an agent needs them: from the first frame of the application,
/// each application frame as `orders::get  src/orders/mod.rs:31`, and every run of other
/// frames folded into one line. Frames of Balise, its dependencies and the standard library
/// are never named.
pub fn fold(backtrace: &str, crate_name: &str, root: &str) -> Vec<String> {
    let prefix = format!("{crate_name}::");
    let mut lines = Vec::new();
    let mut outside = 0;
    let mut seen_app = false;
    let mut frames = backtrace.lines().peekable();
    while let Some(line) = frames.next() {
        let Some((number, symbol)) = line.trim().split_once(": ") else {
            continue;
        };
        if number.parse::<u32>().is_err() {
            continue;
        }
        let at = frames
            .next_if(|next| next.trim_start().starts_with("at "))
            .map(|next| next.trim_start()[3..].to_string());
        // `orders::get::{closure#0}` is `orders::get`; code generated by Balise's macros
        // (`__balise_…`) is not the application's.
        let name = symbol.strip_prefix(&prefix).map(|name| {
            name.split("::")
                .filter(|part| !part.starts_with('{'))
                .collect::<Vec<_>>()
                .join("::")
        });
        let generated = |name: &String| {
            let last = name.rsplit("::").next().unwrap_or(name);
            name.is_empty() || last.starts_with("__balise")
        };
        let Some(name) = name.filter(|name| !generated(name)) else {
            outside += usize::from(seen_app);
            continue;
        };
        seen_app = true;
        if outside > 0 {
            lines.push(folded(outside));
            outside = 0;
        }
        let line = match at {
            Some(at) => {
                // `/app/src/orders/mod.rs:31:9`: relative, without the column.
                let at = at.strip_prefix(root).unwrap_or(&at);
                let at = at.strip_prefix("./").unwrap_or(at);
                let at = at.rsplit_once(':').map_or(at, |(path, _)| path);
                format!("{name}  {at}")
            }
            None => name,
        };
        // An `async fn` shows one frame per level of its state machine.
        if lines.last() != Some(&line) {
            lines.push(line);
        }
    }
    if outside > 0 {
        lines.push(folded(outside));
    }
    lines
}

fn folded(count: usize) -> String {
    match count {
        1 => "… 1 frame outside the application".to_string(),
        n => format!("… {n} frames outside the application"),
    }
}

/// Where crash reports wait for the next start.
fn crashes() -> PathBuf {
    crate::file::storage().join(".balise").join("crashes")
}

/// Writes the report of a panic that stops the process. A failure to write is reported on
/// stderr: nothing else can be done at this point.
fn write_crash_report(report: &PanicReport) {
    let time_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    let location = report
        .location
        .map(|(file, line)| Value::String(format!("{file}:{line}")))
        .unwrap_or(Value::Null);
    let value = Value::Object(vec![
        ("time_ms".into(), Value::String(time_ms.to_string())),
        ("message".into(), Value::String(report.message.clone())),
        ("location".into(), location),
        ("backtrace".into(), backtrace_value(&report.backtrace)),
    ]);
    let mut text = String::new();
    json::write_value(&mut text, &value);
    let dir = crashes();
    let path = dir.join(format!("crash-{time_ms}-{}.json", crate::text::unique_id()));
    let written = std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&path, text));
    if let Err(error) = written {
        eprintln!(
            "Balise could not write a crash report to {}: {error}",
            path.display()
        );
    }
}

/// The crash reports left by previous runs, as `RUN0007` events, deleted once read. Called at
/// startup, once logging is set up.
pub fn previous_crashes() -> Vec<Event> {
    let Ok(entries) = std::fs::read_dir(crashes()) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| Some(entry.ok()?.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    paths.sort();
    let mut events = Vec::new();
    for path in paths {
        if let Some(event) = std::fs::read(&path)
            .ok()
            .and_then(|text| crash_event(&text))
        {
            events.push(event);
        }
        let _ = std::fs::remove_file(&path);
    }
    events
}

fn crash_event(text: &[u8]) -> Option<Event> {
    let Value::Object(entries) = json::parse(text).ok()? else {
        return None;
    };
    let get = |key: &str| entries.iter().find(|(k, _)| k == key).map(|(_, v)| v);
    let text = |key: &str| match get(key) {
        Some(Value::String(text)) => Some(text.clone()),
        _ => None,
    };
    let time_ms: u64 = text("time_ms")?.parse().ok()?;
    let message = text("message")?;
    let mut event = Event::coded(
        Level::Error,
        RUN0007,
        &[("time", &rfc3339(time_ms)), ("message", &message)],
    );
    event.location = text("location");
    if let Some(Value::Array(lines)) = get("backtrace")
        && !lines.is_empty()
    {
        event
            .fields
            .push(("backtrace".into(), Value::Array(lines.clone())));
    }
    Some(event)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BACKTRACE: &str = "   0: std::backtrace::Backtrace::force_capture
             at /rustc/abc/library/std/src/backtrace.rs:312:9
   1: balise_core::panic::hook
   2: core::option::unwrap_failed
             at /rustc/abc/library/core/src/option.rs:2015:5
   3: shop::orders::find
             at /app/src/orders/model.rs:12:5
   4: shop::orders::get::{{closure}}
             at /app/src/orders/mod.rs:31:9
   5: shop::orders::get::{closure#0}
             at /app/src/orders/mod.rs:31:9
   6: shop::orders::__balise_invoke_get::{closure#0}
             at ./src/orders/mod.rs:28:1
   7: balise_engine::handle::{{closure}}
   8: tokio::runtime::task::harness::poll
   9: std::thread::spawn::{{closure}}
";

    #[test]
    fn framework_frames_are_folded() {
        assert_eq!(
            fold(BACKTRACE, "shop", "/app/"),
            [
                "orders::find  src/orders/model.rs:12",
                "orders::get  src/orders/mod.rs:31",
                "… 4 frames outside the application",
            ]
        );
    }

    #[test]
    fn a_caught_panic_keeps_its_message() {
        let (report, _) = catching(|| panic!("boom")).unwrap_err();
        assert_eq!(report.message, "boom");
        assert_eq!(report.event("A task").message, "A task panicked: boom");
    }

    #[test]
    fn crash_reports_are_read_back_as_events() {
        let report = br#"{"time_ms":"0","message":"boom","location":"src/main.rs:4","backtrace":["main  src/main.rs:4"]}"#;
        let event = crash_event(report).unwrap();
        assert_eq!(
            event.message,
            "The previous run crashed at 1970-01-01T00:00:00.000Z: boom"
        );
        assert_eq!(event.location.as_deref(), Some("src/main.rs:4"));
    }
}
