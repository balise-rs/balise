//! A panic fails its request or task only; `/health` and `/ready` answer orchestrators; a
//! shutdown waits for what is in progress, for `shutdown.timeout` at most.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use balise::__private::routes;
use balise::prelude::*;
use balise_core::config::{Config, Environment};
use balise_core::event::Level;
use balise_core::log::{self, Format, Logger, Sink};
use balise_core::router::Router;
use balise_engine::Server;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::oneshot;

#[derive(Output)]
struct Text {
    text: String,
}

#[get("/boom")]
async fn boom() -> Text {
    let quantity: u32 = "many".parse().unwrap();
    Text {
        text: quantity.to_string(),
    }
}

#[get("/later")]
async fn later() -> NoContent {
    balise::spawn(async move || {
        panic!("task failed");
    });
    NoContent
}

#[get("/slow")]
async fn slow() -> Text {
    tokio::time::sleep(Duration::from_millis(300)).await;
    Text {
        text: "done".into(),
    }
}

async fn request(address: SocketAddr, path: &str) -> String {
    let mut stream = TcpStream::connect(address).await.unwrap();
    let raw = format!("GET {path} HTTP/1.1\r\nHost: test\r\nConnection: close\r\n\r\n");
    stream.write_all(raw.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    response
}

/// A server that stops when the sender is used, and the task that serves.
async fn start(
    shutdown_timeout: Duration,
) -> (SocketAddr, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let mut config = Config::defaults(Environment::Dev);
    config.shutdown_timeout = shutdown_timeout;
    let server = Server::new(
        Router::build(routes(), &config.api_base()).unwrap(),
        &config,
    );
    let listener = balise_engine::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = oneshot::channel::<()>();
    let serving = tokio::spawn(balise_engine::serve(listener, Arc::new(server), async {
        let _ = stopped.await;
    }));
    (address, stop, serving)
}

#[tokio::test(flavor = "multi_thread")]
async fn panics_health_and_shutdown() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    log::install(Logger {
        format: Format::Text,
        min_level: Level::Info,
        color: false,
        sink: Sink::Memory(lines.clone()),
    });
    let storage = std::env::temp_dir().join("balise-lifecycle-tests");
    balise_core::file::set_storage(&storage.display().to_string());
    balise_core::panic::install("lifecycle", env!("CARGO_MANIFEST_DIR"));
    let logged = || lines.lock().unwrap().join("\n");

    let (address, stop, serving) = start(Duration::from_secs(5)).await;

    // A panic fails its request, at the line that raised it, and the server goes on.
    let response = request(address, "/api/boom").await;
    assert!(response.starts_with("HTTP/1.1 500"), "{response}");
    assert!(response.contains(r#""code":"RUN0006""#), "{response}");
    let source = include_str!("lifecycle.rs");
    let line = 1 + source
        .lines()
        .position(|line| line.contains("\"many\".parse().unwrap()"))
        .unwrap();
    let expected = format!(
        "RUN0006  `lifecycle::boom` panicked: called `Result::unwrap()` on an `Err` value: ParseIntError {{ kind: InvalidDigit }}\n         at crates/balise/tests/lifecycle.rs:{line}"
    );
    assert!(logged().contains(&expected), "{}", logged());
    assert!(
        logged().contains(&format!(
            "backtrace:\n           boom  tests/lifecycle.rs:{line}\n           … "
        )),
        "{}",
        logged()
    );

    request(address, "/api/later").await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        logged().contains("RUN0006  A task started by `lifecycle::later` panicked: task failed"),
        "{}",
        logged()
    );

    // Health checks answer, outside the API base, without a line in the logs.
    assert!(
        request(address, "/health")
            .await
            .ends_with(r#"{"status":"ok"}"#)
    );
    assert!(
        request(address, "/ready")
            .await
            .ends_with(r#"{"status":"ready"}"#)
    );
    assert!(!logged().contains("GET /health"), "{}", logged());

    // A shutdown waits for the request in progress.
    let slow = tokio::spawn(request(address, "/api/slow"));
    tokio::time::sleep(Duration::from_millis(50)).await;
    stop.send(()).unwrap();
    assert!(slow.await.unwrap().ends_with(r#"{"text":"done"}"#));
    serving.await.unwrap();
    assert!(
        logged().contains("Shutting down: 1 request and 0 tasks in progress"),
        "{}",
        logged()
    );
    assert!(logged().ends_with("Stopped"), "{}", logged());
    assert!(TcpStream::connect(address).await.is_err());

    // Past the timeout, what is left is reported.
    let (address, stop, serving) = start(Duration::from_millis(100)).await;
    let slow = tokio::spawn(request(address, "/api/slow"));
    tokio::time::sleep(Duration::from_millis(50)).await;
    stop.send(()).unwrap();
    serving.await.unwrap();
    assert!(
        logged().contains(
            "WARN  RUN0008  Stopped after 100 ms with 1 request(s) and 0 task(s) unfinished"
        ),
        "{}",
        logged()
    );
    slow.abort();
}
