//! Reading a command's arguments from what its declaration allows.
//!
//! Every mistake has a `CLI` code: an unknown option with the closest one, a missing value, a
//! value outside the allowed ones, two options of a group that excludes the others.

use balise_core::error_codes::{CLI0002, CLI0003, CLI0004, CLI0005};
use balise_core::text::closest;

use crate::Failure;

/// An option of a command: `--no-example`, or `--front <framework>`.
pub struct Opt {
    pub name: &'static str,
    /// What the value is called in help (`framework`), for an option that takes one.
    pub value: Option<&'static str>,
    /// The values accepted, when they are a fixed list.
    pub values: &'static [&'static str],
    pub help: &'static str,
}

impl Opt {
    pub const fn flag(name: &'static str, help: &'static str) -> Opt {
        Opt {
            name,
            value: None,
            values: &[],
            help,
        }
    }

    pub const fn value(
        name: &'static str,
        value: &'static str,
        values: &'static [&'static str],
        help: &'static str,
    ) -> Opt {
        Opt {
            name,
            value: Some(value),
            values,
            help,
        }
    }
}

/// Options shared by every command.
pub const GLOBAL: &[Opt] = &[
    Opt::flag("--json", "print the result as JSON, for programs"),
    Opt::flag("--no-input", "never ask a question, even in a terminal"),
    Opt::flag("--help", "describe the command"),
];

/// What a command was given.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Parsed {
    pub positionals: Vec<String>,
    pub flags: Vec<&'static str>,
    pub values: Vec<(&'static str, String)>,
}

impl Parsed {
    pub fn has(&self, name: &str) -> bool {
        self.flags.contains(&name)
    }

    pub fn value(&self, name: &str) -> Option<&str> {
        self.values
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.as_str())
    }

    /// Whether the option was given, with or without a value.
    pub fn given(&self, name: &str) -> bool {
        self.has(name) || self.value(name).is_some()
    }
}

/// Reads `args` for `command`, whose options are `options` and the global ones. `exclusive`
/// lists groups of options of which at most one may be given.
pub fn parse(
    command: &str,
    args: &[String],
    options: &'static [Opt],
    exclusive: &[&[&str]],
) -> Result<Parsed, Failure> {
    let mut parsed = Parsed::default();
    let all = || options.iter().chain(GLOBAL);
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        i += 1;
        if !arg.starts_with("--") || arg == "--" {
            parsed.positionals.push(arg.clone());
            continue;
        }
        let (name, inline) = match arg.split_once('=') {
            Some((name, value)) => (name, Some(value.to_string())),
            None => (arg.as_str(), None),
        };
        let Some(opt) = all().find(|opt| opt.name == name) else {
            let names: Vec<&str> = all().map(|opt| opt.name).collect();
            let suggestion = closest(name, &names)
                .map(|name| format!("; did you mean `{name}`?"))
                .unwrap_or_default();
            return Err(Failure::usage(
                CLI0002,
                &[
                    ("option", name),
                    ("command", command),
                    ("suggestion", &suggestion),
                ],
                &[("command", command)],
            ));
        };
        match opt.value {
            None => parsed.flags.push(opt.name),
            Some(placeholder) => {
                let value = match inline {
                    Some(value) => value,
                    None if i < args.len() && !args[i].starts_with("--") => {
                        i += 1;
                        args[i - 1].clone()
                    }
                    None => {
                        let missing = format!("a value after `{}`", opt.name);
                        let example = format!("balise {command} {} <{placeholder}>", opt.name);
                        return Err(Failure::usage(
                            CLI0003,
                            &[("command", command), ("missing", &missing)],
                            &[("example", &example)],
                        ));
                    }
                };
                if !opt.values.is_empty() && !opt.values.contains(&value.as_str()) {
                    let expected = opt
                        .values
                        .iter()
                        .map(|v| format!("`{v}`"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    return Err(Failure::usage(
                        CLI0004,
                        &[
                            ("value", &value),
                            ("option", opt.name),
                            ("expected", &expected),
                        ],
                        &[],
                    ));
                }
                parsed.values.push((opt.name, value));
            }
        }
    }
    for group in exclusive {
        let given: Vec<&&str> = group.iter().filter(|name| parsed.given(name)).collect();
        if let [first, second, ..] = given.as_slice() {
            return Err(Failure::usage(
                CLI0005,
                &[("first", first), ("second", second)],
                &[],
            ));
        }
    }
    Ok(parsed)
}

/// `--front <framework>` as help shows it.
pub fn usage(opt: &Opt) -> String {
    match opt.value {
        Some(value) => format!("{} <{value}>", opt.name),
        None => opt.name.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPTIONS: &[Opt] = &[
        Opt::value("--front", "framework", &["react", "vue"], "the front end"),
        Opt::flag("--no-front", "no front end"),
        Opt::flag("--no-example", "no example"),
    ];

    fn run(args: &[&str]) -> Result<Parsed, String> {
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        parse("new", &args, OPTIONS, &[&["--front", "--no-front"]])
            .map_err(|failure| failure.report())
    }

    #[test]
    fn options_and_positionals_are_read() {
        let parsed = run(&["shop", "--front", "react", "--no-example"]).unwrap();
        assert_eq!(parsed.positionals, ["shop"]);
        assert_eq!(parsed.value("--front"), Some("react"));
        assert!(parsed.has("--no-example"));
        assert_eq!(run(&["--front=vue"]).unwrap().value("--front"), Some("vue"));
    }

    #[test]
    fn mistakes_have_their_codes() {
        assert_eq!(
            run(&["--no-exemple"]).unwrap_err(),
            "CLI0002: `--no-exemple` is not an option of `balise new`; did you mean `--no-example`?\n  fix: Run `balise help new` to list its options.\n  more: balise explain CLI0002"
        );
        assert!(run(&["--front"]).unwrap_err().starts_with("CLI0003"));
        assert!(run(&["--front", "angular"]).unwrap_err().starts_with(
            "CLI0004: `angular` is not a valid value for `--front`; expected `react`, `vue`"
        ));
        assert!(
            run(&["--front", "react", "--no-front"])
                .unwrap_err()
                .starts_with("CLI0005: `--front` and `--no-front` cannot be used together")
        );
    }
}
