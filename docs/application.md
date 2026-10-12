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
    balise::run!();
}
```

`balise::run!()` embeds `balise.toml`, which sits next to `Cargo.toml`, in the binary: a deployment needs nothing else, and environment variables override it. A missing, misplaced or malformed `balise.toml` is a compile error (`CMP0013`, `CMP0014`).
 
If the app needs shared state, declare it here (see [Shared state](#shared-state)):
 
```rust
mod prices;

fn main() {
    balise::app()
        .state(PriceCache::new())
        .run();
}
```
 
`main.rs` contains nothing else than module declarations and this call. Configuration comes from environment variables and `balise.toml`, not from code ([environments.md](environments.md#configuration)).
 
## Routes and handlers
 
A handler is an `async fn` with a route attribute.
 
```rust
use balise::prelude::*;
 
// src/orders/mod.rs
#[get("/orders/{id}")]
async fn get(id: Path<OrderId>, user: CurrentUser, db: Db) -> Result<OrderView> {
    let order = Order::get(db, id).owned_by(&user).await?;     // missing row, or someone else's: 404
    Ok(OrderView::from(order))
}
```
 
- Route attributes: `#[get]`, `#[post]`, `#[put]`, `#[patch]`, `#[delete]`.
- Path parameters are written `{name}` and read with `Path<T>`, in the order they appear.
- Paths never include the API prefix: `#[get("/orders")]` is served at `/api/orders` by default. The prefix and an optional version are set once in `balise.toml` (`[api] prefix = "/api"`, `version = 1` for `/api/v1`).
- A route lives in the resource of what it returns: `GET /shops/{shop_id}/orders` returns orders, so it goes in `orders/`. Its handler is named `<verb>_in_<parent>`: `list_in_shop`.
- A handler returns a response type, or `Result<T>` when it can fail, which is almost always.
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

`Path`, `Query` and `Body` give you the value itself: in the handler, `id: Path<OrderId>` is an `OrderId` and `input: Body<CreateOrder>` is a `CreateOrder`. Use them directly, and move their fields out: no `*id`, no `.into_inner()`, no `.clone()`. Several path parameters can be read in one tuple and destructured: `(shop_id, id): Path<(ShopId, OrderId)>`. `Db`, `State<T>`, `CurrentUser` and `RequestId` are services, used as they are.

A type Balise cannot provide as a parameter is a compile error (`CMP0006`), and so is a return type that is not a response (`CMP0007`).
 
### Input
 
Inputs and outputs live in the resource's `api.rs`. It is the contract with the front end: TypeScript types are generated from it.

Inputs are structs that derive `Input`. One rule for every field:
 
- `T`: required; absent or `null` is an error.
- `Option<T>`: optional; absent or `null` gives `None`.
- `#[input(default = ...)]`: optional, with a default value.
- `Patch<T>`: in an update input, absent leaves the field unchanged, a value sets it, `null` clears it. Apply it with `order.update(db, |o| input.apply_to(o))`.

An `Input` never goes to the database as a whole: the handler copies the fields it accepts into the model's `NewOrder` ([data.md](data.md#naming)).
```rust
// src/orders/api.rs
#[derive(Input)]
#[input(model = Order)]     // the model's rules apply to fields of the same name
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
| `length(min = n, max = n)` | strings (characters), lists (items); either bound alone |
| `email`, `url` | strings |
| `one_of("a", "b")` | strings |
| `default = value` | any field |
| `secret` | any field: its value is never repeated in messages or logs |
 
If validation fails, Balise answers `422` (`VAL0001`) with a code and a message per field: missing (`VAL0004`), wrong type (`VAL0005`), rule not met (`VAL0006`) or unknown (`VAL0007`). The handler is not called.

A field the `Input` does not declare is rejected, with the closest declared name when there is one (`quantiy`: did you mean `quantity`?): a typo never passes silently. An `Input` that describes a payload sent by a third party, such as a webhook, accepts unknown fields with `#[input(allow_unknown_fields)]`.
 
Fields hold text (`String`), numbers, `bool` and lists of them (`Vec<T>`); identifiers, enums and dates join with the models. On an `Option<T>` or `Patch<T>` field, rules apply to the value when there is one. A rule on a type it does not apply to is a compile error (`CMP0009`), and so is a field type Balise cannot read (`CMP0010`) or an unknown attribute (`CMP0008`).

Each field gets at most one error: its type first, then its rules in the order written. All fields are read before answering, so one `422` lists everything to fix.

The same `Input` struct works for `Body` and `Query`. In a query string every value is text: numbers and booleans are read from it, and a list is written `?tags=a,b`. Name inputs after the action: `CreateOrder`, `UpdateOrder`, `OrderFilters`.
 
### Responses
 
Return a struct that derives `Output`, or one of the wrappers below.
 
