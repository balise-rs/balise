//! Building the app with `--features dev`, errors kept short.

use std::path::Path;
use std::process::Command;
use std::time::Instant;

use balise_core::error_codes::DEV0003;

use crate::Failure;

/// How a build ended.
pub enum Built {
    Ok {
        seconds: f64,
        warnings: Option<String>,
    },
    Failed {
        errors: String,
        lines: Vec<String>,
    },
}

/// Builds the app of the project at `root`.
pub fn build(root: &Path) -> Result<Built, Failure> {
    let started = Instant::now();
    let output = Command::new("cargo")
        .args([
            "build",
            "--features",
            "dev",
            "--message-format",
            "short",
            "--color",
            "never",
        ])
        .current_dir(root)
        .output()
        .map_err(|error| Failure::problem(DEV0003, &[("cause", &error.to_string())], &[]))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    Ok(match output.status.success() {
        true => Built::Ok {
            seconds: started.elapsed().as_secs_f64(),
            warnings: warnings(&stderr),
        },
        false => {
            let (errors, lines) = errors(&stderr);
            Built::Failed { errors, lines }
        }
    })
}

/// `2 warnings`, from cargo's summary, when there are some.
fn warnings(stderr: &str) -> Option<String> {
    stderr.lines().find_map(|line| {
        let (_, rest) = line.split_once(" generated ")?;
        Some(
            rest.trim_end_matches(|c: char| !c.is_alphanumeric())
                .to_string(),
        )
    })
}

/// The number of errors as words (`1 error`), and each error in short form.
fn errors(stderr: &str) -> (String, Vec<String>) {
    let lines: Vec<String> = stderr
        .lines()
        .filter(|line| {
            line.contains(": error")
                || (line.starts_with("error") && !line.starts_with("error: could not compile"))
        })
        .map(str::to_string)
        .collect();
    let count = stderr
        .lines()
        .find_map(|line| line.split_once("due to ").map(|(_, rest)| rest.to_string()))
        .map(|rest| {
            rest.split(';')
                .next()
                .unwrap_or_default()
                .replace("previous ", "")
        })
        .unwrap_or_else(|| match lines.len() {
            1 => "1 error".into(),
            n => format!("{n} errors"),
        });
    (count, lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_output_is_kept_short() {
        let stderr = "   Compiling shop v0.1.0 (/app)\nsrc/main.rs:3:5: error[E0425]: cannot find value `x` in this scope\nsrc/main.rs:4:1: warning: unused import\nerror: could not compile `shop` (bin \"shop\") due to 1 previous error; 1 warning emitted\n";
        let (count, lines) = errors(stderr);
        assert_eq!(count, "1 error");
        assert_eq!(
            lines,
            ["src/main.rs:3:5: error[E0425]: cannot find value `x` in this scope"]
        );
        let ok =
            "warning: `shop` (bin \"shop\") generated 2 warnings\n    Finished `dev` profile\n";
        assert_eq!(warnings(ok).as_deref(), Some("2 warnings"));
    }
}
