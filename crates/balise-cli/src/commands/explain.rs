//! `balise explain <CODE>`: the explanation of an error code.

use balise_core::error_codes::{self, CLI0004};
use balise_core::json::Value;

use crate::args::Parsed;
use crate::{Command, Context, Failure, Report, missing};

pub const COMMAND: Command = Command {
    name: "explain",
    summary: "Explain an error code, Balise's (`HTTP0003`) or Rust's (`E0308`): why it happens and how to fix it. `balise explain config` lists every setting.",
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
    if code.eq_ignore_ascii_case("config") {
        return Ok(config());
    }
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

/// `balise explain config`: every setting, its default in each environment, and its
/// variables.
fn config() -> Report {
    use balise_core::config::load::variable_name;
    use balise_core::config::{Config, Environment, SETTINGS};

    let environments = [Environment::Dev, Environment::Test, Environment::Prod];
    let mut lines = vec![
        "Settings of `balise.toml`, at the top for every environment or in `[env.dev]`, `[env.test]`, `[env.prod]`. Each one can also come from its variable, which wins.".to_string(),
        String::new(),
    ];
    let mut data = Vec::new();
    for setting in SETTINGS {
        // As `balise.toml` writes it: text in quotes.
        let toml = |value: Option<String>| match value {
            None => "(not set)".to_string(),
            Some(value) if value.parse::<i64>().is_ok() => value,
            Some(value) => format!("\"{value}\""),
        };
        // Environments with the same default together: `"text" in dev, "json" in test, prod`.
        let mut groups: Vec<(String, Vec<&str>)> = Vec::new();
        for env in environments {
            let value = toml((setting.value)(&Config::defaults(env)));
            match groups.iter_mut().find(|(v, _)| *v == value) {
                Some((_, names)) => names.push(env.name()),
                None => groups.push((value, vec![env.name()])),
            }
        }
        let default = match groups.as_slice() {
            [(value, _)] => value.clone(),
            groups => groups
                .iter()
                .map(|(value, names)| format!("{value} in {}", names.join(", ")))
                .collect::<Vec<_>>()
                .join("; "),
        };
        let mut variables = vec![variable_name(setting.key)];
        variables.extend(setting.also.iter().map(|name| name.to_string()));
        lines.push(format!("{} = {default}", setting.key));
        lines.push(format!(
            "  {}; variable {}",
            setting.expected,
            variables.join(" or ")
        ));
        data.push(Value::Object(vec![
            ("default".into(), Value::String(default)),
            ("expected".into(), Value::String(setting.expected.into())),
            ("key".into(), Value::String(setting.key.into())),
            (
                "variables".into(),
                Value::Array(variables.into_iter().map(Value::String).collect()),
            ),
        ]));
    }
    Report {
        text: lines.join("\n"),
        data: Value::Array(data),
        exit: 0,
    }
}
