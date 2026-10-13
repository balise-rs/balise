# Log of alternatives

Status: living document.

When two approaches seem equally good, the simpler one is implemented first and the other is recorded here, with what would decide between them. Entries are settled by measurement, not by opinion.

## How entries are measured

| Method | Measures | Where |
| --- | --- | --- |
| **Compile** | cold build, incremental `cargo check` and build on the reference project | [compilation.md](compilation.md#benchmark) |
| **Runtime** | throughput, latency, memory, number of queries, lock duration, on a scripted load | runtime benchmark, to set up with the MVP |
| **Agents** | compile errors before the first green build, invented API names, iterations to green, audit warnings left | agent evaluations ([agent-experience.md](agent-experience.md#evaluating-the-agent-experience)) |

An entry can need more than one method.

## Process

- To add an entry: date, topic, the option in place (A), the alternative (B), how it is measured, and what would decide.
- Compile entries are settled during the dedicated compile-time phase after the MVP ([compilation.md](compilation.md#dedicated-phase)). Runtime and agent entries are settled once their benchmark or evaluation exists.
- When an entry is settled, update its status and link the result. A settled entry stays in the log, so the reason for a choice is never lost.

## Entries

Option A is the one implemented first.

### Engine

| Recorded | Topic | Option A | Option B | Measured by | Status |
| --- | --- | --- | --- | --- | --- |
| 2026-10-10 | Handler execution | Multi-threaded Tokio runtime (futures must be `Send`) | `spawn_local` on a runtime per core (no `Send` requirement) | Agents ("not Send" errors), runtime | to test |
| 2026-10-11 | API versions | One version at a time (`api.version`) | Several versions served side by side (`/api/v1` and `/api/v2`) | Agents, real needs | to test when a real case appears |
| 2026-10-11 | HTTP versions | HTTP/1.1 only, until built-in TLS | HTTP/2 (and h2c) from the start | Compile (dependencies), runtime (behind a proxy) | to test |
| 2026-10-10 | Middleware | Tower, internally | Small in-house model | Compile, runtime | to test |
| 2026-10-10 | Handlers | Type erasure (`dyn`) after extraction | Fully monomorphized | Compile, runtime | to test |
| 2026-10-10 | Route collection | Distributed slices (`linkme`), behind Balise's registry module | Registration at startup (`inventory`), or an in-house linker-section module | Compile, reliability across linkers | A implemented; to measure once the reference project compiles |
| 2026-10-12 | Multipart parsing | In-house streaming parser | `multer` | Compile (its dependencies in every project), runtime on large uploads, robustness on malformed bodies | to test |
| 2026-10-12 | CLI and versions | A launcher running the CLI of the project's version | One CLI supporting a range of Balise versions | Maintenance cost, agents' errors on version mismatches | to test |
| 2026-10-12 | CLI arguments | Own parser, with codes and suggestions | `clap` | Quality of the help and errors for agents, maintenance | to test |
| 2026-10-13 | Watching files in dev | Reading file dates every 300 ms | System notifications (`notify`) | Reaction time, CPU on large projects, reliability through Docker mounts on macOS and Windows | to test |

### Application API

| Recorded | Topic | Option A | Option B | Measured by | Status |
| --- | --- | --- | --- | --- | --- |
| 2026-10-10 | Handler inputs | Typed parameters in the signature | A single `ctx` object | Agents | to test |
| 2026-10-11 | Extractor syntax | `id: Path<OrderId>`, unwrapped by the route macro: `id` is an `OrderId` in the body | Plain types with parameter attributes: `#[path] id: OrderId`, `#[body] input: CreateOrder` | Agents (confusion between the declared and the received type) | to test |
| 2026-10-10 | Handler names | Short verbs in the resource module (`orders::create`) | Verb and resource (`create_order`) | Agents | to test |

### Data

| Recorded | Topic | Option A | Option B | Measured by | Status |
| --- | --- | --- | --- | --- | --- |
| 2026-10-10 | Database drivers | First one that works (tokio-postgres and rusqlite, or sqlx-core) | The other | Compile, runtime, quality of errors to convert | to test |
| 2026-10-10 | Query API | Closures over a column struct (`\|o\| o.quantity.gt(3)`) | Macro translating Rust expressions (`q!(o.quantity > 3)`) | Agents, compile | to test |
| 2026-10-10 | Loading for responses | Driven by the `Output` (`ShopView::query()`) | `.with()` only | Agents (adoption), runtime | experimental, dropped if not adopted |
| 2026-10-10 | Loading relations to many rows | One query with `IN (…)` per relation | Join | Runtime, at several volumes | to test |
| 2026-10-10 | Columns loaded | Only those used by the `Output` | Whole row | Runtime | to test |
| 2026-10-10 | Primary keys | UUIDv7, typed and prefixed | Auto-incremented integer | Runtime (index size, insert and join speed) | to test |
| 2026-10-10 | Transactions | Lazy transaction per request | Explicit transactions only | Runtime (cost, lock duration), agents (missing transactions) | to test |
| 2026-10-11 | Ownership enforcement | Compile-time `.owned_by()` in the ORM | Also PostgreSQL row-level security policies generated from `owned_by`, as defense in depth | Runtime (query cost), agents (bypasses caught) | to test |
| 2026-10-11 | Converting a page | Synchronous `.map(f)` only | `.try_map(f)` for fallible conversions | Agents (cases where a conversion can fail) | to test when a real case appears |
| 2026-10-11 | Bounded queries on a page's ids | `.unbounded(reason)` when filtering on `page.ids()` | An `is_in` on a page's ids counts as bounded | Agents (`.unbounded()` noise), compile (cost of tracking the bound) | to test |
| 2026-10-11 | Optional fields at creation | Generated `.with_<field>()` returning `Insert<Order>` | Default field values in Rust structs (`NewOrder { user, .. }`), once stable | Check when the language feature is stabilized, then agents | to test |
| 2026-10-11 | Concurrent edits by people | Row locks (`.for_update()`) only | Optimistic locking with `#[model(versioned)]` for models edited by several people at once | Agents, runtime (conflicts in long edits) | to test |
| 2026-10-11 | Thresholds for agent decisions | A policy per type: `human` or `agent` | Thresholds within a type (`agent_up_to = "2x"` for budgets, a minimum age for dependencies) | Agents (decisions taken), human review load | to test |
| 2026-10-10 | Test isolation | Fresh database per test, copied from a template | Transaction rolled back after each test | Runtime (test suite duration) | to test |

### Agent experience

| Recorded | Topic | Option A | Option B | Measured by | Status |
| --- | --- | --- | --- | --- | --- |
| 2026-10-10 | Hallucination traps | Hidden methods failing at compile time | Suggestions from `balise check` only | Agents | settled 2026-10-11 by review: A does not compile on stable; B, plus `on_unimplemented` for trait-bound cases ([decisions.md](decisions.md)) |
| 2026-10-13 | Engine of `balise check` | `cargo clippy`, with the project's lints | `cargo check` (`--no-clippy`), lints only with `--ci` | Compile (check time per edit), errors caught | to test |
| 2026-10-10 | Hook timing | `balise check` after each edit | Once at the end of a task | Agents (iterations, total time) | to test |
| 2026-10-10 | Hook integration | `--hook` mode in the CLI, speaking each agent's hook protocol | Script in the project (`.claude/hooks/`) calling plain `balise check` | Agents (errors reaching the agent, false runs), on a real generated project | to test |
| 2026-10-11 | Example resource in a new project | `orders` owned by the signed-in user, one model | (B) `orders` referencing a `Product` model, with a relation; (C) the relation shown in the docs only, not in the project | Agents: blind task "add a `Shop` that owns orders" on each variant, compare errors, invented names and conventions followed | to test |

### Environments and build

| Recorded | Topic | Option A | Option B | Measured by | Status |
| --- | --- | --- | --- | --- | --- |
| 2026-10-10 | Dev environment | Docker | Native | Compile (incremental loop), especially on macOS | to test |
| 2026-10-10 | Codegen backend in dev | LLVM | Cranelift (nightly) | Compile | to test |
| 2026-10-10 | Production binary | musl with mimalloc | glibc in a distroless image | Runtime, image size | to test |
| 2026-10-10 | Production findings | In-memory summary by fingerprint, written periodically | No summary: report built from collected logs only | Runtime (overhead on requests), completeness of reports | to test |
| 2026-10-12 | `config check` | A command of the app's binary: needs a build, one validation path | Validation in the `balise` CLI from `balise.toml` and the enabled features: no build | CI time, risk of the two paths diverging | to test |
