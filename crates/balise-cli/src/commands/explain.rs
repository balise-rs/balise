//! `balise explain <CODE>`: the explanation of an error code.

use balise_core::error_codes::{self, CLI0004};
use balise_core::json::Value;

use crate::args::Parsed;
use crate::{Command, Context, Failure, Report, missing};

pub const COMMAND: Command = Command {
    name: "explain",
    summary: "Explain an error code, Balise's (`HTTP0003`) or Rust's (`E0308`): why it happens and how to fix it.",
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
    let code = code.to_ascii_uppercase();
    if crate::check::is_rustc_code(&code) {
        return rustc(&code);
    }
    let Some(found) = error_codes::find(&code) else {
        return Err(Failure::usage(
            CLI0004,
            &[
                ("value", &code),
                ("option", "explain"),
                (
                    "expected",
                    "a Balise error code, such as `HTTP0003`, or a Rust one, such as `E0308`",
                ),
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
        exit: 0,
    })
}

/// A Rust error code, explained by the compiler. Balise may add its own words to some later.
fn rustc(code: &str) -> Result<Report, Failure> {
    let output = std::process::Command::new("rustc")
        .args(["--explain", code])
        .output();
    let text = match output {
        Ok(output) if output.status.success() => format!(
            "{code}  (explained by rustc)\n\n{}",
            String::from_utf8_lossy(&output.stdout).trim_end()
        ),
        Ok(_) => {
            return Err(Failure::usage(
                CLI0004,
                &[
                    ("value", code),
                    ("option", "explain"),
                    ("expected", "an error code that exists"),
                ],
                &[],
            ));
        }
        // Without Rust on this machine, the same explanation online.
        Err(_) => format!(
            "{code} is a Rust error code, explained at https://doc.rust-lang.org/error_codes/{code}.html"
        ),
    };
    Ok(Report {
        data: Value::Object(vec![
            ("code".into(), Value::String(code.into())),
            ("explanation".into(), Value::String(text.clone())),
        ]),
        text,
        exit: 0,
    })
}
