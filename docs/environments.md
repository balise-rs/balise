# Environments

Status: draft. This describes the target design; nothing is implemented yet.

This document covers the three environments (dev, test, prod), configuration, Docker, and production builds.

## Three environments

The environments differ from compile time, not only through a variable: dev tooling does not exist in the production binary.

| | Dev | Test | Prod |
| --- | --- | --- | --- |
| Build | debug, feature `dev` | debug, feature `test` | release, without `dev` |
| MCP server | yes | no | absent from the binary |
| Instrumentation | full (N+1, query plans, blocked event loop) | counters for budgets | lightweight metrics |
| Logs | readable, colored | JSON | JSON on stdout |
| HTTP errors | detailed page, cause and fix | detailed | generic message and request ID |
| Migrations | generated on each schema change | database recreated | applied under a lock |
| Supervision | `balise` CLI | test runner | orchestrator |

`BALISE_ENV` selects the environment: `dev`, `test` or `prod`. A binary built with `dev` refuses to start with `BALISE_ENV=prod`, so a dev build cannot reach production by mistake.

## Configuration

Sources, from highest to lowest priority:

1. environment variables (`BALISE_` prefix, plus `DATABASE_URL` and `PORT`);
2. the `[env.<name>]` section of `balise.toml`;
3. the top level of `balise.toml`;
4. Balise's defaults.

- Secrets are never written in `balise.toml`. They come from environment variables, or from a file through the `_FILE` suffix (`DATABASE_URL_FILE=/run/secrets/db`), which matches Docker and Kubernetes secrets.
- Validation rules (strict in dev, fail at startup in prod for security and data settings) are described in [engine.md](engine.md#configuration).
- `balise config check --env prod` validates a configuration without starting the app. Run it in CI.

## On the host

The only thing to install is the `balise` CLI: a static binary, from a direct download, Homebrew, or `cargo binstall`. Rust is not needed on the host.

## Dev with Docker

`balise new` generates `compose.yaml` and a `Dockerfile`. `balise dev` starts:

- the application, in the official `balise/dev` image (pinned Rust toolchain, Node for Vite), with reload;
- the database (PostgreSQL, or a volume for SQLite);
- the Vite dev server ([web.md](web.md#dev-server-and-vite)).

Practical points:

- `target/` and the Cargo registry are named volumes. Without them, every restart recompiles everything.
- On macOS and Windows, files mounted from the host are slow: `target/` stays inside the container.
- The agent reaches MCP through `balise mcp` on the host, which relays to the dev server in the container. The agent never deals with Docker ([agent-experience.md](agent-experience.md#mcp)).
- `balise dev --native` skips Docker when Rust is installed on the host.

## Test

- `balise test` runs the test suite in the `test` environment.
- Each test gets a fresh database: a copy of a template database on PostgreSQL, an in-memory database on SQLite.
- Query counters are on, so budgets can fail tests ([agent-experience.md](agent-experience.md#budgets)).

## Production build

`balise build` produces one binary that contains the server, the application, and the front-end assets.

- Static binary (musl target) with mimalloc as the global allocator, because musl's allocator collapses under a multi-threaded runtime.
- Generated multi-stage `Dockerfile`: a dependency-caching stage (cargo-chef), a build stage, and a minimal final image (`scratch` or distroless) containing only the binary. A few MB.
- rustls uses the `ring` crypto provider, which avoids a C toolchain when cross-compiling.

## Running in production

What the binary provides for orchestrators (Docker, systemd, Kubernetes):

- `/health` (process alive) and `/ready` (ready for traffic);
- graceful shutdown on `SIGTERM` ([engine.md](engine.md#shutdown));
- JSON logs on stdout;
- the crash report of the previous run, published at startup;
- a summary of findings for agents in `balise-findings.json`, and `./app report` to export it with the database's real sizes ([errors-and-logs.md](errors-and-logs.md#production-findings));
- migrations applied at startup under a PostgreSQL advisory lock, so several instances starting together do not race. A setting can make startup fail instead when a migration is pending.

The orchestrator restarts the process after a crash; Balise does not supervise itself in production.

Optional, for small deployments without a reverse proxy: built-in TLS with automatic certificates (ACME).

## Open questions

- Scope of a future `balise deploy` command (which targets).
- Metrics format in production (Prometheus endpoint, OpenTelemetry, or both).