//! HTTP engine wrapper: startup, shutdown, error capture.
//!
//! hyper and Tokio are used here and nowhere else. Nothing from them crosses this crate's
//! boundary into the facade: errors leave as Balise events with their codes.

mod start;

pub use start::start;

use std::convert::Infallible;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use balise_core::config::Config;
use balise_core::error::Error;
use balise_core::error_codes;
use balise_core::error_codes::{
    EXT0001, HTTP0001, HTTP0002, HTTP0003, HTTP0004, HTTP0005, HTTP0006, HTTP0007, HTTP0008,
    HTTP0009, HTTP0012,
};
use balise_core::event::{Event, Level, RequestLine};
use balise_core::handler::{self, Method};
use balise_core::log::{self, RequestContext};
use balise_core::registry::{self, Route};
use balise_core::router::{Match, Router};
use balise_core::text::format_size;
use bytes::Bytes;
use http_body_util::{BodyExt, Full, LengthLimitError, Limited};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

/// How long to wait before accepting again when the process is out of file descriptors.
const ACCEPT_BACKOFF: Duration = Duration::from_millis(100);

/// At most one report per period for the same accept error, with the number of occurrences.
const REPORT_PERIOD: Duration = Duration::from_secs(1);

/// Builds the router from the registered routes, starts the runtime and serves. Never
/// returns: on a fatal error, reports it and exits with a non-zero code.
pub fn run(config: Config) -> ! {
    if let Err(event) = error_codes::check_app_codes() {
        fail(event);
    }
    let server = match Router::build(registry::routes(), &config.api_base()) {
        Ok(router) => Arc::new(Server::new(router, &config)),
        Err(event) => fail(event),
    };
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
        match bind(&config.address()).await {
            Ok(listener) => serve(listener, server).await,
            Err(event) => fail(event),
        }
    })
}

/// Opens the listening socket, or the event explaining why it could not.
#[allow(clippy::result_large_err)] // startup only: the size costs nothing
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
/// What every request needs: the routes, and the limits from configuration.
#[derive(Debug)]
pub struct Server {
    router: Router,
    body_size: usize,
    error_details: bool,
}

impl Server {
    pub fn new(router: Router, config: &Config) -> Server {
        Server {
            router,
            body_size: config.body_size,
            error_details: config.error_details,
        }
    }
}

