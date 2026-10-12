//! The facade never exposes a delegated dependency (rule 1 of AGENTS.md).
//!
//! Rustdoc's JSON output, which would list the public API exactly, needs nightly. This test
//! checks a stricter, stable proxy: the facade does not depend on a delegated crate directly,
//! and its source never names one.

use std::fs;
use std::path::Path;

const DELEGATED: &[&str] = &[
    "tokio",
    "hyper",
    "hyper_util",
    "hyper-util",
    "http_body_util",
    "http-body-util",
    "bytes",
    "rustls",
];

#[test]
fn the_facade_does_not_depend_on_delegated_crates() {
    let manifest =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).unwrap();
    // Only what a project gets: test-only dependencies are fine.
    let mut in_dependencies = false;
    for line in manifest.lines() {
        if line.starts_with('[') {
            in_dependencies = line.trim() == "[dependencies]";
            continue;
        }
        if !in_dependencies {
            continue;
        }
        let name = line.split(['=', '.', ' ']).next().unwrap_or_default();
        assert!(
            !DELEGATED.contains(&name),
            "the facade depends on `{name}` directly"
        );
    }
}

#[test]
fn the_facade_source_never_names_delegated_crates() {
    let mut files = Vec::new();
    collect(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    assert!(files.len() >= 2, "lib.rs and the launcher");
    for path in files {
        let text = fs::read_to_string(&path).unwrap();
        for word in text.split(|c: char| !(c.is_alphanumeric() || c == '_')) {
            assert!(
                !DELEGATED.contains(&word),
                "{} names `{word}`",
                path.display()
            );
        }
    }
}

/// Every file under `dir`, the launcher in `src/bin/` included.
fn collect(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        match path.is_dir() {
            true => collect(&path, files),
            false => files.push(path),
        }
    }
}
