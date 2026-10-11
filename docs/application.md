# Writing a Balise application
 
Status: draft. This describes the target design; nothing is implemented yet.
 
This document describes the code you write in a Balise project. It is meant to be read by coding agents and can be given to one as is. It does not cover framework internals ([engine.md](engine.md)), the ORM ([data.md](data.md)) or realtime features ([web.md](web.md)).
 
## Project layout
 
One folder per resource. Everything a feature needs lives in that folder.
 
```
src/
  main.rs           declares the resources and starts the app
  orders/
    mod.rs          handlers for /orders routes
    api.rs          inputs and outputs: the contract with the front end
    model.rs        the Order model
    tests.rs        tests for this resource
  users/
    mod.rs
    api.rs
    model.rs
    tests.rs
  seeds/            datasets for dev and tests, never in production
```
 
- Add a new resource with `balise generate resource <name>`: it creates the folder and declares its module in `main.rs`.
- Do not create `controllers/`, `services/` or `utils/` folders. A helper about one model is a method on it, in `model.rs`; a helper for the handlers stays in `mod.rs`, below them.
- Routes are collected automatically. There is no router file to update: `main.rs` declares modules, not routes. A resource folder missing from `main.rs` is reported by `balise check`, since its routes would silently not exist.
## Starting the app
 
```rust
mod orders;
mod users;

fn main() {
    balise::run();
}
```
 
