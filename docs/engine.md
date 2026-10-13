# Engine
 
Status: draft. This describes the target design; nothing is implemented yet.
 
This document covers what Balise does underneath application code: crate structure, startup, routing, request handling, errors, and process lifecycle. For what application code looks like, see [application.md](application.md).
 
## Own or delegate
 
The engine applies principle 7 from [architecture.md](architecture.md):
 
Owned by Balise:
 
- routing, extractors, validation, file uploads;
- error types, codes, log format;
- configuration;
- startup, shutdown, supervision in dev;
- diagnostics: crash reports, backtraces, request traces.
Delegated:
 
- HTTP parsing and connections: hyper;
- async runtime: Tokio;
- TLS and crypto: rustls;
- database wire protocols: drivers ([data.md](data.md)).
A delegated dependency is never re-exported by the facade and never appears in a public type, a user-facing error message, or the docs given to agents. If a delegated component causes trouble, we fix it upstream first and replace it only if that is justified.
 
## Crates and profiles
 
```
balise            facade, the only dependency of a project
balise-core       routing, extractors, errors, logs, config
balise-macros     procedural macros: routes, derives
balise-engine     hyper / Tokio wrapper, startup, shutdown
balise-realtime   WebSocket, SSE, pub/sub
balise-ts         TypeScript generation
balise-orm        ORM                       feature `orm`
balise-auth       authentication            feature `auth`
balise-mail       sending mail              feature `mail`
balise-dev        MCP, instrumentation      feature `dev`
balise-cli        the `balise` binary       separate, installed on the host
```
 
Profiles:
 
- `base` (default): core, engine, realtime, TypeScript generation.
- `full`: `base` plus every optional module.
- `dev`: enabled by `balise dev`, which builds with `--features dev`. A plain `cargo build` does not have it, and release builds never do. `test` is enabled the same way by `balise test`.
Rules:
 
- The core and the facade assume `base` is present and never assume an optional module.
- Optional modules plug into extension points defined by the core (traits). An integration between two optional modules, such as auth storage in the ORM, compiles only when both features are enabled.
- CI runs `cargo hack check --feature-powerset --depth 2` plus a list of real-world combinations.
## Startup
 
`balise::run!()` does the following, in order:
 
