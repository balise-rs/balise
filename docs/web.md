# Web
 
Status: draft. This describes the target design; nothing is implemented yet.
 
Choosing Rust for a back end usually means wiring the front end alone: front-end frameworks are not documented for a Rust back end. Balise removes that gap. The front end is a first-class part of a Balise project: created, wired, checked and secured by the same tool, under the same rule as the back end: **Balise owns what the agent sees, on the front end too.**
 
## Project layout
 
```
src/                 Rust application (see application.md)
frontend/
  src/
    balise/          generated client, types and validation rules, never edited by hand
    pages/
      orders/        example resource: list, detail, form
  package.json
  pnpm-lock.yaml
```
 
- API routes live under `/api` by default. Everything else is served by the front end.
- Front end and API share one origin, in dev and in prod, so CORS is not needed by default.
## Frameworks
 
React first: it is the framework agents know best, so the one where they make the fewest mistakes. Vue and Svelte follow, with the same features.
 
`balise new shop --front react` creates a project that works end to end:
 
- Vite, React, TypeScript and a router, configured;
- the generated client, already wired;
- an example resource with a list page, a detail page and a form, calling the real API;
- with `--auth`: working sign-in, sign-up and sign-out pages, and route guards.
The agent starts from a live example, not a blank page.
 
## Generated client
 
From every route, `Input`, `Output` and channel, Balise generates:
 
- a TypeScript type for each `Input` and `Output`, with optional fields marked as optional;
- a typed function per route and per channel;
- each route's limits: body size, accepted file types;
- the validation rules of each `Input` (`min`, `max`, `length`, `email`, `url`, `one_of`).
```ts
import { orders } from "@/balise";
 
const res = await orders.create({ shopId: 3, body: { productId: 1, contact: "a@b.fr" } });
if (res.ok) {
  showOrder(res.data);          // typed as OrderView
} else {
  showError(res.error);         // code, message, fields
}
```
 
- Every call returns `{ ok: true, data }` or `{ ok: false, error }`. No exception for HTTP errors.
- Names follow the front end's convention (`product_id` becomes `productId`).
- `balise dev` regenerates the client on every Rust change. `balise check` fails if it is out of date.
- When a Rust type changes, the front end stops compiling until it follows: the agent is told exactly what to update.
## Framework adapter
 
`@balise-rs/react` (then `@balise-rs/vue`, `@balise-rs/svelte`) builds on the generated client:
 
| Hook | Purpose |
| --- | --- |
| `useQuery(orders.list, params)` | fetch with loading, error and cache |
| `useMutation(orders.create)` | send, then invalidate the affected queries |
| `useForm(orders.create)` | form state, validation, submission |
| `useChannel(shops.orders, { shopId })` | realtime subscription, with automatic reconnection |
| `useCurrentUser()` | signed-in user, or `null` |
 
Caching and invalidation are delegated to TanStack Query, hidden behind Balise's API. Replacing it later is possible without changing application code.
 
### Forms
 
Validation rules are declared once, in the Rust `Input`, and enforced on both sides:
 
- `useForm` validates on the client with the generated rules, before sending;
- the server validates again, with the same rules;
- a `422` from the server places each message under its field.
File fields check size and type before upload, with the limits declared on the route.
 
## The front end in the agent loop
 
- `balise check` also runs TypeScript and the linter on the front end, and reports in Balise's format.
- **Browser errors reach the dev server's logs.** In dev, uncaught JavaScript errors and failed API calls are sent to the Balise server and appear in `balise logs` and through MCP, with the page and the line. An agent cannot see the browser console; this closes that blind spot.
- A lint rule forbids calling `/api` with raw `fetch`: the generated client is the only way. This is the front-end version of the hallucination traps.
- The MCP project map includes front-end pages and the API routes each one calls.
- `balise generate resource orders --front` creates the back end and its pages, already wired.
## Dependency security
 
