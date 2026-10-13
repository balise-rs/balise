//! `balise policy preset <preset>` and `balise policy set <type> human|agent`: who takes
//! sensitive decisions, chosen by the user.

use std::path::Path;

use balise_core::error_codes::{CLI0004, CLI0009, CLI0012};
use balise_core::json::Value;

use crate::args::Parsed;
use crate::{Command, Context, Failure, Report, ask, missing};

pub const COMMAND: Command = Command {
    name: "policy",
    summary: "Choose who takes sensitive decisions: `balise policy preset autonomous|strict|full-auto`, or one type at a time with `balise policy set <type> human|agent`. Needs a terminal.",
    arguments: "preset <preset> | set <type> human|agent",
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

/// Who takes each type of decision in a preset.
fn preset_values(preset: &str) -> Vec<(&'static str, &'static str)> {
    TYPES
        .iter()
        .map(|(kind, autonomous)| {
            let who = match preset {
                "strict" => "human",
                "full-auto" => "agent",
                _ => autonomous,
            };
            (*kind, who)
        })
        .collect()
}

/// The content of `.balise/policy.toml` for a preset.
pub fn file(preset: &str) -> String {
    render(&preset_values(preset))
}

fn render(values: &[(&str, &str)]) -> String {
    let preset = PRESETS
        .iter()
        .find(|(name, _)| preset_values(name) == values)
        .map_or("none, set type by type", |(name, _)| name);
    let mut out = String::from(
        "# Who takes each sensitive decision: \"human\" or \"agent\".\n\
         # Change it only in a terminal: `balise policy preset <preset>`, or one type at a\n\
         # time with `balise policy set <type> human|agent`.\n",
    );
    out.push_str(&format!("# Preset: {preset}.\n\n"));
    for (kind, who) in values {
        out.push_str(&format!("{kind} = \"{who}\"\n"));
    }
    out
}

/// Who takes each type in the project's policy; `strict` for a type it does not name.
fn current(root: &Path) -> Vec<(&'static str, &'static str)> {
    let text = std::fs::read_to_string(root.join(".balise/policy.toml")).unwrap_or_default();
    TYPES
        .iter()
        .map(|(kind, _)| {
            let agent = text.lines().any(|line| {
                line.split_once('=').is_some_and(|(key, value)| {
                    key.trim() == *kind && value.trim().trim_matches('"') == "agent"
                })
            });
            (*kind, if agent { "agent" } else { "human" })
        })
        .collect()
}

/// Writes the policy of a preset in the project at `root`.
pub fn write(root: &Path, preset: &str) -> Result<(), Failure> {
    save(root, &file(preset))
}

fn save(root: &Path, content: &str) -> Result<(), Failure> {
    let dir = root.join(".balise");
    let path = dir.join("policy.toml");
    std::fs::create_dir_all(&dir)
        .and_then(|_| std::fs::write(&path, content))
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
    let words: Vec<&str> = parsed.positionals.iter().map(String::as_str).collect();
    match words.as_slice() {
        ["preset", preset] => run_preset(preset, context),
        ["set", kind, who] => run_set(kind, who, context),
        ["set", ..] => Err(missing(
            "policy set",
            "a type and `human` or `agent`",
            "balise policy set budget agent",
        )),
        _ => Err(missing(
            "policy",
            "`preset` and a preset, or `set`, a type and who decides",
            "balise policy preset autonomous",
        )),
    }
}

fn needs_terminal(context: &Context, command: &str, example: &str) -> Result<(), Failure> {
    match context.interactive {
        true => Ok(()),
        false => Err(Failure::problem(
            CLI0009,
            &[("command", command)],
            &[("example", example)],
        )),
    }
}

fn run_set(kind: &str, who: &str, context: &mut Context) -> Result<Report, Failure> {
    let Some((kind, _)) = TYPES.iter().find(|(name, _)| *name == kind) else {
        let names: Vec<String> = TYPES.iter().map(|(name, _)| format!("`{name}`")).collect();
        let expected = format!("a type of decision: {}", names.join(", "));
        return Err(Failure::usage(
            CLI0004,
            &[
                ("value", kind),
                ("option", "policy set"),
                ("expected", &expected),
            ],
            &[],
        ));
    };
    let who = match who {
        "human" => "human",
        "agent" => "agent",
        _ => {
            return Err(Failure::usage(
                CLI0004,
                &[
                    ("value", who),
                    ("option", "policy set"),
                    ("expected", "`human` or `agent`"),
                ],
                &[],
            ));
        }
    };
    let example = format!("balise policy set {kind} {who}");
    needs_terminal(context, "policy set", &example)?;
    let root = crate::project::require(&context.cwd, "policy set")?;
    let values: Vec<(&str, &str)> = current(&root)
        .into_iter()
        .map(|(name, previous)| match name == *kind {
            true => (name, who),
            false => (name, previous),
        })
        .collect();
    save(&root, &render(&values))?;
    let decides = match who {
        "human" => "you decide",
        _ => "the agent decides",
    };
    Ok(Report {
        text: format!("{kind}: {decides} from now on (.balise/policy.toml)."),
        data: Value::Object(vec![
            ("type".into(), Value::String((*kind).into())),
            ("who".into(), Value::String(who.into())),
        ]),
        exit: 0,
    })
}

fn run_preset(preset: &str, context: &mut Context) -> Result<Report, Failure> {
    if !PRESETS.iter().any(|(name, _)| *name == preset) {
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
    let example = format!("balise policy preset {preset}");
    needs_terminal(context, "policy preset", &example)?;
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
        data: Value::Object(vec![("preset".into(), Value::String(preset.into()))]),
        exit: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_policy_set_type_by_type_names_its_preset_when_it_is_one() {
        assert!(file("strict").contains("# Preset: strict."));
        let mut values = preset_values("strict");
        values[0].1 = "agent";
        assert!(render(&values).contains("# Preset: none, set type by type."));
        let dir = std::env::temp_dir().join("balise-policy-set");
        std::fs::create_dir_all(dir.join(".balise")).unwrap();
        std::fs::write(dir.join(".balise/policy.toml"), file("autonomous")).unwrap();
        assert_eq!(current(&dir), preset_values("autonomous"));
    }

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
