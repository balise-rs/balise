//! `balise new <name>`: creates a project.
//!
//! In a terminal, it asks what its flags do not say; without one, it asks nothing, and the
//! choices without a default must be given. Each choice is declared once below, so that the
//! questions and the flags cannot disagree. Only what works in this version is offered.

use std::path::{Path, PathBuf};

use balise_core::config::{Config, Environment};
use balise_core::error_codes::{CLI0005, CLI0006, CLI0007, CLI0008, CLI0012};
use balise_core::json::Value;

use crate::args::{Opt, Parsed};
use crate::commands::policy;
use crate::style::Painter;
use crate::template::{self, Choices};
use crate::tools::{DOCKER, GIT, Tool};
use crate::{Command, Context, Failure, Report, ask, missing};

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Front-end frameworks `--front` knows, and those this version can generate.
const FRONTS: &[&str] = &["react", "vue", "svelte"];
const AVAILABLE_FRONTS: &[&str] = &[];

/// Names a project cannot take: crates every project already has.
const RESERVED: &[&str] = &[
    "std",
    "core",
    "alloc",
    "proc_macro",
    "test",
    "self",
    "crate",
    "super",
    "balise",
    "build",
];

pub const COMMAND: Command = Command {
    name: "new",
    summary: "Create a project. In a terminal, it asks what its options do not say.",
    arguments: "<name>",
    options: &[
        Opt::value("--front", "framework", FRONTS, "the front end's framework"),
        Opt::flag("--no-front", "no front end: an API alone"),
        Opt::flag("--no-orm", "without the database module"),
        Opt::flag("--no-auth", "without the authentication module"),
        Opt::flag("--no-mail", "without the mail module"),
        Opt::flag("--minimal", "without any optional module"),
        Opt::value(
            "--db",
            "database",
            &["postgres", "sqlite"],
            "the database (default: postgres)",
        ),
        Opt::flag("--no-example", "without the example resource"),
        Opt::value(
            "--balise-path",
            "path",
            &[],
            "use a local checkout of Balise, to work on Balise itself",
        ),
    ],
    for_agents: false,
    exclusive: &[&["--front", "--no-front"]],
    run,
};

