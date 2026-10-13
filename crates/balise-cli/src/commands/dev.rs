//! `balise dev`: the dev environment, with rebuild on change. In Docker by default, on this
//! machine with `--native`; `status` and `stop` for the container started with `--detach`.

use std::io::IsTerminal;

use balise_core::error_codes::{CLI0004, CLI0005};
use balise_core::json::Value;

use crate::args::{Opt, Parsed};
use crate::dev::docker::Compose;
use crate::{Command, Context, Failure, Report, dev, project, style};

pub const COMMAND: Command = Command {
    name: "dev",
    summary: "Run the app in dev: rebuilt and restarted at each change, one stream of logs. `balise dev status` and `balise dev stop` for a detached one.",
    arguments: "[status|stop]",
    options: &[
        Opt::flag(
            "--detach",
            "run in the background; `balise dev stop` stops it",
        ),
        Opt::flag(
            "--volumes",
            "with `stop`: also remove the volumes of `target/` and the registry; the next start compiles everything",
        ),
        Opt::flag(
            "--native",
            "build and run on this machine, with its Rust, without Docker",
        ),
        Opt::flag(
            "--inside",
            "the supervisor inside the dev container; run by the container",
        ),
    ],
    for_agents: true,
    exclusive: &[&["--native", "--inside", "--detach"]],
    run,
};

const ACTIONS: &[&str] = &["status", "stop"];

fn run(parsed: &Parsed, context: &mut Context) -> Result<Report, Failure> {
    let action = parsed.positionals.first().map(String::as_str);
    if let Some(action) = action {
        if !ACTIONS.contains(&action) {
            return Err(Failure::usage(
                CLI0004,
                &[
                    ("value", action),
                    ("option", "dev"),
                    ("expected", "`status`, `stop`, or nothing"),
                ],
                &[],
            ));
        }
        if let Some(flag) = ["--native", "--inside", "--detach"]
            .into_iter()
            .find(|flag| parsed.has(flag))
        {
            return Err(Failure::usage(
                CLI0005,
                &[("first", action), ("second", flag)],
                &[],
            ));
        }
    }
    if parsed.has("--volumes") && action != Some("stop") {
        return Err(crate::missing(
            "dev --volumes",
            "`stop`",
            "balise dev stop --volumes",
        ));
    }
    let root = project::require(&context.cwd, "dev")?;
    let color = style::wanted(std::io::stdout().is_terminal());

    if parsed.has("--native") || parsed.has("--inside") {
        dev::catch_signals();
        let mut out = std::io::stdout();
        dev::supervise(&root, &mut out, color, &dev::stop_requested)?;
        return Ok(crate::text("balise dev stopped.".into()));
    }

    let compose = Compose::new(&root, context.machine, color)?;
    match action {
        Some("status") => {
            let running = compose.running(context.machine)?;
            let answers = running && compose.answers();
            let text = match (running, answers) {
                (false, _) => "The app is stopped. Start it with `balise dev`.".to_string(),
                (true, true) => format!("The app runs and answers on {}", compose.url()),
                (true, false) => "The app's container runs, but the app does not answer yet: it is building, or its last build failed. `balise dev` shows its logs.".to_string(),
            };
            Ok(Report {
                text,
                data: Value::Object(vec![
                    ("running".into(), Value::Bool(running)),
                    ("answers".into(), Value::Bool(answers)),
                    ("url".into(), Value::String(compose.url())),
                ]),
                exit: 0,
            })
        }
        Some(_) => {
            let volumes = parsed.has("--volumes");
            compose.down(volumes)?;
            Ok(crate::text(match volumes {
                true => "The app is stopped, and its volumes removed: the next `balise dev` compiles everything.".into(),
                false => "The app is stopped.".into(),
            }))
        }
        None if parsed.has("--detach") => {
            compose.up(context.machine, true)?;
            Ok(Report {
                text: format!(
                    "The app runs in the background, on {}. It builds first: `balise dev status` says when it answers; `balise dev stop` stops it.",
                    compose.url()
                ),
                data: Value::Object(vec![("url".into(), Value::String(compose.url()))]),
                exit: 0,
            })
        }
        None => {
            compose.up(context.machine, false)?;
            Ok(crate::text("balise dev stopped.".into()))
        }
    }
}
