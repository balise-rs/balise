# Compilation
 
Status: draft. The benchmark tooling exists; the rest describes the target design.
 
An agent works in a loop: edit, compile, read errors, fix. Compile time sets the speed of that loop, so it is treated as a feature of Balise, measured like one.
 
## What we control
 
The framework is compiled once and cached, but its generics and macros expand inside the user's code, at every compilation. Balise's design decides a large part of the user's compile time.
 
In the framework's design:
 
- Handlers are monomorphized only up to parameter extraction, then type-erased behind one boxed interface ([engine.md](engine.md#handling-a-request)).
- The public API has few generic types, and users never declare generics ([application.md](application.md)).
- Macros (routes, `Input`, `Output`, models) generate as little code as possible and call into functions compiled in Balise's crates.
- Heavy logic lives in Balise's crates, compiled once, not in code generated inside the user's project.

In the generated project:
 
- The agent loop runs `balise check`, which uses `cargo check`. A full build happens only when the app is started.
- Dependencies are optimized in dev; the user's code is not. Debug info is reduced to line tables.
- The default linker is kept: rust-lld is already the default on Linux x86_64 since Rust 1.90.
- `target/` and the Cargo registry are Docker volumes, so restarts do not recompile ([environments.md](environments.md#dev-with-docker)).
- Changing the front end never recompiles Rust ([web.md](web.md)).
- Optional, not default: a shared cache (sccache), and the Cranelift backend, which requires nightly.

Out of our control: the amount of code the agent writes and the dependencies it adds.
 
## Benchmark
 
A benchmark runs in CI from the first prototype onward, so that API choices are measured before they are frozen.
 
- **Reference project**: 8 resources copied from the mock-up's `orders`, plus `users` and `auth`: 54 routes and 9 models, a realistic ratio. Generated from the mock-up, so it evolves with the API. Realtime joins it when the mock-up has a channel.
- **Tracking**: in CI, on every commit to `main`, results are kept in the CI cache and compared with the previous commit. A slowdown of more than 10% and more than 0.1 s is flagged as a warning, never as a failure: timings vary between runs.
- Contributors report the difference for any change touching macros or public generics ([AGENTS.md](../AGENTS.md)).

### Measures

| Scenario | Measures |
| --- | --- |
| `cold_check` | `cargo check` on a fresh project (the first `balise check`) |
| `cold_build` | cold build (the first `balise dev`) |
| `body_edit_check` | `cargo check` after editing one handler body |
| `field_edit_check` | `cargo check` after changing an `Input` struct |
| `model_edit_check` | `cargo check` after changing a model |
| `body_edit_build` | incremental build and link after editing one handler |

Cold scenarios run once. Edit scenarios run once to warm up, then five times; the median is kept.

### Tooling

- `bench/compile-bench` generates the reference project and measures a project:
  - `cargo run --release -p compile-bench -- generate target/bench/reference [--resources N]` copies the mock-up's `orders` resource once per resource, renamed (`Order` → `Invoice`, `ORD0001` → `INV0001`…), with the rest of the mock-up as is. The front end is left out.
  - `--variant web` builds the project from `bench/web-template` instead: what Balise can already compile (routes, `Path`, `Query`, `Body`, `Input`, `Output`, `Patch`), six routes per resource, 48 with the default 8 resources. The template is a workspace member, so every API change is checked against it in CI, and the generated project uses the repository's `Cargo.lock`.
  - `cargo run --release -p compile-bench -- measure <project> [--runs N] [--baseline FILE] [--out FILE]` measures and compares with a previous result.
- The project marks where to edit with comment lines, inserted by the generator in the reference project: `// bench:body` inside a handler body, `// bench:field` inside an `Input` struct, `// bench:model` inside a model. A public field is added at each marker, so these structs must never be built with a literal without `..` (struct update).
- Until the full reference project compiles, CI measures the web variant. Its results are kept in their own file, never compared with another variant's.

## Dedicated phase
 
Once the MVP is stable, a full phase measures each lead precisely and settles every entry of the [log of alternatives](alternatives.md) measured by compilation. Until then, when two approaches seem equivalent, the simpler one is implemented and the other is recorded in that log.
 