```rust
// src/orders/api.rs
#[derive(Output)]
#[output(from = Order)]     // `order.into()` copies the fields of the same name
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
| `Result<T>` | the status of `T`, or the error's |
| `Redirect` | `303` |
 
Name outputs after the resource: `OrderView`, `OrderSummary`. TypeScript types are generated from every `Input` and `Output` ([web.md](web.md)).
 
The response body is the `Output` as plain JSON: fields in declaration order, under their Rust names, `None` written `null`. Fields hold text, numbers, `bool`, `Option<T>`, `Vec<T>` and other `Output` structs. You never set a status code by number: the return type decides it. Errors always use the same JSON body, described in [engine.md](engine.md#response-format).
 
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
- The route's size limit and timeout are computed from the declaration. Other routes keep the 2 MB default, unless `budgets.toml` raises their `body_size` ([budgets](agent-experience.md#budgets)).
- `File` is already on disk when the handler runs; it is never fully loaded in memory.
- The generated TypeScript client knows each route's limits and accepted types, and rejects a file that is too large before sending it. Front end and back end cannot disagree.
- Too large or wrong type: `413` or `415`, with the limit in the error message.
### Errors
 
Use `?`. Errors from Balise (database, validation, not found…) become the right HTTP response with their code. Any other error becomes a `500` (`EXT0001`) that keeps it as its cause.

To return an error yourself, declare it once in the resource's `api.rs`: its code is part of the contract with the front end. Every error has a code, a message and a fix, so there is no error without them.

```rust
// src/orders/api.rs
#[derive(AppError)]
pub enum OrderError {
    /// Shown by `balise explain ORD0001`: why the rule exists.
    #[error(status = 409, code = "ORD0001",
            message = "Only pending orders can be cancelled",
            fix = "Check the order's status before cancelling it.")]
    NotPending,
    #[error(status = 422, code = "ORD0002",
            message = "An order has at most {max} lines",
            fix = "Split the order: at most {max} lines each.")]
    TooManyLines { max: u32 },
}

// src/orders/mod.rs
if order.status != OrderStatus::Pending {
    return Err(OrderError::NotPending.into());
}
```

- `status` is a `4xx`: a `5xx` comes from a real failure, never from a choice of the application.
- `code` is your own prefix (2 to 4 letters, not one of Balise's: `HTTP`, `VAL`, `DB`…) and four digits. A code is used once in the application.
- `message` and `fix` can use the variant's fields: `{max}`.
- The response and the logs give the place where the error was returned, in your code.

They appear in logs and responses in the same format as Balise's own errors.
 
## Ownership

A model that belongs to a user declares it, and every query on it says whose rows it reads. Forgetting is a compile error, not a data leak.

```rust
// src/orders/model.rs
#[derive(Model)]
#[model(owned_by = User)]
pub struct Order { pub id: OrderId, pub user: BelongsTo<User>, … }

// src/orders/mod.rs
let orders = Order::query().owned_by(&user).page(db, page).await?;   // only this user's orders
let order = Order::create(db, NewOrder { user: user.owner(), … }).await?;   // the owner comes from `user`, never from an input
```

- `user` is the handler's `CurrentUser` parameter.
- Someone else's row answers `404`, as if it did not exist.
- Screens that need every row (admin, jobs) use `.all_owners("reason")`, a [sensitive decision](agent-experience.md#sensitive-decisions).
- Details, and models owned through another model: [data.md](data.md#ownership).

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
- Work that continues after the response goes in `balise::spawn(|db| async move { ... })`, which logs failures in Balise's format. The task gets its own `db`: never use the request's.
- Work that must happen only if the request's writes are saved goes in `db.after_commit(async move { ... })`.
- To change a row you read, lock it: `Order::get(db, id).owned_by(&user).for_update()`. Without the lock, `update` and `delete` do not compile ([data.md](data.md#locking-rows)).
- Do not call `tokio::spawn` or `std::thread::spawn` directly.

## Logging

```rust
balise::log::info!(order_id = id, "Order {id} cancelled");
balise::log::warn!("Stock is low for {sku}");
```

- `info!`, `warn!`, `error!` and `debug!` write in Balise's format, with the request's ID and handler and the line that logged. Never use `println!`.
- Fields come first, as `name = value`, then the message, written like `format!`. A field holds text, a number, a boolean or an `Option` of one; convert anything else with `.to_string()` (`CMP0012` otherwise).
- A field whose name contains `password`, `token` or `secret` is written `***`.
- Every request is already logged once when it ends, with its status, duration and error: do not log requests yourself.
## Tests
 
```rust
use balise::test::*;

#[balise::test]
async fn create_order_returns_201(app: TestApp) {
    app.sign_in_as_new_user().await;
    let res = app.post("/orders").body(CreateOrder::fake()).send().await;
    res.assert_status(201);
}
```
 
- Each test gets a fresh database.
- Paths are written without the API prefix, as in route attributes: the test client adds it.
- Request bodies are typed: an `Input` value or `CreateOrder::fake()`. `json!` (from `balise::test`) is only for invalid bodies, to test validation.
- Check a response with its type first: `let order: OrderView = res.json(); assert_eq!(order.quantity, 2);`. A renamed field then breaks at compile time. For a partial check, `res.assert_json_includes(json!({ "quantity": 2 }))` ignores absent keys and compares the others.
- `sign_in_as_new_user` exists when the `auth` module is enabled.
- Mails are captured: `app.mails()` checks that a mail was sent and gives its links, as `/_balise/mails` does in dev.
- Every request checks its route's query budget ([budgets](agent-experience.md#budgets)): no assertion to write.
- `cargo test` works as is; `balise test` also starts the test database.
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

## Open questions

From the review of the mock-up by agents (2026-10-11):
- **Declaring shared state with `balise::run!()`**: the start is a macro, so the builder shown in [Starting the app](#starting-the-app) (`balise::app().state(…).run()`) cannot end with it as written.
