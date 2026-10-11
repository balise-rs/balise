# Agent experience

Status: draft. This describes the target design; nothing is implemented yet.

Agents have never seen Balise in their training data: left alone, they will guess its API from other frameworks. Everything in this document exists to replace guessing with looking up, and to turn mistakes into fast, precise feedback. This is the core of the product.

## The loop

1. **Look up**: the agent reads `AGENTS.md`, then queries the project through MCP.
2. **Write**: generators produce the skeleton, the agent fills it in.
3. **Check**: after each change, hooks run `balise check`, which compiles and enriches errors.
4. **Audit**: after each feature, `balise audit` reports performance problems and checks budgets.
5. **Fix**: every error and warning comes with its location and its fix.

## The CLI

`balise` is a standalone binary installed on the host. It never requires Rust on the host: it drives Docker ([environments.md](environments.md)).

| Command | Purpose |
| --- | --- |
| `balise new <name>` | create a project, with `AGENTS.md`, agent hooks, `compose.yaml` |
| `balise dev` | start the dev environment, with reload |
| `balise check` | compile and enrich errors |
| `balise audit` | run tests or a scenario and report performance problems |
| `balise explain <CODE>` | explain an error code; `balise explain config` lists every setting and its default, `balise explain budgets` how default budgets are chosen |
| `balise introspect` | print the project map as JSON |
| `balise routes` | list routes and handlers |
| `balise why <METHOD> <path>` | explain a `404` or `405` |
| `balise trace <request_id>` | replay what happened to a request |
| `balise generate <kind> <name>` | generate a resource, model or route |
| `balise migrate …` | create, apply, squash migrations ([data.md](data.md)) |
| `balise config check --env <env>` | validate configuration without starting the app |
| `balise mcp` | MCP server for agents (stdio) |

### Optional modules at `balise new`

`balise new` enables every optional module (`orm`, `auth`, and those to come), listed in the project's `Cargo.toml`. Each can be refused at creation with `--no-<module>` (`--no-orm`, `--no-auth`). The example adapts: with `orm`, a resource stored in the database; without it, a route and a page, and storage is left to the agent.

### Prerequisites at `balise new`

`balise new` checks that git and Docker are installed, and that the Docker daemon is running.

- **git present**: the project is initialized as a repository, with a generated `.gitignore`.
- **Something missing**: `balise new` says so and keeps going. Neither is needed to create the project; Docker is needed later, for `balise dev`.
- For each missing tool, Balise prints two ready-to-paste prompts, adapted to the detected operating system, and saves them in `.balise/prompts/`:
  - one asking the agent how to install the tool on this system;
  - one asking the agent for a short tutorial: what the tool is, why Balise uses it, and the few commands worth knowing.

The user does not need to know what git or Docker are to get started: the agent becomes their guide.

### Versions

The CLI does not read the project's source to understand it. Project-specific commands (`introspect`, `routes`, `why`, MCP) ask the project's own dev build, so the answer always matches the Balise version the project uses. The CLI supports a range of library versions and says so when a project falls outside it.

## MCP

### Transport

The agent is configured with one command, whatever the setup:

```json
{ "mcpServers": { "balise": { "command": "balise", "args": ["mcp"] } } }
```