/// What `balise new` decided, from flags, answers and defaults.
struct Plan {
    name: String,
    front: Option<String>,
    example: bool,
    policy: String,
    /// `--balise-path`, kept in the equivalent command.
    balise_path: Option<String>,
    /// Defaults applied without a question: `(choice, value, how to change it)`.
    defaults: Vec<(&'static str, String, String)>,
}

fn run(parsed: &Parsed, context: &mut Context) -> Result<Report, Failure> {
    let plan = decide(parsed, context)?;
    let root = context.cwd.join(&plan.name);
    if root.exists() && std::fs::read_dir(&root).map_or(true, |mut dir| dir.next().is_some()) {
        return Err(Failure::problem(
            CLI0008,
            &[("path", &root.display().to_string())],
            &[],
        ));
    }
    let p = Painter(context.color);
    if context.interactive {
        let _ = writeln!(context.output, "\n{}", summary(&plan, p));
        if !ask::confirm(context, "\nCreate the project?", true) {
            return Ok(crate::text("Nothing was created.".into()));
        }
    }
    let source = source(parsed.value("--balise-path"));
    write_files(&root, &plan, &source)?;
    policy::write(&root, &plan.policy)?;
    let mut notes = Vec::new();
    // The lockfile pins third-party crates; cargo adds Balise's own and drops what is unused.
    if let Err(cause) = context
        .machine
        .run("cargo", &["metadata", "--format-version", "1"], &root)
    {
        notes.push(format!(
            "Cargo.lock is completed at the first build ({cause})."
        ));
    }
    let git = setup_git(context, &root, &mut notes);
    let docker = match (
        context.machine.installed(DOCKER.program),
        context.machine.docker_running(),
    ) {
        (false, _) => "missing",
        (true, false) => "stopped",
        (true, true) => "running",
    };
    let mut missing_tools = Vec::new();
    if git == "missing" {
        missing_tools.push(&GIT);
    }
    if docker == "missing" {
        missing_tools.push(&DOCKER);
    }
    let prompts = write_prompts(context, &root, &missing_tools)?;
    Ok(report(&plan, &root, git, docker, &prompts, &notes, context))
}

/// Settles every choice: flags first, then a question in a terminal, then a default or an
/// error without one.
fn decide(parsed: &Parsed, context: &mut Context) -> Result<Plan, Failure> {
    if context.interactive {
        let p = Painter(context.color);
        let _ = writeln!(
            context.output,
            "{} {} {}",
            p.accent("Balise"),
            p.dim(VERSION),
            p.bold("· a new project")
        );
    }
    let name = match parsed.positionals.first() {
        Some(name) => name.clone(),
        None if context.interactive => match ask::line(context, "Project name") {
            Some(name) => name,
            None => {
                return Err(missing(
                    "new",
                    "a project name",
                    "balise new shop --no-front",
                ));
            }
        },
        None => {
            return Err(missing(
                "new",
                "a project name",
                "balise new shop --no-front",
            ));
        }
    };
    check_name(&name)?;

    // Optional modules and databases come with their parts of Balise: refusing a module is
    // already what this version does, choosing a database is not possible yet.
    if let Some(db) = parsed.value("--db") {
        if parsed.has("--no-orm") || parsed.has("--minimal") {
            let other = if parsed.has("--no-orm") {
                "--no-orm"
            } else {
                "--minimal"
            };
            return Err(Failure::usage(
                CLI0005,
                &[("first", "--db"), ("second", other)],
                &[],
            ));
        }
        let what = format!("The database `{db}`");
        return Err(unavailable(
            &what,
            "the database comes with the `orm` module",
        ));
    }

    let front = match (parsed.value("--front"), parsed.has("--no-front")) {
        (Some(front), _) if !AVAILABLE_FRONTS.contains(&front) => {
            let what = format!("The `{front}` front end");
            return Err(unavailable(&what, "use `--no-front`"));
        }
        (Some(front), _) => Some(front.to_string()),
        (None, true) => None,
        (None, false) if context.interactive => {
            let mut options: Vec<(&str, &str)> =
                AVAILABLE_FRONTS.iter().map(|front| (*front, "")).collect();
            options.push(("none", "an API alone"));
            // A question with one answer is not asked.
            match options.len() {
                1 => None,
                _ => {
                    let i = ask::choose(context, "Front end?", &options, 0);
                    AVAILABLE_FRONTS.get(i).map(|front| front.to_string())
                }
            }
        }
        (None, false) => {
            let missing_what = "a front end: `--front <framework>` or `--no-front`";
            let example = format!("balise new {name} --no-front");
            return Err(missing("new", missing_what, &example));
        }
    };

    let mut defaults = Vec::new();
    let example = match parsed.has("--no-example") {
        true => false,
        false if context.interactive => ask::confirm(
            context,
            "\nAdd the example resource, `orders`, to start from?",
            true,
        ),
        false => {
            defaults.push((
                "example",
                "orders".to_string(),
                "remove it later: balise remove resource orders".to_string(),
            ));
            true
        }
    };

    // The policy says what the agent may decide alone: only a person chooses it.
    let policy = match context.interactive {
        true => {
            let i = ask::choose(
                context,
                "Who takes sensitive decisions (raising a budget, reading every user's rows…)?",
                policy::PRESETS,
                0,
            );
            let preset = policy::PRESETS[i].0;
            if preset == "full-auto" {
                let _ = writeln!(context.output, "{}", policy::FULL_AUTO_WARNING);
            }
            preset.to_string()
        }
        false => "strict".to_string(),
    };

    Ok(Plan {
        name,
        front,
        example,
        policy,
        balise_path: parsed.value("--balise-path").map(str::to_string),
        defaults,
    })
}

fn unavailable(what: &str, instead: &str) -> Failure {
    Failure::usage(
        CLI0006,
        &[("what", what), ("version", VERSION)],
        &[("instead", instead)],
    )
}

/// A crate name: lowercase letters, digits, `-` and `_`, from a letter.
fn check_name(name: &str) -> Result<(), Failure> {
    let reason = if !name.starts_with(|c: char| c.is_ascii_lowercase()) {
        Some("it must start with a lowercase letter")
    } else if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        Some("it can only hold lowercase letters, digits, `-` and `_`")
    } else if RESERVED.contains(&name.replace('-', "_").as_str()) {
        Some("every project already has a crate of that name")
    } else {
        None
    };
    match reason {
        Some(reason) => Err(Failure::usage(
            CLI0007,
            &[("name", name), ("reason", reason)],
            &[],
        )),
        None => Ok(()),
    }
}

