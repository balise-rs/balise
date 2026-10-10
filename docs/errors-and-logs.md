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
| Message | `No Order with id 18` |
| Location | `src/orders/mod.rs:27`, in `orders::get` |
| Fix | `Return Error::not_found, or use Order::find_optional if a missing row is expected.` |
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
| `EXT` | errors from outside Balise that it could not classify |

Applications define their own codes with `#[derive(AppError)]` and their own prefix (`ORD001`), see [application.md](application.md#errors).

Rules:

- A code is never reused or renumbered. A retired code stays in the registry, and its explanation points to its replacement.
- Every code is declared once, in a registry in the source with one file per area: code, title, message template, fix, explanation. Messages and `balise explain` read it directly; there is no separate doc page.
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
| other fields | no | structured key-value pairs |

Each request produces one summary event when it ends: method, path, handler, status, duration, number of queries, request ID.

Application code logs through `balise::log` (`info!`, `warn!`…), which produces the same events. `println!` is flagged by `balise check`.

## Renderings

The format depends on the environment and can be overridden with `log.format` in `balise.toml`.

Dev (terminal, colored):

```
14:02:11 ERROR DB0042  No Order with id 18
         at src/orders/mod.rs:27 in orders::get
         fix: Return Error::not_found, or use Order::find_optional if a missing row is expected.
         req_7f3a2c · balise explain DB0042
```

Test and prod (one JSON object per line, on stdout):

```json
{"time":"2026-10-10T12:02:11Z","level":"error","code":"DB0042","message":"No Order with id 18","handler":"orders::get","location":"src/orders/mod.rs:27","request_id":"req_7f3a2c","fix":"Return Error::not_found, or use Order::find_optional if a missing row is expected."}
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
- `Input` fields marked `#[input(secret)]`, and any field named `password`, `token` or `secret`, are replaced by `***` in logs and error messages.
- Database URLs are logged without their password.

## Compile-time errors

The agent meets most errors at compile time. Balise improves them at four levels.

1. **Custom trait errors.** Balise's traits carry `#[diagnostic::on_unimplemented]`, so a wrong handler parameter or return type produces a Balise message instead of a generic trait-bound error.
2. **Macro errors.** Route and model macros check their input and point to the exact line: unknown parameter type, two `Body` parameters, a `Path` count that doesn't match the route.
3. **`balise check`.** It runs `cargo check`, reads the compiler's JSON diagnostics, and enriches the ones that involve Balise:
   - unknown method or type: the closest Balise API, its doc, and the MCP tool to call;
   - "future is not Send": the variable held across `.await` and how to fix it;
   - every diagnostic gets a `CMP` code and the same rendering as runtime errors.
4. **Generated lints.** The generated `Cargo.toml` turns `deprecated` and `clippy::await_holding_lock` into errors. `balise check` also rejects `static mut`, global mutable state, `std::sync::Mutex`, `RefCell`, `tokio::spawn` and `println!` in application code.

### Hallucination traps

Agents often write names they know from other frameworks (Axum's `Json`, Django's `objects.filter`, Laravel's `response()->json`). For the most frequent ones, Balise defines a hidden method whose trait bound can never be satisfied:

```rust
#[diagnostic::on_unimplemented(
    message = "Balise has no `json()` on responses: return the Output type directly",
    note = "see `balise explain CMP0107`"
)]
trait UseOutputType {}

impl OrderView {
    #[doc(hidden)]
    pub fn json(self) where Self: UseOutputType { unreachable!() }
}
```

The code never compiles, so the hallucinated name cannot become a second API, and the error comes from the compiler itself: it shows up in `cargo check` and in the editor, even without `balise check`. The list of traps is built from agent evaluations ([agent-experience.md](agent-experience.md#evaluating-the-agent-experience)).

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

A panic in a handler becomes a `500` with an error event. Crashes that cannot be caught produce a crash report with application frames first and framework frames folded. Details: [engine.md](engine.md#panics-and-supervision).