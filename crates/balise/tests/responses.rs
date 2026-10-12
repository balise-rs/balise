//! Return types decide the status; errors, from `AppError` or converted by `?`, use the single
//! error body, with details in dev only.

use std::sync::Arc;

use balise::__private::routes;
use balise::prelude::*;
use balise_core::config::Config;
use balise_core::error_codes;
use balise_core::router::Router;
use balise_engine::Server;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[derive(Output)]
struct OrderView {
    id: u32,
}

#[derive(AppError)]
enum OrderError {
    /// Only a pending order can be cancelled: a shipped order goes through a return instead.
    #[error(
        status = 409,
        code = "ORD0001",
        message = "Only pending orders can be cancelled",
        fix = "Check the order's status before cancelling it."
    )]
    NotPending,
    #[error(
        status = 422,
        code = "ORD0002",
        message = "An order has at most {max} lines",
        fix = "Split the order: at most {max} lines each."
    )]
    TooManyLines { max: u32 },
}

#[post("/orders")]
async fn create() -> Result<Created<OrderView>> {
    Ok(Created(OrderView { id: 1 }))
}

#[get("/orders")]
async fn list() -> Vec<OrderView> {
    vec![OrderView { id: 1 }, OrderView { id: 2 }]
}

#[delete("/orders/{id}")]
async fn delete(_id: Path<u32>) -> NoContent {
    NoContent
}

#[get("/orders/latest")]
async fn latest() -> Redirect {
    Redirect::to("/orders/2")
}

#[post("/orders/{id}/cancel")]
async fn cancel(id: Path<u32>) -> Result<OrderView> {
    if id == 2 {
        return Err(OrderError::NotPending.into());
    }
    Ok(OrderView { id })
}

#[post("/orders/{id}/lines")]
async fn add_line(_id: Path<u32>) -> Result<OrderView> {
    Err(OrderError::TooManyLines { max: 50 })?
}

#[get("/orders/{id}/total")]
async fn total(_id: Path<u32>) -> Result<OrderView> {
    let id: u32 = "not a number".parse()?;
    Ok(OrderView { id })
}

async fn start(error_details: bool) -> std::net::SocketAddr {
    let config = Config {
        error_details,
        ..Config::default()
    };
    let server = Server::new(
        Router::build(routes(), &config.api_base()).unwrap(),
        &config,
    );
    let listener = balise_engine::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(balise_engine::serve(listener, Arc::new(server)));
    address
}

async fn request(address: std::net::SocketAddr, method: &str, path: &str) -> String {
    let mut stream = TcpStream::connect(address).await.unwrap();
    let raw = format!(
        "{method} /api{path} HTTP/1.1\r\nHost: test\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
    );
    stream.write_all(raw.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    response
}

#[tokio::test(flavor = "multi_thread")]
async fn the_return_type_decides_the_status() {
    let address = start(false).await;

    let response = request(address, "POST", "/orders").await;
    assert!(response.starts_with("HTTP/1.1 201"), "{response}");
    assert!(response.ends_with(r#"{"id":1}"#), "{response}");

    let response = request(address, "GET", "/orders").await;
    assert!(response.ends_with(r#"[{"id":1},{"id":2}]"#), "{response}");

    let response = request(address, "DELETE", "/orders/1").await;
    assert!(response.starts_with("HTTP/1.1 204"), "{response}");
    assert!(response.ends_with("\r\n\r\n"), "{response}");

    let response = request(address, "GET", "/orders/latest").await;
    assert!(response.starts_with("HTTP/1.1 303"), "{response}");
    assert!(response.contains("location: /orders/2\r\n"), "{response}");
}

#[tokio::test(flavor = "multi_thread")]
async fn app_errors_answer_with_their_status_code_and_message() {
    let address = start(false).await;

    let response = request(address, "POST", "/orders/1/cancel").await;
    assert!(response.ends_with(r#"{"id":1}"#), "{response}");

    let response = request(address, "POST", "/orders/2/cancel").await;
    assert!(response.starts_with("HTTP/1.1 409"), "{response}");
    assert!(
        response.contains(
            r#"{"error":{"code":"ORD0001","message":"Only pending orders can be cancelled","request_id":"req_"#
        ),
        "{response}"
    );

    let response = request(address, "POST", "/orders/2/lines").await;
    assert!(response.starts_with("HTTP/1.1 422"), "{response}");
    assert!(
        response.contains(r#""message":"An order has at most 50 lines""#),
        "{response}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn dev_adds_location_fix_and_cause_and_prod_hides_a_5xx() {
    let prod = start(false).await;
    let response = request(prod, "GET", "/orders/1/total").await;
    assert!(response.starts_with("HTTP/1.1 500"), "{response}");
    assert!(
        response.contains(r#"{"error":{"code":"HTTP0500","message":"Internal error","request_id""#),
        "{response}"
    );

    let dev = start(true).await;
    let response = request(dev, "GET", "/orders/1/total").await;
    assert!(response.starts_with("HTTP/1.1 500"), "{response}");
    let line = line_of(r#"let id: u32 = "not a number".parse()?;"#);
    assert!(
        response.contains(&format!(
            r#""code":"EXT0001","message":"invalid digit found in string","location":"crates/balise/tests/responses.rs:{line}","fix":"#
        )),
        "{response}"
    );
    assert!(
        response.contains(r#""cause":"invalid digit found in string""#),
        "{response}"
    );

    let response = request(dev, "POST", "/orders/2/cancel").await;
    let line = line_of("return Err(OrderError::NotPending.into());");
    assert!(
        response.contains(&format!(
            r#""location":"crates/balise/tests/responses.rs:{line}","fix":"Check the order's status before cancelling it.""#
        )),
        "{response}"
    );
}

/// The line of this file that contains `text`.
fn line_of(text: &str) -> usize {
    include_str!("responses.rs")
        .lines()
        .position(|line| line.contains(text))
        .unwrap()
        + 1
}

#[test]
fn app_codes_are_registered_with_their_explanation() {
    let code = error_codes::find("ORD0001").unwrap();
    assert_eq!(code.title(), "OrderError::NotPending");
    assert_eq!(
        code.explanation(),
        "Only a pending order can be cancelled: a shipped order goes through a return instead."
    );
    assert_eq!(code.fix(), "Check the order's status before cancelling it.");
    assert_eq!(
        error_codes::find("ORD0002").unwrap().message(),
        "An order has at most {max} lines"
    );
    assert!(error_codes::check_app_codes().is_ok());
}
