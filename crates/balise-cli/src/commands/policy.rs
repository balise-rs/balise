//! `balise policy preset <preset>`: who decides sensitive decisions, chosen by the user.

use std::path::Path;

use balise_core::error_codes::{CLI0004, CLI0009, CLI0012};
use balise_core::json::Value;

use crate::args::Parsed;
use crate::{Command, Context, Failure, Report, ask, missing};

pub const COMMAND: Command = Command {
    name: "policy",
    summary: "Choose who takes sensitive decisions: `balise policy preset autonomous|strict|full-auto`. Needs a terminal.",
    arguments: "preset <preset>",
    options: &[],
    exclusive: &[],
    run,
};

/// The types of sensitive decisions, each with who decides it in `autonomous`.
const TYPES: &[(&str, &str)] = &[
    ("budget", "agent"),
    ("accept", "agent"),
    ("unbounded", "agent"),
    ("dependency", "agent"),
    ("all_owners", "human"),
    ("raw_unscoped", "human"),
    ("breaking_migration", "human"),
];

/// The presets, each with one line saying what it means.
pub const PRESETS: &[(&str, &str)] = &[
    (
        "autonomous",
        "the agent decides technical trade-offs; you decide what touches security and data",
    ),
    ("strict", "you decide every sensitive decision"),
    (
        "full-auto",
        "the agent decides everything; review its decisions afterwards",
    ),
];

/// What `full-auto` means, said once when it is chosen.
pub const FULL_AUTO_WARNING: &str = "With full-auto, the agent may read every user's rows, run raw SQL on owned tables and ship breaking migrations without asking. Every decision is still recorded in .balise/approvals.toml.";

/// The content of `.balise/policy.toml` for a preset.
pub fn file(preset: &str) -> String {
    let mut out = String::from(
        "# Who takes each sensitive decision: \"human\" or \"agent\".\n\
         # Change it only in a terminal: `balise policy preset <preset>`, or one type at a\n\
         # time with `balise policy set <type> human|agent`.\n",
    );
    out.push_str(&format!("# Preset: {preset}.\n\n"));
    for (kind, autonomous) in TYPES {
        let who = match preset {
            "strict" => "human",
            "full-auto" => "agent",
            _ => autonomous,
        };
        out.push_str(&format!("{kind} = \"{who}\"\n"));
    }
    out
}

/// Writes the policy of a preset in the project at `root`.
pub fn write(root: &Path, preset: &str) -> Result<(), Failure> {
    let dir = root.join(".balise");
    let path = dir.join("policy.toml");
    std::fs::create_dir_all(&dir)
        .and_then(|_| std::fs::write(&path, file(preset)))
        .map_err(|error| {
            Failure::problem(
                CLI0012,
                &[
                    ("path", &path.display().to_string()),
                    ("cause", &error.to_string()),
                ],
                &[],
            )
        })
}

fn run(parsed: &Parsed, context: &mut Context) -> Result<Report, Failure> {
    let example = "balise policy preset autonomous";
    let (Some("preset"), Some(preset)) = (
        parsed.positionals.first().map(String::as_str),
        parsed.positionals.get(1),
    ) else {
        return Err(missing("policy", "`preset` and a preset", example));
    };
    if !PRESETS.iter().any(|(name, _)| name == preset) {
        return Err(Failure::usage(
            CLI0004,
            &[
                ("value", preset),
                ("option", "policy preset"),
                ("expected", "`autonomous`, `strict` or `full-auto`"),
            ],
            &[],
        ));
    }
    if !context.interactive {
        let example = format!("balise policy preset {preset}");
        return Err(Failure::problem(
            CLI0009,
            &[("command", "policy preset")],
            &[("example", &example)],
        ));
    }
    let root = crate::project::require(&context.cwd, "policy preset")?;
    if preset == "full-auto" {
        let _ = writeln!(context.output, "{FULL_AUTO_WARNING}");
        if !ask::confirm(context, "Use full-auto?", false) {
            return Ok(crate::text("The policy is unchanged.".into()));
        }
    }
    write(&root, preset)?;
    Ok(Report {
        text: format!("The policy is now {preset}: .balise/policy.toml"),
        data: Value::Object(vec![("preset".into(), Value::String(preset.clone()))]),
        exit: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_say_who_decides_each_type() {
        let decided = |preset, who: &str| {
            let line = format!("= \"{who}\"");
            file(preset).lines().filter(|l| l.ends_with(&line)).count()
        };
        assert_eq!(
            (decided("strict", "human"), decided("strict", "agent")),
            (7, 0)
        );
        assert_eq!(
            (decided("full-auto", "human"), decided("full-auto", "agent")),
            (0, 7)
        );
        assert_eq!(
            (
                decided("autonomous", "human"),
                decided("autonomous", "agent")
            ),
            (3, 4)
        );
    }
}
