//! The project template, embedded at build time, and how it is rendered.
//!
//! Files ending in `.tpl` are rendered: `{{name}}` takes a value, `{{#if flag}}…{{/if}}` keeps
//! its content when the flag is set. Other files are copied as they are. Names that would
//! change how the template is packaged are written differently: `gitignore` is
//! `.gitignore`, `dockerignore` is `.dockerignore`, `claude/` is `.claude/`, and
//! `Cargo.toml.tpl` and `Cargo.lock.tpl` are not a package of their own.
//!
//! `Cargo.lock.tpl` pins every third-party crate as Balise is tested; a test keeps it equal
//! to the workspace's lockfile.

mod files {
    include!(concat!(env!("OUT_DIR"), "/template.rs"));
}

/// The choices that change what is written.
pub struct Choices<'a> {
    pub name: &'a str,
    /// What `[dependencies]` says for `balise`: `"=0.1.0"` or `{ path = "…" }`.
    pub dependency: &'a str,
    /// The local checkout of Balise (`--balise-path`): the dev container mounts it and
    /// builds its CLI from it, instead of installing a published one.
    pub checkout: Option<&'a str>,
    pub example: bool,
}

/// Every file of the project, with its path and content.
pub fn render(choices: &Choices) -> Vec<(String, Vec<u8>)> {
    let rust = rust_version();
    let values = [
        ("name", choices.name),
        ("balise_dependency", choices.dependency),
        ("balise_path", choices.checkout.unwrap_or_default()),
        ("rust", rust),
    ];
    let flags = [
        ("example", choices.example),
        ("local", choices.checkout.is_some()),
        ("published", choices.checkout.is_none()),
    ];
    files::FILES
        .iter()
        .filter(|(path, _)| choices.example || !path.starts_with("src/orders/"))
        .map(|(path, content)| {
            let (path, content) = match path.strip_suffix(".tpl") {
                Some(path) => {
                    let text = String::from_utf8_lossy(content);
                    (path, fill(&text, &values, &flags).into_bytes())
                }
                None => (*path, content.to_vec()),
            };
            let path = match path {
                "gitignore" => ".gitignore",
                "dockerignore" => ".dockerignore",
                "claude/settings.json" => ".claude/settings.json",
                path => path,
            };
            (path.to_string(), content)
        })
        .collect()
}

/// The Rust version `rust-toolchain.toml` pins, which the dev image starts from.
fn rust_version() -> &'static str {
    let (_, content) = files::FILES
        .iter()
        .find(|(path, _)| *path == "rust-toolchain.toml")
        .expect("the template pins a toolchain");
    let text = std::str::from_utf8(content).expect("rust-toolchain.toml is UTF-8");
    text.lines()
        .find_map(|line| line.trim().strip_prefix("channel = "))
        .map(|value| value.trim_matches('"'))
        .expect("rust-toolchain.toml has a channel")
}

