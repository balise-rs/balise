//! `balise::run!()`: embeds the project's files in the binary, then starts the app.
//!
//! The files are found next to `Cargo.toml` and checked here, so that a missing, misplaced or
//! malformed `balise.toml` or `budgets.toml` is a compile error rather than a surprise at
//! deployment. Both are required: cargo only notices a file that `include_str!` already
//! reads, so an optional file created later would be ignored until the next change of code.

use std::path::{Path, PathBuf};

use balise_core::error_codes::{CMP0013, CMP0014, CMP0019};
use proc_macro2::{Span, TokenStream};
use quote::quote;

use crate::error;

/// Names someone may give `balise.toml` by mistake.
const NEAR_NAMES: &[&str] = &[
    "Balise.toml",
    "BALISE.toml",
    ".balise.toml",
    "balise.yaml",
    "balise.yml",
    "balise.json",
    "balise.tml",
    "config.toml",
];

/// Names someone may give `budgets.toml` by mistake.
const NEAR_BUDGETS: &[&str] = &["Budgets.toml", "budget.toml", "budgets.tml", "budgets.yaml"];

/// Folders never searched for a misplaced `balise.toml`.
const SKIPPED: &[&str] = &["target", "node_modules", ".git"];

pub(crate) fn expand() -> syn::Result<TokenStream> {
    expand_in(&PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default(),
    ))
}

