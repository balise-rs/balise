//! `DEV`: the dev environment: `balise dev`, its builds, and the app it runs.

codes! {
    DEV0001 {
        title: "The app stopped",
        message: "The app stopped on its own ({status})",
        fix: "Read what it wrote just above. Save a file to rebuild and start it again.",
        explanation: r#"
`balise dev` runs the app and watches it. The app stopped without being asked to: a panic
outside a request or a task, a stack overflow, or an error at startup (an invalid setting,
a port already in use). Its last lines say why.

When the app stops long after starting, `balise dev` starts it again. When it stops right
after starting, starting it again would fail the same way: `balise dev` waits for a change
of file instead, then rebuilds and starts it.
"#,
    }
    DEV0002 {
        title: "The build failed",
        message: "The build failed with {errors}",
        fix: "Fix the errors above and save: `balise dev` rebuilds on its own. Until then, the last version that built keeps running.",
        explanation: r#"
`balise dev` rebuilds the app at each change of file. While the code does not compile, the
last version that built keeps answering, so that the front end and the agent's requests
still work. The errors are shown in short form; `balise check` explains them in full.
"#,
    }
    DEV0003 {
        title: "Cannot build the app",
        message: "Cannot run `cargo`: {cause}",
        fix: "Install Rust (https://rustup.rs), or run `balise dev` without `--native` to build in Docker.",
        explanation: r#"
`balise dev --native` builds and runs the app on this machine, with its Rust toolchain.
Without `--native`, `balise dev` builds in Docker, and Rust is not needed on the machine.
"#,
    }
    DEV0004 {
        title: "Docker is not installed",
        message: "`balise dev` runs the app in Docker, and Docker is not installed",
        fix: "Install Docker (https://docs.docker.com/get-docker/), or run `balise dev --native` to build with the Rust of this machine.",
        explanation: r#"
`balise dev` builds and runs the app in a container, with the Rust toolchain pinned by
`rust-toolchain.toml` and the Balise CLI of the project's version: nothing else to install,
and the same environment on every machine.

`balise dev --native` runs the same supervisor on this machine instead, with its own Rust.
"#,
    }
    DEV0005 {
        title: "Docker is not running",
        message: "Docker is installed but does not answer",
        fix: "Start Docker (Docker Desktop, or `sudo systemctl start docker` on Linux), then run `balise dev` again.",
        explanation: r#"
`balise dev` asked the Docker daemon for its state and got no answer within a few seconds.
The daemon is stopped, still starting, or this user cannot reach it (on Linux, the user
must be in the `docker` group).
"#,
    }
    DEV0006 {
        title: "The port is taken",
        message: "Port {port} is already used by another program",
        fix: "Stop the program that uses it, or set another port in `balise.toml`: `port = {other}`.",
        explanation: r#"
The app listens on the port of `balise.toml` (`port`, or the `PORT` variable), and Docker
publishes it on the machine. Another program already listens on it: often another app in
dev, or a `balise dev --native` still running.

    [env.dev]
    port = 3001
"#,
    }
    DEV0007 {
        title: "Docker failed",
        message: "`docker compose {command}` failed: {cause}",
        fix: "Read Docker's output above. `compose.yaml` and `Dockerfile` come from `balise new`: compare them with a new project's if they were changed.",
        explanation: r#"
`balise dev` runs `docker compose` with the project's `compose.yaml`, which builds the
`Dockerfile`'s image (Rust, and the Balise CLI of the project's version) and starts the
container. Building the image the first time downloads Rust and compiles the CLI: it needs
the network.
"#,
    }
}