`balise mcp` speaks stdio to the agent and relays to the dev server, in Docker or native. The dev server can also expose MCP over HTTP for agents that support it. The MCP server exists only in dev builds ([engine.md](engine.md#the-dev-feature)).

### Resources (read)

- **Project map**: routes and their handlers, inputs and outputs, models, database schema, enabled modules, configuration (secrets hidden).
- **Error codes**: the explanation of each code, as `balise explain` shows it.
- **API reference**: the same content as `llms.txt`.

### Tools

| Tool | Writes | MVP |
| --- | --- | --- |
| `project_map` | no | yes |
| `search_api` | no | yes |
| `explain` | no | yes |
| `check` | no | yes |
| `routes`, `why`, `trace` | no | |
| `logs` (recent events, filterable) | no | |
| `audit` | no | |
| `generate` | yes, after confirmation | |
| `migrate` | yes, after confirmation | |

Every tool returns structured JSON plus a short text summary.

`search_api` searches an index built from Balise's source at release time. It also knows the names agents commonly invent (from the hallucination traps) and maps them to the right API.

### Prompts

Guided recipes, such as "add a CRUD resource" or "fix all audit warnings", which chain generators, `check` and `audit`.

## Docs for models

- **`AGENTS.md`**, generated in every project: conventions, commands, the working loop, the rule to debug through Balise and never inside dependencies, and the project's enabled modules. Short enough to stay in context.
- **`llms.txt`**: a compact reference of the whole public API, generated from the source for each release.
- **Error explanations**: read from the error registry by `balise explain` and MCP ([errors-and-logs.md](errors-and-logs.md#codes)).

Docs are generated, never written by hand, so they cannot drift from the code.

## Hooks

Agents run `cargo check` by habit, not `balise check`. `balise new` therefore installs hooks for common agents, so feedback does not depend on the agent remembering:

- Claude Code: a hook that runs `balise check` after each edit of a `.rs` file, and `balise audit` at the end of a task;
- Cursor: project rules that call the same commands;
- other agents: the same instructions in `AGENTS.md`.

Hooks are plain project files, versioned with the code.

Hooks call `balise check --hook` and `balise audit --hook`. In this mode the CLI speaks the agent's hook protocol: it reads the JSON describing the edit, skips files that are neither Rust nor TypeScript, and reports errors where the agent sees them (on Claude Code: stderr with exit code 2, the only output that reaches the agent). Without it, a plain `balise check` would run after every Markdown edit, and its failures would never reach the agent.

## Generators

- `balise generate resource orders` creates the folder, the handlers, the model, the tests, and the migration.
- Generated code follows every convention and compiles as is.
- The agent fills in the generated skeleton rather than writing structure from memory.

## Guided optimization

Agents ship code that works, rarely code that performs. Balise makes optimization a step of every feature.

### Detection

| Level | What it catches |
| --- | --- |
| Compile time | checks in Balise macros (unbounded query, implicit relation loading), Clippy lints as errors |
| Runtime, in dev | N+1 queries, slow queries with their plan, missing index, oversized response, missing pagination, task holding the event loop over 10 ms |
| `balise audit` | replays the tests or a scenario and produces a report |

Each finding is a `PERF` event with its location and expected fix ([errors-and-logs.md](errors-and-logs.md)).

### Budgets

- Every route has a default budget (maximum queries, maximum response size), deduced from its shape: return type (`Page<T>`, `T`, `Created<T>`, `NoContent`) and HTTP method. Defaults live in Balise, not in the project, so improving them reaches every project.
- `budgets.toml` holds only overrides, one entry per handler (`orders::list`), so changing the API prefix does not touch them.
- `balise routes` and the `project_map` MCP tool show each route's effective budget, and whether it is a default or an override.
- Only deterministic budgets fail tests. Latency varies between machines, so it is reported in the audit, never enforced: a flaky test teaches an agent to retry, not to fix.
- Any increase in `budgets.toml` is flagged by `balise check` as a change that needs human approval, so an agent cannot just raise the limit to make a test pass.

### Audit report

`balise audit` returns findings ranked by impact, each with code, route, location, measured value, budget and fix. The same report is available through MCP.

## Production feedback

Production reports ([errors-and-logs.md](errors-and-logs.md#production-findings)) bring real data back to where the agent works.

- `balise audit --from-prod report.json` imports a report: findings, real table sizes, index usage, expensive queries.
- Findings fixed or accepted are recorded in `.balise/prod-findings.toml`, versioned with the code. The next import shows only new or returning findings.
- The agent cannot read production itself: it asks the user to run `./app report` and share the file.

`AGENTS.md` describes this, and Balise reminds the agent at key moments:

- when `balise audit` runs and no production report was imported in the last 30 days;
- when the agent works on performance (a `PERF` finding, a slow route, an `explain` on a query);
- when a model has no `expected_rows` and the project is deployed;
- after `balise build`, when a new version is about to reach production.

Each reminder is one line, with the exact command to give the user.

## Evaluating the agent experience

The agent experience is measured, not assumed.

- A set of tasks ("add a paginated list of orders", "add an image upload", "fix the N+1 on this page") run by agents on fresh projects.
- Metrics: compile errors before the first green build, invented API names, iterations to green, audit warnings left at the end.
- Invented names feed the hallucination traps and the `search_api` index.
- Runs on every release, so a change that hurts agents is caught like a failing test.

## Open questions

- Final list of MCP tools for the MVP.
- How `balise audit` builds a scenario when the project has few tests.
- Which agents get pre-configured hooks beyond Claude Code and Cursor.