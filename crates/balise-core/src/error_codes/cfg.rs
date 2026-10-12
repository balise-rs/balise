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
    CFG0009 {
        title: "Budget for an unknown handler",
        message: "`[\"{handler}\"]` names no handler ({source}){suggestion}",
        fix: "Use the handler's name as `balise routes` shows it (`orders::list`), or remove the entry.",
        explanation: r#"
Each section of `budgets.toml` is named after a handler: its module and function,
`["orders::list"]`. A section that names no handler would be ignored silently, typically
after a rename, so the app does not start. The message suggests the closest handler.
"#,
    }
    CFG0010 {
        title: "Invalid budget",
        message: "{problem} ({source})",
        fix: "Write `body_size = \"20MB\"` or `timeout = \"2m\"` under the handler's section; `balise explain CFG0010` lists the budgets.",
        explanation: r#"
`budgets.toml` holds a section per handler whose budgets differ from the defaults:

    ["imports::create"]
    body_size = "20MB"
    timeout = "2m"

- `body_size`: the largest request body, as `512KB`, `20MB` or `1GB`.
- `timeout`: how long the request may take, as `500ms`, `30s`, `2m` or `1h`.

Other budgets (queries, response size) come with the test tooling. An unknown key or an
invalid value stops the startup, so that a budget is never ignored silently.
"#,
    }
    CFG0011 {
        title: "Body budget below the declared files",
        message: "`body_size` of `{handler}` is {budget}, less than the {files} its `File` fields declare ({source})",
        fix: "Remove `body_size` from the entry, so that the limit follows the declared files, or lower `max_size` on the fields.",
        explanation: r#"
A route with `File` fields gets a body limit computed from their `max_size` and
`max_count`. A `body_size` in `budgets.toml` replaces it, and this one is smaller than the
files the route declares: the declared limits could never be reached. Change one or the
other, so that a single place says how large an upload can be.
"#,
    }
    CFG0012 {
        title: "Default page size above the maximum",
        message: "`pagination.per_page` is {per_page}, above `pagination.max_per_page` ({max})",
        fix: "Lower `pagination.per_page`, or raise `pagination.max_per_page`.",
        explanation: r#"
`pagination.per_page` is the page size when a client does not ask for one, and
`pagination.max_per_page` the largest a client can ask for. A default above the maximum
cannot be served. A route that needs other limits declares them on its attribute:
`#[get("/exports", page(default = 500, max = 5000))]`.
"#,
    }
}
