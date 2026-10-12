//! `balise explain <CODE>`: the explanation of an error code.

use balise_core::error_codes;
use balise_core::json::Value;

use crate::args::Parsed;
use crate::{Command, Context, Failure, Report, missing};

pub const COMMAND: Command = Command {
    name: "explain",
    summary: "Explain an error code: why it happens and how to fix it.",
    arguments: "<CODE>",
    options: &[],
    exclusive: &[],
    run,
};

fn run(parsed: &Parsed, _: &mut Context) -> Result<Report, Failure> {
    let Some(code) = parsed.positionals.first() else {
        return Err(missing(
            "explain",
            "an error code",
            "balise explain HTTP0003",
        ));
    };
    let Some(found) = error_codes::find(&code.to_ascii_uppercase()) else {
        return Err(Failure::usage(
            balise_core::error_codes::CLI0004,
            &[
                ("value", code),
                ("option", "explain"),
                ("expected", "a Balise error code, such as `HTTP0003`"),
            ],
            &[],
        ));
    };
    Ok(Report {
        text: found.explain().trim_end().to_string(),
        data: Value::Object(vec![
            ("code".into(), Value::String(found.code().into())),
            ("title".into(), Value::String(found.title().into())),
            ("message".into(), Value::String(found.message().into())),
            ("fix".into(), Value::String(found.fix().into())),
            (
                "explanation".into(),
                Value::String(found.explanation().trim().into()),
            ),
        ]),
    })
}
