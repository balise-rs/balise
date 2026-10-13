//! Diagnostics as text, laid out like Balise's runtime errors.
//!
//! ```text
//! error E0308  mismatched types
//!       at src/orders/mod.rs:14 in orders::list
//!
//!          14 │     let n: u32 = "a";
//!             │                  ^^^ expected `u32`, found `&str`
//!
//!       fix: …
//!       more: balise explain E0308
//! ```

use crate::style::Painter;

use super::diagnostic::{Diagnostic, Severity};

/// Under the first line, as runtime errors are.
const INDENT: &str = "      ";

pub fn diagnostic(d: &Diagnostic, p: Painter) -> String {
    let severity = match d.severity {
        Severity::Error => p.error(d.severity.name()),
        Severity::Warning => p.warning(d.severity.name()),
    };
    let mut out = match d.code.is_empty() {
        true => format!("{severity} {}\n", d.message),
        false => format!("{severity} {}  {}\n", p.bold(&d.code), d.message),
    };
    if let Some(file) = &d.file {
        let mut at = format!("at {file}:{}", d.line);
        if let Some(function) = &d.function {
            at.push_str(&format!(" in {function}"));
        }
        out.push_str(&format!("{INDENT}{}\n", p.dim(&at)));
    }
    if !d.snippet.is_empty() {
        let width = d
            .snippet
            .iter()
            .map(|line| line.number.to_string().len())
            .max()
            .unwrap_or(1);
        let gutter = |number: &str| p.dim(&format!("{INDENT}   {number:>width$} │"));
        out.push('\n');
        let last = d.snippet.len() - 1;
        for (i, line) in d.snippet.iter().enumerate() {
            out.push_str(&format!(
                "{} {}\n",
                gutter(&line.number.to_string()),
                line.text
            ));
            if line.end > line.start || i == last {
                let carets = "^".repeat(line.end.saturating_sub(line.start).max(1));
                let mut underline = format!(
                    "{} {}{}",
                    gutter(""),
                    " ".repeat(line.start.saturating_sub(1)),
                    match d.severity {
                        Severity::Error => p.error(&carets),
                        Severity::Warning => p.warning(&carets),
                    }
                );
                if i == last
                    && let Some(label) = &d.label
                {
                    underline.push(' ');
                    underline.push_str(label);
                }
                out.push_str(underline.trim_end());
                out.push('\n');
            }
        }
        out.push('\n');
    }
    if let Some(fix) = &d.fix {
        out.push_str(&format!("{INDENT}{} {fix}\n", p.fix("fix:")));
    }
    if let Some(more) = &d.more {
        out.push_str(&format!("{INDENT}{}\n", p.dim(&format!("more: {more}"))));
    }
    out.trim_end().to_string()
}

/// The last line: `✗ 2 errors · 1 warning`.
pub fn summary(errors: usize, warnings: usize, ci: bool, p: Painter) -> String {
    let counts = format!(
        "{} · {}",
        plural(errors, "error"),
        plural(warnings, "warning")
    );
    let failed = errors > 0 || (ci && warnings > 0);
    match (failed, ci && warnings > 0) {
        (false, _) => p.success(&format!("✓ {counts}")),
        (true, false) => p.error(&format!("✗ {counts}")),
        (true, true) => p.error(&format!("✗ {counts} (`--ci` fails on warnings)")),
    }
}

fn plural(count: usize, word: &str) -> String {
    match count {
        1 => format!("1 {word}"),
        _ => format!("{count} {word}s"),
    }
}

#[cfg(test)]
mod tests {
    use super::super::diagnostic::Line;
    use super::*;

    #[test]
    fn a_diagnostic_reads_like_a_runtime_error() {
        let d = Diagnostic {
            severity: Severity::Error,
            code: "E0308".into(),
            message: "mismatched types".into(),
            file: Some("src/orders/mod.rs".into()),
            line: 14,
            column: 18,
            function: Some("orders::list".into()),
            snippet: vec![Line {
                number: 14,
                text: "    let n: u32 = \"a\";".into(),
                start: 18,
                end: 21,
            }],
            label: Some("expected `u32`, found `&str`".into()),
            fix: Some("Use a number.".into()),
            more: Some("balise explain E0308".into()),
        };
        assert_eq!(
            diagnostic(&d, Painter(false)),
            "error E0308  mismatched types
      at src/orders/mod.rs:14 in orders::list

         14 │     let n: u32 = \"a\";
            │                  ^^^ expected `u32`, found `&str`

      fix: Use a number.
      more: balise explain E0308"
        );
        assert_eq!(
            summary(1, 2, false, Painter(false)),
            "✗ 1 error · 2 warnings"
        );
        assert_eq!(
            summary(0, 1, true, Painter(false)),
            "✗ 0 errors · 1 warning (`--ci` fails on warnings)"
        );
        assert_eq!(
            summary(0, 0, false, Painter(false)),
            "✓ 0 errors · 0 warnings"
        );
    }
}
