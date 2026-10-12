//! `RUN`: the life of the process: startup checks, shared state, panics, crashes, shutdown.

codes! {
    RUN0001 {
        title: "Cannot start the server",
        message: "Cannot start the server: {cause}",
        fix: "The system refused to create the server's threads: check the memory and the process limits of this machine or container.",
        explanation: r#"
Before listening, Balise starts the threads that run requests. The system refused to create
them, which happens when the machine or the container is out of memory, or when its limit of
processes or threads is reached (`ulimit -u`, a container's `pids` limit).
"#,
    }
    RUN0002 {
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
    RUN0003 {
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
    RUN0004 {
        title: "State declared twice",
        message: "`{type}` is created by two `#[balise::state]` functions: {first} and {second}",
        fix: "Keep one of the two functions: each type of state exists once. Wrap one of them in its own type if both are needed.",
        explanation: r#"
`State<T>` finds its value by type, so each type has one `#[balise::state]` function. Two
values of the same type, such as two HTTP clients with different settings, each get their
own type: `struct BillingClient(Client)`.
"#,
    }
    RUN0005 {
        title: "State cannot be created",
        message: "Cannot create the state `{type}`: {message}",
        fix: "Fix what its `#[balise::state]` function reports: the cause and location say where it failed.",
        explanation: r#"
A `#[balise::state]` function returned an error at startup: a service it connects to is
unreachable, a file it loads is missing… The app does not start without its state, rather
than failing on every request that reads it.
"#,
    }
    RUN0006 {
        title: "Panic",
        message: "{what} panicked: {message}",
        fix: "Fix the code at the location shown: a panic is a bug, such as an `unwrap()` on a value that can be missing. Return an error with `?` for what can fail.",
        explanation: r#"
A panic stops the code that raised it: `unwrap()` on `None` or `Err`, an index out of
bounds, an explicit `panic!`. Balise catches it where it happens, so only that request or
task fails: the request answers `500`, and the event shows where the panic was raised and
the frames of the application that led there, with the framework's frames folded.

A panic is a bug, not a way to answer: what can fail returns an error, with `?` or an
`AppError`.
"#,
    }
    RUN0007 {
        title: "The previous run crashed",
        message: "The previous run crashed at {time}: {message}",
        fix: "Fix the code at the location shown. A panic outside a request or a task, or a second panic while the first one unwinds, stops the whole process.",
        explanation: r#"
Balise catches panics in requests and tasks. A panic it cannot catch stops the process: one
raised outside them (a thread started by hand), or one raised while another unwinds, such
as in a `Drop`. Before the process stops, Balise writes a crash report to
`<storage.dir>/.balise/crashes/`; the next start publishes it with this code and deletes it.

Crashes that leave no time to write anything, such as a stack overflow or the machine
running out of memory, have no report: the orchestrator's logs keep what the process wrote
on stderr.
"#,
    }
    RUN0008 {
        title: "Shutdown timeout reached",
        message: "Stopped after {timeout} with {requests} request(s) and {tasks} task(s) unfinished",
        fix: "Find what kept them running: a slow handler or a long task. Raise `shutdown.timeout` only if they must finish, and the orchestrator's grace period with it.",
        explanation: r#"
On `SIGTERM` (or Ctrl-C), Balise stops accepting connections and waits for the requests and
background tasks in progress, for `shutdown.timeout` at most (10 s by default). What is still
running then is cut: a request's transaction rolls back, a task stops where it is.

The orchestrator must wait longer than `shutdown.timeout` before it kills the process: the
generated `compose.yaml` derives its `stop_grace_period` from it.
"#,
    }
}