npm packages can run code when they are installed, and compromised releases spread fast (the Shai-Hulud worm stole tokens and credentials through install scripts). A user who vibe codes cannot judge each package. Balise sets safe defaults for them.
 
- **pnpm, pinned.** Projects use pnpm, at a version pinned in the dev image.
- **Minimum release age of 7 days.** A package version is installable only a week after publication. Most malicious releases are detected and removed within that time.
- **No install scripts by default.** Dependency lifecycle scripts do not run. A package that really needs one goes on an explicit allowlist.
- **Human approval for trust changes.** `balise check` flags as needing approval: a new direct dependency, an allowlist addition, a lower release age. An agent cannot loosen the policy on its own.
- **`balise add <package>`** is the way to add a dependency. It checks release age, install scripts, known advisories, and names close to a popular package (typosquatting), then explains its findings.
- **Installs run inside the dev container.** Host credentials (npm tokens, SSH keys, cloud credentials) are not mounted in it, so a malicious package has nothing to steal from the host.
- **Frozen lockfile.** CI and production builds install exactly what `pnpm-lock.yaml` records.
- **Nothing from npm in production.** The front end is built in a build stage; the final image contains only the static assets embedded in the binary.
- **Rust crates too.** Build scripts and procedural macros also run code at compile time. `balise add` applies the same checks to crates, and compilation runs in the container.
- **Balise's own packages** are published from CI with provenance, and keep few dependencies.
## Realtime
 
Two transports, one model:
 
- **SSE** for server-to-client updates (notifications, live lists). The default.
- **WebSocket** when the client also sends messages (chat, collaborative editing).
Channels are declared and typed like routes:
 
```rust
#[derive(Output)]
struct OrderEvent { id: OrderId, status: String }
 
#[channel("/shops/{shop_id}/orders")]
async fn subscribe(shop_id: Path<ShopId>, user: CurrentUser) -> Result<Channel<OrderEvent>> {
    user.require_shop(*shop_id)?;            // authorization runs once, on subscribe
    Ok(Channel::open())
}
 
// anywhere in a handler
channels.publish(format!("/shops/{}/orders", shop_id), OrderEvent { id, status })?;
```
 
- The front end subscribes with `useChannel`.
- Each connection has a bounded buffer. A client too slow to keep up is disconnected with an `RT` code rather than growing memory.
- Pub/sub runs in-process. With several instances, the Redis module relays messages between them.
- Connections are closed cleanly on shutdown ([engine.md](engine.md#shutdown)).
## Dev server
 
`balise dev` starts the Rust server and the Vite dev server together:
 
- requests to `/api` go to Rust, everything else to Vite, with hot module replacement;
- one URL, one command, one log stream in Balise's format, including browser errors;
- changing the front end never recompiles Rust.
## Production assets
 
- `balise build` runs the front end's production build and embeds the result in the binary.
- File names are content-hashed and served with long-lived cache headers; `index.html` is never cached.
- Unknown `GET` paths outside `/api` return `index.html`, so client-side routing works.
- Compression (gzip, brotli) is applied at build time, not per request.
## Authentication for SPAs
 
Provided by the `auth` module.
 
- Sessions in `HttpOnly`, `Secure`, `SameSite=Lax` cookies. No tokens in `localStorage`.
- CSRF protection on every non-`GET` request. The generated client sends the token automatically.
- Generated routes: sign up, sign in, sign out, current user, password reset. Generated pages for each, when the project has a front end.
- Passwords hashed with Argon2 (delegated crate). Sign-in attempts are rate-limited.
- `CurrentUser` in a handler or channel returns the user or answers `401`.
## Open questions
 
- Replacing TanStack Query with an in-house cache, once the adapter's API is stable.
- Token-based authentication for mobile or third-party clients, alongside cookies.
- Whether to generate an OpenAPI description in addition to the TypeScript client.
- Server-side rendering: out of scope for now.
