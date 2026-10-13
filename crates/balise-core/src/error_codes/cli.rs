//! `CLI`: the `balise` command itself: its arguments, its versions, and what it creates.

codes! {
    CLI0001 {
        title: "Unknown command",
        message: "Unknown command `{command}`{suggestion}",
        fix: "Run `balise help` to list the commands.",
        explanation: r#"
The first word after `balise` names a command: `new`, `dev`, `check`, `explain`… The
message suggests the closest command when there is one. `balise help` lists them all, and
`balise help <command>` describes one.
"#,
    }
    CLI0002 {
        title: "Unknown option",
        message: "`{option}` is not an option of `balise {command}`{suggestion}",
        fix: "Run `balise help {command}` to list its options.",
        explanation: r#"
Each command has its own options, written `--name` or `--name value`. The message suggests
the closest option of the command when there is one.
"#,
    }
    CLI0003 {
        title: "Missing argument",
        message: "`balise {command}` needs {missing}",
        fix: "Run it again with what is missing: `{example}`.",
        explanation: r#"
Some arguments have no default: the project's name for `balise new`, or a choice that must
be made explicitly, such as `--front react` or `--no-front`. In a terminal, `balise new`
asks for what is missing; without one, as when an agent runs it, every required argument
must be given, and the message shows the complete command.
"#,
    }
    CLI0004 {
        title: "Invalid value",
        message: "`{value}` is not a valid value for `{option}`; expected {expected}",
        fix: "Use one of the values the message lists.",
        explanation: r#"
An option that takes a value accepts only some values: `--db` takes `postgres` or `sqlite`,
for instance. `balise help <command>` lists them.
"#,
    }
    CLI0005 {
        title: "Conflicting options",
        message: "`{first}` and `{second}` cannot be used together",
        fix: "Keep one of the two.",
        explanation: r#"
Some options say opposite things, such as `--front react` and `--no-front`, or need
something another option removes, such as `--db sqlite` with `--no-orm`.
"#,
    }
    CLI0006 {
        title: "Not available yet",
        message: "{what} is not available in Balise {version} yet",
        fix: "Leave it out for now: {instead}.",
        explanation: r#"
Balise is built step by step, and `balise new` only generates what works in its version.
An option for something that comes later (a front-end framework, a module) is refused
rather than accepted without effect. The option will work, unchanged, once the version
that brings it is out.
"#,
    }
    CLI0007 {
        title: "Invalid project name",
        message: "`{name}` cannot be a project name: {reason}",
        fix: "Use lowercase letters, digits, `-` and `_`, starting with a letter, such as `shop` or `my-shop`.",
        explanation: r#"
The project's name is also the name of its Rust crate and of its folder, so it follows the
rules of crate names, and cannot be the name of a crate every project already has (`std`,
`core`, `balise`…).
"#,
    }
    CLI0008 {
        title: "Folder not empty",
        message: "`{path}` already exists and is not empty",
        fix: "Choose another name, or remove the folder if it holds nothing worth keeping.",
        explanation: r#"
`balise new` creates the project in a new folder, or an empty one, so that it never
overwrites files.
"#,
    }
    CLI0009 {
        title: "Needs a terminal",
        message: "`balise {command}` must be run by the user, in a terminal",
        fix: "Ask the user to run `{example}` in their terminal.",
        explanation: r#"
Some commands decide what an agent is allowed to do on its own, such as the policy for
sensitive decisions. An agent must not run them for itself, so they need a terminal, and
refuse when they run in an agent's shell or in CI. An agent that needs them asks the user.
"#,
    }
    CLI0010 {
        title: "Not in a Balise project",
        message: "`balise {command}` runs inside a Balise project, and none was found from {path}",
        fix: "Run it from the project's folder, the one with `Cargo.toml` and `balise.toml`, or one of its subfolders.",
        explanation: r#"
Most commands work on a project: Balise looks for it from the current folder upwards,
through `Cargo.toml` and `balise.toml`. Only `balise new`, `balise explain` and
`balise help` work anywhere.
"#,
    }
    CLI0011 {
        title: "Cannot get the CLI of this version",
        message: "Cannot get the Balise CLI {version}: {cause}",
        fix: "Check that Rust is installed (`cargo --version`), that the network is reachable, and that Balise {version} is published on crates.io, then run the command again.",
        explanation: r#"
`balise` runs the CLI of the project's Balise version, read in `Cargo.lock`. The first time
a version is needed, it installs it with `cargo install balise-cli`, into
`~/.balise/versions/<version>/`; every later command reuses it. Installing needs Rust and
the network once per version.
"#,
    }
    CLI0012 {
        title: "Cannot create the project",
        message: "Cannot write {path}: {cause}",
        fix: "Check that the folder can be written, and that the disk is not full.",
        explanation: r#"
`balise new` writes the project's files in its folder. Writing failed: no permission, a
read-only disk, or a full one. Nothing is left half written: remove the folder before
trying again.
"#,
    }
    CLI0013 {
        title: "Nothing to compile with",
        message: "`balise {command}` needs Rust or Docker, and neither is installed",
        fix: "Install Rust (https://rustup.rs) to compile on this machine, or Docker (https://docs.docker.com/get-docker/) to compile in the dev container.",
        explanation: r#"
Commands that compile the project run cargo on this machine when Rust is installed, and in
the project's dev container otherwise, where the toolchain of `rust-toolchain.toml` is
installed. Without either, nothing can compile it.
"#,
    }
}