fn expand_in(root: &Path) -> syn::Result<TokenStream> {
    let config = root.join("balise.toml");
    if !config.is_file() {
        return Err(missing(root, "balise.toml", NEAR_NAMES));
    }
    check_syntax(&config, "balise.toml")?;
    let budgets = root.join("budgets.toml");
    if !budgets.is_file() {
        return Err(missing(root, "budgets.toml", NEAR_BUDGETS));
    }
    check_syntax(&budgets, "budgets.toml")?;
    let budgets = budgets.display().to_string();

    let path = config.display().to_string();
    let abort = error(Span::call_site(), CMP0019, &[], &[]).to_string();
    Ok(quote! {
        // A panic is caught in its request or task; aborting would stop the whole server.
        #[cfg(panic = "abort")]
        ::core::compile_error!(#abort);
        ::balise::__private::start(::balise::__private::Files {
            config: ::core::include_str!(#path),
            budgets: ::core::include_str!(#budgets),
            crate_name: ::core::env!("CARGO_CRATE_NAME"),
            root: ::core::env!("CARGO_MANIFEST_DIR"),
        })
    })
}

/// Reads a file and checks that it is TOML.
fn check_syntax(path: &Path, file: &str) -> syn::Result<()> {
    let invalid = |reason: &str| {
        error(
            Span::call_site(),
            CMP0014,
            &[("file", file), ("reason", reason)],
            &[],
        )
    };
    let text = std::fs::read_to_string(path)
        .map_err(|cause| invalid(&format!("it cannot be read: {cause}")))?;
    if let Err(problem) = toml::de::DeTable::parse(&text) {
        let (line, column) = problem
            .span()
            .map_or((1, 1), |span| position(&text, span.start));
        return Err(invalid(&format!(
            "line {line}, column {column}: {}",
            problem.message().trim()
        )));
    }
    Ok(())
}

/// The error for a file found elsewhere, or under a close name.
fn misplaced(root: &Path, file: &str, found: &Path) -> syn::Error {
    let relative = found
        .strip_prefix(root)
        .unwrap_or(found)
        .display()
        .to_string();
    let reason =
        format!("found `{relative}` instead; move or rename it to `{file}`, next to `Cargo.toml`");
    error(
        Span::call_site(),
        CMP0013,
        &[("file", file), ("reason", &reason)],
        &[("file", file)],
    )
}

/// The error for a missing file, pointing to a misplaced or misnamed one if any.
fn missing(root: &Path, file: &str, near: &[&str]) -> syn::Error {
    let found = near
        .iter()
        .map(|name| root.join(name))
        .find(|path| path.is_file())
        .or_else(|| find_misplaced(root, root, file, 0));
    match found {
        Some(path) => misplaced(root, file, &path),
        None => error(
            Span::call_site(),
            CMP0013,
            &[
                ("file", file),
                ("reason", "there is none next to `Cargo.toml`"),
            ],
            &[("file", file)],
        ),
    }
}

/// A file with this name in a folder of the project, a few levels down at most.
fn find_misplaced(root: &Path, dir: &Path, file: &str, depth: usize) -> Option<PathBuf> {
    if depth > 3 {
        return None;
    }
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| Some(entry.ok()?.path()))
        .collect();
    entries.sort();
    for path in &entries {
        if dir != root && path.file_name().is_some_and(|name| name == file) {
            return Some(path.clone());
        }
    }
    entries
        .into_iter()
        .filter(|path| path.is_dir())
        .find_map(|path| {
            let skipped = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| SKIPPED.contains(&name));
            match skipped {
                true => None,
                false => find_misplaced(root, &path, file, depth + 1),
            }
        })
}

/// Line and column, from 1, of a byte offset.
fn position(text: &str, offset: usize) -> (usize, usize) {
    let before = &text[..offset.min(text.len())];
    let line = before.matches('\n').count() + 1;
    let column = before
        .rsplit('\n')
        .next()
        .unwrap_or_default()
        .chars()
        .count()
        + 1;
    (line, column)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A project folder holding `files`, each `(path, content)`.
    fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join("balise-run-tests").join(name);
        let _ = std::fs::remove_dir_all(&root);
        for (path, content) in files {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, content).unwrap();
        }
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn message(root: &Path) -> String {
        expand_in(root).unwrap_err().to_string()
    }

    #[test]
    fn a_valid_file_is_embedded() {
        let root = project(
            "valid",
            &[("balise.toml", "port = 4000\n"), ("budgets.toml", "")],
        );
        let tokens = expand_in(&root).unwrap().to_string();
        assert!(tokens.contains("include_str"), "{tokens}");
        assert!(tokens.contains("panic = \"abort\""), "{tokens}");
        assert!(tokens.contains("CMP0019"), "{tokens}");
    }

    #[test]
    fn a_missing_file_points_to_a_misplaced_or_misnamed_one() {
        let none = project("none", &[]);
        assert!(message(&none).contains("there is none next to `Cargo.toml`"));

        let misplaced = project("misplaced", &[("src/balise.toml", "")]);
        assert!(message(&misplaced).contains("found `src/balise.toml` instead"));

        let misnamed = project("misnamed", &[("Balise.toml", "")]);
        assert!(message(&misnamed).contains("found `Balise.toml` instead"));

        let ignored = project("ignored", &[("target/balise.toml", "")]);
        assert!(message(&ignored).contains("there is none"));
    }

    #[test]
    fn budgets_are_required_and_checked() {
        let both = project("budgets", &[("balise.toml", ""), ("budgets.toml", "")]);
        assert!(
            expand_in(&both)
                .unwrap()
                .to_string()
                .contains("budgets.toml")
        );

        let none = project("no-budgets", &[("balise.toml", "")]);
        assert!(message(&none).starts_with(
            "CMP0013: `balise::run!()` cannot find budgets.toml: there is none next to `Cargo.toml`"
        ));

        let misplaced = project(
            "misplaced-budgets",
            &[("balise.toml", ""), ("src/budgets.toml", "")],
        );
        assert!(message(&misplaced).starts_with(
            "CMP0013: `balise::run!()` cannot find budgets.toml: found `src/budgets.toml` instead"
        ));

        let invalid = project(
            "invalid-budgets",
            &[("balise.toml", ""), ("budgets.toml", "[\"a::b\"\n")],
        );
        assert!(message(&invalid).starts_with("CMP0014: budgets.toml is not valid TOML: line 1"));
    }

    #[test]
    fn a_syntax_error_names_its_line_and_column() {
        let root = project(
            "syntax",
            &[
                ("balise.toml", "port = 4000\nlog.level = debug\n"),
                ("budgets.toml", ""),
            ],
        );
        let message = message(&root);
        assert!(
            message.starts_with("CMP0014: balise.toml is not valid TOML: line 2, column 13"),
            "{message}"
        );
    }
}
