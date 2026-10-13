//! The Balise CLI of one version, run through the `balise` launcher.
//!
//! Each command is declared once in [`COMMANDS`]: its options, and a function that returns
//! data without printing anything. The terminal shows the data as compact text, `--json` as
//! JSON; the MCP server will serve the same commands as tools.

pub mod args;
mod ask;
mod commands;
pub mod dev;
mod project;
pub mod style;
mod template;
mod tools;

use std::io::{BufRead, IsTerminal, Write};
use std::path::PathBuf;

use balise_core::error_codes::{CLI0001, CLI0003, ErrorCode};
use balise_core::json::{self, Value};
use balise_core::text::closest;

use args::{GLOBAL, Opt, Parsed};
pub use tools::{Machine, System};

/// One command of the CLI.
pub struct Command {
    pub name: &'static str,
    pub summary: &'static str,
    /// Positional arguments, as help shows them: `<name>`.
    pub arguments: &'static str,
    pub options: &'static [Opt],
    /// Groups of options of which at most one may be given.
    pub exclusive: &'static [&'static [&'static str]],
    pub run: fn(&Parsed, &mut Context) -> Result<Report, Failure>,
}

/// Every command, in the order help lists them.
pub const COMMANDS: &[&Command] = &[
    &commands::new::COMMAND,
    &commands::dev::COMMAND,
    &commands::explain::COMMAND,
    &commands::policy::COMMAND,
];

/// What a command works with: where it runs, whether it may ask questions, and the machine.
pub struct Context<'a> {
    pub cwd: PathBuf,
    /// Whether a person can answer questions: a terminal, no agent, no `--no-input`.
    pub interactive: bool,
    pub input: &'a mut dyn BufRead,
    /// Where questions and progress go: the terminal, never the result.
    pub output: &'a mut dyn Write,
    pub machine: &'a dyn Machine,
}

/// What a command returns: the text a person or an agent reads, and the same as data.
pub struct Report {
    pub text: String,
    pub data: Value,
}

/// Why a command failed, with its code and the exit code: `2` for a wrong usage, `1`
/// otherwise.
#[derive(Debug)]
pub struct Failure {
    pub code: &'static ErrorCode,
    pub message: String,
    pub fix: String,
    pub exit: u8,
}

impl Failure {
    /// A wrong usage of the command.
    pub fn usage(code: &'static ErrorCode, args: &[(&str, &str)], fix: &[(&str, &str)]) -> Failure {
        Failure {
            code,
            message: code.format_message(args),
            fix: code.format_fix(fix),
            exit: 2,
        }
    }

    /// A problem met while running.
    pub fn problem(
        code: &'static ErrorCode,
        args: &[(&str, &str)],
        fix: &[(&str, &str)],
    ) -> Failure {
        Failure {
            exit: 1,
            ..Failure::usage(code, args, fix)
        }
    }

    /// As the terminal shows it.
    pub fn report(&self) -> String {
        format!(
            "{}: {}\n  fix: {}\n  more: balise explain {}",
            self.code.code(),
            self.message,
            self.fix,
            self.code.code()
        )
    }

    fn json(&self) -> Value {
        Value::Object(vec![(
            "error".into(),
            Value::Object(vec![
                ("code".into(), Value::String(self.code.code().into())),
                ("message".into(), Value::String(self.message.clone())),
                ("fix".into(), Value::String(self.fix.clone())),
            ]),
        )])
    }
}

/// Whether a person is at the keyboard: standard input and output are a terminal, and the
/// environment shows neither an agent nor CI.
pub fn person_at_terminal() -> bool {
    const AGENTS: &[&str] = &["CLAUDECODE", "GEMINI_CLI", "CI"];
    std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal()
        && AGENTS.iter().all(|name| std::env::var_os(name).is_none())
}

/// Runs the command `args` names. Returns what to print on standard output and the exit
/// code.
pub fn run(args: &[String], context: &mut Context) -> (String, u8) {
    let json = args.iter().any(|arg| arg == "--json");
    let result = dispatch(args, context);
    match (result, json) {
        (Ok(report), false) => (report.text, 0),
        (Ok(report), true) => (to_json(&report.data), 0),
        (Err(failure), false) => (failure.report(), failure.exit),
        (Err(failure), true) => (to_json(&failure.json()), failure.exit),
    }
}

fn dispatch(args: &[String], context: &mut Context) -> Result<Report, Failure> {
    let (name, rest) = match args.split_first() {
        None => return Ok(help(None)),
        Some((name, rest)) => (name.as_str(), rest),
    };
    match name {
        "help" | "--help" | "-h" => return Ok(help(rest.first().map(String::as_str))),
        "--version" | "-V" => {
            return Ok(text(format!("balise {}", env!("CARGO_PKG_VERSION"))));
        }
        _ => {}
    }
    let Some(command) = COMMANDS.iter().find(|command| command.name == name) else {
        let names: Vec<&str> = COMMANDS.iter().map(|command| command.name).collect();
        let suggestion = closest(name, &names)
            .map(|name| format!("; did you mean `{name}`?"))
            .unwrap_or_default();
        return Err(Failure::usage(
            CLI0001,
            &[("command", name), ("suggestion", &suggestion)],
            &[],
        ));
    };
    let parsed = args::parse(command.name, rest, command.options, command.exclusive)?;
    if parsed.has("--help") {
        return Ok(help(Some(command.name)));
    }
    if parsed.has("--no-input") {
        context.interactive = false;
    }
    (command.run)(&parsed, context)
}

/// A report that is only text.
pub(crate) fn text(text: String) -> Report {
    Report {
        data: Value::String(text.clone()),
        text,
    }
}

/// `balise help`, or `balise help <command>`.
fn help(command: Option<&str>) -> Report {
    let mut out = String::new();
    match command.and_then(|name| COMMANDS.iter().find(|c| c.name == name)) {
        Some(command) => {
            out.push_str(&format!(
                "balise {} {}\n\n{}\n",
                command.name, command.arguments, command.summary
            ));
            out.push_str("\nOptions:\n");
            for opt in command.options.iter().chain(GLOBAL) {
                let mut line = format!("  {:<26} {}", args::usage(opt), opt.help);
                if !opt.values.is_empty() {
                    line.push_str(&format!(" ({})", opt.values.join(", ")));
                }
                out.push_str(&line);
                out.push('\n');
            }
        }
        None => {
            out.push_str(&format!(
                "balise {}: the Balise command-line tool\n\nCommands:\n",
                env!("CARGO_PKG_VERSION")
            ));
            for command in COMMANDS {
                out.push_str(&format!("  {:<10} {}\n", command.name, command.summary));
            }
            out.push_str("\nRun `balise help <command>` for its options.\n");
        }
    }
    text(out.trim_end().to_string())
}

/// A required argument is missing.
pub(crate) fn missing(command: &str, missing: &str, example: &str) -> Failure {
    Failure::usage(
        CLI0003,
        &[("command", command), ("missing", missing)],
        &[("example", example)],
    )
}

fn to_json(value: &Value) -> String {
    let mut out = String::new();
    json::write_value(&mut out, value);
    out
}