/// Where Balise comes from: this version, or a local checkout given as its root or as the
/// facade's folder.
struct Source {
    /// What `[dependencies]` says for Balise.
    dependency: String,
    /// The checkout's root.
    checkout: Option<String>,
}

fn source(path: Option<&str>) -> Source {
    let Some(path) = path else {
        return Source {
            dependency: format!("\"={VERSION}\""),
            checkout: None,
        };
    };
    let path = PathBuf::from(path);
    let path = std::fs::canonicalize(&path).unwrap_or(path);
    let (checkout, facade) = match path.join("crates/balise/Cargo.toml").is_file() {
        true => (path.clone(), path.join("crates/balise")),
        false => (path.join("../.."), path),
    };
    let checkout = std::fs::canonicalize(&checkout).unwrap_or(checkout);
    let text = |path: &Path| path.display().to_string().replace('\\', "/");
    Source {
        dependency: format!("{{ path = \"{}\" }}", text(&facade)),
        checkout: Some(text(&checkout)),
    }
}

fn write_files(root: &Path, plan: &Plan, source: &Source) -> Result<(), Failure> {
    let choices = Choices {
        name: &plan.name,
        dependency: &source.dependency,
        checkout: source.checkout.as_deref(),
        example: plan.example,
    };
    for (path, content) in template::render(&choices) {
        let path = root.join(path);
        let written = match path.parent() {
            Some(parent) => std::fs::create_dir_all(parent),
            None => Ok(()),
        }
        .and_then(|_| std::fs::write(&path, content));
        written.map_err(|error| cannot_write(&path, &error))?;
    }
    Ok(())
}

fn cannot_write(path: &Path, error: &std::io::Error) -> Failure {
    Failure::problem(
        CLI0012,
        &[
            ("path", &path.display().to_string()),
            ("cause", &error.to_string()),
        ],
        &[],
    )
}

/// Initializes the repository, without committing: what goes in the first commit is the
/// user's call. `initialized`, or `missing` without git.
fn setup_git(context: &mut Context, root: &Path, notes: &mut Vec<String>) -> &'static str {
    if !context.machine.installed(GIT.program) {
        return "missing";
    }
    match context.machine.run("git", &["init", "--quiet"], root) {
        Ok(()) => "initialized",
        Err(cause) => {
            notes.push(format!("git init failed: {cause}"));
            "missing"
        }
    }
}

/// Saves the two prompts of each missing tool in `.balise/prompts/`.
fn write_prompts(context: &Context, root: &Path, tools: &[&Tool]) -> Result<Vec<String>, Failure> {
    let os = context.machine.os();
    let mut written = Vec::new();
    for tool in tools {
        for (kind, text) in [
            ("install", tool.install_prompt(&os)),
            ("learn", tool.learn_prompt()),
        ] {
            let relative = format!(".balise/prompts/{kind}-{}.md", tool.program);
            let path = root.join(&relative);
            let result = std::fs::create_dir_all(root.join(".balise/prompts"))
                .and_then(|_| std::fs::write(&path, text));
            result.map_err(|error| cannot_write(&path, &error))?;
            written.push(relative);
        }
    }
    Ok(written)
}

/// The choices, before creating: what the user confirms.
fn summary(plan: &Plan, p: Painter) -> String {
    [
        ("Project", plan.name.as_str()),
        ("Front end", plan.front.as_deref().unwrap_or("none")),
        ("Example", if plan.example { "orders" } else { "none" }),
        ("Policy", plan.policy.as_str()),
    ]
    .iter()
    .map(|(label, value)| format!("  {} {}", p.dim(&format!("{label:<9}")), p.bold(value)))
    .collect::<Vec<_>>()
    .join("\n")
}

/// The same project, with flags only.
fn equivalent_command(plan: &Plan) -> String {
    let mut command = format!("balise new {}", plan.name);
    match &plan.front {
        Some(front) => command.push_str(&format!(" --front {front}")),
        None => command.push_str(" --no-front"),
    }
    if !plan.example {
        command.push_str(" --no-example");
    }
    if let Some(path) = &plan.balise_path {
        command.push_str(&format!(" --balise-path {path}"));
    }
    command
}

