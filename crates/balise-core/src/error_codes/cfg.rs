//! `CFG`: configuration, from `balise.toml` and environment variables.

codes! {
    CFG0001 {
        title: "Unknown setting",
        message: "Unknown setting `{key}` ({source}){suggestion}",
        fix: "Fix the name, using the suggestion if there is one, or remove the setting. `balise explain config` lists every setting.",
        explanation: r#"
A key of `balise.toml`, or an environment variable starting with `BALISE_`, names no
setting. It is most likely a typo, and the setting it was meant for keeps its default. The
message suggests the closest setting.

In dev and test, the app does not start. In production, the key is ignored with a warning,
so that a leftover key does not take a service down; `config check` treats it as an error.
Settings of a module (`database` for `orm`…) exist only when the module is enabled.
"#,
    }
    CFG0002 {
        title: "Invalid setting value",
        message: "`{key}` is {value} ({source}); expected {expected}",
        fix: "Write a value of the expected form, or remove it to use the default.",
        explanation: r#"
A setting has a value it cannot use. The message says where the value comes from: a line
of `balise.toml` (and its `[env.…]` section), or an environment variable, which takes
precedence.

In dev and test, the app does not start. In production, a setting with a safe default (log
level, limits…) keeps its default with a warning; a setting that affects security or data
stops the startup. `config check` treats both as errors. A secret's value is never shown.
"#,
    }
    CFG0003 {
        title: "Unknown environment",
        message: "Unknown environment `{name}` ({source}){suggestion}",
        fix: "Use `dev`, `test` or `prod`, in `BALISE_ENV` and in `[env.<name>]` sections.",
        explanation: r#"
Balise has three environments: `dev`, `test` and `prod`. `BALISE_ENV` selects one; without
it, the environment is the one the binary was built for. A section `[env.<name>]` of
`balise.toml` applies only in its environment, so a section with another name would be
ignored silently: it is reported instead.
"#,
    }
    CFG0004 {
        title: "Dev build in production",
        message: "This binary is built with dev tooling and cannot run with BALISE_ENV=prod",
        fix: "Build for production without the `dev` feature: `balise build`.",
        explanation: r#"
A binary built with the `dev` feature contains the MCP server and the dev instrumentation,
which must never be reachable in production. It refuses to start in the `prod` environment,
so a dev build cannot be deployed by mistake.
"#,
    }
    CFG0005 {
        title: "Secret in balise.toml",
        message: "`{key}` is a secret and cannot be written in balise.toml ({source})",
        fix: "Remove it from balise.toml, then set `{variable}`, or `{variable}_FILE` with the path of a file that holds it.",
        explanation: r#"
`balise.toml` is committed and embedded in the binary: a secret written there would leak
with the code and the image. Secrets come from environment variables, or from a file named
by the `_FILE` variable, which matches Docker and Kubernetes secrets.
"#,
    }
    CFG0006 {
        title: "Secret file unreadable",
        message: "Cannot read the file of `{variable}` at {path}: {cause}",
        fix: "Check that the file exists and that the app can read it, or set `{secret}` directly.",
        explanation: r#"
A `_FILE` variable names the file that holds a secret, such as a Docker or Kubernetes
secret mounted in the container. The file must exist and be readable by the app's user.
A trailing newline is ignored.
"#,
    }
    CFG0007 {
        title: "Secret set twice",
        message: "`{variable}` and `{variable}_FILE` are both set",
        fix: "Set only one of them.",
        explanation: r#"
A secret comes from its variable or from the file its `_FILE` variable names, never both:
Balise cannot know which one is meant to win.
"#,
    }
    CFG0008 {
        title: "Unknown command",
        message: "Unknown command `{command}`",
        fix: "Run the app without arguments to serve, or with `config check [--env dev|test|prod]` to check its configuration.",
        explanation: r#"
The app's binary serves without arguments. Its only command is `config check`, which reads
the configuration of an environment with the strictest rules and exits: `0` when it is
valid, `1` otherwise. Run it in CI with `--env prod` before deploying.
"#,
    }
}
