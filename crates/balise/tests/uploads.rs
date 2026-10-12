//! Files arrive as multipart bodies, checked and streamed to disk; bodies are read last and
//! within each route's limits; paginated routes read their page sizes.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use balise::__private::routes;
use balise::prelude::*;
use balise_core::config::{Config, Environment};
use balise_core::limits::RouteLimits;
use balise_core::router::Router;
use balise_engine::Server;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[derive(Input)]
struct UploadPhoto {
    title: String,
    #[input(max_size = "1KB", types("image/png", "image/jpeg"))]
    photo: File,
    #[input(max_size = "1KB", types("text/csv"), max_count = 2, length(max = 2))]
    sheets: Vec<File>,
}

#[derive(Output)]
struct Stored {
    title: String,
    path: String,
    content_type: String,
    size: u64,
    sheet: String,
}

#[post("/photos")]
async fn upload(input: Body<UploadPhoto>) -> Result<Created<Stored>> {
    let content_type = input.photo.content_type().to_string();
    let size = input.photo.size();
    let sheet = String::from_utf8(input.sheets[0].read().await?).unwrap_or_default();
    let stored = input.photo.store("photos").await?;
    Ok(Created(Stored {
        title: input.title,
        path: stored.path().to_string(),
        content_type,
        size,
        sheet,
    }))
}

#[derive(Input)]
struct Note {
    note: String,
}

#[derive(Output)]
struct Text {
    text: String,
}

/// `Body` comes first in the signature, and is still read last.
#[post("/orders/{id}/notes")]
async fn note(input: Body<Note>, id: Path<u32>) -> Text {
    Text {
        text: format!("{id}: {}", input.note),
    }
}

#[post("/slow")]
async fn slow() -> NoContent {
    tokio::time::sleep(Duration::from_secs(5)).await;
    NoContent
}

#[derive(Input)]
struct Filters {
    status: Option<String>,
}

#[get("/exports", page(default = 50, max = 200))]
async fn export(page: PageParams, filters: Query<Filters>) -> Text {
    Text {
        text: format!("{} {}", page.per_page(), filters.status.unwrap_or_default()),
    }
}

const BUDGETS: &str = r#"
["uploads::slow"]
timeout = "100ms"

["uploads::note"]
timeout = "300ms"
"#;

fn storage() -> std::path::PathBuf {
    std::env::temp_dir().join("balise-upload-tests")
}

async fn start() -> SocketAddr {
    balise_core::file::set_storage(&storage().display().to_string());
    let config = Config::defaults(Environment::Dev);
    let limits = RouteLimits::resolve(routes(), &config, BUDGETS).unwrap();
    let router = Router::build(routes(), &config.api_base()).unwrap();
    let server = Server::with_limits(router, &config, limits);
    let listener = balise_engine::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(balise_engine::serve(
        listener,
        Arc::new(server),
        std::future::pending(),
    ));
    address
}