If the app needs shared state, declare it here (see [Shared state](#shared-state)):
 
```rust
mod prices;

fn main() {
    balise::app()
        .state(PriceCache::new())
        .run();
}
```
 
`main.rs` contains nothing else than module declarations and this call. Configuration comes from environment variables and `balise.toml`, not from code.
 
## Routes and handlers
 
A handler is an `async fn` with a route attribute.
 
```rust
use balise::prelude::*;
 
// src/orders/mod.rs
#[get("/orders/{id}")]
async fn get(id: Path<OrderId>, db: Db) -> Result<OrderView> {
    let order = Order::get(db, *id).await?;     // missing row: 404
    Ok(OrderView::from(order))
}
```
 
- Route attributes: `#[get]`, `#[post]`, `#[put]`, `#[patch]`, `#[delete]`.
- Path parameters are written `{name}` and read with `Path<T>`, in the order they appear.
- Paths never include the API prefix: `#[get("/orders")]` is served at `/api/orders` by default. The prefix, with its version if any (`/api/v1`), is set once in `balise.toml`.
- Every handler returns `Result<T>`, where `T` is a response type.
- Handler names are short verbs; the folder gives the resource: `list`, `get`, `create`, `update`, `delete`, or a specific verb such as `cancel`. Logs, errors and MCP tools refer to a handler as `orders::create`.
### Parameters
 
Declare what the handler needs as parameters. Balise provides them.
 
| Parameter | Gives you |
| --- | --- |
| `Path<T>` | Path parameters. One `Path` per `{param}`, or a tuple: `Path<(ShopId, OrderId)>` |
| `Query<T>` | Query string, parsed into an `Input` struct. `Query<PageParams>` gives the pagination parameters |
| `Body<T>` | JSON body, parsed into an `Input` struct |
| `Db` | A database connection from the pool (requires the `orm` feature) |
| `State<T>` | Shared state declared in `main.rs` |
| `CurrentUser` | The signed-in user, or a 401 (requires the `auth` feature) |
| `RequestId` | The request identifier, as it appears in logs |
 
At most one `Body` per handler. The order of parameters does not matter.
 
### Input
 
Inputs and outputs live in the resource's `api.rs`. It is the contract with the front end: TypeScript types are generated from it.

Inputs are structs that derive `Input`. One rule for every field:
 
- `T`: required.
- `Option<T>`: optional.
- `#[input(default = ...)]`: optional, with a default value.
```rust
// src/orders/api.rs
#[derive(Input)]
struct CreateOrder {
    product_id: ProductId,               // required
    #[input(default = 1, min = 1, max = 100)]
    quantity: u32,                       // defaults to 1
    note: Option<String>,                // optional
    #[input(email)]
    contact: String,                     // required, must be an email
}
```
 
Validation attributes:
 
| Attribute | Applies to |
| --- | --- |
| `min = n`, `max = n` | numbers |
| `length(min = n, max = n)` | strings, lists |
| `email`, `url` | strings |
| `one_of("a", "b")` | strings |
| `default = value` | any field |
 
If validation fails, Balise answers `422` with one message per field. The handler is not called.
 
The same `Input` struct works for `Body` and `Query`. Name inputs after the action: `CreateOrder`, `UpdateOrder`, `OrderFilters`.
 
### Responses
 
Return a struct that derives `Output`, or one of the wrappers below.
 
```rust
// src/orders/api.rs
#[derive(Output)]
struct OrderView {
    id: OrderId,
    total_cents: i64,
    note: Option<String>,
}
```
 
| Return type | Status |
| --- | --- |
| `T` (an `Output`) | `200` with JSON |
| `Vec<T>` | `200` with a JSON array |
| `Page<T>` | `200` with a paginated list |
| `Created<T>` | `201` with JSON |
| `NoContent` | `204` |
| `Redirect` | `303` |
 
Name outputs after the resource: `OrderView`, `OrderSummary`. TypeScript types are generated from every `Input` and `Output` ([web.md](web.md)).
 
The response body is the `Output` as plain JSON. You never set a status code by number: the return type decides it. Errors always use the same JSON body, described in [engine.md](engine.md#response-format).
 
Success or failure is never written by hand in the body. The HTTP status carries it, and the generated TypeScript client turns every call into `{ ok: true, data }` or `{ ok: false, error }` for the front end.
 
Lists must be paginated: return `Page<T>` rather than an unbounded `Vec<T>`. Pages hold 20 items by default and 100 at most; a route that needs other limits declares them: `#[get("/exports/orders", page(default = 500, max = 5000))]`. Details in [data.md](data.md#pagination).
 
### Files
 
A route that accepts files declares them as fields of its `Input`, with their size and type. That declaration is the only place the limit is set.
 
```rust
#[derive(Input)]
struct UploadVideo {
    title: String,
    #[input(file, max_size = "5GB", types("video/mp4", "video/webm"))]
    video: File,
}
 
// src/videos/mod.rs
#[post("/videos")]
async fn upload(input: Body<UploadVideo>, user: CurrentUser) -> Result<Created<VideoView>> {
    let stored = input.video.store("videos").await?;
    ...
}
```
 
- A `Body` that contains a `File` is read as `multipart/form-data` automatically.
- The route's size limit and timeout are computed from the declaration. Other routes keep the 2 MB default.
- `File` is already on disk when the handler runs; it is never fully loaded in memory.
- The generated TypeScript client knows each route's limits and accepted types, and rejects a file that is too large before sending it. Front end and back end cannot disagree.
- Too large or wrong type: `413` or `415`, with the limit in the error message.
### Errors
 
Use `?`. Errors from Balise (database, validation, not found…) are turned into the right HTTP response with their code.
 
To return an error yourself:
 
```rust
if order.shipped {
    return Err(Error::conflict("order already shipped"));
}
```
 
Available: `Error::bad_request`, `Error::forbidden`, `Error::not_found`, `Error::conflict`.
 
For errors specific to your application, declare them once per resource, in `api.rs`: their codes are part of the contract with the front end. Codes are a prefix and four digits.
 
```rust
// src/orders/api.rs
#[derive(AppError)]
enum OrderError {
    #[error(status = 409, code = "ORD0001", fix = "Cancel the order before refunding it.")]
    NotCancelled,
}
```
 
They appear in logs and responses in the same format as Balise's own errors.
 
## Shared state
 
The application runs as one long-lived process. Values live for one of two durations:
 
| Lifetime | Examples | How |
| --- | --- | --- |
| Request | parameters, current user, a transaction | handler parameters |
| Process | config, caches, clients for other services | declared in `main.rs`, read with `State<T>` |
 
```rust
// main.rs
balise::app().state(PriceCache::new()).run();
 
// prices/mod.rs
#[get("/prices")]
async fn list(cache: State<PriceCache>) -> Result<Vec<PriceView>> { ... }
```
 
- State is read-only by default.
- For state that changes, wrap it in `Shared<T>` and use `.write().await` / `.read().await`.
- Never use `static mut`, global variables, `std::sync::Mutex` or `RefCell` in application code. `balise check` rejects them.
## Heavy work and background tasks
 
- CPU-heavy work (image processing, large computations) goes in `balise::blocking(|| ...)`, which keeps the server responsive.
- Work that continues after the response goes in `balise::spawn(async { ... })`, which logs failures in Balise's format.
- Do not call `tokio::spawn` or `std::thread::spawn` directly.
## Tests
 
```rust
use balise::test::*;

#[balise::test]
async fn create_order_returns_201(app: TestApp) {
    app.sign_in_as_new_user().await;
    let res = app.post("/orders").body(CreateOrder::fake()).send().await;
    res.assert_status(201);
    res.assert_queries_at_most(3);
}
```
 
- Each test gets a fresh database.
- Paths are written without the API prefix, as in route attributes: the test client adds it.
- Request bodies are typed: an `Input` value or `CreateOrder::fake()`. `json!` (from `balise::test`) is only for invalid bodies, to test validation.
- Check a response with its type first: `let order: OrderView = res.json(); assert_eq!(order.quantity, 2);`. A renamed field then breaks at compile time. For a partial check, `res.assert_json_includes(json!({ "quantity": 2 }))` ignores absent keys and compares the others.
- `sign_in_as_new_user` exists when the `auth` module is enabled.
- Mails are captured: `app.mails()` checks that a mail was sent and gives its links, as `/_balise/mails` does in dev.
- `assert_queries_at_most` enforces a query budget for the route.
## Working loop
 
1. Before writing code, look up the project with the MCP tools (`project_map`, `search_api`), or `balise introspect`.
2. After each change, run `balise check`.
3. After each feature, run `balise audit` and fix every warning.
4. If an error code is unclear, run `balise explain <CODE>`.
## Debugging
 
Investigate through Balise, never in the framework's dependencies. Everything below the facade is internal: reading or patching it will not fix your application.
 
| Problem | Tool |
| --- | --- |
| A route returns `404` or `405` | `balise why GET /orders/5`: lists the routes that almost match and why they don't |
| Unsure which routes exist | `balise routes`, or the `project_map` MCP tool |
| A request misbehaves | `balise trace <request_id>`: routing decision, extracted parameters, queries, error |
| A network or connection error | its `HTTP` or `EXT` code, then `balise explain <CODE>` |
| A panic | the Balise crash report: it shows your code first and folds framework frames |
