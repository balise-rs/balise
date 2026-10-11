# Decisions

Status: living document.

Decisions taken while building Balise, with their reason. Most recent last. Options still to be measured live in the [log of alternatives](alternatives.md).

| Date | Decision | Reason |
| --- | --- | --- |
| 2026-10-10 | Crate names `balise` and `balise-*` reserved on crates.io, including `balise-macros` | Route attributes and derives need a separate proc-macro crate, so its name is reserved with the others |
| 2026-10-10 | npm scope `@balise-rs` for front-end packages (`@balise-rs/react`…) | `@balise` and the unscoped `balise` are taken on npm |
| 2026-10-10 | GitHub organization `balise-rs` | `balise` is taken on GitHub; same name as the npm scope, one identity everywhere |
| 2026-10-10 | Error codes declared with a `macro_rules!` in `balise-core/src/error_codes/`, one file per area prefix | No build script or parser, so no compile cost; one file per area keeps the registry readable |
| 2026-10-10 | Each code is a typed constant (`error_codes::EXT0001`) that only the registry can create | Raising an unregistered code from Rust does not compile; a test scanning the sources catches the rest (strings, comments) |
| 2026-10-10 | No doc page per error code: `balise explain` and MCP read the explanation from the registry | A page would repeat the registry; agents get the same text without leaving the loop. A website can be generated from the registry later if needed |
| 2026-10-10 | Compile benchmark results kept in the CI cache, each commit compared with the previous one, instead of stored per commit | Enough to catch a regression when it lands, without a results branch or a CI token |
| 2026-10-10 | Compile-time slowdowns are warnings, never failures | Timings vary between runs; a flaky failure teaches to retry, not to fix |
| 2026-10-10 | `balise new` enables every optional module, listed explicitly in the project's `Cargo.toml`; `--no-<module>` refuses one | A new project works end to end out of the box; the list in `Cargo.toml` says what the project uses |
| 2026-10-10 | Without `orm`, the generated example is a route and a page, with no storage | Storage is the agent's job then; an in-memory example would teach a pattern to avoid |
| 2026-10-10 | No published image for now: the dev environment is the `dev` stage of the project's `Dockerfile`, built by `compose.yaml` | Nothing to publish or version while the toolchain changes; dev and production builds share one base. A published image comes later: on Docker Hub, `balise` is taken and `balise-rs` is invalid (no hyphens), `balisers` is free |
| 2026-10-10 | `main.rs` declares one `mod` per resource, added by `balise generate resource`; `balise check` reports a resource folder that is not declared | Rust needs the declaration; a forgotten module means routes that silently do not exist. `main.rs` declares modules, not routes |
| 2026-10-10 | Seeds live in `src/seeds/`; seeds and factories compile only in dev and test builds | Cargo only builds `src/`; datasets have nothing to do in the production binary (no dev-only code in prod) |
| 2026-10-10 | The API prefix (`/api` by default, version included, such as `/api/v1`) is set in `balise.toml` and never written in route attributes | Routes stay the same whatever the prefix; changing it is one line |
| 2026-10-10 | Inputs and outputs of a resource live in its `api.rs`; `model.rs` holds only the model | `api.rs` is the contract with the front end, the one TypeScript is generated from |
| 2026-10-10 | Budgets are keyed by handler (`orders::list`), not by URL | Changing the API prefix or a path does not invalidate them |
| 2026-10-10 | The facade has a `test` feature, enabled by `balise test` | Test builds differ from dev builds at compile time (counters for budgets) |
| 2026-10-10 | One npm package, `@balise-rs/react`, also provides the Vite plugin and the lint configuration | Fewer packages to install and version; split later only if useful |
| 2026-10-10 | One dev supervisor in the CLI: `balise dev --inside` in the container, `balise dev --native` on the host; the host's `balise dev` only starts Docker and relays | One binary to write; supervision, rebuilds and logs are the same with or without Docker |
| 2026-10-10 | Agent hooks call `balise check --hook` / `balise audit --hook`, which speak the agent's hook protocol | A plain command would run on every edit and its errors would not reach the agent. To verify on a real project (log of alternatives) |
| 2026-10-10 | Auth routes generated in `src/auth/`; the `User` model generated in `src/users/model.rs` and owned by the application, checked by `#[auth::user]` | Rust has no inheritance: a model provided by the module could not get new fields |
| 2026-10-10 | Default budgets deduced from each route's shape; `budgets.toml` only holds overrides | Defaults are not duplicated in every project, and improving them reaches existing projects |
