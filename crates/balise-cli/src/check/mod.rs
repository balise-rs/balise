//! `balise check`: compiles the project with clippy, which also applies the project's lints,
//! and shows each diagnostic as Balise shows its errors, rewritten when Balise recognizes
//! it. Errors come first, then warnings, then one summary line.
//!
//! It compiles in its own `target/check` folder, so that it never waits for the build of
//! `balise dev`.

mod diagnostic;
mod enrich;
pub mod render;
mod source;

use std::path::Path;
use std::process::Command;

use balise_core::error_codes::CMP0027;
use balise_core::json::{self, Number, Value};

use crate::{Failure, project};

pub use diagnostic::{Diagnostic, Severity, is_rustc_code};

/// What a check found.
pub struct Checked {
    pub diagnostics: Vec<Diagnostic>,
}

impl Checked {
    pub fn errors(&self) -> usize {
        self.count(Severity::Error)
    }

    pub fn warnings(&self) -> usize {
        self.count(Severity::Warning)
    }

    fn count(&self, severity: Severity) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == severity)
            .count()
    }

    /// Whether the check fails: an error, or a warning with `--ci`.
    pub fn failed(&self, ci: bool) -> bool {
        self.errors() > 0 || (ci && self.warnings() > 0)
    }

    pub fn to_value(&self) -> Value {
        let count = |n: usize| Value::Number(Number::Unsigned(n as u64));
        Value::Object(vec![
            (
                "diagnostics".into(),
                Value::Array(self.diagnostics.iter().map(Diagnostic::to_value).collect()),
            ),
            ("errors".into(), count(self.errors())),
            ("warnings".into(), count(self.warnings())),
        ])
    }

    /// Reads [`to_value`](Self::to_value)'s data back.
    pub fn from_value(value: &Value) -> Option<Checked> {
        let diagnostics = diagnostic::array(value, "diagnostics")
            .iter()
            .map(Diagnostic::from_value)
            .collect::<Option<_>>()?;
        Some(Checked { diagnostics })
    }
}

/// Checks the project at `root` with the cargo of this machine. With `ci`, also its
/// formatting and the release build.
pub fn run(root: &Path, ci: bool) -> Result<Checked, Failure> {
    let target = project::target_dir(root).join("check");
    let mut diagnostics = cargo(
        root,
        &target,
        &["clippy", "--all-targets", "--features", "dev"],
    )?;
    diagnostics.extend(source::undeclared_modules(root));
    diagnostics.extend(source::static_muts(root));
    if ci {
        diagnostics.extend(formatting(root)?);
        // Only what the release build adds: the rest was just reported.
        if diagnostics.iter().all(|d| d.severity != Severity::Error) {
            diagnostics.extend(cargo(root, &target, &["build", "--release"])?);
        }
    }
    // Each target (the binary, its tests) reports the same diagnostic; one of Balise's is
    // said once per line (`Mutex<u32> = Mutex::new(0)` holds two `Mutex`).
    let mut unique: Vec<Diagnostic> = Vec::new();
    for d in diagnostics {
        let same = |other: &Diagnostic| {
            other.code == d.code
                && other.message == d.message
                && other.file == d.file
                && other.line == d.line
                && (other.column == d.column || d.code.starts_with("CMP"))
        };
        if !unique.iter().any(same) {
            unique.push(d);
        }
    }
    // Errors first, each group in the order of the files.
    unique.sort_by(|a, b| {
        (a.severity != Severity::Error, &a.file, a.line).cmp(&(
            b.severity != Severity::Error,
            &b.file,
            b.line,
        ))
    });
    Ok(Checked {
        diagnostics: unique,
    })
}

/// Runs a cargo command with diagnostics as JSON, and reads them.
fn cargo(root: &Path, target: &Path, args: &[&str]) -> Result<Vec<Diagnostic>, Failure> {
    let output = Command::new("cargo")
        .args(args)
        .args(["--quiet", "--message-format", "json"])
        .env("CARGO_TARGET_DIR", target)
        .current_dir(root)
        .output()
        .map_err(|error| cargo_failed(&error.to_string()))?;
    let diagnostics: Vec<Diagnostic> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| json::parse(line.as_bytes()).ok())
        .filter(|message| diagnostic::text(message, "reason") == Some("compiler-message"))
        .filter_map(|message| {
            let message = diagnostic::get(&message, "message")?.clone();
            Diagnostic::from_rustc(root, &message)
        })
        .collect();
    let compiled = diagnostics.iter().any(|d| d.severity == Severity::Error);
    if !output.status.success() && !compiled {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let cause: Vec<&str> = stderr
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .take(6)
            .collect();
        return Err(cargo_failed(&cause.join(" ")));
    }
    Ok(diagnostics)
}

fn cargo_failed(cause: &str) -> Failure {
    Failure::problem(CMP0027, &[("cause", cause)], &[])
}

/// Files `cargo fmt` would change.
fn formatting(root: &Path) -> Result<Vec<Diagnostic>, Failure> {
    let output = Command::new("cargo")
        .args(["fmt", "--check", "--", "-l"])
        .current_dir(root)
        .output()
        .map_err(|error| cargo_failed(&error.to_string()))?;
    let prefix = format!("{}/", root.display());
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let file = line
                .strip_prefix(&prefix)
                .unwrap_or(line)
                .replace('\\', "/");
            Diagnostic {
                severity: Severity::Error,
                code: "rustfmt".into(),
                message: format!("`{file}` is not formatted"),
                file: Some(file),
                line: 1,
                column: 1,
                function: None,
                snippet: Vec::new(),
                label: None,
                fix: Some("Run `cargo fmt`.".into()),
                more: None,
            }
        })
        .collect())
}
