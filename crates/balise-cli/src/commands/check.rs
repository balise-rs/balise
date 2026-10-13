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
    options: &[
        Opt::flag(
            "--ci",
            "also check formatting and the release build, and fail on warnings",
        ),
        Opt::flag(
            "--hook",
            "run by an agent's hook at the end of a task: reads the hook's JSON, blocks on what is left",
        ),
    ],
    for_agents: true,
    exclusive: &[&["--ci", "--hook"]],
    run,
};

/// At most this many problems in what a hook says: the agent runs `balise check` for all.
const HOOK_SHOWN: usize = 10;

fn run(parsed: &Parsed, context: &mut Context) -> Result<Report, Failure> {
    if parsed.has("--hook") && !hook_should_check(context) {
        return Ok(silent(0));
    }
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
        let note = crate::dev::docker::note(running);
        if context.interactive {
            let _ = writeln!(context.output, "{}", p.dim(note));
        }
        (compose.check(running, ci)?, Some(note))
    };
    if parsed.has("--hook") {
        return Ok(hook(&checked, context));
    }
    Ok(report(&checked, note, ci, p))
}

/// Whether the hook's event asks for a check: the end of a task, once. Claude Code says
/// `stop_hook_active` when the agent continues because this hook blocked: it is let finish
/// then, so that it never loops on what it cannot fix.
fn hook_should_check(context: &mut Context) -> bool {
    let mut input = String::new();
    let _ = context.input.read_to_string(&mut input);
    let Ok(event) = balise_core::json::parse(input.as_bytes()) else {
        return true;
    };
    let name = crate::value::text(&event, "hook_event_name").unwrap_or("Stop");
    let again = matches!(
        crate::value::get(&event, "stop_hook_active"),
        Some(Value::Bool(true))
    );
    matches!(name, "Stop" | "SubagentStop") && !again
}

/// What the agent reads on standard error, with exit code `2` so that its hook blocks: the
/// problems in short, without the code around them.
fn hook(checked: &Checked, context: &mut Context) -> Report {
    if checked.diagnostics.is_empty() {
        return silent(0);
    }
    let count = render::summary(checked.errors(), checked.warnings(), true, Painter(false));
    let count = count
        .trim_start_matches("✗ ")
        .replace(" (`--ci` fails on warnings)", "");
    let mut out = format!("balise check: {count} left. Fix them before finishing:\n");
    for d in checked.diagnostics.iter().take(HOOK_SHOWN) {
        out.push_str(&format!("\n{} {}", d.severity.name(), d.code));
        if let Some(file) = &d.file {
            out.push_str(&format!(" at {file}:{}", d.line));
            if let Some(function) = &d.function {
                out.push_str(&format!(" in {function}"));
            }
        }
        out.push_str(&format!(": {}\n", d.message));
        if let Some(fix) = &d.fix {
            out.push_str(&format!("  fix: {fix}\n"));
        }
    }
    let more = checked.diagnostics.len().saturating_sub(HOOK_SHOWN);
    if more > 0 {
        out.push_str(&format!("\n…and {more} more: run `balise check`.\n"));
    }
    let _ = write!(context.output, "{out}");
    let _ = context.output.flush();
    silent(2)
}

/// A result with nothing to print: a hook speaks on standard error, or not at all.
fn silent(exit: u8) -> Report {
    Report {
        text: String::new(),
        data: Value::Null,
        exit,
    }
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
