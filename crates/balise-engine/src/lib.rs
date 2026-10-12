//! HTTP engine wrapper: startup, shutdown, error capture.
//!
//! hyper and Tokio are used here and nowhere else. Nothing from them crosses this crate's
//! boundary into the facade: errors leave as Balise events with their codes.

mod shutdown;
mod start;
mod tasks;

pub use shutdown::signal;
pub use start::start;
pub use tasks::{Shared, SharedRead, SharedWrite, blocking, spawn};

use std::convert::Infallible;
use std::future::{Future, poll_fn};
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::pin::pin;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::task::{Context, Poll};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use balise_core::body::{BodySource, Progress, RequestBody};
use balise_core::config::Config;
use balise_core::error::Error;
use balise_core::error_codes;
use balise_core::error_codes::{
    EXT0001, HTTP0001, HTTP0002, HTTP0003, HTTP0004, HTTP0005, HTTP0006, HTTP0007, HTTP0008,
    HTTP0012, HTTP0013, RUN0001,
};
use balise_core::event::{Event, Level, RequestLine};
use balise_core::handler::{self, Method};
use balise_core::limits::RouteLimits;
use balise_core::log::{self, RequestContext};
use balise_core::panic::Catch;
use balise_core::registry::{self, Route};
use balise_core::router::{HEALTH_PATHS, Match, Router};
use balise_core::state;
use balise_core::text::format_duration;
use bytes::Bytes;
use http_body_util::Full;
use hyper::body::{Body as _, Incoming};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use hyper_util::server::graceful::GracefulShutdown;
use tokio::net::TcpListener;

/// How long to wait before accepting again when the process is out of file descriptors.
const ACCEPT_BACKOFF: Duration = Duration::from_millis(100);

/// At most one report per period for the same accept error, with the number of occurrences.
const REPORT_PERIOD: Duration = Duration::from_secs(1);