/// Sends a request whose body is `body`, and returns the whole response.
async fn send(address: SocketAddr, head: &str, body: &[u8]) -> String {
    let mut stream = TcpStream::connect(address).await.unwrap();
    let raw = format!(
        "{head}\r\nHost: test\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    stream.write_all(raw.as_bytes()).await.unwrap();
    stream.write_all(body).await.unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();
    String::from_utf8_lossy(&response).into_owned()
}

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01\x08\x02\0\0\0";

/// A `multipart/form-data` body: `(name, Some(filename), content)` for a file.
fn form(parts: &[(&str, Option<&str>, &[u8])]) -> Vec<u8> {
    let mut body = Vec::new();
    for (name, filename, content) in parts {
        let disposition = match filename {
            Some(file) => format!("form-data; name=\"{name}\"; filename=\"{file}\""),
            None => format!("form-data; name=\"{name}\""),
        };
        body.extend_from_slice(
            format!("--b0undary\r\nContent-Disposition: {disposition}\r\n\r\n").as_bytes(),
        );
        body.extend_from_slice(content);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(b"--b0undary--\r\n");
    body
}

const MULTIPART: &str =
    "POST /api/photos HTTP/1.1\r\nContent-Type: multipart/form-data; boundary=b0undary";

#[tokio::test(flavor = "multi_thread")]
async fn files_are_checked_and_stored() {
    let address = start().await;

    let body = form(&[
        ("title", None, b"Beach"),
        ("photo", Some("beach.png"), PNG),
        ("sheets", Some("prices.csv"), b"tea,4.50\n"),
    ]);
    let response = send(address, MULTIPART, &body).await;
    assert!(response.starts_with("HTTP/1.1 201"), "{response}");
    let path = response
        .split(r#""path":""#)
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .unwrap();
    assert!(
        path.starts_with("photos/") && path.ends_with(".png"),
        "{path}"
    );
    assert_eq!(std::fs::read(storage().join(path)).unwrap(), PNG);
    assert!(
        response.contains(&format!(
            r#""content_type":"image/png","size":{},"sheet":"tea,4.50\n""#,
            PNG.len()
        )),
        "{response}"
    );
    // Files not stored are gone with the request.
    let uploads = std::fs::read_dir(storage().join(".uploads"))
        .unwrap()
        .count();
    assert_eq!(uploads, 0);

    let body = form(&[
        ("title", None, b"Beach"),
        ("photo", Some("beach.png"), b"%PDF-1.7"),
        ("sheets", Some("prices.csv"), b"x"),
    ]);
    let response = send(address, MULTIPART, &body).await;
    assert!(response.starts_with("HTTP/1.1 415"), "{response}");
    assert!(
        response.contains(r#""code":"FILE0002","message":"`photo` must be image/png or image/jpeg; its content is application/pdf""#),
        "{response}"
    );

    let body = form(&[("title", None, b"Beach")]);
    let response = send(address, MULTIPART, &body).await;
    assert!(response.starts_with("HTTP/1.1 422"), "{response}");
    assert!(
        response.contains(r#""photo":{"code":"VAL0004","message":"is required"}"#),
        "{response}"
    );

    let json = "POST /api/photos HTTP/1.1\r\nContent-Type: application/json";
    let response = send(address, json, b"{}").await;
    assert!(response.starts_with("HTTP/1.1 415"), "{response}");
    assert!(
        response.contains("The request body must be `multipart/form-data`, not `application/json`"),
        "{response}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_body_is_read_after_the_other_parameters() {
    let address = start().await;
    let json = "POST /api/orders/7/notes HTTP/1.1\r\nContent-Type: application/json";
    let response = send(address, json, br#"{"note": "fragile"}"#).await;
    assert!(response.ends_with(r#"{"text":"7: fragile"}"#), "{response}");

    // A body announced but never sent: an invalid path answers without waiting for it.
    let mut stream = TcpStream::connect(address).await.unwrap();
    let head = "POST /api/orders/abc/notes HTTP/1.1\r\nHost: test\r\nContent-Type: application/json\r\nContent-Length: 1000000\r\n\r\n";
    stream.write_all(head.as_bytes()).await.unwrap();
    let mut response = vec![0; 1024];
    let read = tokio::time::timeout(Duration::from_secs(2), stream.read(&mut response))
        .await
        .expect("answered before the body arrived")
        .unwrap();
    let response = String::from_utf8_lossy(&response[..read]);
    assert!(response.starts_with("HTTP/1.1 404"), "{response}");
}

#[tokio::test(flavor = "multi_thread")]
async fn timeouts_tell_a_slow_client_from_a_slow_handler() {
    let address = start().await;

    let response = send(address, "POST /api/slow HTTP/1.1", b"").await;
    assert!(response.starts_with("HTTP/1.1 503"), "{response}");
    assert!(
        response.contains(r#""code":"HTTP0018","message":"`uploads::slow` did not answer within its timeout of 100 ms""#),
        "{response}"
    );

    let mut stream = TcpStream::connect(address).await.unwrap();
    let head = "POST /api/orders/7/notes HTTP/1.1\r\nHost: test\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: 100\r\n\r\n{\"note\"";
    stream.write_all(head.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    assert!(response.starts_with("HTTP/1.1 408"), "{response}");
    assert!(response.contains(r#""code":"HTTP0017""#), "{response}");
}

#[tokio::test(flavor = "multi_thread")]
async fn page_sizes_follow_the_route() {
    let address = start().await;
    let response = send(address, "GET /api/exports?status=paid HTTP/1.1", b"").await;
    assert!(response.ends_with(r#"{"text":"50 paid"}"#), "{response}");
    // Above the route's maximum, lowered; the page keys are not unknown fields of `Filters`.
    let response = send(
        address,
        "GET /api/exports?per_page=999&cursor=x HTTP/1.1",
        b"",
    )
    .await;
    assert!(response.ends_with(r#"{"text":"200 "}"#), "{response}");
}
