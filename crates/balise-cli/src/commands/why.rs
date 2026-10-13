//! `balise why <METHOD> <path>`: the route a request reaches, or why none does, as the app
//! itself decides.

use balise_core::json::Value;

use crate::args::Parsed;
use crate::commands::routes::{as_text, limits};
use crate::style::Painter;
use crate::value::get;
use crate::{Command, Context, Failure, Report, app, missing, project};

pub const COMMAND: Command = Command {
    name: "why",
    summary: "Say which route answers a request, or why none does: `balise why GET /api/orders/5`.",
    arguments: "<METHOD> <path>",
    options: &[],
    exclusive: &[],
    run,
};

fn run(parsed: &Parsed, context: &mut Context) -> Result<Report, Failure> {
    let example = "balise why GET /api/orders/5";
    let [method, path] = parsed.positionals.as_slice() else {
        return Err(missing("why", "a method and a path", example));
    };
    let root = project::require(&context.cwd, "why")?;
    let answer = app::ask(context, &root, "why", &["why", method, path])?;
    let value = answer.json("why")?;
    let p = Painter(context.color);
    let text = |value: &Value, key| {
        get(value, key)
            .and_then(as_text)
            .unwrap_or_default()
            .to_string()
    };
    let request = format!("{} {}", text(&value, "method"), text(&value, "path"));
    let mut lines = Vec::new();
    match text(&value, "result").as_str() {
        "found" => {
            let route = get(&value, "route").cloned().unwrap_or(Value::Null);
            lines.push(format!(
                "{} → {} {}",
                p.bold(&request),
                p.success(&text(&route, "handler")),
                p.dim(&format!("({})", text(&route, "location")))
            ));
            if let Some(Value::Object(params)) = get(&value, "params")
                && !params.is_empty()
            {
                let params: Vec<String> = params
                    .iter()
                    .map(|(name, value)| format!("{name} = {}", as_text(value).unwrap_or_default()))
                    .collect();
                lines.push(format!("  params  {}", params.join(", ")));
            }
            lines.push(format!("  route   {}", text(&route, "path")));
            lines.push(format!("  limits  {}", limits(&route)));
            lines.push(p.dim(
                "  Path parameters are converted when the handler reads them: a value of the wrong type answers `400` (HTTP0010).",
            ));
        }
        "method_not_allowed" => {
            let allowed: Vec<&str> = match get(&value, "allowed") {
                Some(Value::Array(allowed)) => allowed.iter().filter_map(as_text).collect(),
                _ => Vec::new(),
            };
            lines.push(format!(
                "{}  {request} answers 405: the path exists, with {} only",
                p.bold("HTTP0008"),
                allowed.join(", ")
            ));
        }
        _ => {
            lines.push(format!(
                "{}  {request} answers 404: no route matches",
                p.bold("HTTP0003")
            ));
            if let Some(instead) = get(&value, "instead").filter(|v| **v != Value::Null) {
                lines.push(format!(
                    "  {} {} {}",
                    p.fix("instead:"),
                    p.command(&format!(
                        "{} {}",
                        text(&value, "method"),
                        text(instead, "path")
                    )),
                    p.dim(&format!("({})", text(instead, "reason")))
                ));
            }
            let near = match get(&value, "near") {
                Some(Value::Array(near)) => near.as_slice(),
                _ => &[],
            };
            if !near.is_empty() {
                lines.push("  Routes that almost match:".to_string());
                for near in near {
                    let routes = match get(near, "routes") {
                        Some(Value::Array(routes)) => routes.as_slice(),
                        _ => &[],
                    };
                    let methods: Vec<String> = routes.iter().map(|r| text(r, "method")).collect();
                    let handlers: Vec<String> = routes
                        .iter()
                        .map(|r| format!("{} ({})", text(r, "handler"), text(r, "location")))
                        .collect();
                    lines.push(format!(
                        "    {} {}: {}",
                        methods.join(", "),
                        p.command(&text(near, "path")),
                        text(near, "reason"),
                    ));
                    lines.push(format!("      {}", p.dim(&handlers.join(", "))));
                }
            } else if get(&value, "instead").is_none_or(|v| *v == Value::Null) {
                lines.push(format!(
                    "  {} lists every route.",
                    p.command("balise routes")
                ));
            }
        }
    }
    if let Some(note) = answer.note {
        lines.push(format!("note: {note}"));
    }
    Ok(Report {
        text: lines.join("\n"),
        data: value,
        exit: 0,
    })
}
