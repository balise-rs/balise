//! `balise routes`: every route, with its handler and limits, as the app serves them.

use std::time::Duration;

use balise_core::json::Value;
use balise_core::text::{format_duration, format_size};

use crate::args::Parsed;
use crate::style::Painter;
use crate::value::{get, number};
use crate::{Command, Context, Failure, Report, app, project};

pub const COMMAND: Command = Command {
    name: "routes",
    summary: "List the routes, with their handler, where it is, and their limits.",
    arguments: "",
    options: &[],
    exclusive: &[],
    run,
};

fn run(_: &Parsed, context: &mut Context) -> Result<Report, Failure> {
    let root = project::require(&context.cwd, "routes")?;
    let answer = app::ask(context, &root, "routes", &["routes"])?;
    let value = answer.json("routes")?;
    let p = Painter(context.color);
    let routes = match get(&value, "routes") {
        Some(Value::Array(routes)) => routes.as_slice(),
        _ => &[],
    };
    let rows: Vec<[String; 4]> = routes
        .iter()
        .map(|route| {
            let text = |key| get(route, key).and_then(as_text).unwrap_or_default();
            [
                text("method").to_string(),
                text("path").to_string(),
                text("handler").to_string(),
                format!("{}  {}", text("location"), limits(route)),
            ]
        })
        .collect();
    let width = |i: usize| {
        rows.iter()
            .map(|row| row[i].chars().count())
            .max()
            .unwrap_or(0)
    };
    let (w0, w1, w2) = (width(0), width(1), width(2));
    let mut lines: Vec<String> = rows
        .iter()
        .map(|[method, path, handler, rest]| {
            format!(
                "{}  {}  {handler:<w2$}  {}",
                p.bold(&format!("{method:<w0$}")),
                p.command(&format!("{path:<w1$}")),
                p.dim(rest)
            )
        })
        .collect();
    let base = get(&value, "base").and_then(as_text).unwrap_or_default();
    lines.push(String::new());
    lines.push(match routes.len() {
        0 => "No routes: a route is an `async fn` with `#[get(\"/path\")]`, in a module declared in `main.rs`.".to_string(),
        1 => format!("1 route under {base}"),
        n => format!("{n} routes under {base}"),
    });
    if let Some(note) = answer.note {
        lines.push(format!("note: {note}"));
    }
    Ok(Report {
        text: lines.join("\n").trim_start().to_string(),
        data: value,
        exit: 0,
    })
}

/// The limits of a route, on one line: body size, timeout, page sizes.
pub fn limits(route: &Value) -> String {
    let mut parts = vec![
        format!("body {}", format_size(number(route, "body_size"))),
        format_duration(Duration::from_millis(number(route, "timeout_ms") as u64)),
    ];
    if let Some(page) = get(route, "page").filter(|page| **page != Value::Null) {
        parts.push(format!(
            "page {}/{}",
            number(page, "default"),
            number(page, "max")
        ));
    }
    parts.join(" · ")
}

pub fn as_text(value: &Value) -> Option<&str> {
    match value {
        Value::String(text) => Some(text),
        _ => None,
    }
}
