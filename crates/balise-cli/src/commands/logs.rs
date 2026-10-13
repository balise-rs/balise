//! `balise logs`: recent events of the app in dev, and of `balise dev` itself, from the dev
//! journal. Works whether the app runs or not, and across its restarts.

use std::time::{SystemTime, UNIX_EPOCH};

use balise_core::error_codes::CLI0004;
use balise_core::json::{Number, Value};
use balise_core::text::parse_duration;

use crate::args::{Opt, Parsed};
use crate::journal::{self, Record};
use crate::value::text;
use crate::{Command, Context, Failure, Report, project};

pub const COMMAND: Command = Command {
    name: "logs",
    summary: "Show recent events of the app in dev and of `balise dev`, filtered by level, code, request or period.",
    arguments: "",
    options: &[
        Opt::value(
            "--level",
            "level",
            LEVELS,
            "the lowest level shown (default: info)",
        ),
        Opt::value(
            "--code",
            "code",
            &[],
            "only events with this code: `ORD0001`",
        ),
        Opt::value(
            "--request",
            "id",
            &[],
            "only the events of a request: `req_7f3a2c`",
        ),
        Opt::value(
            "--since",
            "duration",
            &[],
            "only the last `30s`, `10m` or `2h`",
        ),
        Opt::value(
            "--limit",
            "count",
            &[],
            "the number of events shown (default: 30)",
        ),
    ],
    for_agents: true,
    exclusive: &[],
    run,
};

const LEVELS: &[&str] = &["debug", "info", "warn", "error"];
const LIMIT: usize = 30;

fn run(parsed: &Parsed, context: &mut Context) -> Result<Report, Failure> {
    let root = project::require(&context.cwd, "logs")?;
    let level = parsed.value("--level").unwrap_or("info");
    let lowest = LEVELS.iter().position(|l| *l == level).unwrap_or(1);
    let since =
        match parsed.value("--since") {
            Some(text) => Some(parse_duration(text).ok_or_else(|| {
                invalid(text, "--since", "a duration such as `30s`, `10m` or `2h`")
            })?),
            None => None,
        };
    let limit = match parsed.value("--limit") {
        Some(text) => text
            .parse::<usize>()
            .ok()
            .filter(|n| *n > 0)
            .ok_or_else(|| invalid(text, "--limit", "a whole number from 1"))?,
        None => LIMIT,
    };
    let Some(records) = journal::read(&root) else {
        return Ok(crate::text(
            "No events yet: the journal fills while the app runs with `balise dev`.".into(),
        ));
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    let code = parsed.value("--code").map(str::to_ascii_uppercase);
    let request = parsed.value("--request");
    let matching: Vec<&Record> = records
        .iter()
        .filter(|record| record.kind() == "event")
        .filter(|record| {
            let event = record.event();
            let field = |key| event.and_then(|event| text(event, key));
            let rank = field("level")
                .and_then(|l| LEVELS.iter().position(|known| *known == l))
                .unwrap_or(0);
            rank >= lowest
                && code
                    .as_deref()
                    .is_none_or(|code| field("code") == Some(code))
                && request.is_none_or(|request| field("request_id") == Some(request))
                && since.is_none_or(|since| record.ms() + since.as_millis() as u64 >= now)
        })
        .collect();
    let shown = &matching[matching.len().saturating_sub(limit)..];
    let mut text = shown
        .iter()
        .map(|record| record.text())
        .collect::<Vec<_>>()
        .join("\n");
    let footer = match (matching.len(), shown.len()) {
        (0, _) => "No event matches.".to_string(),
        (total, count) if count < total => {
            format!("The last {count} of {total} events; `--limit {total}` shows them all.")
        }
        (total, _) => format!("{total} events."),
    };
    if !text.is_empty() {
        text.push_str("\n\n");
    }
    text.push_str(&footer);
    let events = shown
        .iter()
        .filter_map(|record| record.event().cloned())
        .collect();
    Ok(Report {
        text,
        data: Value::Object(vec![
            ("events".into(), Value::Array(events)),
            (
                "total".into(),
                Value::Number(Number::Unsigned(matching.len() as u64)),
            ),
        ]),
        exit: 0,
    })
}

fn invalid(value: &str, option: &str, expected: &str) -> Failure {
    Failure::usage(
        CLI0004,
        &[("value", value), ("option", option), ("expected", expected)],
        &[],
    )
}
