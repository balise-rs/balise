//! HTTP engine wrapper: startup, shutdown, error capture.
//!
//! hyper and Tokio are used here and nowhere else. Nothing from them crosses this crate's
//! boundary into the facade: errors leave as Balise events with their codes.

use std::convert::Infallible;
use std::io;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use balise_core::error_codes::{
    EXT0001, HTTP0001, HTTP0002, HTTP0003, HTTP0004, HTTP0005, HTTP0006, HTTP0007, HTTP0008,
};
use balise_core::event::{Event, Level};
use bytes::Bytes;
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

/// Where the app listens until configuration provides the address.
pub const DEFAULT_ADDRESS: &str = "0.0.0.0:3000";

/// How long to wait before accepting again when the process is out of file descriptors.
const ACCEPT_BACKOFF: Duration = Duration::from_millis(100);

/// At most one report per period for the same accept error, with the number of occurrences.
const REPORT_PERIOD: Duration = Duration::from_secs(1);

/// Starts the runtime and serves on `address`. Never returns: on a fatal error, reports it
/// and exits with a non-zero code.
pub fn run(address: &str) -> ! {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => fail(
            Event::coded(Level::Error, HTTP0007, &[("cause", &error.to_string())])
                .with_cause(error),
        ),
    };
    runtime.block_on(async {
        match bind(address).await {
            Ok(listener) => serve(listener).await,
            Err(event) => fail(event),
        }
    })
}

/// Opens the listening socket, or the event explaining why it could not.
pub async fn bind(address: &str) -> Result<TcpListener, Event> {
    TcpListener::bind(address)
        .await
        .map_err(|error| listen_event(address, &error))
}

/// One code per cause, so that each fix matches its cause.
fn listen_event(address: &str, error: &io::Error) -> Event {
    let code = match error.kind() {
        io::ErrorKind::AddrInUse => HTTP0001,
        io::ErrorKind::PermissionDenied => HTTP0004,
        io::ErrorKind::AddrNotAvailable => HTTP0005,
        _ => HTTP0006,
    };
    Event::coded(Level::Error, code, &[("address", address)]).with_cause(error)
}

/// What an accept error means, and what to do next.
enum AcceptError {
    /// The client gave up while the connection was being opened: try the next one.
    FromClient(Event),
    /// The process is out of file descriptors: wait before accepting again.
    OutOfFiles(Event),
    /// Not recognized: report and keep accepting.
    Other(Event),
}

fn accept_error(error: &io::Error) -> AcceptError {
    if error.kind() == io::ErrorKind::ConnectionAborted {
        return AcceptError::FromClient(Event::coded(
            Level::Debug,
            HTTP0002,
            &[("cause", &error.to_string())],
        ));
    }
    if is_out_of_files(error) {
        return AcceptError::OutOfFiles(
            Event::coded(Level::Error, HTTP0008, &[]).with_cause(error),
        );
    }
    AcceptError::Other(
        Event::coded(Level::Error, EXT0001, &[("cause", &error.to_string())]).with_cause(error),
    )
}

/// `EMFILE` (per process) or `ENFILE` (system-wide). The standard library has no `ErrorKind`
/// for them, so they are recognized by their OS code.
fn is_out_of_files(error: &io::Error) -> bool {
    #[cfg(unix)]
    const CODES: &[i32] = &[24, 23];
    #[cfg(windows)]
    const CODES: &[i32] = &[10024];
    #[cfg(not(any(unix, windows)))]
    const CODES: &[i32] = &[];
    error
        .raw_os_error()
        .is_some_and(|code| CODES.contains(&code))
}

/// Reports a repeated error at most once per period, with how many times it happened.
struct Throttle {
    last_report: Option<Instant>,
    unreported: u32,
}

impl Throttle {
    fn report(&mut self, event: Event) {
        if let Some(event) = self.admit(event, Instant::now()) {
            event.emit();
        }
    }

    /// The event to show now, if any, carrying the count of those held back since the last one.
    fn admit(&mut self, event: Event, now: Instant) -> Option<Event> {
        if self
            .last_report
            .is_some_and(|last| now - last < REPORT_PERIOD)
        {
            self.unreported += 1;
            return None;
        }
        let event = match self.unreported {
            0 => event,
            n => {
                let cause = event.cause.clone().unwrap_or_default();
                event.with_cause(format!("{cause}, {n} more times since the last report"))
            }
        };
        self.last_report = Some(now);
        self.unreported = 0;
        Some(event)
    }
}

