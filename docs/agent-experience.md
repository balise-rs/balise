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

`balise` is installed once on the host, before any project exists. It never requires Rust on the host: it drives Docker ([environments.md](environments.md)).

### A launcher and a CLI per version

Almost every command depends on the project's Balise version: `explain` reads its error registry, `check` its diagnostics, `generate` its templates. So `balise` is two programs:

- **The launcher**, the `balise` command itself: small and stable. Inside a project, it reads the Balise version in `Cargo.lock` and runs the CLI of that version; outside a project (`balise new`), it uses the latest version, or `--version`. Each version is downloaded once into `~/.balise/versions/<version>/`, checksum verified. Upgrading Balise in a project changes the CLI it gets, and other projects keep theirs: no CLI has to support a range of versions.
- **The versioned CLI**, published with each release of Balise. It holds every command and the MCP server. The project's dev image contains the same version, which `balise dev --inside` uses.

What reads the project itself (routes, models, configuration) is answered by the project's dev build, as subcommands of the app's binary built with `dev`, like `./app config check` already: `./app routes`, `./app why GET /orders/5`. The CLI calls them, so one implementation serves every version. `logs` and `trace` read the dev journal, where the app built by `balise dev` and the supervisor write every event and what routing decided for each request ([environments.md](environments.md#dev-with-docker)): they answer whether the app runs or not, across its restarts.

**One implementation for the CLI and MCP.** Each command is a function that takes typed arguments and returns structured data; it prints nothing itself. Commands are declared in one registry, from which both the CLI's arguments and the MCP tools' schemas are derived. The terminal shows compact text, `--json` the data; an MCP tool answers with the same text as `content` and the same data as `structuredContent`. Every MCP tool is a command (`search_api` is `balise search`); commands that need a human (`policy`, `approve` in `human` mode) are not MCP tools.

Errors of the CLI itself have codes in the `CLI` area, with their fix and explanation, like every other error. Exit codes: `0` success, `1` a problem found, `2` wrong usage.

| Command | Purpose |
| --- | --- |
| `balise new <name>` | create a project ([below](#balise-new)) |
| `balise dev` | start the dev environment, with reload; `--detach` runs it in the background, `balise dev status` and `balise dev stop` control it |
| `balise check` | compile and enrich errors; `--ci` checks everything, the release configuration included |
| `balise test` | start the test database, then run `cargo test` |
| `balise logs` | recent events of the app in dev and of `balise dev`, filtered by level, code, request and period |
| `balise approve` | review and decide pending sensitive decisions ([Sensitive decisions](#sensitive-decisions)) |
| `balise approvals` | list past decisions, filtered by author and date |
| `balise policy preset <preset>` | choose who decides sensitive decisions, for all types at once; needs a terminal |
| `balise policy set <type> human\|agent` | change who decides one type of sensitive decision; needs a terminal |
| `balise audit` | run tests or a scenario and report performance problems |
| `balise explain <CODE>` | explain an error code, Balise's or Rust's (`E0308`, explained by rustc); `balise explain config` lists every setting and its default, `balise explain budgets` how default budgets are chosen |
| `balise search <name>` | find a name in Balise's API, or what to write instead of a name from another framework |
| `balise introspect` | print the project map as JSON |
| `balise routes` | list routes and handlers |
| `balise why <METHOD> <path>` | explain a `404` or `405` |
| `balise trace <request_id>` | what happened to a request in dev: its route, parameters, limits and events |
| `balise generate <kind> <name>` | generate a resource, model or route |
| `balise migrate …` | regenerate the draft, `freeze` it, apply migrations; never interactive ([data.md](data.md#draft-migrations)) |
| `balise config check --env <env>` | validate configuration without starting the app |
| `balise mcp` | MCP server for agents (stdio) |

### `balise new`

**Interactive or not.** In a terminal, `balise new` asks only the questions whose flag is missing: without flags, it asks everything. Out of a terminal (an agent's shell, CI), or when the environment shows an agent (`CLAUDECODE`, `CI`), or with `--no-input`, it asks nothing: a missing required flag is an error with a code, which gives the complete command to run again.

The questions, in order:

1. modules: all (`orm`, `auth`, `mail`, and those to come), none, or one by one;
2. front end: React, or none;
3. database: PostgreSQL or SQLite, when `orm` is chosen;
4. the example resource: yes by default;
5. the policy for sensitive decisions: `autonomous`, `strict` or `full-auto`, each explained in one line ([Sensitive decisions](#sensitive-decisions)).

A summary is confirmed, then `balise new` prints the equivalent command with flags, to create the same project again.

| Flag | Without a terminal |
| --- | --- |
| `--front <framework>` or `--no-front` | required, exactly one of the two; `react` today, Vue and Svelte later |
| `--no-orm`, `--no-auth`, `--no-mail` | every module by default; `--minimal` refuses them all |
| `--db postgres\|sqlite` | `postgres` by default; an error with `--no-orm` |
| `--no-example` | the example resource by default |

**The policy is never chosen by a flag.** It says what the agent may decide alone, so an agent must not choose it at creation. In a terminal, the question is asked; otherwise the policy is `strict`, and `balise new` ends by asking the agent to have the user choose, in their terminal: `balise policy preset autonomous`.

**Defaults are announced.** Without a terminal, `balise new` lists the defaults it applied, each with how to change it: recreating the project costs nothing yet, and the example resource can be removed later with `balise remove resource`.

```
Created shop/ (Balise 0.4.2)

Defaults applied:
  modules   orm, auth, mail   change: recreate with --no-orm, --no-auth or --no-mail
  database  postgres          change: recreate with --db sqlite
  example   orders            remove it later: balise remove resource orders

Recreate with other choices (nothing to lose yet):
  rm -r shop && balise new shop --front react --no-mail

Policy: strict (every sensitive decision waits for the user).
Ask the user to choose how much you decide alone, in their terminal:
  balise policy preset autonomous   (recommended)
```

**What it writes.** The project comes from a reference template kept in the repository, a real project compiled in CI, never from the mock-up. Most files are copied as they are (the example resource, front-end pages, configuration files); a few are assembled from the choices (`Cargo.toml`, the `mod` lines of `main.rs`, `compose.yaml`, the module sections of `AGENTS.md`, `.balise/policy.toml`). One `Cargo.lock` and one `pnpm-lock.yaml`, produced in CI with every module enabled, are copied whatever the choices: cargo and pnpm drop what a project does not use at its first build, and versions stay pinned. The lockfile also tells the launcher the project's version before its first build. CI runs `balise new` with the main combinations (all modules, `--minimal`, `--no-front`, SQLite) and checks that each project builds.

The example resource adapts: with `orm`, a resource stored in the database; without it, a route and a page, and storage is left to the agent.

### Prerequisites at `balise new`

`balise new` checks that git and Docker are installed, and that the Docker daemon is running.

- **git present**: the project is initialized as a repository, with a generated `.gitignore`. Nothing is committed: the first commit is the user's.
- **Something missing**: `balise new` says so and keeps going. Neither is needed to create the project; Docker is needed later, for `balise dev`.
- For each missing tool, Balise prints two ready-to-paste prompts, adapted to the detected operating system, and saves them in `.balise/prompts/`:
  - one asking the agent how to install the tool on this system;
  - one asking the agent for a short tutorial: what the tool is, why Balise uses it, and the few commands worth knowing.

The user does not need to know what git or Docker are to get started: the agent becomes their guide.

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
| `logs` (recent events, filterable by code, level and period) | no | yes |
| `mails` (emails captured in dev, with their links) | no | |
| `audit` | no | |
| `approve` (a sensitive decision, in `agent` mode) | yes | yes |
| `generate` | yes, after confirmation | |
| `migrate` | yes, after confirmation | |

Each tool is served by what it really needs, so the agent keeps its tools when the code does not compile:

| Tool | Served by | Without a successful build |
| --- | --- | --- |
| `search_api`, `explain` | `balise mcp` itself, with the index and the registry of the project's Balise version | yes |
| `check` | the CLI | yes |
| `project_map` | the map of the last successful build, cached in `.balise/` | yes, with a `stale_since` field and a line saying the code does not compile |
| `logs`, `trace` | the dev journal, a file in the project | yes, with what the app logged while it ran |
| `mails` | the dev server | no: the answer gives the command to start it (`balise dev --detach`) |

The CLI reads the project's Balise version in `Cargo.lock` and downloads the matching index once, cached in `~/.balise/`. The first use of a version needs the network, and the CLI says so plainly when there is none.

Every tool answers in two parts: `content`, compact text written for a model (what the agent reads), and `structuredContent`, the same data as JSON for programs. Text is 1.5 to 2 times smaller than the same data in JSON.

- **`project_map` has two levels.** Without argument, one line per route and per model, about 2,000 tokens for a project of 50 routes; beyond about 3,000 tokens, one line per resource (`orders: 6 routes, model Order`). It also lists what modules add for the agent (the `mails` tool, the `/_balise/mails` page). With `resource`, the details of one resource: fields and rules, owner, budgets with their origin, possible errors, tests, front-end pages. Every item points to a file and a line.
- **`explain` takes a code or a topic**: `explain(code: "DB0042")`, `explain(topic: "budgets")`. Topics cover what an error code does not: pagination, responses, writes, budgets, configuration. They live in the same registry as codes, one file per topic.
- **`search_api` answers invented names**: it says the name does not exist in Balise, which framework it comes from, and what to write instead, with an example. A search with no result says so, and suggests related names.

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

- Claude Code: hooks after each edit and at the end of a task;
- Cursor: project rules that call the same commands;
- other agents: the same instructions in `AGENTS.md`.

Hooks are plain project files, versioned with the code. They call `balise check --hook` and `balise audit --hook`: in this mode the CLI speaks the agent's hook protocol, reads the JSON describing the edit, skips files that are neither Rust nor TypeScript, and reports where the agent sees it (on Claude Code: stderr with exit code 2).

Checks run at two levels, plus CI:

| When | What runs | Effect |
| --- | --- | --- |
| after each edit (`check --hook`) | incremental `cargo check` for a `.rs` file, `tsc --incremental` for a `.ts` file | short output: the number of errors and the first three, with "finish your change, then fix". A new edit cancels the check in progress |
| end of task (`audit --hook`) | ESLint, schema drift, tests, audit, runtime errors seen in dev, sensitive decisions pending in `agent` mode | blocks on what the agent can fix |
| CI (`check --ci`) | everything, plus the release build and decisions pending in `human` mode | fails |

A change across several files is not interrupted, and the slow checks run once per task. Whether checking after each edit is worth it is measured in the [log of alternatives](alternatives.md).

## Sensitive decisions

Some changes are decisions, not fixes: they trade a guarantee for something else. One list:

| Type | Example |
| --- | --- |
| `budget` | raising a value in `budgets.toml` |
| `accept` | `.accept(PERF0007, reason)` on a finding |
| `unbounded` | `.unbounded(reason)` on a query |
| `all_owners` | `.all_owners(reason)` on an owned model |
| `raw_unscoped` | raw SQL on an owned table |
| `breaking_migration` | `balise migrate --breaking` |
| `dependency` | a new Cargo or npm dependency |

**Who decides** is set per type in `.balise/policy.toml`: `human` or `agent`, `human` by default. The file changes only through `balise policy preset <preset>` and `balise policy set <type> human|agent`, which need a terminal; the agent is not allowed to write it, and the file is versioned, so any change of policy shows in the diff. Three presets:

- `autonomous` (recommended): `agent` for technical decisions, `human` for `all_owners`, `raw_unscoped` and `breaking_migration`;
- `strict`: `human` for everything; the policy of a project created without a terminal, until the user chooses;
- `full-auto`: `agent` for everything; choosing it says once what it means for security.

**In `human` mode**, the hook does not block. Each pending decision is shown once to the agent, which tells the user. The user runs `balise approve`, which lists what is pending, with its context, one by one or all at once.

**In `agent` mode**, deciding stays a separate act: the `approve` MCP tool, or `balise approve --agent <id> --reason …`, without a terminal. Balise gives the context (measured values, possible alternatives). The end-of-task hook blocks with "decide with a reason, or change the code". The decision can be given to a reviewing agent, in a fresh context.

**Every decision is recorded** in `.balise/approvals.toml`: type, handler, reason, fingerprint, author (`human:<name>` or `agent`) and date. The fingerprint covers the normalized content, not the line: reformatting or moving code needs no new decision, a new value or a new reason does. `balise approvals --by agent --since …` reviews decisions afterwards. `balise check --ci` fails while a decision is pending.

These guard rails stop a shortcut, not a deliberate bypass: an agent could still edit files it should not. The real control is reviewing the diff.

## Generators

- `balise generate resource orders` creates the folder, the handlers, the model, the tests, and the migration.
- `balise remove resource orders` undoes it: the back-end and front-end folders, the `mod` line, the seeds and pages that use it, and the table, by rewriting draft migrations ([data.md](data.md#draft-migrations)). A module declared without its folder is reported by `balise check` with the exact fix.
- To remove reliably without parsing TypeScript or Rust, generators wrap what they add outside the resource's folders in markers:

  ```tsx
  {/* balise:orders */}
  <Route path="/orders" element={<OrderList />} />
  {/* /balise:orders */}
  ```

  `remove` deletes the marked blocks and both folders, rewrites the draft migration, then runs `balise check`. A marked block that was moved or edited is reported with its file and line, never guessed. `remove` refuses when another resource depends on this one.
- The example resource of a new project doubles as a tutorial: removing it is the first use of `balise remove`. When the resource removed is the example itself, Balise suggests the next step, `balise generate resource <name>`.
- Generated code follows every convention and compiles as is. Generators write the `pub` that models, inputs and outputs need to be used from the handlers.
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

- Every route has a default budget (maximum queries, maximum response size, maximum body size), deduced from its shape: return type (`Page<T>`, `T`, `Created<T>`, `NoContent`) and HTTP method. Defaults live in Balise, not in the project, so improving them reaches every project.
- `budgets.toml` holds only overrides, one entry per handler (`orders::list`), so changing the API prefix does not touch them.
- `balise routes` and the `project_map` MCP tool show each route's effective budget, and whether it is a default or an override.
- `body_size` and `timeout` are the budgets enforced at runtime: a larger body answers `413` (`HTTP0011`), a longer request `503` (`HTTP0013`) or `408` (`HTTP0012`). `budgets.toml` is embedded in the binary, so a deployment cannot leave it behind; it is required, even with only comments, so that cargo notices every change. An entry naming no handler (`CFG0009`) or an unknown budget (`CFG0010`) stops the startup.
- Only deterministic budgets fail tests. Latency varies between machines, so it is reported in the audit, never enforced: a flaky test teaches an agent to retry, not to fix.
- Every request made in a test checks its route's budget automatically. There is no per-test assertion: budgets live only in `budgets.toml`. Lowering a value is free; raising one is a [sensitive decision](#sensitive-decisions), so an agent cannot raise the limit to make a test pass.

### Audit report

`balise audit` returns findings ranked by impact, each with code, route, location, measured value, budget and fix. It also brings back the runtime errors and `PERF` warnings seen in dev since the last audit, grouped by code and location with a count, so the end-of-task hook shows them without the agent looking for them. The same report is available through MCP.

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