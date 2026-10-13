//! The compiler's diagnostics that Balise recognizes, rewritten with a Balise code and a fix
//! that says what to write.

use std::path::Path;

use balise_core::error_codes::{CMP0021, CMP0022, CMP0023, CMP0024, CMP0025, CMP0026};
use balise_core::json::Value;

use super::diagnostic::{Diagnostic, Severity, array, is_local, message_children, text};

/// Names agents bring from other frameworks: the name, where it comes from, and the Balise
/// way. Built from agent evaluations; a name is a crate, a type, a trait or a macro.
const TRAPS: &[(&str, &str, &str)] = &[
    (
        "axum",
        "Axum",
        "Use `balise::prelude::*`: a route is an `async fn` with `#[get(\"/path\")]`, `#[post(\"/path\")]`…, found on its own.",
    ),
    (
        "actix_web",
        "Actix Web",
        "Use `balise::prelude::*`: a route is an `async fn` with `#[get(\"/path\")]`, `#[post(\"/path\")]`…, found on its own.",
    ),
    (
        "rocket",
        "Rocket",
        "Use `balise::prelude::*`: a route is an `async fn` with `#[get(\"/path\")]`, `#[post(\"/path\")]`…, found on its own.",
    ),
    (
        "warp",
        "warp",
        "Use `balise::prelude::*`: a route is an `async fn` with `#[get(\"/path\")]`, `#[post(\"/path\")]`…, found on its own.",
    ),
    (
        "tokio",
        "Tokio, which Balise runs for you",
        "`balise::run!()` starts the runtime; run background work with `balise::spawn`, and blocking code with `balise::blocking`.",
    ),
    (
        "Json",
        "Axum and Actix Web",
        "Take a request's body as `Body<T>`, where `T` derives `Input`; return a struct that derives `Output`. Balise reads and writes the JSON.",
    ),
    (
        "Router",
        "Axum",
        "Routes are not registered: `#[get(\"/path\")]` on an `async fn` is enough, in a module declared in `main.rs`.",
    ),
    (
        "IntoResponse",
        "Axum",
        "Return a struct that derives `Output`, `Created<T>`, `NoContent` or `Redirect`, or a `Result` of one of these.",
    ),
    (
        "HttpResponse",
        "Actix Web",
        "Return a struct that derives `Output`, `Created<T>`, `NoContent` or `Redirect`, or a `Result` of one of these.",
    ),
    (
        "Responder",
        "Actix Web",
        "Return a struct that derives `Output`, `Created<T>`, `NoContent` or `Redirect`, or a `Result` of one of these.",
    ),
    (
        "StatusCode",
        "Axum and the `http` crate",
        "Return `Created<T>` or `NoContent` for success statuses; an error's status comes from its `#[derive(AppError)]` declaration.",
    ),
    (
        "Serialize",
        "serde",
        "Derive `Input` for what a request sends and `Output` for what a response returns.",
    ),
    (
        "Deserialize",
        "serde",
        "Derive `Input` for what a request sends and `Output` for what a response returns.",
    ),
    (
        "serde",
        "serde",
        "Derive `Input` for what a request sends and `Output` for what a response returns.",
    ),
    (
        "serde_json",
        "serde_json",
        "Derive `Input` for what a request sends and `Output` for what a response returns: Balise reads and writes the JSON.",
    ),
];

