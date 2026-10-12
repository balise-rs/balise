//! The engine serves HTTP and reports its own errors with Balise codes.

use balise_core::error_codes::HTTP0001;
use std::sync::Arc;

use balise_core::config::Config;
use balise_core::event::Level;
use balise_core::router::Router;
use balise_engine::Server;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

async fn request(address: std::net::SocketAddr, raw: &str) -> String {
    let mut stream = TcpStream::connect(address).await.unwrap();
    stream.write_all(raw.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    response
}

#[tokio::test(flavor = "multi_thread")]
async fn unknown_route_answers_404_with_the_error_body() {
    let listener = balise_engine::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = Server::new(Router::build(&[], "/api").unwrap(), &Config::default());
    tokio::spawn(balise_engine::serve(
        listener,
        Arc::new(server),
        std::future::pending(),
    ));

    let response = request(
        address,
        "GET /api/orders HTTP/1.1\r\nHost: test\r\nConnection: close\r\n\r\n",
    )
    .await;

    assert!(
        response.starts_with("HTTP/1.1 404 Not Found\r\n"),
        "{response}"
    );
    assert!(
        response.contains("content-type: application/json\r\n"),
        "{response}"
    );
    assert!(
        response.contains(
            r#"{"error":{"code":"HTTP0003","message":"No route for GET /api/orders","request_id":"req_"#
        ),
        "{response}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_port_in_use_is_reported_as_http0001() {
    let taken = balise_engine::bind("127.0.0.1:0").await.unwrap();
    let address = taken.local_addr().unwrap().to_string();

    let event = balise_engine::bind(&address).await.unwrap_err();

    assert_eq!(event.level, Level::Error);
    assert_eq!(event.code, Some(HTTP0001));
    assert_eq!(
        event.message,
        format!("Cannot listen on {address}: the port is already in use")
    );
    assert!(event.cause.is_some());
}