pub async fn serve(listener: TcpListener, server: Arc<Server>) -> ! {
    log::set_context(current_request);
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
        let server = server.clone();
        tokio::spawn(async move {
            let service = service_fn(move |request| {
                let server = server.clone();
                async move {
                    let started = Instant::now();
                    let request_id = request_id();
                    let method = request.method().to_string();
                    let path = request.uri().path().to_string();
                    let details = server.error_details;
                    let task = tokio::spawn(handle(server, request, request_id.clone()));
                    let (mut response, route) = match task.await {
                        Ok(handled) => handled,
                        // Panics become proper crash reports later; for now, a plain 500.
                        Err(error) => {
                            let cause = match error.try_into_panic() {
                                Ok(panic) => panic_message(&*panic),
                                Err(error) => error.to_string(),
                            };
                            let error =
                                Error::coded(500, EXT0001, &[("cause", &cause)]).with_cause(cause);
                            (error.into_response(&request_id, details), None)
                        }
                    };
                    let line = RequestLine {
                        method,
                        path,
                        status: response.status,
                        duration_us: started.elapsed().as_micros() as u64,
                        queries: None,
                    };
                    log_request(line, &request_id, route, &mut response);
                    Ok::<_, Infallible>(to_hyper(response))
                }
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

/// Logs a request: its error first when it is a `5xx`, then its summary.
fn log_request(
    line: RequestLine,
    request_id: &str,
    route: Option<&'static Route>,
    response: &mut handler::Response,
) {
    let handler = route.map(Route::handler_name);
    let error = response.error.take();
    if let Some(error) = &error
        && error.status() >= 500
    {
        let mut event = error.event();
        event.handler = handler.clone();
        event.request_id = Some(request_id.to_string());
        event.emit();
    }
    let mut summary = Event::request(line, request_id);
    summary.handler = handler;
    if let Some(error) = &error {
        error.summarize(&mut summary);
    }
    summary.emit();
}

/// The text a panic was raised with.
fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    match (panic.downcast_ref::<&str>(), panic.downcast_ref::<String>()) {
        (Some(text), _) => format!("the handler panicked: {text}"),
        (_, Some(text)) => format!("the handler panicked: {text}"),
        _ => "the handler panicked".to_string(),
    }
}

tokio::task_local! {
    /// The request the current task runs, for application logs.
    static REQUEST: (String, &'static Route);
}

fn current_request() -> Option<RequestContext> {
    REQUEST
        .try_with(|(request_id, route)| RequestContext {
            request_id: request_id.clone(),
            handler: route.handler_name(),
        })
        .ok()
}

/// Routes and runs a request; the route is returned for the logs.
async fn handle(
    server: Arc<Server>,
    request: Request<Incoming>,
    request_id: String,
) -> (handler::Response, Option<&'static Route>) {
    let details = server.error_details;
    let fail = |error: Error| (error.into_response(&request_id, details), None);
    let (parts, body) = request.into_parts();
    let method = Method::parse(parts.method.as_str());
    let path = parts.uri.path();
    match server.router.find(method, path) {
        Match::Found {
            route,
            params,
            path,
        } => {
            let body = match Limited::new(body, server.body_size).collect().await {
                Ok(collected) => collected.to_bytes().to_vec(),
                Err(error) if error.is::<LengthLimitError>() => {
                    let limit = format_size(server.body_size);
                    let error = Error::coded(413, HTTP0012, &[("limit", &limit)]);
                    return (error.into_response(&request_id, details), Some(route));
                }
                // The client closed the connection or sent a malformed body: it will most
                // likely never read this answer.
                Err(error) => {
                    let error = Error::coded(400, HTTP0002, &[("cause", &error.to_string())]);
                    return (error.into_response(&request_id, details), Some(route));
                }
            };
            let headers = parts
                .headers
                .iter()
                .filter_map(|(name, value)| {
                    Some((name.as_str().to_string(), value.to_str().ok()?.to_string()))
                })
                .collect();
            let request = handler::Request {
                method: route.method,
                path,
                params,
                query: parts.uri.query().unwrap_or_default().to_string(),
                headers,
                body,
                request_id: request_id.clone(),
                error_details: details,
            };
            let response = REQUEST
                .scope((request_id, route), (route.invoke)(request))
                .await;
            (response, Some(route))
        }
        Match::MethodNotAllowed { allowed } => {
            let allowed: Vec<&str> = allowed.iter().map(|m| m.as_str()).collect();
            let allowed = allowed.join(", ");
            let (mut response, route) = fail(Error::coded(
                405,
                HTTP0009,
                &[
                    ("method", parts.method.as_str()),
                    ("path", path),
                    ("allowed", &allowed),
                ],
            ));
            response.headers.push(("allow", allowed));
            (response, route)
        }
        Match::NotFound => fail(Error::coded(
            404,
            HTTP0003,
            &[("method", parts.method.as_str()), ("path", path)],
        )),
    }
}

/// A short identifier for a request, as logs show it: `req_7f3a2c`. Unique enough to find a
/// request in the logs, not a secret.
fn request_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    static SEED: OnceLock<u64> = OnceLock::new();
    let seed = *SEED.get_or_init(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64)
    });
    // SplitMix64 over a counter: consecutive requests get unrelated-looking ids.
    let mut x = seed.wrapping_add(
        COUNTER
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_mul(0x9E37_79B9_7F4A_7C15),
    );
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    format!("req_{:06x}", x & 0xFF_FFFF)
}

fn to_hyper(response: handler::Response) -> Response<Full<Bytes>> {
    let mut hyper_response = Response::new(Full::new(Bytes::from(response.body)));
    *hyper_response.status_mut() =
        StatusCode::from_u16(response.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    for (name, value) in response.headers {
        if let Ok(value) = hyper::header::HeaderValue::from_str(&value) {
            hyper_response
                .headers_mut()
                .insert(hyper::header::HeaderName::from_static(name), value);
        }
    }
    hyper_response
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
}
