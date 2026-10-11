//! The address and the API prefix come from `Config`: no other code hard-codes them.

use std::fs;
use std::path::{Path, PathBuf};

/// Literals that only `Config`'s defaults may contain.
const FORBIDDEN: &[&str] = &["\"/api", ":3000", "3000\""];

#[test]
fn only_config_has_the_default_address_and_prefix() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    for entry in fs::read_dir(&crates).unwrap() {
        collect(&entry.unwrap().path().join("src"), &mut files);
    }
    let mut found = Vec::new();
    for file in files {
        if file.ends_with("balise-core/src/config.rs") {
            continue;
        }
        let text = fs::read_to_string(&file).unwrap();
        // Unit tests build their own values.
        let code = text.split("#[cfg(test)]").next().unwrap_or_default();
        for (index, line) in code.lines().enumerate() {
            if FORBIDDEN.iter().any(|literal| line.contains(literal)) {
                let file = file
                    .strip_prefix(&crates)
                    .unwrap_or(&file)
                    .display()
                    .to_string();
                found.push(format!("{file}:{}: {}", index + 1, line.trim()));
            }
        }
    }
    assert!(
        found.is_empty(),
        "read these from `Config` instead:\n{}",
        found.join("\n")
    );
}

fn collect(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, files);
        } else if path.extension().is_some_and(|e| e == "rs") {
            files.push(path);
        }
    }
}
