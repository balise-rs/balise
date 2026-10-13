//! Commands answered by the app itself, built with `dev`, so that the answer is always the one
//! of the code as it is: `./app routes`, `./app why GET /orders/5`, `./app config check`.
//! Built and run on this machine with its Rust, or in the dev container without it.

use std::path::Path;
use std::process::{Command, Output};

use balise_core::error_codes::{CLI0013, DEV0009, DEV0010};
use balise_core::json::Value;

use crate::dev::docker::{self, Compose};
use crate::{Context, Failure, project};

/// Exit code of the container's script when the app does not build.
const DOES_NOT_BUILD: i32 = 86;

/// What the app answered.
pub struct Answer {
    pub stdout: String,
    pub exit: i32,
    /// Set when it ran in the dev container: why it was slower.
    pub note: Option<&'static str>,
}

impl Answer {
    /// The answer as JSON, or the error it reports, which is the command's.
    pub fn json(&self, command: &str) -> Result<Value, Failure> {
        let last = self.stdout.trim().lines().last().unwrap_or_default();
        let value = balise_core::json::parse(last.as_bytes())
            .map_err(|_| does_not_answer(command, last))?;
        match Failure::from_json(&value) {
            Some(failure) => Err(failure),
            None => Ok(value),
        }
    }
}

/// Builds the app with `dev`, then runs it with `args`, for `balise <command>`.
pub fn ask(
    context: &mut Context,
    root: &Path,
    command: &str,
    args: &[&str],
) -> Result<Answer, Failure> {
    let name = project::package_name(root).unwrap_or_default();
    if context.machine.installed("cargo") {
        let built = Command::new("cargo")
            .args(["build", "--features", "dev", "--quiet"])
            .current_dir(root)
            .output()
            .map_err(|error| does_not_answer(command, &error.to_string()))?;
        if !built.status.success() {
            return Err(Failure::problem(
                DEV0009,
                &[("command", command)],
                &[("command", command)],
            ));
        }
        let binary = project::target_dir(root)
            .join("debug")
            .join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
        let output = Command::new(&binary)
            .args(args)
            .current_dir(root)
            .output()
            .map_err(|error| does_not_answer(command, &error.to_string()))?;
        return Ok(answer(output, None));
    }
    if !context.machine.installed("docker") {
        return Err(Failure::problem(CLI0013, &[("command", command)], &[]));
    }
    let compose = Compose::new(root, context.machine, false)?;
    let running = compose.running(context.machine)?;
    let note = docker::note(running);
    if context.interactive {
        let _ = writeln!(context.output, "{note}");
    }
    let script = format!(
        r#"cargo build --features dev --quiet 2>/dev/null || exit {DOES_NOT_BUILD}; exec target/debug/{name} "$@""#
    );
    let (_, output) = compose.run_in(running, &script, args)?;
    if output.status.code() == Some(DOES_NOT_BUILD) {
        return Err(Failure::problem(
            DEV0009,
            &[("command", command)],
            &[("command", command)],
        ));
    }
    Ok(answer(output, Some(note)))
}

fn answer(output: Output, note: Option<&'static str>) -> Answer {
    Answer {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        exit: output.status.code().unwrap_or(1),
        note,
    }
}

fn does_not_answer(command: &str, cause: &str) -> Failure {
    Failure::problem(DEV0010, &[("command", command), ("cause", cause)], &[])
}
