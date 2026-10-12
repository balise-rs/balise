//! Each request is logged once, in Balise's format: a `5xx` with its error event first, a
//! `4xx` with its detail under the request line, application logs with their request.

use std::sync::{Arc, Mutex};

use balise::__private::routes;
use balise::prelude::*;
use balise_core::config::Config;
use balise_core::event::Level;
use balise_core::log::{self, Format, Logger, Sink};
use balise_core::router::Router;
use balise_engine::Server;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[derive(Output)]
struct OrderView {
    id: u32,
}

#[derive(Input)]
struct Reset {
    password: u32,
}

#[get("/orders/{id}")]
async fn get(id: Path<u32>) -> Result<OrderView> {
    balise::log::info!(order_id = id, token = "abc", "Order {id} read");
    let total: u32 = "x".parse()?;
    Ok(OrderView { id: total })
}

#[post("/reset")]
async fn reset(input: Query<Reset>) -> NoContent {
    balise::log::info!(pin = input.password, "Reset");
    NoContent
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

#[tokio::test(flavor = "multi_thread")]
async fn requests_are_logged_once_in_balise_format() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    log::install(Logger {
        format: Format::Text,
        min_level: Level::Info,
        color: false,
        sink: Sink::Memory(lines.clone()),
    });
    let config = Config::default();
    let server = Server::new(
        Router::build(routes(), &config.api_base()).unwrap(),
        &config,
    );
    let listener = balise_engine::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(balise_engine::serve(listener, Arc::new(server)));

    request(address, "GET", "/api/orders/7").await;
    let response = request(address, "POST", "/api/reset?password=hunter2").await;
    assert!(
        response.contains(r#""password":{"code":"VAL0005","message":"must be a whole number, 0 or more, not ***"}"#),
        "{response}"
    );
    request(address, "GET", "/api/missing").await;

    let logged = normalize(&lines.lock().unwrap().join("\n"));
    let expected = format!(
        r#"
<time>  Order 7 read
          at crates/balise/tests/logs.rs:{info_line} in logs::get, request <req>
          order_id: 7
          token: ***
<time> ERROR EXT0001  invalid digit found in string
         at crates/balise/tests/logs.rs:{parse_line} in logs::get, request <req>
         cause: invalid digit found in string
         fix: Read the original cause. If it comes from Balise or one of its dependencies, report it so it gets its own code.
         more: balise explain EXT0001
<time>  GET /api/orders/7 → request <req> (logs::get returned 500 in <ms> ms)
<time>  POST /api/reset → request <req> (logs::reset returned 422 in <ms> ms)
          VAL0001  Invalid fields: password
          in logs::reset
          fields:
            password: VAL0005 must be a whole number, 0 or more, not ***
          fix: Fix each field listed in `fields`: each one has its own code and message.
          more: balise explain VAL0001
<time>  GET /api/missing → request <req> (no handler, 404 in <ms> ms)
          HTTP0003  No route for GET /api/missing
          fix: Check the method and the path. `balise routes` lists the routes of the app.
          more: balise explain HTTP0003"#,
        info_line = line_of("balise::log::info!(order_id"),
        parse_line = line_of(r#"let total: u32 = "x".parse()?;"#),
    );
    assert_eq!(logged, expected.trim_start());
    assert!(!logged.contains("hunter2"));
}

/// The line of this file that contains `text`.
fn line_of(text: &str) -> usize {
    include_str!("logs.rs")
        .lines()
        .position(|line| line.contains(text))
        .unwrap()
        + 1
}

/// Times, request IDs and durations vary: they become `<time>`, `<req>` and `<ms>`.
fn normalize(text: &str) -> String {
    text.lines()
        .map(|line| {
            let mut line = line.to_string();
            if line.as_bytes().get(2) == Some(&b':') && line.as_bytes().get(5) == Some(&b':') {
                line.replace_range(..8, "<time>");
            }
            while let Some(start) = line.find("req_").filter(|_| !line.contains("req_<")) {
                line.replace_range(start..start + 10, "<req>");
            }
            if let Some(start) = line.find(" in ")
                && let Some(end) = line[start + 4..].find(" ms")
                && line[start + 4..start + 4 + end]
                    .bytes()
                    .all(|b| b.is_ascii_digit())
            {
                line.replace_range(start + 4..start + 4 + end, "<ms>");
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}
