//! The project the CLI works on: found from the current folder upwards.

use std::path::{Path, PathBuf};

use balise_core::config::load::{self, Files, Mode};
use balise_core::config::{Config, Environment};
use balise_core::error_codes::CLI0010;

use crate::Failure;

/// The project around `dir`: the first folder upwards with `Cargo.toml` and `balise.toml`.
pub fn root(dir: &Path) -> Option<PathBuf> {
    dir.ancestors()
        .find(|dir| dir.join("Cargo.toml").is_file() && dir.join("balise.toml").is_file())
        .map(Path::to_path_buf)
}

/// The project around `dir`, or `CLI0010` for `command`.
pub fn require(dir: &Path, command: &str) -> Result<PathBuf, Failure> {
    root(dir).ok_or_else(|| {
        Failure::problem(
            CLI0010,
            &[("command", command), ("path", &dir.display().to_string())],
            &[],
        )
    })
}

/// The package's name in `Cargo.toml`: the name of the app's binary.
pub fn package_name(root: &Path) -> Option<String> {
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).ok()?;
    let mut in_package = false;
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            in_package = line == "[package]";
        } else if in_package && let Some(value) = line.strip_prefix("name") {
            let value = value.trim_start().strip_prefix('=')?.trim();
            return Some(value.trim_matches('"').to_string());
        }
    }
    None
}

/// Where cargo writes what it builds.
pub fn target_dir(root: &Path) -> PathBuf {
    match std::env::var_os("CARGO_TARGET_DIR") {
        Some(dir) => root.join(dir),
        None => root.join("target"),
    }
}

/// The project's dev configuration, read as the app reads it with these environment
/// variables. Balise's defaults when it cannot be read: the app reports why when it starts.
pub fn config(root: &Path, variables: &[(String, String)]) -> Config {
    let read = |name: &str| -> &'static str {
        let text = std::fs::read_to_string(root.join(name)).unwrap_or_default();
        Box::leak(text.into_boxed_str())
    };
    let files = Files {
        config: read("balise.toml"),
        budgets: read("budgets.toml"),
        crate_name: "app",
        root: "",
    };
    let dev = Environment::Dev;
    match load::load(&files, dev, dev, variables, Mode::Start) {
        Ok(loaded) => loaded.config,
        Err(_) => Config::defaults(dev),
    }
}
