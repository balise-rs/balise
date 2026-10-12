//! `HTTP`: listening, connections and routing.

codes! {
    HTTP0001 {
        title: "Port already in use",
        message: "Cannot listen on {address}: the port is already in use",
        fix: "Stop the other process that listens on this port, or run the app on another port.",
        explanation: r#"
Another process already listens on the address the app wants, so the app did not start.

It is often a previous run of the app that is still alive, or another service.
`lsof -i :<port>` (or `ss -ltnp`) shows which process holds the port.
"#,
    }
    HTTP0002 {
        title: "Connection error from the client",
        message: "Connection error from the client: {cause}",
        fix: "Nothing to fix in the app: the client closed the connection or sent an invalid request.",
        explanation: r#"
The connection ended in an error caused by the client: it closed the connection before the
response was sent, aborted it while it was being opened, or sent bytes that are not valid HTTP.

This is not an error of the server, so it is logged at the `debug` level. If it shows up
often for the same client, look at that client or at the proxy in front of the app.
"#,
    }
    HTTP0003 {
        title: "No route",
        message: "No route for {method} {path}",
        fix: "Check the method and the path. `balise routes` lists the routes of the app.",
        explanation: r#"
No route of the app matches this method and path, so Balise answered `404`.

Paths in route attributes never include the API prefix: `#[get("/orders")]` is served at
`/api/orders`. `balise why GET /api/orders/1` explains why a request matches no route.
"#,
    }
    HTTP0004 {
        title: "Not allowed to listen",
        message: "Cannot listen on {address}: permission denied",
        fix: "Use a port above 1024 (`PORT=8080`), or give the app the right to use low ports: on Linux, `sudo setcap cap_net_bind_service=+ep <binary>`.",
        explanation: r#"
The system refused to let the app listen on this address. On Linux and macOS, ports below
1024 are reserved to privileged processes; Balise does not forbid them, the system does.

To serve on port 80 or 443:

- in a container, the app's user may bind them, or the container's port mapping can expose
  the app's port as 80 (`-p 80:<port>`);
- on Linux, give the binary the right without running it as root:
  `sudo setcap cap_net_bind_service=+ep <binary>` (again after each build);
- behind a reverse proxy, the proxy listens on 80 or 443 and the app on a higher port.
"#,
    }
    HTTP0005 {
        title: "Address not available",
        message: "Cannot listen on {address}: the address does not belong to this machine",
        fix: "Listen on 0.0.0.0 for every interface, or on an address of this machine.",
        explanation: r#"
The app was asked to listen on an IP address that no network interface of this machine has.

`0.0.0.0` listens on every interface, which is what containers need. `127.0.0.1` listens
only for connections from the same machine.
"#,
    }
    HTTP0006 {
        title: "Cannot listen",
        message: "Cannot listen on {address}",
        fix: "Read the cause below: the system refused to open this address for a reason Balise does not recognize.",
        explanation: r#"
The app could not open the address it listens on, for a reason other than the common ones
(port in use, permission denied, address not available). The cause gives the system's own
message.
"#,
    }
    HTTP0007 {
        title: "Cannot start the server",
        message: "Cannot start the server: {cause}",
        fix: "The system refused to create the server's threads: check the memory and the process limits of this machine or container.",
        explanation: r#"
Before listening, Balise starts the threads that run requests. The system refused to create
them, which happens when the machine or the container is out of memory, or when its limit of
processes or threads is reached (`ulimit -u`, a container's `pids` limit).
"#,
    }
    HTTP0008 {
        title: "Too many open files",
        message: "Cannot accept connections: too many open files",
        fix: "Raise the limit of open files (`ulimit -n`, or the container's `nofile` limit), or look for connections that are never closed.",
        explanation: r#"
Each connection uses a file descriptor, and the process reached its limit, so new
connections wait until others close. The server keeps running and retries.

On a busy server, the default limit (often 1024) is too low: raise it. If the number of open
connections keeps growing while traffic does not, something keeps connections open: a client
that never closes them, or a handler that never finishes.
"#,
    }
    HTTP0009 {
        title: "Method not allowed",
        message: "{method} is not allowed on {path}; allowed: {allowed}",
        fix: "Use one of the allowed methods, or add a route for this method.",
        explanation: r#"
A route matches this path, but not with this method, so Balise answered `405` with an
`Allow` header listing the methods the path accepts.

A common cause is a form or a client sending `POST` where the route expects `PUT` or `PATCH`.
`balise routes` lists every route with its method.
"#,
    }
    HTTP0010 {
        title: "Duplicate route",
        message: "Two handlers for {method} {path}: {first} and {second}",
        fix: "Keep one of the two handlers, or change the path or the method of one of them.",
        explanation: r#"
Two handlers declare the same method and the same path, so Balise cannot choose and the app
does not start. Paths are compared by shape: `/orders/{id}` and `/orders/{order_id}` are the
same route.
"#,
    }
    HTTP0011 {
        title: "Invalid path parameter",
        message: "`{name}` in the path is not a valid {expected}: `{value}`",
        fix: "Use a URL whose parameters have the expected form; `balise routes` shows each route's parameters.",
        explanation: r#"
A path parameter could not be read as the type the handler declares (`Path<u32>` and the
path says `abc`). The URL designates nothing, so the answer is `404`, as for a row that does
not exist.
"#,
    }
    HTTP0012 {
        title: "Request body too large",
        message: "The request body is larger than this route's limit of {limit}",
        fix: "If the route is meant to receive bodies this large, raise its `body_size` in `budgets.toml`, under the handler's name. To receive files, declare `File` fields in the route's `Input`. Otherwise the client is sending more than the route accepts.",
        explanation: r#"
Balise stops reading a body as soon as it exceeds the route's limit and answers `413`,
without loading the rest in memory. There are three usual causes:

- The route is meant to receive this much, such as a bulk import. Raise its limit in
  `budgets.toml`, under the handler's name:

      ["imports::create"]
      body_size = "20MB"

  Raising a limit is a sensitive decision (`.balise/policy.toml`): a larger limit lets any
  client make the server read more.
- The client sends a file inside the JSON, as base64. Declare it as a `File` field of the
  route's `Input` instead: the file is streamed to disk, and the route's limit follows the
  declared sizes.
- The client sends more than the route accepts, by mistake or not. The `413` is the right
  answer and there is nothing to change on the server.

The default limit of a route without files is set in `balise.toml`; `balise explain config`
shows its value, and `balise routes` shows each route's limit and where it comes from.
"#,
    }
    HTTP0013 {
        title: "Application error code declared twice",
        message: "`{code}` is declared by two `AppError` variants: {first} and {second}",
        fix: "Give one of the two variants another code: each code means one thing for the front end.",
        explanation: r#"
Application error codes are part of the contract with the front end, which may react to a
code. Two variants with the same code would make it ambiguous, so the app does not start.
Within one `AppError` enum, the derive already rejects it at compile time; this check
covers codes declared in different enums.
"#,
    }
    HTTP0014 {
        title: "State not declared",
        message: "No `#[balise::state]` creates `{type}`, which {reader} reads with `State<{type}>`",
        fix: "Declare a function that creates it, next to the resource that uses it: `#[balise::state] fn …() -> {type} { … }`.",
        explanation: r#"
`State<T>` reads a value created once at startup by a function marked `#[balise::state]`.
Balise checks at startup that every `State<T>` a handler reads has its function, so the app
does not start with a handler that would always fail. A task that reads a state nobody
declared gets this error when it runs.
"#,
    }
    HTTP0015 {
        title: "State declared twice",
        message: "`{type}` is created by two `#[balise::state]` functions: {first} and {second}",
        fix: "Keep one of the two functions: each type of state exists once. Wrap one of them in its own type if both are needed.",
        explanation: r#"
`State<T>` finds its value by type, so each type has one `#[balise::state]` function. Two
values of the same type, such as two HTTP clients with different settings, each get their
own type: `struct BillingClient(Client)`.
"#,
    }
    HTTP0016 {
        title: "State cannot be created",
        message: "Cannot create the state `{type}`: {message}",
        fix: "Fix what its `#[balise::state]` function reports: the cause and location say where it failed.",
        explanation: r#"
A `#[balise::state]` function returned an error at startup: a service it connects to is
unreachable, a file it loads is missing… The app does not start without its state, rather
than failing on every request that reads it.
"#,
    }
    HTTP0017 {
        title: "Request body too slow",
        message: "The request body was not received within this route's timeout of {timeout}",
        fix: "If clients are expected to send this slowly (large uploads on slow connections), raise `timeout` for `{handler}` in `budgets.toml`. Otherwise nothing is wrong on the server.",
        explanation: r#"
Every route has a timeout: the default (`limits.timeout`), longer for a route with files in
proportion to their declared sizes, or the `timeout` of its entry in `budgets.toml`. The
client was still sending the body when it ran out, so Balise stopped reading and answered
`408`. Most often the client's connection is slow or stalled.
"#,
    }
    HTTP0018 {
        title: "Handler timed out",
        message: "`{handler}` did not answer within its timeout of {timeout}",
        fix: "Find what the handler waits for (a slow query, an external call), make it faster, or move the work to `balise::spawn`. Raise `timeout` for `{handler}` in `budgets.toml` only if it must take this long.",
        explanation: r#"
Every route has a timeout: the default (`limits.timeout`), longer for a route with files, or
the `timeout` of its entry in `budgets.toml`. When it runs out, the handler is cancelled
and the answer is `503`. A cancellation rolls back the request's transaction, but external
calls already sent are not undone.

Work that is allowed to take long belongs in a background task (`balise::spawn`), with the
client told that it was accepted (`Created<T>` or `NoContent`).
"#,
    }
    HTTP0500 {
        title: "Internal error",
        message: "Internal error",
        fix: "Look up the request ID in the server logs: the error is logged there with its code, location and cause.",
        explanation: r#"
In production, a `5xx` response never shows its real code, message or cause: they may
reveal infrastructure details (addresses, queries, file paths). The client gets this
generic code with the request ID, and the server logs the full error under the same ID.

In dev, the response keeps the real code, with its location, fix and cause.
"#,
    }
}