/// Accepts connections forever. Each connection, and each request, runs in its own task:
/// a client that disconnects does not cancel the handler.
pub async fn serve(listener: TcpListener) -> ! {
    let mut throttle = Throttle {
        last_report: None,
        unreported: 0,
    };
    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(accepted) => accepted,
            Err(error) => {
                match accept_error(&error) {
                    AcceptError::FromClient(event) => event.emit(),
                    AcceptError::OutOfFiles(event) => {
                        throttle.report(event);
                        tokio::time::sleep(ACCEPT_BACKOFF).await;
                    }
                    AcceptError::Other(event) => throttle.report(event),
                }
                continue;
            }
        };
        tokio::spawn(async move {
            let service = service_fn(|request| async move {
                Ok::<_, Infallible>(match tokio::spawn(handle(request)).await {
                    Ok(response) => response,
                    // Panics become proper crash reports later; for now, a plain 500.
                    Err(_) => {
                        error_response(StatusCode::INTERNAL_SERVER_ERROR, EXT0001, "Internal error")
                    }
                })
            });
            if let Err(error) = http1::Builder::new()
                .serve_connection(TokioIo::new(stream), service)
                .await
            {
                connection_event(&error, peer).emit();
            }
        });
    }
}

async fn handle(request: Request<Incoming>) -> Response<Full<Bytes>> {
    let method = request.method().to_string();
    let path = request.uri().path().to_string();
    let message = HTTP0003.format_message(&[("method", &method), ("path", &path)]);
    error_response(StatusCode::NOT_FOUND, HTTP0003, &message)
}

/// The single error body ([engine.md](docs/engine.md#response-format)).
fn error_response(
    status: StatusCode,
    code: &balise_core::error_codes::ErrorCode,
    message: &str,
) -> Response<Full<Bytes>> {
    let body = format!(
        r#"{{"error":{{"code":"{}","message":"{}"}}}}"#,
        code.code(),
        json_escape(message)
    );
    let mut response = Response::new(Full::new(Bytes::from(body)));
    *response.status_mut() = status;
    response.headers_mut().insert(
        hyper::header::CONTENT_TYPE,
        hyper::header::HeaderValue::from_static("application/json"),
    );
    response
}

/// Classifies a connection error: caused by the client (`debug`), or not recognized.
fn connection_event(error: &hyper::Error, peer: SocketAddr) -> Event {
    let from_client = error.is_parse()
        || error.is_incomplete_message()
        || error.is_closed()
        || error.is_canceled()
        || error.is_body_write_aborted()
        || error.is_timeout();
    if from_client {
        Event::coded(
            Level::Debug,
            HTTP0002,
            &[("cause", &format!("{error} ({peer})"))],
        )
    } else {
        Event::coded(Level::Error, EXT0001, &[("cause", &error.to_string())]).with_cause(error)
    }
}

fn json_escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            c if c < ' ' => escaped.push_str(&format!("\\u{:04x}", c as u32)),
            c => escaped.push(c),
        }
    }
    escaped
}

fn fail(event: Event) -> ! {
    event.emit();
    std::process::exit(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_listen_cause_has_its_own_code() {
        let code = |kind| {
            listen_event("0.0.0.0:80", &io::Error::from(kind))
                .code
                .unwrap()
                .code()
        };
        assert_eq!(code(io::ErrorKind::AddrInUse), "HTTP0001");
        assert_eq!(code(io::ErrorKind::PermissionDenied), "HTTP0004");
        assert_eq!(code(io::ErrorKind::AddrNotAvailable), "HTTP0005");
        assert_eq!(code(io::ErrorKind::Other), "HTTP0006");
    }

    #[test]
    #[cfg(unix)]
    fn out_of_files_is_recognized() {
        let error = io::Error::from_raw_os_error(24);
        assert!(matches!(accept_error(&error), AcceptError::OutOfFiles(_)));
    }

    #[test]
    fn an_aborted_connection_is_the_client() {
        let error = io::Error::from(io::ErrorKind::ConnectionAborted);
        assert!(
            matches!(accept_error(&error), AcceptError::FromClient(event) if event.level == Level::Debug)
        );
    }

    #[test]
    fn a_repeated_error_is_reported_once_per_period_with_a_count() {
        let mut throttle = Throttle {
            last_report: None,
            unreported: 0,
        };
        let event = || Event::coded(Level::Error, HTTP0008, &[]).with_cause("os error 24");
        let start = Instant::now();

        assert!(throttle.admit(event(), start).is_some());
        assert!(
            throttle
                .admit(event(), start + Duration::from_millis(100))
                .is_none()
        );
        assert!(
            throttle
                .admit(event(), start + Duration::from_millis(200))
                .is_none()
        );
        let next = throttle.admit(event(), start + REPORT_PERIOD).unwrap();
        assert_eq!(
            next.cause.as_deref(),
            Some("os error 24, 2 more times since the last report")
        );
    }

    #[test]
    fn escapes_json_strings() {
        assert_eq!(json_escape("a\"b\\c\nd\u{1}"), "a\\\"b\\\\c\\nd\\u0001");
    }
}
