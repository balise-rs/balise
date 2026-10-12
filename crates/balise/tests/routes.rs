//! Routes declared with attributes are collected, matched and called with their extractors.

use std::sync::Arc;

use balise::__private::routes;
use balise::prelude::*;
use balise_core::config::Config;
use balise_core::router::Router;
use balise_engine::Server;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[derive(Output)]
struct Text {
    text: String,
}

/// `{"note": "…"}`, or `?note=…`.
#[derive(Input)]
struct Note {
    note: String,
}

#[get("/orders/{id}")]
async fn get(id: Path<u32>) -> Text {
    // `id` is the value itself: a `u32`, not a `Path<u32>`.
    Text {
        text: format!("order {}", id + 1),
    }
}

#[get("/shops/{shop_id}/orders/{id}")]
async fn get_in_shop((shop_id, id): Path<(String, u32)>) -> Text {
    Text {
        text: format!("order {id} in {shop_id}"),
    }
}

#[get("/orders")]
async fn list(filters: Query<Note>, request_id: RequestId) -> Text {
    Text {
        text: format!(
            "{} {}",
            filters.note,
            request_id.as_str().starts_with("req_")
        ),
    }
}

#[post("/orders")]
async fn create(input: Body<Note>) -> Text {
    let note: String = input.note; // moved out: no `into_inner()`, no `.clone()`
    Text { text: note }
}

async fn request(address: std::net::SocketAddr, raw_head: &str, body: &str) -> String {
    let mut stream = TcpStream::connect(address).await.unwrap();
    let raw = format!(
        "{raw_head}\r\nHost: test\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(raw.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    response
}

async fn start() -> std::net::SocketAddr {
    let config = Config::default();
    let server = Server::new(
        Router::build(routes(), &config.api_base()).unwrap(),
        &config,
    );
    let listener = balise_engine::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(balise_engine::serve(
        listener,
        Arc::new(server),
        std::future::pending(),
    ));
    address
}

#[test]
fn attributes_register_their_routes() {
    let mut names: Vec<String> = routes().iter().map(|r| r.handler_name()).collect();
    names.sort();
    assert_eq!(
        names,
        [
            "routes::create",
            "routes::get",
            "routes::get_in_shop",
            "routes::list"
        ]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn path_values_arrive_unwrapped_and_typed() {
    let address = start().await;

    let response = request(address, "GET /api/orders/41 HTTP/1.1", "").await;
    assert!(response.ends_with(r#"{"text":"order 42"}"#), "{response}");

    let response = request(address, "GET /api/shops/sho_1/orders/7 HTTP/1.1", "").await;
    assert!(
        response.ends_with(r#"{"text":"order 7 in sho_1"}"#),
        "{response}"
    );

    let response = request(address, "GET /api/orders/abc HTTP/1.1", "").await;
    assert!(response.starts_with("HTTP/1.1 404"), "{response}");
    assert!(response.contains(r#""code":"HTTP0010""#), "{response}");
}

#[tokio::test(flavor = "multi_thread")]
async fn query_and_body_are_read_into_inputs() {
    let address = start().await;

    let response = request(address, "GET /api/orders?note=a+b HTTP/1.1", "").await;
    assert!(response.ends_with(r#"{"text":"a b true"}"#), "{response}");

    let response = request(address, "GET /api/orders HTTP/1.1", "").await;
    assert!(response.starts_with("HTTP/1.1 422"), "{response}");
    assert!(
        response.contains(
            r#""message":"Invalid fields: note","fields":{"note":{"code":"VAL0004","message":"is required"}}"#
        ),
        "{response}"
    );

    let json = "POST /api/orders HTTP/1.1\r\nContent-Type: application/json";
    let response = request(address, json, r#"{"note": "hello"}"#).await;
    assert!(response.ends_with(r#"{"text":"hello"}"#), "{response}");

    let response = request(address, json, r#"{"note": 3}"#).await;
    assert!(
        response.contains(r#"{"code":"VAL0005","message":"must be text, not `3`"}"#),
        "{response}"
    );

    let response = request(address, json, r#"{"note": "#).await;
    assert!(response.starts_with("HTTP/1.1 400"), "{response}");
    assert!(response.contains(r#""code":"VAL0002""#), "{response}");

    let form = "POST /api/orders HTTP/1.1\r\nContent-Type: text/plain";
    let response = request(address, form, "hello").await;
    assert!(response.starts_with("HTTP/1.1 415"), "{response}");
    assert!(response.contains(r#""code":"VAL0003""#), "{response}");

    let large = "x".repeat(Config::default().body_size + 1);
    let response = request(address, json, &large).await;
    assert!(response.starts_with("HTTP/1.1 413"), "{response}");
    let limit = balise_core::text::format_size(Config::default().body_size);
    assert!(
        response.contains(&format!(
            r#""code":"HTTP0011","message":"The request body is larger than this route's limit of {limit}""#
        )),
        "{response}"
    );
}