1. Load configuration from environment variables and `balise.toml`, then validate it (see [Configuration](#configuration)).
2. Install the log subscriber, including the `log` bridge, so every event uses Balise's format.
3. Install Balise's panic hook, then publish the crash reports left by previous runs, if any (`RUN0007`).
4. Build the routing table from the routes collected at compile time.
5. Create shared state: database pool, the values of `#[balise::state]` functions, pub/sub hub. Check that every `State<T>` a route reads exists.
6. Apply pending migrations under a lock, or refuse to start, depending on configuration ([data.md](data.md)).
7. Start listening, then report ready on `/ready`.

`/health` (the process runs) and `/ready` (it accepts traffic) are served at the root, outside the API base, for orchestrators. A route that would take one of them, possible only when `api.prefix` is `/`, stops the startup (`HTTP0014`): `/ready` must keep failing during a shutdown, which an application's handler would not do. Their requests are logged at `debug` only: a probe every few seconds would drown the other lines.
The application is built once. Nothing above runs again per request.

`balise::run!()` is synchronous and never returns: it creates Tokio's multi-threaded runtime itself, so application code has no `#[tokio::main]` and no `Result` in `main`. A fatal error at startup (the port is taken: `HTTP0001`) is reported with its code, and the process exits with a non-zero code.

The server speaks HTTP/1.1, what reverse proxies and the Vite proxy use towards the app. HTTP/2 comes with built-in TLS.
 
## Configuration
 
Strict in dev, fail at startup in prod for what matters, never fail at runtime.
 
| Problem | Dev | Prod |
| --- | --- | --- |
| Invalid or missing setting that affects security or data (TLS, CORS, secrets, database URL, auth) | startup fails | startup fails |
| Invalid setting with a safe default (log level, pool size, compression, timeouts) | startup fails | default used, warning logged at startup and on `/health` |
| Unknown key (likely a typo) | startup fails, nearest valid key suggested | ignored, warning logged with the nearest valid key |
 
- Balise never falls back silently on a setting that affects security or data. A service that starts in an unsafe state is worse than one that does not start.
- Failing at startup in prod does not cause downtime: during a rolling deployment, the orchestrator keeps the previous version running because the new one never becomes ready.
- `balise config check --env prod` runs the same validation without starting the app, so CI catches a bad production config before deployment. It is never more lenient than a startup: every problem is an error, even those that only warn when production starts.
- At runtime, nothing in configuration can make a request fail: everything is validated once, at startup.
## Routing
 
The router is Balise's own, entirely: its answers (`404`, `405`, `balise why`) are part of what the agent sees.

- Each route attribute (`#[get]`, `#[post]`…) registers the handler in a compile-time collection. There is no central router to maintain. The collection is a static array filled by the linker (`linkme`), hidden behind Balise's own registry module: nothing runs before `main`, and the mechanism can be replaced without touching the macros or application code.
- At startup, routes go into a tree of path segments that is never modified afterwards. The API base comes from configuration: `api.prefix` (`/api` by default), followed by `/v{api.version}` when a version is set. Route attributes never include it.
- A static segment wins over a parameter: `/orders/new` is tried before `/orders/{id}`. When a path matches several routes, the first one in that order that accepts the method is chosen; when none does, the answer is `405` (`HTTP0008`) with an `Allow` header.
- Paths are strict: `/orders/` is not `/orders`. Parameters are whole segments (`{id}`), percent-decoded.
- Startup fails if two handlers claim the same method and path shape (`/orders/{id}` and `/orders/{order_id}` are the same), with `HTTP0009` naming both handlers and their locations.
- The route macro also checks the handler's signature at compile time: unknown parameter type, two `Body` parameters, a `Path` count that does not match the route, `page(…)` on a handler without `PageParams`, an `Input` with files read by `Query`. Each case has its own error message.
## Handling a request
 
1. The engine accepts the connection and parses the request (hyper underneath, invisible to the application).
2. Balise assigns a request ID and starts a log span.
3. The router finds the handler, or answers `404` / `405`.
4. Middleware runs: the route's timeout, which covers everything after this step, CORS, compression.
5. Extraction builds each parameter in the order of the signature, except the body, always last: a request that a cheaper parameter rejects (path, query, `CurrentUser`) is answered before a byte of its body is read. A path value of the wrong type answers `404` (`HTTP0010`), a body that is not a JSON object `400` (`VAL0002`), another content type `415` (`VAL0003`), a body over the limit `413` (`HTTP0011`, read no further than the limit), and validation errors `422` (`VAL0001`) with a code and a message per field, without calling the handler. Every request gets an identifier (`req_7f3a2c`), in every error body and in logs.
6. The handler runs.
7. The result is converted into a response. Errors become responses with their code (see below).
8. The request is logged once, with its duration, status, query count, and request ID.
### Response format
 
- Successful responses carry the `Output` as plain JSON, with no envelope. The HTTP status comes from the return type (`Created<T>` is `201`, `NoContent` is `204`…), never from a number in application code.
- Error responses always use the same body:
```json
{
  "error": {
    "code": "VAL0001",
    "message": "Invalid fields: contact",
    "fields": { "contact": { "code": "VAL0006", "message": "must be an email" } },
    "request_id": "req_7f3a2c"
  }
}
```
 
`fields` appears only for validation errors, with one code per field: missing (`VAL0004`), wrong type (`VAL0005`) or rule not met (`VAL0006`). In dev, the body adds `location`, `fix` and `cause`. In prod, a `5xx` keeps its status with only a generic code (`HTTP0500`), a generic message and the request ID; a `4xx` keeps its code and message.
 
Why a typed return rather than a response object built in the handler: the output type is known at compile time, so Balise can generate TypeScript types, check each route's shape, and describe it through MCP. A response object (`Response::ok(json)`) would hide the shape until runtime and open a second way to signal errors. Success or failure is carried by the HTTP status; the generated TypeScript client exposes it to the front end as `{ ok: true, data }` or `{ ok: false, error }`, so no envelope is needed on the wire.
 
### Per-route limits
 
Every route has its own body size limit and timeout, deduced from its signature like its [budgets](agent-experience.md#budgets):
 
- a route without files gets the defaults: `limits.body_size` (2 MB) and `limits.timeout` (30 s);
- a route that accepts files gets the sum of its declared files (`max_size` × `max_count`) plus `limits.body_size` for the other fields, and a timeout longer by the time those files take at 1 MB/s;
- a route meant to receive more or to take longer, such as a bulk import, sets `body_size` or `timeout` in `budgets.toml`, under the handler's name. Raising them is a sensitive decision, like any budget. A `body_size` below the route's declared files stops the startup (`CFG0011`): one place says how large an upload can be.

```toml
# budgets.toml
["imports::create"]
body_size = "20MB"
timeout = "2m"
```

The global limit is never raised for one route's sake. A body over the limit answers `413` (`HTTP0011`), with the limit in the message. When the timeout runs out, the handler is cancelled: a client still sending its body gets `408` (`HTTP0012`), otherwise the answer is `503` (`HTTP0013`).

`budgets.toml` is embedded by `balise::run!()` like `balise.toml`, and required like it, even with only comments: cargo only rebuilds for files that already exist, so an optional file created later would be ignored until the next change of code. A section naming no handler (`CFG0009`), an unknown budget or an invalid value (`CFG0010`) stops the startup; `config check` reports them too.

File bodies are streamed to `<storage.dir>/.uploads` as they arrive, never held in memory, with the disk writes on the blocking pool. Each file is checked while it is written: its size against `max_size`, stopping as soon as it is passed (`FILE0001`); its count (`FILE0003`); its first bytes against the declared types (`FILE0002`), not its name or `Content-Type` header; text types as UTF-8 to the end. A file not stored by the handler is deleted with the request. The multipart parser is Balise's own ([alternatives.md](alternatives.md)).

Handlers are monomorphized up to extraction, then type-erased behind one boxed interface. This keeps compile times down without losing compile-time checks on parameters.
 
## Execution model
 
- One long-lived process, on Tokio's multi-threaded runtime.
- Request-scoped values (parameters, connection, transaction) are dropped at the end of the request.
- Process-scoped values (config, pool, state) live until shutdown. State is created at startup by `#[balise::state]` functions, registered like routes, and stored by type; every `State<T>` a route reads is checked before the server listens. Mutable state uses `Shared<T>`, backed by an async lock that is not poisoned by a panic.
- `balise::blocking` runs CPU-heavy work on Tokio's blocking pool, with the request's context so its logs keep their request. Work cannot be moved off the event loop automatically: in dev, every poll of a handler or task is timed, and one longer than 10 ms is reported with the handler (`PERF0002`). The production binary has no such timing.
- `balise::spawn` runs background tasks: an `async` closure whose parameters are services (`FromTask`, like `FromRequest` for handlers). It inherits the request's context for its logs; its error or panic is logged with the line of `spawn`. The task gets its own `Db`, with the same transaction rules as a request ([data.md](data.md#side-effects-after-commit)).
- **A handler runs to the end even if the client disconnects**: hyper drops the future on disconnection, so Balise runs each handler in its own task, and only the response is discarded.
- **A route's timeout cancels the handler.** A timeout is a cancellation: the transaction rolls back, but external calls already sent are not undone. Without that, the timeout would protect from nothing. The client gets `503` (`HTTP0013`), or `408` (`HTTP0012`) if it was still sending its body.
## Errors
 
- One error type, `balise::Error`, carries an HTTP status, a stable code, a message, a fix, where it was raised in application code, and the original cause. It does not implement `std::error::Error`, so `?` converts any other error into it; the location comes from `#[track_caller]` on the conversion, without a backtrace.
- Applications return their own errors only through `#[derive(AppError)]`: there are no ad hoc constructors (`Error::conflict("…")`), which would carry neither a code nor a fix. Two variants with the same code stop the startup (`RUN0002`).
- Codes are grouped by area: `HTTP`, `VAL` (validation), `DB`, `AUTH`, `CFG`, `EXT`; applications add their own through `#[derive(AppError)]`.
- Errors from delegated dependencies that Balise recognizes (hyper, I/O, drivers) are converted to Balise errors with their own codes. Anything unrecognized becomes `EXT0001`, which keeps the original message.
- Connection errors caused by the client (a closed connection, bytes that are not HTTP) are `debug` events with `HTTP0002`: they are not errors of the server.
- In dev, error responses include the location, the cause and the suggested fix. In prod, a `5xx` is a generic `HTTP0500` with the request ID, and the full error goes to the logs.
Full error and log format: [errors-and-logs.md](errors-and-logs.md).
 
## Panics and supervision
 
- A panic is caught where it happens: in a handler, the request answers `500`; in a task, the task stops; in `balise::blocking`, it continues in the caller. Either way, one `RUN0006` event says where the panic was raised (the line of the `unwrap()`, thanks to `#[track_caller]`) and shows the application's frames, the others folded. A panic in a `#[balise::state]` function stops the startup with `RUN0005`.
- Balise's panic hook replaces Rust's message on stderr for the panics it catches. It captures the backtrace only when a panic happens: requests pay nothing for it.
- Shared state uses locks that are not poisoned, so one panic does not break the requests that follow.
- `panic = "abort"` in a profile is a compile error (`CMP0019`): one panic would stop every request in progress.
- A panic Balise cannot catch stops the process: one in a thread started by hand, or a second one while the first unwinds (in a `Drop`). The hook writes a crash report to `<storage.dir>/.balise/crashes/` before the process stops; the next start publishes it (`RUN0007`) and deletes it. When several instances share the folder, the first to start publishes them.
- Stack overflows and memory exhaustion leave no time to write anything: the process stops with Rust's message on stderr.
  - in dev, the CLI supervises the server, restarts it, and shows that message;
  - in prod, the orchestrator (Docker, systemd, Kubernetes) restarts the process and keeps its output.
## Diagnostics
 
Delegated dependencies do the work below the facade, so an agent debugging a routing or network problem will be tempted to investigate them. Balise makes its own path shorter than that one:
 
- No error, log, or crash report names a delegated crate. Network and connection errors carry `HTTP` or `EXT` codes with their own explanation.
- Backtraces show the application's frames, from the one that panicked, and fold every run of other frames into one line (`… 14 frames outside the application`). Balise reads them from the standard library's text form; in production, frames have file and line only when the release profile keeps line tables.
- `balise why <METHOD> <path>` explains a `404` or `405`: the routes that almost match and why they don't.
- `balise trace <request_id>` (also an MCP tool) shows what happened to a request in dev: routing decision, extracted parameters and limits, then its events; queries come with the ORM. It reads the dev journal ([environments.md](environments.md#dev-with-docker)).
- The generated `AGENTS.md` tells agents to debug through these tools and never inside dependencies.
This cannot stop an agent from opening `Cargo.lock` or a dependency's source, but nothing points it there.
 
## Shutdown
 
On `SIGTERM`, or Ctrl-C:
 
1. `/ready` starts failing, so load balancers stop sending traffic.
2. The server stops accepting connections; open connections close once their request in progress is answered.
3. Requests in progress and background tasks (`balise::spawn`) finish, for `shutdown.timeout` at most (10 s by default). What is left then is cut and reported (`RUN0008`).
4. WebSocket connections receive a close frame.
5. The pool closes and the process exits with `0`.

The orchestrator must wait longer than `shutdown.timeout` before it kills the process. `balise dev` gives the generated `compose.yaml` a `stop_grace_period` of `shutdown.timeout` plus 5 s, read from `balise.toml` at each start and written in `.env` for `docker compose` by hand, since Docker's default (10 s) leaves no margin; Kubernetes waits 30 s by default.
## Defaults
 
| Setting | Default |
| --- | --- |
| Request timeout | 30 s; routes with files: plus their declared size at 1 MB/s; per route: `timeout` in `budgets.toml` |
| Body size limit | 2 MB; routes with files: computed from the declared file sizes; per route: `body_size` in `budgets.toml` |
| Page size | 20 items, 100 at most; per route: `page(default = …, max = …)` on its attribute |
| Database query timeout | 10 s |
| Shutdown timeout | 10 s (`shutdown.timeout`) |
| Health endpoints | `/health` (process alive), `/ready` (ready for traffic), at the root |
 
All defaults can be changed in `balise.toml`.
 
## The `dev` feature
 
Compiled only in dev builds. A binary built with `dev` refuses to start with `BALISE_ENV=prod`.
 
- MCP server, reached through `balise mcp` on the host ([agent-experience.md](agent-experience.md)).
- The dev journal, written for `balise logs` and `balise trace`, and the binary's `routes` and `why <METHOD> <path>` commands, for `balise routes` and `balise why`.
- Detailed instrumentation: N+1 detection, query plans, blocked event loop.
- Detailed error pages.
- Proxy to the Vite dev server ([web.md](web.md)).
## Open questions

- Middleware: no model yet, the request pipeline is fixed code in the engine. Whether to build one, on Tower or in-house, is discussed with the first cross-cutting needs (sessions, CSRF, rate limits), in step 8.1 of the plan ([alternatives.md](alternatives.md)).
- Readiness checks: `/ready` only says the process accepts traffic. Modules such as the database pool will add their checks; whether an application can add its own (a service it cannot work without) is decided when a real case appears.
- Handler execution: multi-threaded runtime (futures must be `Send`) or `spawn_local` on a runtime per core (no `Send` requirement). Also in the log of alternatives.
- Compile-time route collection mechanism: distributed slices (`linkme`) or registration at startup (`inventory`).
