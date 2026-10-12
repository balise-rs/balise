# Errors and logs

Status: draft. This describes the target design; nothing is implemented yet.

Everything Balise reports, at compile time or at runtime, follows one format that both a human and an agent can act on. This document defines that format. For how errors become HTTP responses, see [engine.md](engine.md#response-format).

## Goals

- **Actionable**: every error says what happened, where in the user's code, and how to fix it.
- **Stable**: a code means the same thing forever, so agents, docs and searches can rely on it.
- **One source, two renderings**: each event is produced once, as structured data, then rendered for humans or machines.
- **Nothing escapes the format by default**: output from dependencies is captured and re-emitted (principle 4 in [architecture.md](architecture.md)).

## Anatomy of an error

| Field | Example |
| --- | --- |
| Code | `DB0042` |
| Title | `Query returned no row` |
| Message | `No Order with id ord_01JB8XQ5` |
| Location | `src/orders/mod.rs:27`, in `orders::get` |
| Fix | `Use an id from GET /api/orders. If a missing row is expected, use Order::find, which returns an Option.` |
| Doc | `balise explain DB0042` |
| Request ID | `req_7f3a2c` (runtime errors only) |
| Cause | the original error, kept in full |

The location always points to application code, never to the framework or a dependency.

## Codes

A code is an area prefix and four digits.

| Prefix | Area |
| --- | --- |
| `CMP` | compile-time errors raised by Balise macros and `balise check` |
| `CFG` | configuration |
| `HTTP` | routing, connections, protocol |
| `VAL` | input validation |
| `FILE` | uploads |
| `DB` | database and ORM |
| `AUTH` | authentication and permissions |
| `RT` | realtime (WebSocket, SSE) |
| `PERF` | performance warnings (N+1, slow query, blocked event loop…) |
| `RUN` | the life of the process: startup checks, shared state, panics, crashes, shutdown |
| `EXT` | errors from outside Balise that it could not classify |

Applications define their own codes with `#[derive(AppError)]` and their own prefix (`ORD0001`), see [application.md](application.md#errors).

Rules:

- A code is never reused or renumbered. A retired code stays in the registry, and its explanation points to its replacement.
- Every code is declared once, in a registry in the source with one file per area: code, title, message template, fix, explanation. Messages and `balise explain` read it directly; there is no separate doc page.
- The same registry holds topics (`pagination`, `budgets`, `config`…), one file per topic, for `balise explain <topic>` and the `explain` MCP tool.
- A validation failure has one code for the response, `VAL0001`, and one code per field in `fields`, by cause: missing (`VAL0004`), wrong type (`VAL0005`), rule not met (`VAL0006`). The front end can show or translate each field's message from its code.
- A test fails if a code is raised anywhere without being in the registry.
- `EXT0001` wraps anything Balise could not classify. It always keeps the original message.

## Events

Errors, warnings and ordinary logs are all events with the same fields:

| Field | Always | Notes |
| --- | --- | --- |
| `time` | yes | UTC, RFC 3339 |
| `level` | yes | `error`, `warn`, `info`, `debug`, `trace` |
| `message` | yes | |
| `code` | errors and warnings | |
| `request_id` | inside a request | |
| `handler` | inside a request | e.g. `orders::create` |
| `location` | errors and warnings | file and line in application code |
| `fix` | errors and warnings | |
| `cause` | errors with an original cause | the original error, in full |
| other fields | no | structured key-value pairs |

Each request produces one summary event when it ends, at `info`: method, path, handler, status, duration, number of queries, request ID, and the codes of any error or warning.

- **`5xx`**: the server failed. An `error` event with location, fix and cause, then the request's summary.
- **`4xx`**: the client sent something wrong; the server did its job. No separate event: the summary carries the code, the detail and the location. A bot probing ids does not fill the logs with errors.
- **Logs may say more than the response.** A `404` on another user's row says so in the logs; the response only says the row was not found.

Application code logs through `balise::log` (`info!`, `warn!`…), which produces the same events, with the request's ID and handler. The engine keeps the current request per task, so the core needs no runtime. `println!` is flagged by `balise check`.

## Renderings

The format depends on the environment and can be overridden with `log.format` in `balise.toml`.

Dev (terminal, colored):

```
12:01:58  GET /api/orders → request req_51e07a (orders::list returned 200 in 4 ms with 1 query)
12:02:11 ERROR DB0003  Cannot reach the database at db:5432
         at src/orders/mod.rs:31 in orders::create, request req_7f3a2c
         cause: connection refused (os error 111), 3 attempts in 5 s
         fix: Check that the database is running (`balise dev` starts it) and that DATABASE_URL points to it.
         more: balise explain DB0003
12:02:11  POST /api/orders → request req_7f3a2c (orders::create returned 500 in 5004 ms with 0 queries)
12:03:40  GET /api/orders/ord_01JB8XQ5 → request req_9b21e0 (orders::get returned 404 in 3 ms with 1 query)
          DB0042  No Order with id ord_01JB8XQ5 for usr_01JB7K2M: it belongs to another user
          at src/orders/mod.rs:26 in orders::get
          fix: Use an id from GET /api/orders. If a missing row is expected, use `Order::find`, which returns an Option.
          more: balise explain DB0042
```

- A request line reads as a sentence: path, request ID, then what the handler returned, how fast, with how many queries.
- The request ID of an event is on its `at` line, so it can be matched to its request when requests interleave.
- Every `fix:` line is followed by `more: balise explain CODE`. JSON lines carry the `code` field instead.
- Times are UTC in both renderings, so a dev line and a production line can be compared directly.
- The query count appears only when the `orm` module counts queries; without it, the line ends with the duration.
- A request that no handler answers (`404`, `405`) reads `(no handler, 404 in 1 ms)`, with the code and fix below.
- A `4xx` with field errors lists them under the request line, one per line (`contact: VAL0006 must be an email address`), and in a `fields` object in JSON, the same as in the response.
- The path is logged without its query string, which can carry secrets (`?token=…`). Headers are never logged.
- Logs go to stdout. The format and the minimum level come from the configuration; both renderers are pure functions of the event, and the engine is the only place that logs a request.

Colors carry the level (red for errors and `5xx`, yellow for warnings and `4xx`, dim for `2xx`). With `NO_COLOR`, or when the output is not a terminal, the text is the same without them.

Test and prod (one JSON object per line, on stdout):

```json
{"time":"2026-10-11T12:02:11.478Z","level":"error","code":"DB0003","message":"Cannot reach the database at db:5432","handler":"orders::create","location":"src/orders/mod.rs:31","fix":"Check that the database is running (`balise dev` starts it) and that DATABASE_URL points to it.","cause":"connection refused (os error 111), 3 attempts in 5 s","request_id":"req_7f3a2c"}
{"time":"2026-10-11T12:02:11.479Z","level":"info","message":"POST /api/orders 500","method":"POST","path":"/api/orders","status":500,"handler":"orders::create","duration_ms":5004,"queries":0,"code":"DB0003","request_id":"req_7f3a2c"}
```

## Capturing other output

| Source | Handling |
| --- | --- |
| `tracing` events from delegated crates | captured and re-emitted in Balise's format |
| crates that use `log` | bridged to the same subscriber |
| errors from delegated crates | converted to Balise codes when recognized, `EXT0001` otherwise |
| raw `stdout` / `stderr` lines (a `println!` in a dependency, a C library) | in dev, the CLI wraps each foreign line in an `EXT0001` event; in prod they pass through unchanged |

## Sensitive data

- Headers `Authorization`, `Cookie` and `Set-Cookie` are never logged.
- `Input` fields marked `#[input(secret)]`, and any field whose name contains `password`, `token` or `secret`, are replaced by `***` in logs and error messages. The same rule applies to the fields of `balise::log`.
- Query strings are never logged.
- Database URLs are logged without their password.

## Compile-time errors

The agent meets most errors at compile time. Balise improves them at four levels.

1. **Custom trait errors.** Balise's traits carry `#[diagnostic::on_unimplemented]`, so a wrong handler parameter or return type produces a Balise message instead of a generic trait-bound error.
2. **Macro errors.** Route and model macros check their input and point to the exact line: unknown parameter type, two `Body` parameters, a `Path` count that doesn't match the route.
3. **`balise check`.** It runs `cargo check`, reads the compiler's JSON diagnostics, and enriches the ones that involve Balise:
   - unknown method or type: the closest Balise API, its doc, and the MCP tool to call;
   - "future is not Send": the variable held across `.await` and how to fix it;
   - a resource folder whose module is not declared in `main.rs`: its routes are never compiled, so nothing else would report them missing;
   - a private field or type used from another module (E0616, E0603): the `pub` to add, and where;
   - every diagnostic gets a `CMP` code and the same rendering as runtime errors;
   - the location is where the fix goes, which is not always where rustc points: a query without an owner is reported at `Order::query()`, not at `.page()`;
   - paths inside Balise or the registry are dropped, and the output ends with one summary line: errors, new warnings, known findings.

```
error CMP0201  This query on Order does not say which user's rows it reads
      at src/orders/mod.rs:15 in orders::list

         15 │     let mut query = Order::query()
            │                     ^^^^^^^^^^^^^^ Order is owned by User
         16 │         .order_by(|o| o.created_at.desc());

      fix: Add `user: CurrentUser` to the parameters of `list`, then `.owned_by(&user)`
           right after `Order::query()`: the query returns only this user's orders.
           A screen that needs every order uses `.all_owners("reason")`, a sensitive decision.
      more: balise explain CMP0201

✗ 1 error · 0 new warnings · 2 known findings (balise audit)
```

`balise check --hook`, after each edit, prints a short form on stderr with exit code 2: the number of errors and the first three, with "finish your change, then fix"; `balise check --format json` gives the same data to tools and MCP.
4. **Generated lints.** The generated `Cargo.toml` turns `deprecated` and `clippy::await_holding_lock` into errors. `balise check` also rejects `static mut`, global mutable state, `std::sync::Mutex`, `RefCell`, `tokio::spawn` and `println!` in application code.

### Hallucination traps

Agents often write names they know from other frameworks (Axum's `Json`, Django's `objects.filter`, Laravel's `response()->json`, Rails' `save`). Two mechanisms catch them:

- **`balise check` rewrites the compiler's errors.** rustc reports an unknown method or type as E0599 or E0412, with no room for a custom message. `balise check` reads its JSON output, recognizes the names in its list of traps, and replaces the error with a Balise one: where the name comes from, and what to write instead, with an example.

  ```
  error CMP0107  Balise has no `save()`: it comes from Active Record frameworks
        at src/orders/mod.rs:42 in orders::cancel
        fix: Change fields with `order.update(db, |o| o.status = OrderStatus::Cancelled).await?`:
             only the fields that changed are written.
        more: balise explain CMP0107
  ```

- The same rewriting covers predictable mistakes on Balise's own types: `NewOrder { status: OrderStatus::Paid, … }` gives rustc's E0560 ("no field `status`"), rewritten as "`status` has a default: use `.with_status(OrderStatus::Paid)`".
- **`#[diagnostic::on_unimplemented]`**, stable in Rust, for a few targeted cases where a trait bound is involved: a handler parameter or return type Balise does not accept, a model returned from a handler. Those messages show up in `cargo check` and in the editor too.

The list of traps is built from agent evaluations ([agent-experience.md](agent-experience.md#evaluating-the-agent-experience)) and feeds `search_api`.

## Production findings

Production warnings are rarely read. Instead of hoping someone does, the production binary keeps a summary of what matters to the agent, and a command exports it with its context.

### The summary

- In memory, the binary aggregates the events useful to an agent (errors, `PERF` warnings, panics) **by fingerprint**: code, location in application code, and query shape.
- Each fingerprint keeps its number of occurrences, first and last time seen, and one example.
- **No duplicates**, since a fingerprint appears once.
- **Bounded without rotation**: beyond a few hundred fingerprints, the oldest and least frequent are removed, never everything at once. Arriving at the wrong moment never shows an empty file hiding a pile of errors.
- **Fixed location**: the summary is written periodically to `balise-findings.json`, a stable name, so it survives restarts.

The cost is a counter incremented on relevant events only. It is measured in the [log of alternatives](alternatives.md).

### The report

`./app report`, a small subcommand of the production binary (run with `docker exec` for instance), produces a dated report with:

- the summary of findings;
- the real size of each table, unused indexes and likely missing ones;
- the most expensive queries, when `pg_stat_statements` is enabled;
- the Balise version, the application commit, the database engine and version, and a summary of the configuration without secrets.

The report holds fingerprints and query shapes only, never query values, and the logging redaction rules apply ([Sensitive data](#sensitive-data)).

### Back in the dev loop

`balise audit --from-prod report.json` imports a report ([agent-experience.md](agent-experience.md#production-feedback)). What has been fixed or accepted is recorded in the repository, so the next import shows only new or returning findings. Production does not need to know what was handled.

## Panics and crashes

A panic in a handler becomes a `500`, and in a task stops the task, with one `RUN0006` event: where the panic was raised, and the application's frames, the others folded.

```
14:02:11 ERROR RUN0006  `orders::get` panicked: called `Option::unwrap()` on a `None` value
         at src/orders/mod.rs:31 in orders::get, request req_7f3a2c
         backtrace:
           orders::find  src/orders/model.rs:12
           orders::get  src/orders/mod.rs:31
           … 14 frames outside the application
         fix: Fix the code at the location shown: a panic is a bug, such as an `unwrap()` on a value that can be missing. Return an error with `?` for what can fail.
         more: balise explain RUN0006
```

A panic that cannot be caught leaves a crash report, published by the next start (`RUN0007`). Details: [engine.md](engine.md#panics-and-supervision).

## Open questions

From the review of the mock-up by agents (2026-10-11):

- **`cause` kept in full can leak personal data**: a driver error quotes the row's values (a unique violation repeats the email). The redaction rules must apply to causes too.
