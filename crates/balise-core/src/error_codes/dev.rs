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
}
