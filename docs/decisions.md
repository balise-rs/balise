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
| 2026-10-11 | `create` takes an `Input` or a closure, like `update`; `.set()` combines both; a required field given by none is an error naming it | One verb for every way to build a row, and missing fields are caught early |
| 2026-10-11 | Ownership declared in the model (`owned_by`, `owned_through`, `public`), enforced at compile time by `.owned_by(owner)` or `.all_owners(reason)`; `get` answers `404` for someone else's row; `Owner` only from `CurrentUser` or `Owner::trusted` | Forgetting the owner filter (IDOR) is the easiest handler mistake; the compiler catches it. Part of the ORM, so it works with any authentication |
| 2026-10-11 | `Db` is `Copy`, a handle to the request's context | Used several times per handler without borrowing or cloning |
| 2026-10-11 | `.one()` with no row answers `404` (`DB0042`), like `get` | A missing row behaves the same whatever the method |
| 2026-10-11 | Generators write `pub`; `balise check` explains visibility errors (E0616, E0603) | Models and inputs live in submodules; a missing `pub` is a frequent agent error |
| 2026-10-11 | `AppError` declared in `api.rs`; application codes have four digits (`ORD0001`) | Error codes are part of the contract with the front end; same format as Balise's codes |
| 2026-10-11 | Pagination parameters come from `Query<PageParams>`; `Page<Order>` converts into `Page<OrderView>` with `.into()` | Reuses the existing extractor and conversion, no new concept |
| 2026-10-11 | Helpers about one model are methods in `model.rs`; handler helpers stay in `mod.rs`. Helpers shared between resources: to decide when the case comes | No `utils/` folder, and each helper has an obvious place |
| 2026-10-11 | `id`, `created_at` and `updated_at` are declared in the model, written by generators and recognized by name; removing the timestamp fields replaces `no_timestamps` | Everything stored is visible in `model.rs`; one rule less |
| 2026-10-11 | Model and `Input` rules add up; a detectable contradiction is a compile error | A rule declared once protects everything, and an input can only tighten it |
| 2026-10-11 | One `#[derive(Enum)]` for models, inputs, outputs and TypeScript, same value in the database and in JSON | One way to declare an enum |
| 2026-10-11 | The current user's output is `CurrentUserView` | `CurrentUser` is the extractor; consistent with the `*View` convention |
| 2026-10-11 | Test paths have no API prefix, the test client adds it; bodies are typed (`.body(input)`, `CreateOrder::fake()`); `balise::test::json!` only for invalid bodies | Same paths as route attributes; typed bodies follow the API; no delegated `serde_json` in user code |
| 2026-10-11 | `sign_in_as_new_user` compiles only with both `auth` and `test` | The core never assumes an optional module |
| 2026-10-11 | A `mail` module: captured in dev (`/_balise/mails`, `mails` MCP tool), SMTP in production; without it, `auth` calls a hook the application implements | The agent can test the reset flow on its own; email is needed by `auth` |
| 2026-10-11 | Sign-in through `User::authenticate(db, email, password)`, same error and timing whether the email exists or not | Accounts cannot be probed (to confirm in the auth step) |
| 2026-10-11 | `.owned_by(&user)` is the documented form; `Owner` is a `Copy` marked id; in dev and test, `&User` is accepted too; factories are exempt from ownership | `CurrentUser` holds data and cannot be `Copy`; one syntax in handlers, seeds and tests |
| 2026-10-11 | Responses are checked with their type first (`res.json::<OrderView>()`); `assert_json_includes(json!(…))` for partial checks | A renamed field breaks at compile time; the name says the check is partial |
| 2026-10-11 | `Mailer` is a handler parameter; `send()` is deferred after commit; tests read mails with `app.mails()` | Consistent with `Db` and `State`; no mail for a rolled-back write; the reset flow is testable |
| 2026-10-11 | Draft migrations: a migration can be rewritten until `balise build` freezes it in a versioned file; in dev, the database is rebuilt from drafts and seeds | Balise cannot see production, so "never deployed" must be recorded; removing a resource or squashing becomes one mechanism |
| 2026-10-11 | `balise remove resource <name>`, symmetric to `generate`; the example resource teaches it, and removing the example suggests `balise generate` | Removing the example takes one command instead of hunting it across back end, front end, seeds and migrations |
| 2026-10-11 | Seeds stay central and front routes stay in `App.tsx` | Collecting them per resource would add mechanisms for little gain; `balise remove` handles the cleanup |
| 2026-10-11 | `balise new --no-example` | For those who already know what they want |
| 2026-10-11 | The reference project is generated from the mock-up's `orders`, copied and renamed per resource, rather than from a separate template | One source: the reference project follows the mock-up automatically |
| 2026-10-11 | Reference project: 8 resources plus `users` and `auth` (54 routes, 9 models), without realtime for now | A realistic routes-to-models ratio without a second template to maintain; realtime is added once the mock-up has a channel |
| 2026-10-11 | A `5xx` is an `error` event plus the request line; a `4xx` has no separate event, the request line carries its code, detail and location | A `4xx` is the API working as designed; logging it as an error would bury real failures (bots probing ids) |
| 2026-10-11 | Logs may say more than the response (a `404` on another user's row says so in logs only) | The server needs the reason to debug; the client must not learn that the row exists |
| 2026-10-11 | Prod `5xx` responses carry a generic `HTTP0500`, a generic message and the request ID; `4xx` responses keep their code and message | Infrastructure details stay on the server; the request ID links the client's report to the logs |
| 2026-10-11 | Expected outputs live in `mockup/outputs/` and become the renderer's reference tests | The mock-up becomes a specification the implementation is checked against |
| 2026-10-11 | `balise check` reports where the fix goes, not where rustc points; `--hook` prints the terminal text without colors on stderr | The agent edits the right line; plain text costs fewer tokens than JSON |
| 2026-10-11 | In the terminal, every `fix:` line is followed by `more: balise explain CODE`; the request ID goes on the `at` line | One rule for where `explain` appears; the request ID and the doc command are separate concepts |
| 2026-10-11 | A request line reads as a sentence: `POST /api/orders → request req_… (orders::create returned 500 in 5004 ms with 0 queries)` | Explicit for a human and an agent, no symbols to decode |
| 2026-10-11 | Ownership messages name the owner (`which user's rows`) | "whose" did not say who |
| 2026-10-11 | Reference outputs are compared byte for byte at the renderer level, with fixed event values; end-to-end tests replace times, ids and durations with placeholders | The renderer is a pure function; only end-to-end runs vary |
| 2026-10-11 | `Page::map(f)`, synchronous, keeps the cursor; `.into()` stays for direct conversions | A per-row query cannot be written in the closure; the agent never rebuilds a page by hand |
| 2026-10-11 | Generators wrap what they add outside a resource's folders in `balise:<resource>` markers; `balise remove` deletes marked blocks, reports edited ones, refuses when another resource depends on it | Reliable removal without parsing TypeScript or Rust |
| 2026-10-11 | `page.ids()`, `.index_by(key)` and `.group_by(key)` for bulk-loaded data that is not a relation | Cheap to provide; avoids a hand-written `HashMap` written ten ways. `.with()` stays the way for relations |
| 2026-10-11 | Reference outputs are checked at three levels: renderer (byte for byte), end to end on the mock-up's routes (normalized), compile errors on a test project | Formatting alone does not prove that a real error has the right location, code and cause |