/// Builds the router from the registered routes, starts the runtime and serves, with each
/// route's limits, until `SIGTERM` or Ctrl-C. Never returns: exits with `0` after a graceful
/// shutdown, and on a fatal error, reports it and exits with a non-zero code.
pub fn run(config: Config, limits: RouteLimits) -> ! {
    if let Err(event) = error_codes::check_app_codes() {
        fail(event);
    }
    balise_core::file::set_storage(&config.storage_dir);
    let server = match Router::build(registry::routes(), &config.api_base()) {
        Ok(router) => Arc::new(Server::with_limits(router, &config, limits)),
        Err(event) => fail(event),
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => fail(
            Event::coded(Level::Error, RUN0001, &[("cause", &error.to_string())]).with_cause(error),
        ),
    };
    runtime.block_on(async {
        for crash in balise_core::panic::previous_crashes() {
            crash.emit();
        }
        if let Err(event) = state::create(&state::STATES, registry::routes()).await {
            fail(event);
        }
        match bind(&config.address()).await {
            Ok(listener) => serve(listener, server, shutdown::signal()).await,
            Err(event) => fail(event),
        }
    });
    // Tasks still running past the shutdown timeout stop here.
    std::process::exit(0)
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
            Event::coded(Level::Error, HTTP0007, &[]).with_cause(error),
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

/// What every request needs: the routes, their limits, and the state of the server.
#[derive(Debug)]
pub struct Server {
    router: Router,
    limits: RouteLimits,
    error_details: bool,
    shutdown_timeout: Duration,
    /// What `/ready` answers: true while connections are accepted.
    ready: AtomicBool,
}

impl Server {
    /// A server whose routes have the limits their declarations and `config` give, without
    /// `budgets.toml`.
    pub fn new(router: Router, config: &Config) -> Server {
        // Without budgets, nothing can be wrong.
        let limits = RouteLimits::resolve(registry::routes(), config, "")
            .unwrap_or_else(|_| unreachable!("limits without budgets always resolve"));
        Server::with_limits(router, config, limits)
    }

    pub fn with_limits(router: Router, config: &Config, limits: RouteLimits) -> Server {
        Server {
            router,
            limits,
            error_details: config.error_details,
            shutdown_timeout: config.shutdown_timeout,
            ready: AtomicBool::new(false),
        }
    }
}

/// The body of a hyper request, read by Balise's extractors.
struct HyperBody(Incoming);

impl BodySource for HyperBody {
    fn poll_chunk(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Vec<u8>, String>>> {
        loop {
            return match Pin::new(&mut self.0).poll_frame(cx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(None) => Poll::Ready(None),
                Poll::Ready(Some(Err(error))) => Poll::Ready(Some(Err(error.to_string()))),
                Poll::Ready(Some(Ok(frame))) => match frame.into_data() {
                    Ok(data) => Poll::Ready(Some(Ok(data.to_vec()))),
                    // Trailers carry no body.
                    Err(_) => continue,
                },
            };
        }
    }
}

/// Accepts connections until `shutdown` resolves, then waits for what is in progress (see
/// [`shutdown`]). Each connection, and each request, runs in its own task: a client that
/// disconnects does not cancel the handler.
pub async fn serve(listener: TcpListener, server: Arc<Server>, shutdown: impl Future<Output = ()>) {
    log::set_context(current_request);
    let mut throttle = Throttle {
        last_report: None,
        unreported: 0,
    };
    let connections = GracefulShutdown::new();
    let mut shutdown = pin!(shutdown);
    server.ready.store(true, Ordering::SeqCst);
    loop {
        let accepted = poll_fn(|cx| match shutdown.as_mut().poll(cx) {
            Poll::Ready(()) => Poll::Ready(None),
            Poll::Pending => listener.poll_accept(cx).map(Some),
        })
        .await;
        let (stream, peer) = match accepted {
            None => break,
            Some(Ok(accepted)) => accepted,
            Some(Err(error)) => {
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
        let service = service_fn(move |request| respond(server.clone(), request));
        let connection = http1::Builder::new().serve_connection(TokioIo::new(stream), service);
        // Closed after its request in progress once the shutdown starts.
        let connection = Catch(Box::pin(connections.watch(connection)));
        tokio::spawn(async move {
            match connection.await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => connection_event(&error, peer).emit(),
                Err(report) => report.event("A connection").emit(),
            }
        });
    }
    server.ready.store(false, Ordering::SeqCst);
    drop(listener);
    shutdown::drain(server.shutdown_timeout, connections).await;
}

/// Answers one request, and logs it.
async fn respond(
    server: Arc<Server>,
    request: Request<Incoming>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    let started = Instant::now();
    let request_id = request_id();
    let method = request.method().to_string();
    let path = request.uri().path().to_string();
    let details = server.error_details;
    let task = tokio::spawn(Catch(Box::pin(handle(server, request, request_id.clone()))));
    let (mut response, route) = match task.await {
        Ok(Ok(handled)) => handled,
        // The handler's own panic was answered inside; this one is Balise's.
        Ok(Err(report)) => {
            let error = Error::panicked("Balise's request handling", report);
            (error.into_response(&request_id, details), None)
        }
        Err(error) => {
            let cause = error.to_string();
            let error = Error::coded(500, EXT0001, &[("cause", &cause)]).with_cause(cause);
            (error.into_response(&request_id, details), None)
        }
    };
    let health_check = method == "GET" && HEALTH_PATHS.contains(&path.as_str());
    let line = RequestLine {
        method,
        path,
        status: response.status,
        duration_us: started.elapsed().as_micros() as u64,
        queries: None,
    };
    log_request(line, &request_id, route, &mut response, health_check);
    Ok(to_hyper(response))
}

/// `/health` and `/ready`, for orchestrators.
fn health(server: &Server, path: &str) -> Option<handler::Response> {
    let [health, ready] = HEALTH_PATHS;
    let answer = |status, state: &str| {
        Some(handler::Response::json(
            status,
            format!(r#"{{"status":"{state}"}}"#),
        ))
    };
    match path {
        p if p == health => answer(200, "ok"),
        p if p == ready && server.ready.load(Ordering::SeqCst) => answer(200, "ready"),
        p if p == ready => answer(503, "stopping"),
        _ => None,
    }
}

/// Logs a request: its error first when it is a `5xx`, then its summary.
fn log_request(
    line: RequestLine,
    request_id: &str,
    route: Option<&'static Route>,
    response: &mut handler::Response,
    health_check: bool,
) {
    let handler = match health_check {
        true => Some("health check".to_string()),
        false => route.map(Route::handler_name),
    };
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
    // Orchestrators ask every few seconds: their requests would drown the others.
    if health_check {
        summary.level = Level::Debug;
    }
    summary.handler = handler;
    if let Some(error) = &error {
        error.summarize(&mut summary);
    }
    summary.emit();
}

tokio::task_local! {
    /// The request the current task runs, for application logs.
    pub(crate) static REQUEST: (String, &'static Route);
}

pub(crate) fn current_request() -> Option<RequestContext> {
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
    let _handling = tasks::REQUESTS.enter();
    let details = server.error_details;
    let fail = |error: Error| (error.into_response(&request_id, details), None);
    let (parts, body) = request.into_parts();
    let method = Method::parse(parts.method.as_str());
    let path = parts.uri.path();
    if method == Some(Method::Get)
        && let Some(response) = health(&server, path)
    {
        return (response, None);
    }
    match server.router.find(method, path) {
        Match::Found {
            route,
            params,
            path,
        } => {
            let limits = server.limits.get(route);
            let body = RequestBody::new(HyperBody(body), limits.body_size);
            let progress = body.progress();
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
                limits,
                request_id: request_id.clone(),
                error_details: details,
            };
            let handler = Catch(tasks::watch((route.invoke)(request), "a handler"));
            let handler = REQUEST.scope((request_id.clone(), route), handler);
            // A timeout cancels the handler, reading of the body included.
            let response = match tokio::time::timeout(limits.timeout, handler).await {
                Ok(Ok(response)) => response,
                Ok(Err(report)) => {
                    let what = format!("`{}`", route.handler_name());
                    Error::panicked(&what, report).into_response(&request_id, details)
                }
                Err(_) => {
                    let timeout = format_duration(limits.timeout);
                    let handler = route.handler_name();
                    let args = [("timeout", timeout.as_str()), ("handler", handler.as_str())];
                    let error = match progress.get() {
                        Progress::Reading => Error::coded(408, HTTP0012, &args),
                        Progress::Waiting | Progress::Done => Error::coded(503, HTTP0013, &args),
                    };
                    error.into_response(&request_id, details)
                }
            };
            (response, Some(route))
        }
        Match::MethodNotAllowed { allowed } => {
            let allowed: Vec<&str> = allowed.iter().map(|m| m.as_str()).collect();
            let allowed = allowed.join(", ");
            let (mut response, route) = fail(Error::coded(
                405,
                HTTP0008,
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
        let event = || Event::coded(Level::Error, HTTP0007, &[]).with_cause("os error 24");
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
