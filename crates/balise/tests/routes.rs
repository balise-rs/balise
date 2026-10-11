//! Routes declared with attributes are collected, matched and called.
//!
//! Real extractors and responses come later; this test implements the two traits for its own
//! types, so the whole path (request, router, handler, response) is exercised.

use std::future::Future;
use std::sync::Arc;

use balise::__private::{FromRequest, IntoResponse, Request, Response, routes};
use balise_core::router::Router;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Reads the next path parameter. Named `Path` so the route macro counts it.
struct Path(String);

impl FromRequest for Path {
    fn from_request(request: &mut Request) -> impl Future<Output = Result<Self, Response>> + Send {
        let value = request.params.remove(0).1;
        async move { Ok(Path(value)) }
    }
}

struct Text(String);

impl IntoResponse for Text {
    fn into_response(self) -> Response {
        Response::json(200, format!("{:?}", self.0))
    }
}

#[balise::get("/orders/{id}")]
async fn get(id: Path) -> Text {
    Text(format!("order {}", id.0))
}

#[balise::get("/orders/{id}/lines/{line_id}")]
async fn lines(id: Path, line_id: Path) -> Text {
    Text(format!("line {} of {}", line_id.0, id.0))
}

#[balise::post("/orders")]
async fn create() -> Text {
    Text("created".into())
}

async fn request(address: std::net::SocketAddr, method: &str, path: &str) -> String {
    let mut stream = TcpStream::connect(address).await.unwrap();
    let raw = format!(
        "{method} {path} HTTP/1.1\r\nHost: test\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
    );
    stream.write_all(raw.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    response
}

#[test]
fn attributes_register_their_routes() {
    let mut names: Vec<String> = routes().iter().map(|r| r.handler_name()).collect();
    names.sort();
    assert_eq!(names, ["routes::create", "routes::get", "routes::lines"]);
    let get = routes().iter().find(|r| r.function == "get").unwrap();
    assert_eq!(get.path, "/orders/{id}");
    assert!(get.file.ends_with("routes.rs"));
}

#[tokio::test(flavor = "multi_thread")]
async fn requests_reach_their_handler_under_the_api_base() {
    let router = Arc::new(Router::build(routes(), "/api/v1").unwrap());
    let listener = balise_engine::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(balise_engine::serve(listener, router));

    let response = request(address, "GET", "/api/v1/orders/ord_1").await;
    assert!(response.starts_with("HTTP/1.1 200 OK\r\n"), "{response}");
    assert!(response.ends_with(r#""order ord_1""#), "{response}");

    let response = request(address, "GET", "/api/v1/orders/ord_1/lines/3").await;
    assert!(response.ends_with(r#""line 3 of ord_1""#), "{response}");

    let response = request(address, "DELETE", "/api/v1/orders").await;
    assert!(
        response.starts_with("HTTP/1.1 405 Method Not Allowed\r\n"),
        "{response}"
    );
    assert!(response.contains("allow: POST\r\n"), "{response}");
    assert!(response.contains(r#""code":"HTTP0009""#), "{response}");

    let response = request(address, "GET", "/api/orders/ord_1").await;
    assert!(
        response.starts_with("HTTP/1.1 404 Not Found\r\n"),
        "{response}"
    );
}