/// Rewrites `diagnostic`, read from `message`, when Balise knows better.
pub fn rustc(root: &Path, message: &Value, diagnostic: &mut Diagnostic) {
    if diagnostic.code.starts_with("CMP") {
        return;
    }
    let first_name = quoted(&diagnostic.message).map(str::to_string);
    let name = first_name.as_deref().unwrap_or_default();
    match diagnostic.code.as_str() {
        "clippy::disallowed_types" => {
            let what = format!("`{}`", last_segment(name));
            diagnostic.recode(CMP0024, &[("what", &what)], &[]);
            diagnostic.severity = Severity::Error;
            return;
        }
        "clippy::disallowed_macros" => {
            diagnostic.recode(CMP0025, &[("name", last_segment(name))], &[]);
            diagnostic.severity = Severity::Error;
            return;
        }
        "clippy::disallowed_methods" => {
            diagnostic.recode(CMP0026, &[("name", name)], &[]);
            diagnostic.severity = Severity::Error;
            return;
        }
        "E0603" | "E0616" => {
            private(message, diagnostic, name);
            return;
        }
        _ => {}
    }
    if diagnostic.message == "future cannot be sent between threads safely" {
        not_send(root, message, diagnostic);
        return;
    }
    let unknown = ["cannot find", "unresolved import", "failed to resolve"]
        .iter()
        .any(|start| diagnostic.message.starts_with(start));
    if unknown && let Some((trap, origin, instead)) = trap(name) {
        diagnostic.recode(
            CMP0021,
            &[("name", trap), ("origin", origin)],
            &[("instead", instead)],
        );
    }
}

/// The trap for a name, whole or by its first or last segment: `axum::Json` is Axum's.
fn trap(name: &str) -> Option<(&'static str, &'static str, &'static str)> {
    let first = name.split("::").next().unwrap_or(name);
    [name, first, last_segment(name)]
        .iter()
        .find_map(|candidate| TRAPS.iter().find(|(trap, _, _)| trap == candidate))
        .copied()
}

/// E0603 (`function `secret` is private`) and E0616 (`field `x` of struct `Y` is private`):
/// where to add `pub`.
fn private(message: &Value, diagnostic: &mut Diagnostic, name: &str) {
    let defined = message_children(message).iter().find_map(|child| {
        let note = text(child, "message")?;
        if !note.ends_with("is defined here") {
            return None;
        }
        let span = array(child, "spans").iter().find(|span| is_local(span))?;
        let file = text(span, "file_name")?;
        let line = super::diagnostic::number(span, "line_start");
        Some(format!(" ({file}:{line})"))
    });
    diagnostic.recode(
        CMP0023,
        &[("name", name)],
        &[
            ("name", name),
            ("at", defined.as_deref().unwrap_or_default()),
        ],
    );
}

/// "future cannot be sent between threads safely": the variable kept across an `.await`,
/// reported where it is declared, where the fix starts.
fn not_send(root: &Path, message: &Value, diagnostic: &mut Diagnostic) {
    let spans: Vec<&Value> = message_children(message)
        .iter()
        .flat_map(|child| array(child, "spans"))
        .filter(|span| is_local(span))
        .collect();
    let label = |span: &&Value| text(span, "label").unwrap_or_default().to_string();
    let Some(held) = spans
        .iter()
        .find(|span| label(span).contains("which is not `Send`"))
    else {
        return;
    };
    let Some(awaited) = spans
        .iter()
        .find(|span| label(span).starts_with("await occurs here"))
    else {
        return;
    };
    let held_label = label(held);
    let ty = quoted(&held_label).unwrap_or("?");
    let await_label = label(awaited);
    let variable = quoted(&await_label).unwrap_or("this value");
    let await_line = super::diagnostic::number(awaited, "line_start").to_string();
    diagnostic.recode(
        CMP0022,
        &[("variable", variable), ("type", ty)],
        &[("variable", variable), ("await_line", &await_line)],
    );
    diagnostic.locate(root, held);
    diagnostic.label = Some(format!("kept until the `.await` on line {await_line}"));
}

/// The first name between backticks.
fn quoted(text: &str) -> Option<&str> {
    let (_, rest) = text.split_once('`')?;
    rest.split_once('`').map(|(name, _)| name)
}

fn last_segment(path: &str) -> &str {
    path.rsplit("::").next().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traps_match_any_segment() {
        assert_eq!(trap("axum::Json").map(|t| t.0), Some("axum"));
        assert_eq!(trap("Json").map(|t| t.1), Some("Axum and Actix Web"));
        assert_eq!(trap("serde::Serialize").map(|t| t.0), Some("serde"));
        assert!(trap("Order").is_none());
    }
}
