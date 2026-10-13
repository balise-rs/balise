//! `balise trace <request_id>`: what happened to a request in dev, from the dev journal: the
//! route routing chose, its parameters and limits, then the request's events.

use std::time::Duration;

use balise_core::error_codes::DEV0008;
use balise_core::json::Value;
use balise_core::text::{format_duration, format_size};

use crate::args::Parsed;
use crate::style::Painter;
use crate::value::{get, number, text};
use crate::{Command, Context, Failure, Report, journal, missing, project};

pub const COMMAND: Command = Command {
    name: "trace",
    summary: "Show what happened to a request in dev: its route, parameters, limits and events.",
    arguments: "<request_id>",
    options: &[],
    exclusive: &[],
    run,
};

fn run(parsed: &Parsed, context: &mut Context) -> Result<Report, Failure> {
    let Some(id) = parsed.positionals.first() else {
        return Err(missing("trace", "a request id", "balise trace req_7f3a2c"));
    };
    let root = project::require(&context.cwd, "trace")?;
    let records = journal::read(&root).unwrap_or_default();
    let mine: Vec<&journal::Record> = records
        .iter()
        .filter(|record| record.request_id() == Some(id.as_str()))
        .collect();
    if mine.is_empty() {
        return Err(Failure::problem(DEV0008, &[("request_id", id)], &[]));
    }
    let p = Painter(context.color);
    let route = mine.iter().find(|record| record.kind() == "route");
    let events: Vec<&&journal::Record> = mine
        .iter()
        .filter(|record| record.kind() == "event")
        .collect();
    let mut lines = Vec::new();
    match route {
        Some(route) => {
            let value = &route.value;
            let field = |key| text(value, key).unwrap_or_default();
            let query = match field("query") {
                "" => String::new(),
                query => format!("?{query}"),
            };
            lines.push(format!(
                "{} → {} {}",
                p.bold(&format!("{} {}{query}", field("method"), field("path"))),
                p.success(field("handler")),
                p.dim(&format!("({})", field("location")))
            ));
            lines.push(format!("  route   {}", field("route")));
            if let Some(Value::Object(params)) = get(value, "params")
                && !params.is_empty()
            {
                let params: Vec<String> = params
                    .iter()
                    .map(|(name, value)| match value {
                        Value::String(value) => format!("{name} = {value}"),
                        _ => name.clone(),
                    })
                    .collect();
                lines.push(format!("  params  {}", params.join(", ")));
            }
            lines.push(format!(
                "  limits  body {} · {}",
                format_size(number(value, "body_size")),
                format_duration(Duration::from_millis(number(value, "timeout_ms") as u64))
            ));
        }
        None => {
            // The request's summary says what was asked.
            let asked = events.iter().find_map(|record| {
                let event = record.event()?;
                Some(format!(
                    "{} {}",
                    text(event, "method")?,
                    text(event, "path")?
                ))
            });
            let why = format!(
                "balise why {}",
                asked.as_deref().unwrap_or("<METHOD> <path>")
            );
            lines.push(format!(
                "No route matched this request: `{}` says why.",
                p.command(&why)
            ));
        }
    }
    lines.push(String::new());
    lines.extend(events.iter().map(|record| record.text().to_string()));
    let data = Value::Object(vec![
        (
            "route".into(),
            route.map_or(Value::Null, |record| record.value.clone()),
        ),
        (
            "events".into(),
            Value::Array(
                events
                    .iter()
                    .filter_map(|record| record.event().cloned())
                    .collect(),
            ),
        ),
    ]);
    Ok(Report {
        text: lines.join("\n").trim_end().to_string(),
        data,
        exit: 0,
    })
}