/// Fills `{{value}}` and keeps or removes `{{#if flag}}…{{/if}}`.
fn fill(text: &str, values: &[(&str, &str)], flags: &[(&str, bool)]) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{#if ") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 6..];
        let Some(close) = after.find("}}") else { break };
        let flag = after[..close].trim();
        let body_start = &after[close + 2..];
        let Some(end) = body_start.find("{{/if}}") else {
            break;
        };
        if flags.iter().any(|(name, on)| *name == flag && *on) {
            out.push_str(&body_start[..end]);
        }
        rest = &body_start[end + 7..];
    }
    out.push_str(rest);
    for (name, value) in values {
        out = out.replace(&format!("{{{{{name}}}}}"), value);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_and_conditions_are_filled() {
        let text = "a {{name}}\n{{#if example}}mod orders;\n{{/if}}end";
        assert_eq!(
            fill(text, &[("name", "shop")], &[("example", true)]),
            "a shop\nmod orders;\nend"
        );
        assert_eq!(
            fill(text, &[("name", "shop")], &[("example", false)]),
            "a shop\nend"
        );
    }

    #[test]
    fn the_example_can_be_left_out() {
        let choices = |example| Choices {
            name: "shop",
            dependency: "\"=0.1.0\"",
            checkout: None,
            example,
        };
        let with: Vec<String> = render(&choices(true)).into_iter().map(|(p, _)| p).collect();
        assert!(with.contains(&"src/orders/mod.rs".to_string()));
        assert!(with.contains(&".gitignore".to_string()));
        assert!(with.contains(&"Cargo.toml".to_string()));
        let without: Vec<String> = render(&choices(false))
            .into_iter()
            .map(|(p, _)| p)
            .collect();
        assert!(!without.iter().any(|p| p.starts_with("src/orders")));
    }

    #[test]
    fn the_dev_container_follows_the_choices() {
        let file = |checkout, name: &str| {
            let choices = Choices {
                name: "shop",
                dependency: "\"=0.1.0\"",
                checkout,
                example: false,
            };
            let (_, content) = render(&choices)
                .into_iter()
                .find(|(path, _)| path == name)
                .unwrap();
            String::from_utf8(content).unwrap()
        };
        let image = file(None, "Dockerfile");
        assert!(image.contains(&format!("FROM rust:{}-slim AS dev", rust_version())));
        assert!(image.contains("cargo install balise-cli --version \"=${BALISE_VERSION}\""));
        assert!(file(None, "compose.yaml").contains("BALISE_VERSION: ${BALISE_VERSION"));
        assert!(file(None, ".dockerignore").contains("!rust-toolchain.toml"));

        let image = file(Some("/src/balise"), "Dockerfile");
        assert!(!image.contains("BALISE_VERSION"));
        assert!(image.contains("--path /src/balise/crates/balise-cli"));
        let compose = file(Some("/src/balise"), "compose.yaml");
        assert!(compose.contains("- /src/balise:/src/balise:ro"));
        assert!(!compose.contains("BALISE_VERSION"));
    }

    fn agents(example: bool) -> String {
        let choices = Choices {
            name: "shop",
            dependency: "\"=0.1.0\"",
            checkout: None,
            example,
        };
        let (_, content) = render(&choices)
            .into_iter()
            .find(|(path, _)| path == "AGENTS.md")
            .unwrap();
        String::from_utf8(content).unwrap()
    }

    /// The commands a text names: `balise check`, `balise dev status`…
    fn commands_named(text: &str) -> Vec<String> {
        text.split("`balise ")
            .skip(1)
            .map(|rest| {
                rest.chars()
                    .take_while(|c| c.is_ascii_lowercase())
                    .collect::<String>()
            })
            .collect()
    }

    /// `AGENTS.md` names only commands that exist, and every command for agents: a new
    /// command cannot be forgotten there.
    #[test]
    fn agents_md_follows_the_commands() {
        for example in [true, false] {
            let text = agents(example);
            let named = commands_named(&text);
            for name in &named {
                assert!(
                    crate::COMMANDS.iter().any(|command| command.name == name),
                    "AGENTS.md names `balise {name}`, which does not exist"
                );
            }
            for command in crate::COMMANDS.iter().filter(|c| c.for_agents) {
                assert!(
                    named.iter().any(|name| name == command.name),
                    "AGENTS.md does not say when to use `balise {}`",
                    command.name
                );
            }
        }
    }

    /// Every path `clippy.toml` disallows is a rule of `AGENTS.md`.
    #[test]
    fn agents_md_states_what_clippy_disallows() {
        let (_, clippy) = files::FILES
            .iter()
            .find(|(path, _)| *path == "clippy.toml")
            .unwrap();
        let clippy = std::str::from_utf8(clippy).unwrap();
        let text = agents(false);
        for path in clippy.split("path = \"").skip(1) {
            let path = path.split('"').next().unwrap();
            let name = path.rsplit("::").next().unwrap();
            assert!(text.contains(name), "AGENTS.md does not mention `{path}`");
        }
    }

    /// The Claude Code settings allow only commands that exist, and its hook is `check`'s.
    #[test]
    fn claude_settings_name_real_commands() {
        let choices = Choices {
            name: "shop",
            dependency: "\"=0.1.0\"",
            checkout: None,
            example: false,
        };
        let (_, content) = render(&choices)
            .into_iter()
            .find(|(path, _)| path == ".claude/settings.json")
            .unwrap();
        let text = String::from_utf8(content).unwrap();
        balise_core::json::parse(text.as_bytes()).unwrap();
        for allowed in text.split("Bash(balise ").skip(1) {
            let name: String = allowed
                .chars()
                .take_while(|c| c.is_ascii_lowercase())
                .collect();
            assert!(
                crate::COMMANDS.iter().any(|command| command.name == name),
                "`balise {name}` is allowed, and does not exist"
            );
        }
        let check = crate::COMMANDS.iter().find(|c| c.name == "check").unwrap();
        assert!(text.contains("\"command\": \"balise check --hook\""));
        assert!(check.options.iter().any(|option| option.name == "--hook"));
    }
}
