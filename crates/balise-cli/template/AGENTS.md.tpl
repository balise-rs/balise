# {{name}}

A web application built with Balise, a Rust web framework designed for coding agents.
Its API is not in your training data: never guess it.{{#if example}} `src/orders/` is an example
resource that uses most of it: read it before writing a resource.{{/if}}

## Working loop

1. After each coherent change, run `balise check` instead of `cargo check`: it also runs the
   project's lints, and gives each problem its code, where to fix it and how. It runs again
   when you finish a task, and blocks on what is left.
2. An unclear error code, Balise's (`HTTP0003`) or Rust's (`E0308`): `balise explain <CODE>`.
   Every setting of `balise.toml`: `balise explain config`.
3. Run the app with `balise dev --detach`, then `balise dev status` until it answers;
   `balise dev stop` stops it. It rebuilds and restarts on its own at each change.

## Layout

```
src/
  main.rs         declares one module per resource and starts the app; nothing else goes here
  orders/         one folder per resource
    mod.rs        handlers
    api.rs        inputs, outputs and errors: the contract with clients
    model.rs      data and state
balise.toml       configuration; secrets come from environment variables, never from here
budgets.toml      per-handler limits (`body_size`, `timeout`)
```

## API in brief

- `use balise::prelude::*;` brings what handlers use.
- A route is an `async fn` with `#[get("/orders/{id}")]`, `#[post(…)]`, `#[put(…)]`,
  `#[patch(…)]` or `#[delete(…)]`, found on its own once its module is declared in `main.rs`.
  Paths never include the API prefix: `#[get("/orders")]` is served at `/api/orders`.
- Parameters: `Path<T>`, `Query<T>` and `Body<T>` where `T` derives `Input`; `page: PageParams`;
  `State<T>` for shared state; `RequestId`.
- Return a struct that derives `Output`, a `Vec` of them, `Created<T>`, `NoContent` or
  `Redirect`, or `Result<…>` of one of these: the status comes from the type. Errors are an
  enum that derives `AppError`, each variant with
  `#[error(status = 404, code = "ORD0001", message = "…", fix = "…")]`.
- Shared state: a `#[balise::state]` function returning `Shared<T>`, read with
  `State<Shared<T>>` and `.read().await` or `.write().await`.
- Logs: `balise::log::info!("Order paid", order_id = id)`, and `debug!`, `warn!`, `error!`.
- Background work: `balise::spawn(async move |…| { … })`; blocking code: `balise::blocking(|| …)`.

## Rules

- No mutable state outside Balise: no `static mut`, `std::sync::Mutex` or `RefCell`.
- No `println!`, `print!`, `eprintln!`, `eprint!` or `dbg!`: use `balise::log`.
- No `std::thread::spawn`, `tokio::spawn` or `spawn_blocking`: use `balise::spawn` and
  `balise::blocking`.
- Never edit `.balise/policy.toml`: it says what the user decides, and only they change it.

## Debugging

Investigate through Balise, never inside its dependencies:

- `balise routes`: every route, its handler and limits;
- `balise why GET /api/orders/1`: which route answers a request, or why none does;
- `balise trace <request_id>`: what happened to a request (the `request_id` of a response);
- `balise logs`: recent events, `--code <CODE>`, `--level warn`, `--since 10m`;
- `balise config check --env prod`: the configuration, without starting the app.