fn report(
    plan: &Plan,
    root: &Path,
    git: &str,
    docker: &str,
    prompts: &[String],
    notes: &[String],
    context: &Context,
) -> Report {
    let interactive = context.interactive;
    let p = Painter(context.color);
    // A person gets a mark of success, and a farewell; an agent the facts alone.
    let mut text = match interactive {
        true => format!(
            "\n{} {}\n",
            p.success(&format!("✔ Created {}/", plan.name)),
            p.dim(&format!("(Balise {VERSION})"))
        ),
        false => format!("Created {}/ (Balise {VERSION})\n", plan.name),
    };
    if !plan.defaults.is_empty() {
        text.push_str(&format!("\n{}\n", p.bold("Defaults applied:")));
        for (choice, value, change) in &plan.defaults {
            let value = p.bold(&format!("{value:<9}"));
            text.push_str(&format!("  {choice:<9} {value} {}\n", p.dim(change)));
        }
        text.push_str(&format!(
            "\n{}\n  {}\n",
            p.bold("Recreate with other choices (nothing to lose yet):"),
            p.command(&format!(
                "rm -r {} && {}",
                plan.name,
                equivalent_command(plan)
            ))
        ));
    }
    if interactive {
        text.push_str(&format!(
            "\n{}\n  {}\n",
            p.bold("The same project without questions:"),
            p.command(&equivalent_command(plan))
        ));
    }
    if plan.policy == "strict" && !interactive {
        text.push_str(&format!(
            "\nPolicy: strict (every sensitive decision waits for the user).\n\
             {}\n  {}   {}\n",
            p.bold("Ask the user to choose how much you decide alone, in their terminal:"),
            p.command("balise policy preset autonomous"),
            p.dim("(recommended)")
        ));
    }
    let missing: Vec<&str> = [(git, "git"), (docker, "Docker")]
        .into_iter()
        .filter(|(state, _)| *state == "missing")
        .map(|(_, name)| name)
        .collect();
    if !missing.is_empty() {
        let warning = format!(
            "{} not installed. The project is created; to install and learn {}, give these prompts to your agent:",
            missing.join(" and "),
            if missing.len() == 1 { "it" } else { "them" }
        );
        text.push_str(&format!("\n{}\n", p.warn(&warning)));
        for prompt in prompts {
            text.push_str(&format!(
                "  {}\n",
                p.command(&format!("{}/{prompt}", plan.name))
            ));
        }
    }
    if docker == "stopped" {
        let warning =
            "Docker is installed but not running: start it before running the app in Docker.";
        text.push_str(&format!("\n{}\n", p.warn(warning)));
    }
    for note in notes {
        text.push_str(&format!("\n{note}\n"));
    }
    let config = Config::defaults(Environment::Dev);
    let url = format!("http://localhost:{}{}", config.port, config.api_base());
    text.push_str(&format!(
        "\n{}\n  {}\n  {}     {}\n",
        p.bold("Next:"),
        p.command(&format!("cd {}", plan.name)),
        p.command("balise dev"),
        p.dim(&format!("# the API on {url}"))
    ));
    if interactive {
        text.push_str(&format!("\n{}\n", p.accent("Happy building!")));
    }

    let defaults = plan
        .defaults
        .iter()
        .map(|(choice, value, change)| {
            Value::Object(vec![
                ("choice".into(), Value::String((*choice).into())),
                ("value".into(), Value::String(value.clone())),
                ("change".into(), Value::String(change.clone())),
            ])
        })
        .collect();
    let data = Value::Object(vec![
        ("path".into(), Value::String(root.display().to_string())),
        ("balise".into(), Value::String(VERSION.into())),
        (
            "front".into(),
            plan.front.clone().map_or(Value::Null, Value::String),
        ),
        ("example".into(), Value::Bool(plan.example)),
        ("policy".into(), Value::String(plan.policy.clone())),
        ("defaults".into(), Value::Array(defaults)),
        ("git".into(), Value::String(git.into())),
        ("docker".into(), Value::String(docker.into())),
        (
            "prompts".into(),
            Value::Array(prompts.iter().cloned().map(Value::String).collect()),
        ),
        ("command".into(), Value::String(equivalent_command(plan))),
    ]);
    Report {
        text: text.trim_end().to_string(),
        data,
        exit: 0,
    }
}
