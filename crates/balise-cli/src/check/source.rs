//! What `balise check` reads in the project's source itself: where a line is, and what the
//! compiler cannot report.

use std::path::Path;

use balise_core::error_codes::{CMP0020, CMP0024};

use super::diagnostic::{Diagnostic, Line, Severity};

/// The function around `line` of `file`, with its module: `orders::list`. `None` outside a
/// function.
pub fn function_at(root: &Path, file: &str, line: usize) -> Option<String> {
    let text = std::fs::read_to_string(root.join(file)).ok()?;
    let lines: Vec<&str> = text.lines().collect();
    let target = line.checked_sub(1)?;
    for start in (0..=target.min(lines.len().checked_sub(1)?)).rev() {
        let Some(name) = declared_function(lines[start]) else {
            continue;
        };
        // The line is in this function's signature, or in its body while its braces are
        // still open there.
        let before = &lines[start..target];
        let opened = before.iter().any(|line| line.contains('{'));
        let depth: i64 = before
            .iter()
            .map(|line| line.matches('{').count() as i64 - line.matches('}').count() as i64)
            .sum();
        if depth > 0 || !opened {
            let module = module_path(file);
            return Some(match module.is_empty() {
                true => name.to_string(),
                false => format!("{module}::{name}"),
            });
        }
        return None;
    }
    None
}

/// `orders::list` for `async fn list(`, with any visibility and qualifier.
fn declared_function(line: &str) -> Option<&str> {
    let (_, rest) = line.split_once("fn ")?;
    let before = &line[..line.len() - rest.len() - 3];
    let words_ok = before.split_whitespace().all(|word| {
        matches!(
            word,
            "pub" | "async" | "const" | "unsafe" | "extern" | "\"C\""
        ) || word.starts_with("pub(")
    });
    let name_end = rest
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    (words_ok && name_end > 0).then(|| &rest[..name_end])
}

/// The module of a file: `src/orders/api.rs` is `orders::api`, `src/orders/mod.rs` is
/// `orders`, `src/main.rs` is the crate's root.
pub fn module_path(file: &str) -> String {
    let path = file.strip_prefix("src/").unwrap_or(file);
    let path = path.strip_suffix(".rs").unwrap_or(path);
    let parts: Vec<&str> = path
        .split('/')
        .filter(|part| !matches!(*part, "mod" | "main" | "lib"))
        .collect();
    parts.join("::")
}

/// Folders of `src/` holding a module that `main.rs` does not declare.
pub fn undeclared_modules(root: &Path) -> Vec<Diagnostic> {
    let src = root.join("src");
    let declared: String = ["main.rs", "lib.rs"]
        .iter()
        .filter_map(|file| std::fs::read_to_string(src.join(file)).ok())
        .collect();
    let Ok(entries) = std::fs::read_dir(&src) else {
        return Vec::new();
    };
    let mut found: Vec<Diagnostic> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().join("mod.rs").is_file())
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .filter(|module| !declares(&declared, module))
        .map(|module| {
            let file = format!("src/{module}/mod.rs");
            let mut diagnostic = Diagnostic::coded(
                Severity::Error,
                CMP0020,
                &[("module", &module)],
                &[("module", &module)],
            );
            diagnostic.file = Some(file);
            diagnostic.line = 1;
            diagnostic.column = 1;
            diagnostic
        })
        .collect();
    found.sort_by(|a, b| a.file.cmp(&b.file));
    found
}

/// Whether `text` holds `mod <module>;`, with any visibility.
fn declares(text: &str, module: &str) -> bool {
    text.lines().any(|line| {
        let line = line.trim();
        line.strip_suffix(&format!("mod {module};"))
            .is_some_and(|before| before.is_empty() || before.trim_end().starts_with("pub"))
    })
}

/// `static mut` items in the project's Rust files, which no lint reports at their
/// declaration.
pub fn static_muts(root: &Path) -> Vec<Diagnostic> {
    let mut files = Vec::new();
    collect(&root.join("src"), &mut files);
    files.sort();
    let mut found = Vec::new();
    for path in files {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let file = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        for (index, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or_default();
            let Some(column) = code.find("static mut ") else {
                continue;
            };
            let name: String = code[column + 11..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            let what = format!("`static mut {name}`");
            let mut diagnostic =
                Diagnostic::coded(Severity::Error, CMP0024, &[("what", &what)], &[]);
            diagnostic.function = function_at(root, &file, index + 1);
            diagnostic.file = Some(file.clone());
            diagnostic.line = index + 1;
            diagnostic.column = column + 1;
            diagnostic.snippet = vec![Line {
                number: index + 1,
                text: line.to_string(),
                start: column + 1,
                end: column + 11 + name.len(),
            }];
            found.push(diagnostic);
        }
    }
    found
}

fn collect(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modules_follow_files() {
        assert_eq!(module_path("src/orders/api.rs"), "orders::api");
        assert_eq!(module_path("src/orders/mod.rs"), "orders");
        assert_eq!(module_path("src/main.rs"), "");
    }

    #[test]
    fn functions_are_found_by_their_braces() {
        let dir = std::env::temp_dir().join("balise-check-source");
        std::fs::create_dir_all(dir.join("src/orders")).unwrap();
        std::fs::write(
            dir.join("src/orders/mod.rs"),
            "static A: u32 = 1;\n\n#[get(\"/\")]\npub async fn list(\n    x: u32,\n) -> u32 {\n    x\n}\n\nconst B: u32 = 2;\n",
        )
        .unwrap();
        let at = |line| function_at(&dir, "src/orders/mod.rs", line);
        assert_eq!(at(1), None);
        assert_eq!(at(5).as_deref(), Some("orders::list"));
        assert_eq!(at(7).as_deref(), Some("orders::list"));
        assert_eq!(at(10), None);
    }

    #[test]
    fn declarations_are_read() {
        assert!(declares("mod orders;\n", "orders"));
        assert!(declares("pub mod orders;\n", "orders"));
        assert!(declares("pub(crate) mod orders;\n", "orders"));
        assert!(!declares("// mod orders;\n", "orders"));
        assert!(!declares("mod orders_v2;\n", "orders"));
    }
}
