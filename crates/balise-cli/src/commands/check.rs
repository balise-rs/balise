//! `balise check`: compiles the project and reports its problems as Balise errors. Runs on
//! this machine with its Rust, or in the dev container without it.

use balise_core::error_codes::CLI0013;
use balise_core::json::Value;

use crate::args::{Opt, Parsed};
use crate::check::{self, Checked, render};
use crate::dev::docker::Compose;
use crate::style::Painter;
use crate::{Command, Context, Failure, Report, project};

pub const COMMAND: Command = Command {
    name: "check",
    summary: "Compile and lint the project: each problem with its code, where to fix it and how.",
    arguments: "",
    options: &[Opt::flag(
        "--ci",
        "also check formatting and the release build, and fail on warnings",
    )],
    exclusive: &[],
    run,
};

fn run(parsed: &Parsed, context: &mut Context) -> Result<Report, Failure> {
    let root = project::require(&context.cwd, "check")?;
    let ci = parsed.has("--ci");
    let p = Painter(context.color);
    let (checked, note) = if context.machine.installed("cargo") {
        if context.interactive {
            let name = project::package_name(&root).unwrap_or_default();
            let _ = writeln!(context.output, "{}", p.dim(&format!("Checking {name}…")));
        }
        (check::run(&root, ci)?, None)
    } else if !context.machine.installed("docker") {
        return Err(Failure::problem(CLI0013, &[("command", "check")], &[]));
    } else {
        let compose = Compose::new(&root, context.machine, false)?;
        let running = compose.running(context.machine)?;
        let note = match running {
            true => {
                "No Rust on this machine: checked in the running dev container, which is slower. Installing Rust (https://rustup.rs) makes checks faster."
            }
            false => {
                "No Rust on this machine: checked in a temporary dev container, which is slower. Installing Rust (https://rustup.rs), or keeping `balise dev` running, makes checks faster."
            }
        };
        if context.interactive {
            let _ = writeln!(context.output, "{}", p.dim(note));
        }
        (compose.check(running, ci)?, Some(note))
    };
    Ok(report(&checked, note, ci, p))
}

fn report(checked: &Checked, note: Option<&str>, ci: bool, p: Painter) -> Report {
    let mut parts: Vec<String> = checked
        .diagnostics
        .iter()
        .map(|d| render::diagnostic(d, p))
        .collect();
    if let Some(note) = note {
        parts.push(format!("note: {note}"));
    }
    parts.push(render::summary(checked.errors(), checked.warnings(), ci, p));
    let mut data = checked.to_value();
    if let (Value::Object(fields), Some(note)) = (&mut data, note) {
        fields.push(("note".into(), Value::String(note.into())));
    }
    Report {
        text: parts.join("\n\n"),
        data,
        exit: u8::from(checked.failed(ci)),
    }
}
