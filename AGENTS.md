# AGENTS.md — developing Balise

Balise is a Rust web framework designed to be used by coding agents. This file is for agents working on the framework itself. 
Big picture: [docs/architecture.md](docs/architecture.md).

## Repository layout (planned)

```
crates/
  balise/           facade: the only dependency of user projects; its `balise` binary is the launcher
  balise-core/      routing, extractors, errors, logs, config
  balise-macros/    procedural macros (routes, derives)
  balise-engine/    hyper / Tokio wrapper
  balise-realtime/  WebSocket, SSE, pub/sub
  balise-ts/        TypeScript type generation
  balise-orm/       ORM (feature `orm`)
  balise-auth/      auth (feature `auth`)
  balise-mail/      mail (feature `mail`)
  balise-dev/       MCP, instrumentation (feature `dev`, never in prod)
  balise-cli/       the versioned CLI: commands, MCP server, project templates
bench/              compile-time benchmark
tools/              repository tooling (release-age, release: `just publish <version>`)
docs/               architecture.md and one document per component
```

## Rules

1. **Own what the agent sees, delegate what it doesn't.** No type from a delegated dependency (hyper, Tokio, rustls, drivers) in the facade's public API or in a user-facing error message.
2. **One way to do things.** Don't add an API variant for a use case that is already covered.
3. **No generics or lifetimes for users to declare.** If an API requires them, rethink it.
4. **Every visible error has a stable code** (`DB0042`), a cause, a location, a suggested fix, and an explanation shown by `balise explain`. `EXT0001` is only for errors Balise cannot classify: a known situation gets its own code, and a code's fix must match every cause it covers.
5. **The core never assumes an optional module.** Cross-module integrations compile only when both features are enabled.
6. **Nothing dev-only in the production binary**: all dev tooling goes behind the `dev` feature.
7. **The public API is the same in every build.** No public API changes availability with the build profile, except dev and test tooling (MCP, instrumentation, `TestApp`, factories, seeds).
8. **No warning on correct code.** A detection that fires on code that is correct in most cases is not a warning: the right pattern becomes the default, or goes into the types. Warnings are for what is rare and almost always a real problem. A reminder is shown once.
9. **Equivalent choices**: if two approaches are equally good, implement the simpler one and record the other in the log of alternatives ([docs/alternatives.md](docs/alternatives.md)).

## Before submitting a change

```
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo hack check --feature-powerset --depth 2
```

- Stable toolchain only.
- If the public API changes: update `docs/application.md` and `llms.txt`.
- If a command or a rule for agents changes: update the generated `AGENTS.md` (`crates/balise-cli/template/AGENTS.md.tpl`); tests fail until it names every command for agents and every rule of `clippy.toml`.
- If the change touches macros or public generics: run the compile-time benchmark and report the difference in the description.

## Language

Everything is in English: code, identifiers, comments, commit messages, and documentation.