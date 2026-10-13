//! `balise config check [--env <env>]`: the app checks its configuration, with the strictest
//! rules, without starting.

use balise_core::json::Value;

use crate::args::{Opt, Parsed};
use crate::{Command, Context, Failure, Report, app, missing, project};

pub const COMMAND: Command = Command {
    name: "config",
    summary: "Check the configuration without starting the app: `balise config check --env prod`. Every problem is an error.",
    arguments: "check",
    options: &[Opt::value(
        "--env",
        "environment",
        &["dev", "test", "prod"],
        "the environment to check (default: dev)",
    )],
    exclusive: &[],
    run,
};

fn run(parsed: &Parsed, context: &mut Context) -> Result<Report, Failure> {
    let example = "balise config check --env prod";
    if parsed.positionals.first().map(String::as_str) != Some("check") {
        return Err(missing("config", "`check`", example));
    }
    let root = project::require(&context.cwd, "config check")?;
    let mut args = vec!["config", "check"];
    if let Some(env) = parsed.value("--env") {
        args.extend(["--env", env]);
    }
    let answer = app::ask(context, &root, "config check", &args)?;
    let mut text = answer.stdout.trim_end().to_string();
    if let Some(note) = answer.note {
        text.push_str(&format!("\n\nnote: {note}"));
    }
    let valid = answer.exit == 0;
    Ok(Report {
        data: Value::Object(vec![
            (
                "output".into(),
                Value::String(answer.stdout.trim_end().into()),
            ),
            ("valid".into(), Value::Bool(valid)),
        ]),
        text,
        exit: u8::from(!valid),
    })
}
