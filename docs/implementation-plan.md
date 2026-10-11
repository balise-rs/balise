# Implementation plan

> **Temporary document.** This plan guides the first implementation of Balise and will be deleted once it is done. Nothing in the code or in the other documents may refer to it: no links, no phase numbers, no "see the plan". The design documents in `docs/` remain the reference.

Each step lists what is expected, where to find the design, and the decisions still open, if any.
Each step ends with a commit by the user (do not git add/commit/push without explicit demand).
After each step, the description is removed and replaced by a simple "Done !".

## 1. Foundations

### 1.1 Reserve the name
Done !

### 1.2 Repository, workspace, CI
Done !

### 1.3 Error code registry
Done !

### 1.4 Compile benchmark harness
Done !

## 2. Mock-up

### 2.1 Generated project layout
Done !

### 2.2 Handler, model, Input and Output
Done !

### 2.3 Reference project generator
Done !

### 2.4 Error in the logs and `balise check` output
Done !

### 2.5 MCP response
Write what `project_map`, `search_api` and `explain` return for the mock-up project. This fixes the shape of the MCP data before implementation.
- More: [agent-experience.md](agent-experience.md#mcp)

### 2.6 Review by agents
Give the mock-up to independent agents with a task, and to reviewers asked for bad ideas. Adjust the design documents with what they find before writing framework code.
- More: [agent-experience.md](agent-experience.md#evaluating-the-agent-experience)

## 3. Core

### 3.1 Engine wrapper
Start the server on hyper and Tokio through `balise::run()`, without exposing either. Capture their events and errors in Balise's format.
- More: [engine.md](engine.md#startup)

### 3.2 Routing and route collection
Implement the route attributes, compile-time collection and the radix tree built at startup. Detect duplicate routes and check handler signatures in the macro.
- More: [engine.md](engine.md#routing)
- To decide: `linkme` or `inventory` ([alternatives.md](alternatives.md))

### 3.3 Extractors, validation, Input and Output
Implement `Path`, `Query`, `Body`, `State`, `RequestId` and the `Input` / `Output` derives with their validation attributes. Validation errors return a `422` with one message per field. As soon as routes and extractors compile, the reference project (or the part it can already build) joins the compile benchmark, and every later API change is measured.
- More: [application.md](application.md#parameters)
- To decide: type erasure after extraction or full monomorphization ([alternatives.md](alternatives.md))

### 3.4 Response and error format
Map return types to statuses (`Created`, `NoContent`, `Page`…) and errors to the single error body. Implement `#[derive(AppError)]`. The response files of `mockup/outputs/` are checked end to end on the mock-up's routes.
- More: [engine.md](engine.md#response-format), [application.md](application.md#errors)

### 3.5 Logs and events
Implement the event model, the dev and JSON renderings, the per-request summary and the redaction of sensitive data. Bridge `log` and capture dependency events. Every case of `mockup/outputs/` passes at the renderer level and end to end.
- More: [errors-and-logs.md](errors-and-logs.md#events)

### 3.6 Configuration and environments
Load configuration from environment variables and `balise.toml` with the dev / prod rules, and the `dev` feature guard. Add `balise config check`.
- More: [engine.md](engine.md#configuration), [environments.md](environments.md#configuration)

### 3.7 Shared state, blocking, spawn
Implement `.state()`, `State<T>`, `Shared<T>`, `balise::blocking` and `balise::spawn`. Detect tasks that hold the event loop in dev.
- More: [application.md](application.md#shared-state), [engine.md](engine.md#execution-model)
- To decide: multi-threaded runtime or `spawn_local` per core ([alternatives.md](alternatives.md))

### 3.8 Uploads and per-route limits
Implement `File` fields, multipart parsing streamed to disk, type checks on content, and limits computed from declarations. Add per-route pagination limits on the same model.
- More: [application.md](application.md#files), [engine.md](engine.md#per-route-limits)

### 3.9 Shutdown, health, panics
Implement graceful shutdown, `/health` and `/ready`, per-request panic handling and crash reports. Fold framework frames in backtraces.
- More: [engine.md](engine.md#panics-and-supervision), [engine.md](engine.md#shutdown)
- To decide: Tower or an in-house middleware model ([alternatives.md](alternatives.md))

## 4. CLI

### 4.1 `balise new`
Generate a project from the templates (with `--no-<module>` and `--no-example`), check git and Docker, initialize the repository and print the help prompts when a tool is missing. The generated project must run as is.
- More: [agent-experience.md](agent-experience.md#prerequisites-at-balise-new)

### 4.2 `balise dev`
Start the app, the database and Vite in Docker, with rebuild on change, supervision and one log stream. Support `--native`.
- More: [environments.md](environments.md#dev-with-docker)

### 4.3 `balise check`
Wrap `cargo check`, enrich diagnostics, apply the project lints, check schema drift and the generated client. Rank output: errors, new warnings, one summary line.
- More: [errors-and-logs.md](errors-and-logs.md#compile-time-errors)

### 4.4 `routes`, `why`, `trace`, `explain`
Implement the debugging commands, answered by the project's dev build so versions always match. Each one is also an MCP tool later.
- More: [application.md](application.md#debugging), [engine.md](engine.md#diagnostics)

### 4.5 Agent hooks and generated `AGENTS.md`
Generate hooks for Claude Code and rules for Cursor, and an `AGENTS.md` describing conventions, commands and the working loop. Keep it short enough to stay in context.
- More: [agent-experience.md](agent-experience.md#hooks)
- To decide: `balise check` after each edit or at the end of a task ([alternatives.md](alternatives.md))

## 5. Agent experience

### 5.1 MCP server (read-only)
Implement `balise mcp` as a stdio relay to the dev server, with `project_map`, `search_api`, `explain` and `check`. Write tools come later.
- More: [agent-experience.md](agent-experience.md#mcp)

### 5.2 Introspection
Expose routes, inputs, outputs, models, schema and enabled modules as JSON, from the project's dev build. MCP and `balise introspect` share it.
- More: [agent-experience.md](agent-experience.md#versions)

### 5.3 `llms.txt` and error pages
Generate the API reference and one page per error code at each release. Nothing is written by hand.
- More: [agent-experience.md](agent-experience.md#docs-for-models)

### 5.4 Hallucination traps
Add the first traps for names agents commonly invent, failing at compile time with a custom message. Feed the `search_api` index with them.
- More: [errors-and-logs.md](errors-and-logs.md#hallucination-traps)

### 5.5 First agent evaluation
Run a set of tasks with agents on fresh projects and record the metrics. This is the baseline every later change is compared to.
- More: [agent-experience.md](agent-experience.md#evaluating-the-agent-experience)

## 6. Data

### 6.1 Models and schema snapshot
Implement `#[derive(Model)]`, typed prefixed UUIDv7 ids, timestamps, and the schema snapshot with drift detection.
- More: [data.md](data.md#schema-models-are-the-source-of-truth), [data.md](data.md#identifiers)
- To decide: database drivers, the first that works ([alternatives.md](alternatives.md))

### 6.2 Migrations
Generate migrations from the snapshot diff, with three-step data migrations, `balise rename`, rename questions, draft migrations frozen by `balise build` (squash and resource removal rewrite drafts) and `balise remove resource`. Show a summary before applying.
- More: [data.md](data.md#migrations-can-be-edited), [data.md](data.md#renames)

### 6.3 Queries and raw SQL
Implement the closure-based query builder, the terminal methods and the `.limit()` / `.unbounded()` rule. Add raw SQL checked against the snapshot.
- More: [data.md](data.md#queries)

```rust
let orders = Order::query()
    .filter(|o| o.shop_id.eq(shop_id))
    .order_by(|o| o.created_at.desc())
    .page(db, page)
    .await?;
```

### 6.4 Types and constraints
Implement the type mapping per engine, enums as text with `CHECK`, constraints declared in models, and the conversion of violations into errors naming the field. Inputs inherit model constraints.
- More: [data.md](data.md#types), [data.md](data.md#constraints)

### 6.5 Relations and loading
Deduce relations from foreign keys, implement `.with()`, `get_many` and the loading strategies. Then try loading driven by `Output`, as an experiment.
- More: [data.md](data.md#relations)

### 6.6 Writes and transactions
Implement the lazy transaction per request, deferred side effects, patch inputs, closure updates, atomic operations, bulk writes and upsert.
- More: [data.md](data.md#writes-and-transactions)

### 6.7 Pagination
Implement cursor pagination by default and numbered pagination, totals, limits and their per-route override.
- More: [data.md](data.md#pagination)

### 6.8 Factories and seeds
Derive factories from models and inputs, with reproducible seeds, the `dev` dataset and generated validation tests.
- More: [data.md](data.md#test-data)
- To decide: fresh database or rollback per test ([alternatives.md](alternatives.md))

## 7. Web

### 7.1 TypeScript generation and client
Generate types, route functions, limits and validation rules from Rust. `balise check` fails when the client is out of date.
- More: [web.md](web.md#generated-client)

### 7.2 React template and `@balise/react`
Create the React template with a working example resource, and the adapter hooks over TanStack Query.
- More: [web.md](web.md#framework-adapter)

### 7.3 Forms and shared validation
Implement `useForm` with client-side validation from the generated rules and server errors placed under their fields.
- More: [web.md](web.md#forms)

### 7.4 Front end in the loop
Run TypeScript and lint in `balise check`, forward browser errors to the dev logs, forbid raw `fetch` to `/api`, add pages to the project map.
- More: [web.md](web.md#the-front-end-in-the-agent-loop)

### 7.5 Dependency security
Pin pnpm, set the 7-day release age, block install scripts, flag trust changes, and implement `balise add`.
- More: [web.md](web.md#dependency-security)

### 7.6 Realtime
Implement typed channels, SSE and WebSocket, bounded buffers, deferred publishing, and `useChannel`.
- More: [web.md](web.md#realtime)

## 8. Auth

### 8.1 Sessions and CSRF
Implement cookie sessions and CSRF protection, with the token sent automatically by the generated client.
- More: [web.md](web.md#authentication-for-spas)

### 8.2 Generated routes and pages
Generate sign-up, sign-in, sign-out, current user and password reset, with their pages when the project has a front end.
- More: [web.md](web.md#authentication-for-spas)

### 8.3 `CurrentUser`
Implement the extractor for handlers and channels, and `useCurrentUser` on the front end.
- More: [application.md](application.md#parameters)

## 9. Guided optimization

### 9.1 Dev instrumentation
Fingerprint queries and detect N+1, duplicates, writes in loops, over-fetching and open transactions during network calls. Translate `EXPLAIN` plans into plain words.
- More: [data.md](data.md#four-levels)

### 9.2 Static analysis and declared sizes
Report static candidates, promoted to warnings by `expected_rows` or by measurements.
- More: [data.md](data.md#expected-table-size)

### 9.3 Budgets
Implement `budgets.toml`, query and size budgets failing tests, and approval for increases.
- More: [agent-experience.md](agent-experience.md#budgets)

### 9.4 `balise audit` and audit at scale
Replay scenarios, rank findings, fill tables to their declared sizes with uneven distributions, and measure before and after fixes.
- More: [agent-experience.md](agent-experience.md#guided-optimization), [data.md](data.md#four-levels)

### 9.5 Noise control
Implement `.accept()` with reasons and approval, reconsideration when sizes grow, the baseline, and `balise fix` for safe fixes.
- More: [data.md](data.md#keeping-the-signal-clean)

## 10. Production

### 10.1 `balise build` and production image
Build the binary with embedded assets and the multi-stage Dockerfile.
- More: [environments.md](environments.md#production-build)
- To decide: musl with mimalloc or glibc distroless ([alternatives.md](alternatives.md))

### 10.2 Migrations under lock
Apply migrations at startup under an advisory lock, or refuse to start when one is pending, depending on configuration.
- More: [environments.md](environments.md#running-in-production)

### 10.3 Findings summary and `./app report`
Aggregate findings by fingerprint in a bounded summary written to a fixed file, and export it with table sizes and context.
- More: [errors-and-logs.md](errors-and-logs.md#production-findings)

### 10.4 `balise audit --from-prod`
Import reports, track handled findings in the repository, and remind the agent at key moments.
- More: [agent-experience.md](agent-experience.md#production-feedback)

## 11. Measurement

### 11.1 Compile-time phase
Measure every compile lead on the reference project and settle the compile entries of the log of alternatives.
- More: [compilation.md](compilation.md#dedicated-phase)

### 11.2 Runtime benchmark
Build the scripted load benchmark and use it to settle the runtime entries.
- More: [alternatives.md](alternatives.md#how-entries-are-measured)

### 11.3 Agent evaluations on every release
Run the agent task set on every release, compare with the baseline, and feed invented names into the traps.
- More: [agent-experience.md](agent-experience.md#evaluating-the-agent-experience)

### 11.4 Settle the log of alternatives
Close every remaining entry with its measurement and update the design documents accordingly.
- More: [alternatives.md](alternatives.md)