//! `CMP`: compile-time errors raised by Balise's macros and `balise check`.

codes! {
    CMP0001 {
        title: "Handler is not async",
        message: "`{function}` must be an `async fn` to be a handler",
        fix: "Write `async fn {function}(…)`.",
        explanation: r#"
A route attribute (`#[get]`, `#[post]`…) goes on an `async fn`: handlers wait for the
database and other services without blocking the server.
"#,
    }
    CMP0002 {
        title: "Two request bodies",
        message: "`{function}` has {count} `Body` parameters; a request has one body",
        fix: "Keep one `Body<T>` parameter, with every field in one `Input` struct.",
        explanation: r#"
A request has a single body, so a handler takes at most one `Body<T>`. To receive several
values, put them all in the same `Input` struct.
"#,
    }
    CMP0003 {
        title: "Path parameters do not match the route",
        message: "`{function}` does not read the path parameters of its route: `{path}` has {expected}, the handler reads {found}",
        fix: "Add one `Path<T>` per `{name}` in the route, in order, or one tuple: `Path<(A, B)>`.",
        explanation: r#"
Each `{name}` in a route's path is read by a `Path` parameter, in the order they appear:
one `Path<T>` per parameter, or one `Path<(A, B)>` for all of them.

```rust
#[get("/shops/{shop_id}/orders/{id}")]
async fn get(shop_id: Path<ShopId>, id: Path<OrderId>, …)
```
"#,
    }
    CMP0004 {
        title: "Invalid route path",
        message: "Invalid route path `{path}`: {reason}",
        fix: "Write a path that starts with `/`, with segments separated by `/` and parameters written `{name}`.",
        explanation: r#"
A route path starts with `/`, never includes the API prefix (`/api` is added by Balise), and
writes each parameter as a whole segment: `/orders/{id}`, not `/orders/id-{id}`. Parameter
names are snake_case and unique in the path.
"#,
    }
    CMP0005 {
        title: "Invalid route attribute",
        message: "Invalid route attribute: {reason}",
        fix: "Write the attribute with the path as a string: `#[get(\"/orders/{id}\")]`.",
        explanation: r#"
A route attribute takes the path as a string literal, and goes on a function:
`#[get("/orders")]` above `async fn list(…)`.
"#,
    }
    CMP0006 {
        title: "Not a handler parameter",
        message: "`{type}` cannot be a handler parameter",
        fix: "Use `Path<T>`, `Query<T>`, `Body<T>`, `RequestId`, or a service such as `Db` or `CurrentUser`.",
        explanation: r#"
Every handler parameter is provided by Balise, which must know how to build it from the
request. The parameters it knows are listed in the parameters table of the documentation:
`Path<T>` for path parameters, `Query<T>` and `Body<T>` for an `Input`, and services.

A plain type such as `u32` or `CreateOrder` is wrapped: `Path<u32>`, `Body<CreateOrder>`.
"#,
    }
    CMP0007 {
        title: "Not a handler result",
        message: "`{type}` cannot be returned from a handler",
        fix: "Return a struct that derives `Output`, a `Vec` of them, `Created<T>`, `NoContent` or `Redirect`, or a `Result` of one of these.",
        explanation: r#"
A handler's return type decides the response: an `Output` or a `Vec` of them is a `200`
with JSON, `Created<T>` a `201`, `NoContent` a `204`, `Redirect` a `303`. A handler that can
fail returns `Result<T>`, and its errors become the single error body. A model is never
returned: convert it to its view with `.into()`.
"#,
    }
    CMP0008 {
        title: "Invalid Input or Output declaration",
        message: "Invalid `{derive}` declaration: {reason}",
        fix: "Derive on a struct with named fields and no generics, and use only the attributes listed by `balise explain CMP0008`.",
        explanation: r#"
`Input` and `Output` derive on structs with named fields and no generics. Their attributes
are the ones listed in the documentation, and nothing else:

- on an `Input`: `#[input(model = Order)]` and `#[input(allow_unknown_fields)]`;
- on an `Input` field: `min`, `max`, `length(min = …, max = …)`, `email`, `url`,
  `one_of("a", "b")`, `default = …` and `secret`;
- on an `Output`: `#[output(from = Order)]`.

The message names what is wrong and the fix gives the right form.
"#,
    }
    CMP0009 {
        title: "Validation rule on the wrong type",
        message: "`{rule}` does not apply to `{type}`",
        fix: "Use `min` and `max` on numbers, `length` on text and lists, and `email`, `url` and `one_of` on text.",
        explanation: r#"
Each validation rule of an `Input` applies to some types only: `min` and `max` to numbers,
`length` to `String` and `Vec` (characters or items), `email`, `url` and `one_of` to
`String`. On an `Option<T>` or `Patch<T>` field, the rule applies to `T` when there is a
value. For a fixed set of values, an enum is usually clearer than `one_of`.
"#,
    }
    CMP0010 {
        title: "Field type not supported",
        message: "`{type}` cannot be a field of an `{derive}`",
        fix: "Use text, numbers, booleans and lists of them; `Option<T>` and `Patch<T>` in an `Input`; another `Output` in an `Output`.",
        explanation: r#"
Every field of an `Input` is read from JSON or a query string, and every field of an
`Output` is written as JSON, so Balise must know how. It knows `String`, `bool`, integers,
`f64` and `Vec` of them; in an `Input`, `Option<T>` for an optional field and `Patch<T>` for
a partial update; in an `Output`, `Option<T>` (written `null`) and other structs that derive
`Output`. Identifiers, enums and dates join the list with the models.
"#,
    }
    CMP0011 {
        title: "Invalid AppError declaration",
        message: "Invalid `AppError` declaration: {reason}",
        fix: "Write each variant as `#[error(status = 409, code = \"ORD0001\", message = \"…\", fix = \"…\")]`, with a 4xx status and a code made of your own prefix and four digits.",
        explanation: r#"
`#[derive(AppError)]` goes on an enum without generics, whose variants have no fields or
named fields. Each variant has one `#[error(…)]` attribute with:

- `status`: a `4xx`. A `5xx` comes from a real failure (a database down, a bug), never from
  a choice of the application;
- `code`: a prefix of 2 to 4 uppercase letters and four digits (`ORD0001`). The prefix is
  the application's own: Balise's areas (`HTTP`, `VAL`, `DB`…) are reserved. A code is
  used once;
- `message`: what happened, for the client. Fields of the variant can be used in it:
  `"At most {max} lines"`;
- `fix`: what to do about it, in the same form.

The doc comment of a variant becomes its explanation in `balise explain`.
"#,
    }
    CMP0012 {
        title: "Log field value not supported",
        message: "`{type}` cannot be the value of a log field",
        fix: "Log text, numbers or booleans: `order_id = id`, or `status = status.to_string()`.",
        explanation: r#"
A field of `balise::log::info!(name = value, "…")` becomes a JSON field in production logs,
so its value is text, a number, a boolean, or an `Option` of one of them. Other types are
converted first, usually with `.to_string()`.
"#,
    }
    CMP0013 {
        title: "Project file not found",
        message: "`balise::run!()` cannot find {file}: {reason}",
        fix: "Put `{file}` next to `Cargo.toml`; `balise new` creates both files, and they may hold only comments.",
        explanation: r#"
`balise::run!()`, in `main.rs`, embeds the project's files in the binary, so that a
deployment needs nothing but the binary: the production image never misses its
configuration. Both are read next to `Cargo.toml`, nowhere else:

- `balise.toml`, the configuration, is required;
- `budgets.toml`, the per-handler budgets, is optional, but one in another folder would be
  ignored silently, so it is an error.

When a file with a close name, or one in another folder, exists, the message names it.
Values that differ between deployments (ports, secrets) come from environment variables,
which override the embedded configuration.
"#,
    }
    CMP0014 {
        title: "Project file is not valid TOML",
        message: "{file} is not valid TOML: {reason}",
        fix: "Fix the line the message names: a `key = value` per line, text in double quotes, sections written `[name]`.",
        explanation: r#"
`balise::run!()` reads `balise.toml` and `budgets.toml` when the app is compiled, so a
syntax error is reported by `cargo check` and `balise check`, before the app runs. Whether
each key and value makes sense is checked when the app starts, by the rules of its
environment (`CFG` codes), and by `config check`.
"#,
    }
    CMP0015 {
        title: "Invalid state declaration",
        message: "Invalid `#[balise::state]` function: {reason}",
        fix: "Write `#[balise::state] fn name() -> Type`: no parameters, a return type, `async` and `Result<Type>` allowed.",
        explanation: r#"
A `#[balise::state]` function creates one value of shared state when the app starts. It
takes no parameters and returns the value, or `Result<Type>` when creating it can fail; it
may be `async`. Handlers and tasks read the value with `State<Type>`.

For state that changes, return a `Shared<Type>` and read it with `State<Shared<Type>>`.
"#,
    }
    CMP0016 {
        title: "Not a task",
        message: "This cannot be spawned as a task",
        fix: "Pass an `async move` closure whose parameters are services (`State<T>`, and `Db` or `Mailer` with their modules) and whose body ends with `Ok(())`; capture other values with `move`.",
        explanation: r#"
`balise::spawn` takes an `async move` closure, with at most four parameters, that returns
`Result<()>`, so that `?` works inside and errors are logged. Its parameters are services
Balise provides, like a handler's: `async move |cache: State<PriceCache>| …`. What comes from
the request (its body, its path, the current user) is not a service: copy it into the
task with `move`.
"#,
    }
    CMP0017 {
        title: "Invalid file field",
        message: "Invalid file field `{field}`: {reason}",
        fix: "Declare a file as `#[input(max_size = \"5MB\", types(\"image/png\", \"image/jpeg\"))] photo: File`, with `max_count = …` for a `Vec<File>`.",
        explanation: r#"
A field of type `File`, `Option<File>` or `Vec<File>` declares how large its files can be
and which types they may have; a `Vec<File>` also declares how many files it takes:

    #[derive(Input)]
    struct UploadPhotos {
        #[input(max_size = "5MB", types("image/png", "image/jpeg"), max_count = 4)]
        photos: Vec<File>,
    }

These declarations set the route's body limit and timeout, and the generated client checks
them before sending. Types are checked against the file's content, so only types Balise can
recognize are accepted; the message lists them. A `File` is read from a multipart body
only: it cannot be part of a `Query<T>`.
"#,
    }
    CMP0018 {
        title: "Invalid page declaration",
        message: "Invalid `page(…)` on `{function}`: {reason}",
        fix: "Write `page(default = 50, max = 500)` with whole numbers from 1, `default` at most `max`, on a handler that reads `page: PageParams`.",
        explanation: r#"
Paginated routes read `page: PageParams`. Pages hold `pagination.per_page` items by default
and `pagination.max_per_page` at most, from `balise.toml`. A route that needs other limits
declares them on its attribute, without changing the others:

    #[get("/exports/orders", page(default = 500, max = 5000))]
    async fn export(page: PageParams) -> Result<Page<OrderRow>> { … }

`page(…)` on a handler that does not read `PageParams` would have no effect, so it is an
error.
"#,
    }
    CMP0019 {
        title: "Panics abort the process",
        message: "This build aborts on panic (`panic = \"abort\"`): one panic would stop the whole server",
        fix: "Remove `panic = \"abort\"` from the profiles in `Cargo.toml`.",
        explanation: r#"
Balise catches a panic in the request or the task that raised it, so that one bug fails one
request instead of every request in progress. With `panic = "abort"`, nothing can be caught:
the process stops at the first panic. `balise::run!()` refuses to compile such a build.
"#,
    }
    CMP0020 {
        title: "Resource folder not declared",
        message: "`src/{module}/` is not declared in `src/main.rs`: its routes are never compiled",
        fix: "Add `mod {module};` to `src/main.rs`, or remove the folder.",
        explanation: r#"
Rust compiles only the modules a crate declares. A folder under `src/` without its `mod`
line is ignored: its routes do not exist, and nothing else reports it, because nothing
refers to them. `main.rs` declares one module per resource:

    mod orders;

    fn main() {
        balise::run!();
    }
"#,
    }
    CMP0021 {
        title: "Name from another framework",
        message: "Balise has no `{name}`: it comes from {origin}",
        fix: "{instead}",
        explanation: r#"
The name is common in another framework or library, and does not exist in Balise. The fix
gives the Balise way to do the same thing. `balise::prelude::*` brings what handlers use:

    use balise::prelude::*;

    #[get("/orders/{id}")]
    async fn get(id: Path<u32>, orders: State<Shared<Orders>>) -> Result<OrderView> { … }
"#,
    }
    CMP0022 {
        title: "Value kept across an await cannot move between threads",
        message: "`{variable}` is kept across an `.await`, and its type `{type}` cannot move between threads",
        fix: "End the use of `{variable}` before the `.await` (line {await_line}): put that code in a block `{ … }` that ends before it. Or use a type that can move: `Arc` instead of `Rc`, `Shared<T>` instead of `RefCell`.",
        explanation: r#"
The server runs handlers on several threads, and a handler can move to another thread at
each `.await`. Every value still in use after an `.await` moves with it, so its type must
allow it (`Send`). `Rc`, `RefCell` and the guard of a `std::sync::Mutex` do not.

    let total = {
        let cache = Rc::new(load());
        cache.total()
    }; // `cache` ends here
    save(total).await;
"#,
    }
    CMP0023 {
        title: "Private item",
        message: "`{name}` is private to its module",
        fix: "Add `pub` to `{name}` where it is declared{at}, so that other modules can use it.",
        explanation: r#"
Items are private to their module by default. A handler that uses a struct, a field or a
function of another module needs it declared `pub`:

    pub struct OrderView {
        pub id: u32,
    }
"#,
    }
    CMP0024 {
        title: "Mutable state outside Balise",
        message: "{what} is mutable state that Balise does not manage",
        fix: "Keep shared state in a `#[balise::state]` function returning `Shared<T>`, and take it in handlers as `State<Shared<T>>`. Within one function, `let mut` is enough.",
        explanation: r#"
Global and shared mutable state (`static mut`, `std::sync::Mutex`, `RefCell`) breaks under
concurrent requests, blocks the server's threads, or does not compile across an `.await`.
Balise's state is created once at startup and shared safely:

    #[balise::state]
    fn orders() -> Shared<Orders> {
        Shared::new(Orders::default())
    }

    #[get("/orders")]
    async fn list(orders: State<Shared<Orders>>) -> Result<Vec<OrderView>> { … }
"#,
    }
    CMP0025 {
        title: "Printing outside the logs",
        message: "`{name}!` writes outside Balise's logs",
        fix: "Use `balise::log::info!(\"…\", field = value)`, or `debug!`, `warn!`, `error!`: the line gets the request's id and reaches `balise dev` and production logs.",
        explanation: r#"
Balise's logs are one stream, in text in dev and in JSON in production, where each line
carries the request it belongs to. `println!`, `eprintln!` and `dbg!` write around it: their
lines have no level, no request, and break the JSON.

    balise::log::info!("Order paid", order_id = order.id);
"#,
    }
    CMP0026 {
        title: "Work started outside Balise",
        message: "`{name}` starts work that Balise does not follow",
        fix: "Use `balise::spawn(async move |…| { … })` for background work, and `balise::blocking(|| …)` for blocking code.",
        explanation: r#"
Balise waits for its tasks at shutdown, reports their panics and gives them their
services. A thread or a Tokio task started directly escapes all of it: it is cut at
shutdown, and its panics are lost.

    balise::spawn(async move |orders: State<Shared<Orders>>| {
        orders.write().await.archive_old();
        Ok(())
    });
"#,
    }
    CMP0027 {
        title: "Cargo stopped before compiling",
        message: "`cargo` stopped before compiling the code: {cause}",
        fix: "Read cargo's message: it usually points to `Cargo.toml`, or to a dependency that cannot be downloaded.",
        explanation: r#"
`balise check` runs `cargo clippy`. Cargo failed before reading the code: an invalid
`Cargo.toml`, a dependency that does not exist or cannot be downloaded, or a missing
component of the toolchain (`clippy`, installed by `rust-toolchain.toml` with rustup).
"#,
    }
}
