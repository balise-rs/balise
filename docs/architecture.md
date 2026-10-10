# Balise architecture
 
Balise is a Rust web framework designed to be used by an AI coding agent. The human describes what they want, the agent writes the code, and Balise keeps the agent on track: strict conventions, a demanding compiler, and feedback that both humans and machines can read.
 
This document gives the big picture. It does not go into individual decisions: each section points to the document that does.
 
## Vision
 
- **Who it is for**: an agent writes the code; a human steers it, without necessarily knowing Rust.
- **The promise**: the performance and safety of Rust, without having to learn Rust.
- **The difference**: existing Rust frameworks are designed for humans. Balise is designed for an agent that has never seen its API in its training data and still has to write correct code.
- **The context**: a personal learning project. Time is not a constraint, and rewriting a component is acceptable whenever it gives more control over what the agent sees.
## Principles
 
1. **Application code never touches the hard parts of Rust.** No lifetimes in signatures, owned types, shared state hidden behind the framework. The agent uses generic types provided by Balise but never declares any.
2. **One way to do each thing.** Fewer variants mean fewer hallucinations.
3. **One convention end to end.** Same names, same error format, same logs, same config, from the server to the ORM.
4. **Balise owns what the user sees, by default.** Everything that comes out uses its format; anything that escapes is wrapped in a generic event.
5. **Optimize where it matters**: I/O and the database.
6. **Check at compile time whenever possible**: routes, SQL queries, types shared with the front end.
7. **Own what the agent sees, delegate what it doesn't.** Balise writes everything the agent writes, reads, or meets in an error. It delegates what is invisible and security-sensitive (HTTP, runtime, TLS, database protocols) to proven crates. A delegated dependency is never exposed to the agent.
## Overview
 
```
  User's machine                           Application (binary)
  ──────────────                           ────────────────────
 
  Agent ──── balise mcp ───┐               Application code (written by the agent)
                           │                          │
  balise CLI ──────────────┤               Facade crate (the only dependency)
  (new, dev, check,        │                          │
   audit, migrate…)        │      ┌───────────────────┼──────────────────────┐
                           │      Realtime     TS types   ORM   Auth   Dev tooling
                           │                                              ▲
                           └──────── relays MCP ──────────────────────────┘
                                                      │
                                       Core: routing, extractors, errors, logs, config
                                                      │
                                       Engine wrapper: startup, error capture
                                                      │
                                       Delegated: hyper, Tokio, rustls, database drivers
```
 
Two separate worlds:
 
- **On the user's machine**, the `balise` CLI, a standalone binary. It creates projects, drives Docker, checks the code, and relays MCP to the dev server. → [agent-experience.md](agent-experience.md), [environments.md](environments.md)
- **In the application**, a stack of crates exposed through a single facade. → [engine.md](engine.md)
## Crate architecture
 
- A **facade crate** is the project's only dependency. It re-exports the enabled modules.
- The **`base`** profile (default) contains the core, the engine wrapper, realtime, and TypeScript types.
- **Optional** modules (ORM, Auth, then others) are enabled one by one through Cargo features. The **`full`** profile enables them all.
- **Dev tooling** (MCP server, detailed instrumentation, detailed error pages) sits behind a `dev` feature that is absent from production builds.
- The core never assumes an optional module is present; modules plug into it through extension points.
→ Details: [engine.md](engine.md)
 
## The life of a request
 
1. At startup, the process builds the routing table, the connection pool, and the shared state once and for all. The application is never recreated per request.
2. A request arrives: the router picks the handler, and Balise extracts and validates its parameters.
3. The handler runs with its typed parameters and the services it asks for (database, shared state).
4. The result or error is turned into an HTTP response, and the event is logged in Balise's format.
→ Application side: [application.md](application.md) · Internals: [engine.md](engine.md)
 
## The agent loop
 
1. The agent reads `AGENTS.md` and queries the project through MCP.
2. It writes code; `balise check` (triggered by hooks) compiles it and enriches the errors.
3. After each feature, `balise audit` reports performance problems and checks budgets.
4. The agent fixes things until everything passes.
→ [agent-experience.md](agent-experience.md), [errors-and-logs.md](errors-and-logs.md)
 
## Documentation map
 
| Document | Contents |
| --- | --- |
| [application.md](application.md) | Writing an application: vertical-slice layout, routes, handlers, `Input` / `Output`, shared state and lifetimes |
| [engine.md](engine.md) | Internals: crates and profiles, hyper / Tokio wrapper, routing, persistent process, panics and supervision |
| [errors-and-logs.md](errors-and-logs.md) | Unified format, stable codes, enriched compile errors, hallucination traps |
| [agent-experience.md](agent-experience.md) | CLI, MCP, introspection, hooks, `AGENTS.md`, guided optimization (detection, budgets, audit) |
| [web.md](web.md) | Front end: project setup, generated client, React adapter, forms, dependency security, realtime, SPA authentication |
| [data.md](data.md) | ORM, delegated drivers, migrations |
| [environments.md](environments.md) | Dev, test and prod, Docker, production binary and image |
| [compilation.md](compilation.md) | Compile-time strategy, benchmark, dedicated phase |
| [alternatives.md](alternatives.md) | Log of alternatives: options to measure, by compilation, runtime or agent evaluation |
 
`application.md` is written so it can be handed to an agent as is: it describes what the agent writes, without internal details.
 
## Key decisions
 
| Decision | Document |
| --- | --- |
| In-house routing and extractors, on delegated hyper and Tokio | [engine.md](engine.md) |
| Vertical-slice layout, attribute routes collected automatically | [application.md](application.md) |
| Handlers with typed parameters | [application.md](application.md) |
| Persistent process, request and process lifetimes | [application.md](application.md), [engine.md](engine.md) |
| Facade crate, `base` / `full` profiles, no dev tooling in prod | [engine.md](engine.md) |
| Separate CLI on the host, MCP relayed to the dev server | [agent-experience.md](agent-experience.md) |
| Hallucination traps that fail at compile time | [errors-and-logs.md](errors-and-logs.md) |
| In-house ORM on delegated drivers | [data.md](data.md) |
| Explicit migrations, squashed when a feature is validated | [data.md](data.md) |
| Environments that differ from compile time, first-class Docker | [environments.md](environments.md) |
| Dedicated compile-time phase, measured from the prototype onward | [compilation.md](compilation.md) |
| Databases: PostgreSQL and SQLite in the MVP, MySQL later | [data.md](data.md) |
| First-class front end, React first, TanStack Query behind Balise's adapter | [web.md](web.md) |
| Validation rules declared once in Rust, enforced on both sides | [web.md](web.md) |
| Safe npm and crate defaults: 7-day release age, no install scripts, approval for new dependencies | [web.md](web.md#dependency-security) |
| License: dual MIT or Apache-2.0 | — |
