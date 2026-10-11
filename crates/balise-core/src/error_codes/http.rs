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
        fix: "Use a port above 1024: lower ports need special rights.",
        explanation: r#"
The system refused to let the app listen on this address. On Linux and macOS, ports below
1024 are reserved to privileged processes.

Run the app on a higher port, and let a reverse proxy, or the container's port mapping,
expose port 80 or 443.
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
}
