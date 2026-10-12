//! The error bodies of `mockup/outputs/` are what the renderer writes, key order included.
//!
//! The renderer builds each case with the values written in the file. The `DB` codes do not
//! exist yet: stand-ins carry their message and fix. The same files are checked end to end on
//! the mock-up's routes once the ORM exists.

use std::path::Path;

use balise_core::error::Error;
use balise_core::error_codes::ErrorCode;

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
    Error::coded(404, &DB0042, &[]).at("src/orders/mod.rs", 26)
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
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../mockup/outputs")
        .join(name);
    let text = std::fs::read_to_string(&path).unwrap();
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
