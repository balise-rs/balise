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
- `dev`: enabled by `balise dev` and in debug builds. Never present in release builds.
Rules:
 
- The core and the facade assume `base` is present and never assume an optional module.
- Optional modules plug into extension points defined by the core (traits). An integration between two optional modules, such as auth storage in the ORM, compiles only when both features are enabled.
- CI runs `cargo hack check --feature-powerset --depth 2` plus a list of real-world combinations.
## Startup
 
`balise::run()` and `balise::app().run()` do the following, in order:
 
1. Load configuration from environment variables and `balise.toml`, then validate it (see [Configuration](#configuration)).
2. Install the log subscriber, including the `log` bridge, so every event uses Balise's format.
3. In production, publish the crash report left by the previous run, if any.
4. Build the routing table from the routes collected at compile time.
5. Create shared state: database pool, declared `State` values, pub/sub hub.
6. Apply pending migrations under a lock, or refuse to start, depending on configuration ([data.md](data.md)).
7. Start listening, then report ready on `/ready`.
The application is built once. Nothing above runs again per request.
 
## Configuration
 
Strict in dev, fail at startup in prod for what matters, never fail at runtime.
 
| Problem | Dev | Prod |
| --- | --- | --- |
| Invalid or missing setting that affects security or data (TLS, CORS, secrets, database URL, auth) | startup fails | startup fails |
| Invalid setting with a safe default (log level, pool size, compression, timeouts) | startup fails | default used, warning logged at startup and on `/health` |
| Unknown key (likely a typo) | startup fails, nearest valid key suggested | ignored, warning logged with the nearest valid key |
 
- Balise never falls back silently on a setting that affects security or data. A service that starts in an unsafe state is worse than one that does not start.
- Failing at startup in prod does not cause downtime: during a rolling deployment, the orchestrator keeps the previous version running because the new one never becomes ready.
- `balise config check --env prod` runs the same validation without starting the app, so CI catches a bad production config before deployment.
- At runtime, nothing in configuration can make a request fail: everything is validated once, at startup.
## Routing
 
- Each route attribute (`#[get]`, `#[post]`…) registers the handler in a compile-time collection. There is no central router to maintain.
- At startup, routes go into a radix tree that is never modified afterwards. The API prefix from configuration (`/api` by default) is added to every route then; route attributes never include it.
- Startup fails if two handlers claim the same method and path, with an error naming both handlers.
- The route macro also checks the handler's signature at compile time: unknown parameter type, two `Body` parameters, a `Path` count that does not match the route. Each case has its own error message.
## Handling a request
 
1. The engine accepts the connection and parses the request (hyper underneath, invisible to the application).
2. Balise assigns a request ID and starts a log span.
3. The router finds the handler, or answers `404` / `405`.
4. Middleware runs: the route's timeout and body size limit, CORS, compression, authentication when enabled.
5. Extraction builds each parameter: path, query, body, then services (`Db`, `State`, `CurrentUser`). Validation errors answer `422` without calling the handler.
6. The handler runs.
7. The result is converted into a response. Errors become responses with their code (see below).
8. The request is logged once, with its duration, status, query count, and request ID.
### Response format
 
- Successful responses carry the `Output` as plain JSON, with no envelope. The HTTP status comes from the return type (`Created<T>` is `201`, `NoContent` is `204`…), never from a number in application code.
- Error responses always use the same body:
```json
{
  "error": {
    "code": "VAL0003",
    "message": "Validation failed",
    "fields": { "contact": "must be an email" },
    "request_id": "req_7f3a2c"
  }
}
```
 
`fields` appears only for validation errors. In prod, `message` is generic for `5xx` errors.
 
Why a typed return rather than a response object built in the handler: the output type is known at compile time, so Balise can generate TypeScript types, check each route's shape, and describe it through MCP. A response object (`Response::ok(json)`) would hide the shape until runtime and open a second way to signal errors. Success or failure is carried by the HTTP status; the generated TypeScript client exposes it to the front end as `{ ok: true, data }` or `{ ok: false, error }`, so no envelope is needed on the wire.
 
### Per-route limits
 
Every route has its own body size limit and timeout, computed at compile time from its signature:
 
- a route without files gets the defaults (2 MB, 30 s);
- a route that accepts files gets the sum of its declared file sizes plus a margin for the other fields, and a timeout scaled to that size, unless the route sets one.
The global limit is never raised for one route's sake.
 
File bodies are streamed to a temporary file as they arrive, never held in memory. The size is checked while streaming: the upload stops as soon as it exceeds the limit. The declared file type is checked against the file's content (magic bytes), not only its `Content-Type` header.
 
Handlers are monomorphized up to extraction, then type-erased behind one boxed interface. This keeps compile times down without losing compile-time checks on parameters.
 
## Execution model
 
- One long-lived process, on Tokio's multi-threaded runtime.
- Request-scoped values (parameters, connection, transaction) are dropped at the end of the request.
- Process-scoped values (config, pool, state) live until shutdown. The framework wraps them in `Arc`; mutable state uses `Shared<T>`, backed by an async lock that is not poisoned by a panic.
- `balise::blocking` runs CPU-heavy work on Tokio's blocking pool. Work cannot be moved off the event loop automatically: in dev, Balise detects tasks that hold the event loop for more than 10 ms and reports the handler.
- `balise::spawn` runs background tasks and logs their failures.
## Errors
 
- One error type, `balise::Error`, carries an HTTP status, a stable code, a message, and the original cause.
- Codes are grouped by area: `HTTP`, `VAL` (validation), `DB`, `AUTH`, `CFG`, `EXT`; applications add their own through `#[derive(AppError)]`.
- Errors from delegated dependencies that Balise recognizes (hyper, I/O, drivers) are converted to Balise errors with their own codes. Anything unrecognized becomes `EXT0001`, which keeps the original message.
- In dev, error responses include the cause and the suggested fix. In prod, they include only a generic message and the request ID.
Full error and log format: [errors-and-logs.md](errors-and-logs.md).
 
## Panics and supervision
 
- A panic in a handler is caught per request and turns into a `500` with a Balise log entry.
- Shared state uses locks that are not poisoned, so one panic does not break the requests that follow.
- Aborts, stack overflows, and allocation failures cannot be caught in the process:
  - in dev, the CLI supervises the server, restarts it, and shows the crash report;
  - in prod, the orchestrator (Docker, systemd, Kubernetes) restarts the process, and the next start publishes the crash report.
## Diagnostics
 
Delegated dependencies do the work below the facade, so an agent debugging a routing or network problem will be tempted to investigate them. Balise makes its own path shorter than that one:
 
- No error, log, or crash report names a delegated crate. Network and connection errors carry `HTTP` or `EXT` codes with their own explanation.
- Backtraces in dev show application frames first and fold framework and dependency frames into one line (`… 14 frames in the engine`).
- `balise why <METHOD> <path>` explains a `404` or `405`: the routes that almost match and why they don't.
- `balise trace <request_id>` (also an MCP tool) replays what happened to a request: routing decision, extracted parameters, queries, conversion of the error.
- The generated `AGENTS.md` tells agents to debug through these tools and never inside dependencies.
This cannot stop an agent from opening `Cargo.lock` or a dependency's source, but nothing points it there.
 
## Shutdown
 
On `SIGTERM`:
 
1. `/ready` starts failing, so load balancers stop sending traffic.
2. The server stops accepting connections.
3. In-flight requests finish, up to a configurable drain timeout (default 30 s).
4. WebSocket connections receive a close frame.
5. The pool closes and the process exits.
## Defaults
 
| Setting | Default |
| --- | --- |
| Request timeout | 30 s; routes with files: scaled to the declared size |
| Body size limit | 2 MB; routes with files: computed from the declared file sizes |
| Database query timeout | 10 s |
| Shutdown drain timeout | 30 s |
| Health endpoints | `/health` (process alive), `/ready` (ready for traffic) |
 
All defaults can be changed in `balise.toml`.
 
## The `dev` feature
 
Compiled only in dev builds. A binary built with `dev` refuses to start with `BALISE_ENV=prod`.
 
- MCP server, reached through `balise mcp` on the host ([agent-experience.md](agent-experience.md)).
- Detailed instrumentation: N+1 detection, query plans, blocked event loop.
- Detailed error pages.
- Proxy to the Vite dev server ([web.md](web.md)).
## Open questions
 
- Middleware: build on Tower internally or write a small in-house model. Recorded in the log of alternatives ([alternatives.md](alternatives.md)).
- Handler execution: multi-threaded runtime (futures must be `Send`) or `spawn_local` on a runtime per core (no `Send` requirement). Also in the log of alternatives.
- Compile-time route collection mechanism: distributed slices (`linkme`) or registration at startup (`inventory`).
