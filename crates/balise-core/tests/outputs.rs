//! The error bodies and the logs of `mockup/outputs/` are what the renderers write, key order
//! included.
//!
//! Each case is built with the values written in the files. The `DB` codes do not
//! exist yet: stand-ins carry their message and fix. The same files are checked end to end on
//! the mock-up's routes once the ORM exists.

use std::path::Path;

use balise_core::error::Error;
use balise_core::error_codes::ErrorCode;
use balise_core::event::{Event, RequestLine};

const REQUEST_ID_SERVER_ERROR: &str = "req_7f3a2c";
const REQUEST_ID_NOT_FOUND: &str = "req_9b21e0";

static DB0003: ErrorCode = ErrorCode::app(
    "DB0003",
    "Database unreachable",
    "Cannot reach the database at db:5432",
    "Check that the database is running (`balise dev` starts it) and that DATABASE_URL points to it.",
    "",
);

static DB0042: ErrorCode = ErrorCode::app(
    "DB0042",
    "Row not found",
    "No Order with id ord_01JB8XQ5",
    "Use an id from GET /api/orders. If a missing row is expected, use `Order::find`, which returns an Option.",
    "",
);

fn server_error() -> Error {
    Error::coded(500, &DB0003, &[])
        .at("src/orders/mod.rs", 31)
        .with_cause("connection refused (os error 111), 3 attempts in 5 s")
}

fn not_found() -> Error {
    Error::coded(404, &DB0042, &[])
        .at("src/orders/mod.rs", 26)
        .with_detail("No Order with id ord_01JB8XQ5 for usr_01JB7K2M: it belongs to another user")
}

/// 2026-10-11 at this time of day, UTC, in milliseconds since the epoch.
fn at(hours: u64, minutes: u64, seconds: u64, millis: u64) -> u64 {
    1_791_676_800_000 + ((hours * 60 + minutes) * 60 + seconds) * 1000 + millis
}

/// The summary of a request, with its error if any, as the engine builds it.
fn summary(
    time_ms: u64,
    line: (&str, &str, u16, u64, u32),
    handler: &str,
    request_id: &str,
    error: Option<&Error>,
) -> Event {
    let (method, path, status, duration_ms, queries) = line;
    let duration_us = duration_ms * 1000;
    let line = RequestLine {
        method: method.into(),
        path: path.into(),
        status,
        duration_us,
        queries: Some(queries),
    };
    let mut event = Event::request(line, request_id);
    event.time_ms = time_ms;
    event.handler = Some(handler.into());
    if let Some(error) = error {
        error.summarize(&mut event);
    }
    event
}

fn server_error_events() -> Vec<Event> {
    let error = server_error();
    let mut event = error.event();
    event.time_ms = at(12, 2, 11, 478);
    event.handler = Some("orders::create".into());
    event.request_id = Some(REQUEST_ID_SERVER_ERROR.into());
    vec![
        summary(
            at(12, 1, 58, 120),
            ("GET", "/api/orders", 200, 4, 1),
            "orders::list",
            "req_51e07a",
            None,
        ),
        event,
        summary(
            at(12, 2, 11, 479),
            ("POST", "/api/orders", 500, 5004, 0),
            "orders::create",
            REQUEST_ID_SERVER_ERROR,
            Some(&error),
        ),
    ]
}

fn not_found_events() -> Vec<Event> {
    vec![summary(
        at(12, 3, 40, 201),
        ("GET", "/api/orders/ord_01JB8XQ5", 404, 3, 1),
        "orders::get",
        REQUEST_ID_NOT_FOUND,
        Some(&not_found()),
    )]
}

#[test]
fn server_error_logs_match_the_mockup() {
    let events = server_error_events();
    assert_eq!(text(&events), raw("1-server-error/dev.txt"));
    assert_eq!(json(&events), raw("1-server-error/prod.jsonl"));
}

#[test]
fn not_found_logs_match_the_mockup() {
    let events = not_found_events();
    assert_eq!(text(&events), raw("2-not-found/dev.txt"));
    assert_eq!(json(&events), raw("2-not-found/prod.jsonl"));
}

fn text(events: &[Event]) -> String {
    events.iter().map(|e| e.render_text(false) + "\n").collect()
}

fn json(events: &[Event]) -> String {
    events.iter().map(|e| e.render_json() + "\n").collect()
}

fn raw(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../mockup/outputs")
        .join(name);
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn server_error_bodies_match_the_mockup() {
    let error = server_error();
    let dev = error.render(REQUEST_ID_SERVER_ERROR, true);
    let prod = error.render(REQUEST_ID_SERVER_ERROR, false);
    assert_eq!(dev.status, 500);
    assert_eq!(prod.status, 500);
    assert_eq!(body(dev.body), file("1-server-error/response-dev.json"));
    assert_eq!(body(prod.body), file("1-server-error/response-prod.json"));
}

#[test]
fn not_found_bodies_match_the_mockup() {
    let error = not_found();
    let dev = error.render(REQUEST_ID_NOT_FOUND, true);
    let prod = error.render(REQUEST_ID_NOT_FOUND, false);
    assert_eq!(dev.status, 404);
    assert_eq!(body(dev.body), file("2-not-found/response-dev.json"));
    assert_eq!(body(prod.body), file("2-not-found/response-prod.json"));
}

fn body(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).unwrap()
}

/// The file's JSON without the whitespace between tokens: the bytes a response carries.
fn file(name: &str) -> String {
    let text = raw(name);
    let mut compact = String::with_capacity(text.len());
    let (mut in_string, mut escaped) = (false, false);
    for c in text.chars() {
        if in_string {
            compact.push(c);
            match c {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
        } else if !c.is_whitespace() {
            compact.push(c);
            in_string = c == '"';
        }
    }
    compact
}
