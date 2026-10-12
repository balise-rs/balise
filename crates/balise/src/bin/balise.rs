//! The `balise` command: a launcher that runs the CLI of the project's Balise version.
//!
//! Inside a project, the version is read in `Cargo.lock` (or `Cargo.toml` before the first
//! build); outside one, it is this launcher's own version, or `--balise-version <version>`.
//! Each version of the CLI is installed once, with `cargo install`, into
//! `~/.balise/versions/<version>/`. `BALISE_CLI` names a CLI binary to run instead, to work
//! on Balise itself.

use std::env;
use std::ffi::OsString;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use balise_core::error_codes::{CLI0003, CLI0011};

const LAUNCHER_VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ExitCode {
    let mut args: Vec<OsString> = env::args_os().skip(1).collect();
    let requested = match take_option(&mut args, "--balise-version") {
        Ok(requested) => requested,
        Err(()) => {
            let example = "balise new shop --balise-version 0.1.0";
            let report = CLI0003.report(
                &[("command", "--balise-version"), ("missing", "a version")],
                &[("example", example)],
            );
            return fail(&report, 2);
        }
    };
    if let Some(cli) = env::var_os("BALISE_CLI") {
        return run(Path::new(&cli), &args);
    }
    let cwd = env::current_dir().unwrap_or_default();
    let version = requested
        .or_else(|| project_version(&cwd))
        .unwrap_or_else(|| LAUNCHER_VERSION.to_string());
    let root = home().join(".balise").join("versions").join(&version);
    let cli = root
        .join("bin")
        .join(format!("balise-cli{}", env::consts::EXE_SUFFIX));
    if !cli.is_file()
        && let Err(cause) = install(&version, &root)
    {
        let args = [("version", version.as_str()), ("cause", cause.as_str())];
        let report = CLI0011.report(&args, &args);
        return fail(&report, 1);
    }
    run(&cli, &args)
}

/// Removes `--name value` (or `--name=value`) from the arguments. `Err` when the value is
/// missing.
fn take_option(args: &mut Vec<OsString>, name: &str) -> Result<Option<String>, ()> {
    let prefix = format!("{name}=");
    for i in 0..args.len() {
        let arg = args[i].to_string_lossy().into_owned();
        if arg == name {
            if i + 1 >= args.len() {
                return Err(());
            }
            let value = args.remove(i + 1).to_string_lossy().into_owned();
            args.remove(i);
            return Ok(Some(value));
        }
        if let Some(value) = arg.strip_prefix(&prefix) {
            args.remove(i);
            return Ok(Some(value.to_string()));
        }
    }
    Ok(None)
}

/// The Balise version of the project around `dir`, if it is one.
fn project_version(dir: &Path) -> Option<String> {
    let root = dir
        .ancestors()
        .find(|dir| dir.join("Cargo.toml").is_file() && dir.join("balise.toml").is_file())?;
    std::fs::read_to_string(root.join("Cargo.lock"))
        .ok()
        .and_then(|lock| locked_version(&lock))
        .or_else(|| {
            let manifest = std::fs::read_to_string(root.join("Cargo.toml")).ok()?;
            required_version(&manifest)
        })
}

/// The version of the `balise` package in a `Cargo.lock`.
fn locked_version(lock: &str) -> Option<String> {
    let mut lines = lock.lines().map(str::trim);
    while let Some(line) = lines.next() {
        if line == "name = \"balise\"" {
            let version = lines.next()?.strip_prefix("version = \"")?;
            return Some(version.trim_end_matches('"').to_string());
        }
    }
    None
}

/// The version `Cargo.toml` requires for `balise`: `balise = "=0.1.0"`, or the `version`
/// of `balise = { version = "=0.1.0", … }`.
fn required_version(manifest: &str) -> Option<String> {
    let line = manifest
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("balise ") || line.starts_with("balise="))?;
    let value = line.split_once('=')?.1.trim();
    let quoted = match value.strip_prefix('{') {
        Some(table) => table.split_once("version")?.1.split_once('=')?.1.trim(),
        None => value,
    };
    let version = quoted.strip_prefix('"')?.split('"').next()?;
    Some(version.trim_start_matches(['=', '^', '~']).to_string())
}

fn home() -> PathBuf {
    env::var_os("BALISE_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(PathBuf::from))
        .or_else(|| env::var_os("USERPROFILE").map(PathBuf::from))
        .unwrap_or_default()
}

/// Installs the CLI of `version` into `root`, once.
fn install(version: &str, root: &Path) -> Result<(), String> {
    eprintln!("Installing the Balise CLI {version}, once for this version…");
    let status = Command::new("cargo")
        .args(["install", "balise-cli", "--locked", "--quiet", "--version"])
        .arg(format!("={version}"))
        .arg("--root")
        .arg(root)
        .status()
        .map_err(|error| format!("cannot run cargo: {error}"))?;
    match status.success() {
        true => Ok(()),
        false => Err(format!("`cargo install balise-cli` failed ({status})")),
    }
}

/// Runs the CLI with the arguments, as if it had been called directly.
fn run(cli: &Path, args: &[OsString]) -> ExitCode {
    let mut command = Command::new(cli);
    command.args(args);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let error = command.exec();
        let cause = format!("cannot run {}: {error}", cli.display());
        fail(
            &CLI0011.report(&[("version", "requested"), ("cause", &cause)], &[]),
            1,
        )
    }
    #[cfg(not(unix))]
    match command.status() {
        Ok(status) => ExitCode::from(status.code().unwrap_or(1) as u8),
        Err(error) => {
            let cause = format!("cannot run {}: {error}", cli.display());
            fail(
                &CLI0011.report(&[("version", "requested"), ("cause", &cause)], &[]),
                1,
            )
        }
    }
}

fn fail(report: &str, code: u8) -> ExitCode {
    let color = std::io::stderr().is_terminal() && env::var_os("NO_COLOR").is_none();
    match color {
        true => eprintln!("\x1b[31merror\x1b[0m {report}"),
        false => eprintln!("error {report}"),
    }
    ExitCode::from(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_version_comes_from_the_lockfile_then_the_manifest() {
        let lock =
            "[[package]]\nname = \"balise\"\nversion = \"0.1.0-alpha.2\"\nsource = \"registry\"\n";
        assert_eq!(locked_version(lock).as_deref(), Some("0.1.0-alpha.2"));
        assert_eq!(
            required_version("[dependencies]\nbalise = \"=0.1.0-alpha.1\"\n").as_deref(),
            Some("0.1.0-alpha.1")
        );
        assert_eq!(
            required_version("balise = { version = \"=0.3.0\", features = [\"orm\"] }").as_deref(),
            Some("0.3.0")
        );
        assert_eq!(required_version("balise-core = \"1\""), None);
    }

    #[test]
    fn the_requested_version_is_taken_out_of_the_arguments() {
        let mut args: Vec<OsString> = ["new", "shop", "--balise-version", "0.2.0", "--no-front"]
            .map(OsString::from)
            .to_vec();
        assert_eq!(
            take_option(&mut args, "--balise-version"),
            Ok(Some("0.2.0".into()))
        );
        assert_eq!(args, ["new", "shop", "--no-front"].map(OsString::from));
        let mut args = vec![OsString::from("--balise-version")];
        assert_eq!(take_option(&mut args, "--balise-version"), Err(()));
    }
}
