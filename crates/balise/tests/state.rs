//! Shared state is created at startup and read by handlers and tasks; tasks run after the
//! response, with their own services; blocking work runs off the event loop.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use balise::__private::routes;
use balise::prelude::*;
use balise_core::config::Config;
use balise_core::event::Level;
use balise_core::log::{self, Format, Logger, Sink};
use balise_core::router::Router;
use balise_core::state;
use balise_engine::Server;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

struct Prices {
    tea_cents: u32,
}

#[balise::state]
fn prices() -> Prices {
    Prices { tea_cents: 450 }
}

struct Visits(u32);

#[balise::state]
async fn visits() -> Shared<Visits> {
    Shared::new(Visits(0))
}

struct Settings {
    greeting: String,
}

#[balise::state]
fn settings() -> Result<Settings> {
    let greeting = "hello".to_string();
    Ok(Settings { greeting })
}

#[derive(Output)]
struct Count {
    count: u32,
}

#[get("/tea")]
async fn tea(prices: State<Prices>, settings: State<Settings>) -> Count {
    assert_eq!(settings.greeting, "hello");
    Count {
        count: prices.tea_cents,
    }
}

#[post("/visits")]
async fn visit(visits: State<Shared<Visits>>) -> Count {
    let mut visits = visits.write().await;
    visits.0 += 1;
    Count { count: visits.0 }
}

#[post("/visits/later")]
async fn visit_later() -> NoContent {
    balise::spawn(async move |visits: State<Shared<Visits>>| {
        visits.write().await.0 += 10;
        balise::log::info!("Counted later");
        Ok(())
    });
    NoContent
}

#[post("/visits/failing")]
async fn failing() -> NoContent {
    balise::spawn(async move || {
        let n: u32 = "x".parse()?;
        balise::log::info!("never {n}");
        Ok(())
    });
    NoContent
}

#[get("/visits")]
async fn count(visits: State<Shared<Visits>>) -> Count {
    let total = balise::blocking(|| (1..=4).sum::<u32>()).await;
    Count {
        count: visits.read().await.0 + total - 10,
    }
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
async fn state_is_shared_and_tasks_run_after_the_response() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    log::install(Logger {
        format: Format::Text,
        min_level: Level::Info,
        color: false,
        sink: Sink::Memory(lines.clone()),
    });
    state::create(&state::STATES, routes()).await.unwrap();
    let config = Config::default();
    let server = Server::new(
        Router::build(routes(), &config.api_base()).unwrap(),
        &config,
    );
    let listener = balise_engine::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(balise_engine::serve(listener, Arc::new(server)));

    assert!(
        request(address, "GET", "/tea")
            .await
            .ends_with(r#"{"count":450}"#)
    );
    assert!(
        request(address, "POST", "/visits")
            .await
            .ends_with(r#"{"count":1}"#)
    );
    assert!(
        request(address, "POST", "/visits")
            .await
            .ends_with(r#"{"count":2}"#)
    );

    request(address, "POST", "/visits/later").await;
    request(address, "POST", "/visits/failing").await;
    // The tasks finish after their responses.
    let mut count = String::new();
    for _ in 0..100 {
        count = request(address, "GET", "/visits").await;
        if count.ends_with(r#"{"count":12}"#) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(count.ends_with(r#"{"count":12}"#), "{count}");

    let logged = lines.lock().unwrap().join("\n");
    assert!(
        logged.contains("Counted later\n          at crates/balise/tests/state.rs"),
        "{logged}"
    );
    assert!(
        logged.contains("in state::visit_later, request req_"),
        "{logged}"
    );
    assert!(
        logged.contains("ERROR EXT0001  invalid digit found in string"),
        "{logged}"
    );
    assert!(
        logged.contains("in state::failing, request req_"),
        "{logged}"
    );
    assert!(
        logged.contains("spawned_at: crates/balise/tests/state.rs:"),
        "{logged}"
    );
}
