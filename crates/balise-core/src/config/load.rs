//! Reading the configuration: `balise.toml`, embedded in the binary, then environment
//! variables, checked against the settings with the rules of the environment.
//!
//! | Problem | Dev, test | Prod | `config check` |
//! | --- | --- | --- | --- |
//! | unknown key or variable | error | warning | error |
//! | invalid value with a safe default | error | warning, default kept | error |
//! | invalid security or data setting, secret problem | error | error | error |
//!
//! Every problem is reported at once, each naming where the value comes from.

use std::fmt;

use toml::de::{DeTable, DeValue};

use super::{Config, Environment, Kind, SETTINGS, Setting};
use crate::error_codes::{
    CFG0001, CFG0002, CFG0003, CFG0004, CFG0005, CFG0006, CFG0007, CFG0012, ErrorCode,
};
use crate::event::{Event, Level};

/// The files `balise::run!()` embeds in the binary.
#[derive(Debug, Clone, Copy)]
pub struct Files {
    /// The text of `balise.toml`.
    pub config: &'static str,
    /// The text of `budgets.toml`.
    pub budgets: &'static str,
    /// The application's crate, whose frames a backtrace shows: `shop`.
    pub crate_name: &'static str,
    /// The folder of its `Cargo.toml`, removed from the paths of backtraces.
    pub root: &'static str,
}

/// Why the configuration is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// To start the app: in production, what has a safe default only warns.
    Start,
    /// `config check`: the strictest rules, whatever the environment.
    Check,
}

/// A configuration that can be used, with the warnings to log once logging is set up.
#[derive(Debug)]
pub struct Loaded {
    pub config: Config,
    pub warnings: Vec<Event>,
}

/// A value as written, before its setting reads it.
#[derive(Debug, Clone, PartialEq)]
pub enum Raw {
    Text(String),
    Integer(i64),
    Float(f64),
    Bool(bool),
    /// Another TOML type, by name: `an array`.
    Other(&'static str),
}

impl Raw {
    /// Text, as written in `balise.toml` or in a variable.
    pub fn text(&self) -> Option<&str> {
        match self {
            Raw::Text(text) => Some(text),
            _ => None,
        }
    }

    /// A whole number: a TOML integer, or text that is one (variables are text).
    pub fn integer(&self) -> Option<i64> {
        match self {
            Raw::Integer(n) => Some(*n),
            Raw::Text(text) => text.trim().parse().ok(),
            _ => None,
        }
    }

    /// How a message shows it.
    fn shown(&self) -> String {
        match self {
            Raw::Text(text) => format!("`{text}`"),
            Raw::Integer(n) => format!("`{n}`"),
            Raw::Float(n) => format!("`{n}`"),
            Raw::Bool(b) => format!("`{b}`"),
            Raw::Other(kind) => kind.to_string(),
        }
    }
}

/// Where a value comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Source {
    /// A line of `balise.toml`, in a section `[env.<name>]` or at the top level.
    File {
        line: usize,
        section: Option<String>,
    },
    /// An environment variable.
    Variable(String),
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Source::File {
                line,
                section: None,
            } => write!(f, "balise.toml:{line}"),
            Source::File {
                line,
                section: Some(section),
            } => write!(f, "balise.toml:{line}, in [env.{section}]"),
            Source::Variable(name) => write!(f, "variable {name}"),
        }
    }
}

/// One value found for a key.
struct Entry {
    key: String,
    raw: Raw,
    source: Source,
}

/// The environment to run in: `BALISE_ENV`, or what the binary was built for.
pub fn environment(
    build: Environment,
    variables: &[(String, String)],
) -> Result<Environment, Box<Event>> {
    let Some((_, name)) = variables.iter().find(|(name, _)| name == "BALISE_ENV") else {
        return Ok(build);
    };
    Environment::parse(name).ok_or_else(|| {
        Box::new(unknown_environment(
            name,
            &Source::Variable("BALISE_ENV".into()),
        ))
    })
}

/// Reads the configuration for `environment`.
///
/// `build` is the environment the binary was built for: a `dev` build never runs in
/// production. `variables` are the process's environment variables.
pub fn load(
    files: &Files,
    environment: Environment,
    build: Environment,
    variables: &[(String, String)],
    mode: Mode,
) -> Result<Loaded, Vec<Event>> {
    load_with(SETTINGS, files, environment, build, variables, mode)
}

/// [`load`] with a given list of settings, so that tests can declare their own.
pub fn load_with(
    settings: &[Setting],
    files: &Files,
    environment: Environment,
    build: Environment,
    variables: &[(String, String)],
    mode: Mode,
) -> Result<Loaded, Vec<Event>> {
    let lenient = mode == Mode::Start && environment == Environment::Prod;
    let mut problems = Problems::default();
    if mode == Mode::Start && build == Environment::Dev && environment == Environment::Prod {
        problems.error(Event::coded(Level::Error, CFG0004, &[]));
    }

    let (top, sections) = match read_file(files.config) {
        Ok(read) => read,
        // `balise::run!()` already checked the syntax at compile time.
        Err(event) => return Err(vec![event]),
    };
    let mut entries: Vec<Entry> = top
        .into_iter()
        .filter(|entry| {
            check_entry(settings, entry, &mut problems, lenient) && !is_secret(settings, &entry.key)
        })
        .collect();
    for (name, line, section_entries) in sections {
        let source = Source::File {
            line,
            section: None,
        };
        if Environment::parse(&name).is_none() {
            problems.add(unknown_environment(&name, &source), lenient);
            continue;
        }
        // Every section is checked; only the active one applies.
        for entry in section_entries {
            let usable = check_entry(settings, &entry, &mut problems, lenient)
                && !is_secret(settings, &entry.key);
            if usable && name == environment.name() {
                entries.retain(|e| e.key != entry.key);
                entries.push(entry);
            }
        }
    }
    entries.extend(read_variables(settings, variables, &mut problems, lenient));

    let mut config = Config::defaults(environment);
    for entry in &entries {
        let Some(setting) = settings.iter().find(|s| s.key == entry.key) else {
            continue;
        };
        if !(setting.apply)(&mut config, &entry.raw) {
            let shown = match setting.kind {
                Kind::Secret => format!("`{}`", crate::log::REDACTED),
                _ => entry.raw.shown(),
            };
            let event = Event::coded(
                Level::Error,
                CFG0002,
                &[
                    ("key", setting.key),
                    ("value", &shown),
                    ("source", &entry.source.to_string()),
                    ("expected", setting.expected),
                ],
            );
            problems.add(event, lenient && setting.kind == Kind::Safe);
        }
    }

    // Settings that depend on each other, once each is valid on its own.
    let pagination = config.pagination;
    if pagination.per_page > pagination.max_per_page {
        let event = Event::coded(
            Level::Error,
            CFG0012,
            &[
                ("per_page", &pagination.per_page.to_string()),
                ("max", &pagination.max_per_page.to_string()),
            ],
        );
        problems.add(event, lenient);
        config.pagination.per_page = pagination.max_per_page;
    }

    match problems.errors.is_empty() {
        true => Ok(Loaded {
            config,
            warnings: problems.warnings,
        }),
        false => Err(problems.errors),
    }
}

/// Errors stop the startup; warnings are logged once it has started.
#[derive(Default)]
struct Problems {
    errors: Vec<Event>,
    warnings: Vec<Event>,
}

impl Problems {
    /// A problem that only warns when `lenient`.
    fn add(&mut self, mut event: Event, lenient: bool) {
        match lenient {
            true => {
                event.level = Level::Warn;
                self.warnings.push(event);
            }
            false => self.errors.push(event),
        }
    }

    fn error(&mut self, event: Event) {
        self.errors.push(event);
    }
}

/// Checks a key of `balise.toml`: whether it exists, and that a secret is not written there.
fn check_entry(
    settings: &[Setting],
    entry: &Entry,
    problems: &mut Problems,
    lenient: bool,
) -> bool {
    let source = entry.source.to_string();
    match settings.iter().find(|s| s.key == entry.key) {
        None => {
            let names: Vec<&str> = settings.iter().map(|s| s.key).collect();
            let event = coded_with_suggestion(
                CFG0001,
                &[("key", &entry.key), ("source", &source)],
                &entry.key,
                &names,
            );
            problems.add(event, lenient);
            false
        }
        Some(setting) if setting.kind == Kind::Secret => {
            let variable = variable_name(setting.key);
            problems.error(Event::coded(
                Level::Error,
                CFG0005,
                &[
                    ("key", setting.key),
                    ("source", &source),
                    ("variable", &variable),
                ],
            ));
            true
        }
        Some(_) => true,
    }
}

fn is_secret(settings: &[Setting], key: &str) -> bool {
    settings
        .iter()
        .any(|s| s.key == key && s.kind == Kind::Secret)
}

/// Variables of Balise's tools, not settings: the launcher's (`BALISE_CLI`, `BALISE_HOME`),
/// and those `balise dev` gives compose (`BALISE_VERSION`, `BALISE_DEV_…`), which a tool
/// loading `.env` may put in the app's environment.
pub fn is_tooling(name: &str) -> bool {
    matches!(name, "BALISE_CLI" | "BALISE_HOME" | "BALISE_VERSION")
        || name.starts_with("BALISE_DEV_")
}

/// The variable of a setting: `log.level` gives `BALISE_LOG_LEVEL`.
pub fn variable_name(key: &str) -> String {
    format!("BALISE_{}", key.replace('.', "_").to_ascii_uppercase())
}

/// The settings given by environment variables. Unknown `BALISE_` variables are reported
/// like unknown keys.
fn read_variables(
    settings: &[Setting],
    variables: &[(String, String)],
    problems: &mut Problems,
    lenient: bool,
) -> Vec<Entry> {
    let get = |name: &str| {
        variables
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.clone())
    };
    let mut entries = Vec::new();
    let mut known: Vec<String> = vec!["BALISE_ENV".to_string()];
    for setting in settings {
        let names: Vec<String> = std::iter::once(variable_name(setting.key))
            .chain(setting.also.iter().map(|name| name.to_string()))
            .collect();
        known.extend(names.iter().cloned());
        if setting.kind == Kind::Secret {
            known.extend(names.iter().map(|name| format!("{name}_FILE")));
            if let Some(entry) = read_secret(setting, &names, &get, problems) {
                entries.push(entry);
            }
            continue;
        }
        if let Some(name) = names.iter().find(|name| get(name).is_some()) {
            entries.push(Entry {
                key: setting.key.to_string(),
                raw: Raw::Text(get(name).unwrap_or_default()),
                source: Source::Variable(name.clone()),
            });
        }
    }
    for (name, _) in variables {
        if name.starts_with("BALISE_") && !known.contains(name) && !is_tooling(name) {
            let names: Vec<&str> = known.iter().map(String::as_str).collect();
            let source = Source::Variable(name.clone()).to_string();
            let event =
                coded_with_suggestion(CFG0001, &[("key", name), ("source", &source)], name, &names);
            problems.add(event, lenient);
        }
    }
    entries
}

/// A secret, from its variable or from the file its `_FILE` variable names.
fn read_secret(
    setting: &Setting,
    names: &[String],
    get: &dyn Fn(&str) -> Option<String>,
    problems: &mut Problems,
) -> Option<Entry> {
    for name in names {
        let file_variable = format!("{name}_FILE");
        match (get(name), get(&file_variable)) {
            (Some(_), Some(_)) => {
                problems.error(Event::coded(Level::Error, CFG0007, &[("variable", name)]));
                return None;
            }
            (Some(value), None) => {
                return Some(Entry {
                    key: setting.key.to_string(),
                    raw: Raw::Text(value),
                    source: Source::Variable(name.clone()),
                });
            }
            (None, Some(path)) => {
                return match std::fs::read_to_string(&path) {
                    // A file written by hand or by an orchestrator usually ends with a newline.
                    Ok(value) => Some(Entry {
                        key: setting.key.to_string(),
                        raw: Raw::Text(value.trim_end_matches(['\n', '\r']).to_string()),
                        source: Source::Variable(file_variable),
                    }),
                    Err(error) => {
                        problems.error(Event::coded(
                            Level::Error,
                            CFG0006,
                            &[
                                ("variable", &file_variable),
                                ("path", &path),
                                ("cause", &error.to_string()),
                                ("secret", name),
                            ],
                        ));
                        None
                    }
                };
            }
            (None, None) => {}
        }
    }
    None
}

/// The entries of `balise.toml` outside `[env.…]`, and each `[env.<name>]` section with its
/// line and its entries.
type Read = (Vec<Entry>, Vec<(String, usize, Vec<Entry>)>);

#[allow(clippy::result_large_err)] // startup only: the size costs nothing
fn read_file(text: &str) -> Result<Read, Event> {
    let table = DeTable::parse(text).map_err(|error| {
        let line = error.span().map_or(0, |span| line_of(text, span.start));
        Event::new(
            Level::Error,
            format!("balise.toml:{line}: {}", error.message()),
        )
    })?;
    let mut top = Vec::new();
    let mut sections = Vec::new();
    for (key, value) in table.get_ref() {
        let key_text = key.get_ref().as_ref();
        if key_text == "env"
            && let DeValue::Table(envs) = value.get_ref()
        {
            for (name, section) in envs {
                let line = line_of(text, name.span().start);
                let name = name.get_ref().to_string();
                let mut entries = Vec::new();
                let source = |line| Source::File {
                    line,
                    section: Some(name.clone()),
                };
                flatten(text, "", section, &source, &mut entries);
                sections.push((name.clone(), line, entries));
            }
            continue;
        }
        let source = |line| Source::File {
            line,
            section: None,
        };
        flatten(text, key_text, value, &source, &mut top);
    }
    Ok((top, sections))
}

/// Dotted keys for every value: `[log] level = "debug"` gives `log.level`.
fn flatten(
    text: &str,
    prefix: &str,
    value: &toml::Spanned<DeValue<'_>>,
    source: &dyn Fn(usize) -> Source,
    entries: &mut Vec<Entry>,
) {
    let raw = match value.get_ref() {
        DeValue::Table(table) => {
            for (key, value) in table {
                let key = match prefix {
                    "" => key.get_ref().to_string(),
                    _ => format!("{prefix}.{}", key.get_ref()),
                };
                flatten(text, &key, value, source, entries);
            }
            return;
        }
        DeValue::String(s) => Raw::Text(s.to_string()),
        DeValue::Integer(n) => {
            integer(n.as_str(), n.radix()).map_or(Raw::Other("a number too large"), Raw::Integer)
        }
        DeValue::Float(n) => n
            .as_str()
            .replace('_', "")
            .parse()
            .map_or(Raw::Other("a number"), Raw::Float),
        DeValue::Boolean(b) => Raw::Bool(*b),
        DeValue::Datetime(_) => Raw::Other("a date"),
        DeValue::Array(_) => Raw::Other("a list"),
    };
    entries.push(Entry {
        key: prefix.to_string(),
        raw,
        source: source(line_of(text, value.span().start)),
    });
}

/// A TOML integer: `1_000`, `0x1F`, `0o17`, `0b101`.
fn integer(text: &str, radix: u32) -> Option<i64> {
    let digits = text.replace('_', "");
    match radix {
        10 => digits.parse().ok(),
        _ => i64::from_str_radix(digits.get(2..)?, radix).ok(),
    }
}

/// The line, from 1, of a byte offset.
pub(crate) fn line_of(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())].matches('\n').count() + 1
}

fn unknown_environment(name: &str, source: &Source) -> Event {
    let names: Vec<&str> = Environment::ALL.iter().map(|env| env.name()).collect();
    coded_with_suggestion(
        CFG0003,
        &[("name", name), ("source", &source.to_string())],
        name,
        &names,
    )
}

/// An error whose message ends with the closest valid name, when one is close enough.
pub(crate) fn coded_with_suggestion(
    code: &'static ErrorCode,
    args: &[(&str, &str)],
    name: &str,
    known: &[&str],
) -> Event {
    let suggestion = crate::text::closest(name, known)
        .map(|closest| format!("; did you mean `{closest}`?"))
        .unwrap_or_default();
    let mut args = args.to_vec();
    args.push(("suggestion", &suggestion));
    Event::coded(Level::Error, code, &args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::Format;

    fn files(text: &'static str) -> Files {
        Files {
            config: text,
            budgets: "",
            crate_name: "app",
            root: "/app",
        }
    }

    fn vars(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(n, v)| (n.to_string(), v.to_string()))
            .collect()
    }

    fn load_in(
        environment: Environment,
        text: &'static str,
        variables: &[(&str, &str)],
    ) -> Result<Loaded, Vec<Event>> {
        load(
            &files(text),
            environment,
            Environment::Prod,
            &vars(variables),
            Mode::Start,
        )
    }

    fn messages(events: &[Event]) -> Vec<String> {
        events
            .iter()
            .map(|e| format!("{} {}", e.code.map_or("", |c| c.code()), e.message))
            .collect()
    }

    const FILE: &str = r#"
port = 4000

[log]
level = "debug"

[env.dev]
log.format = "json"
api = { prefix = "/backend", version = 2 }
"#;

    #[test]
    fn variables_override_the_section_which_overrides_the_file() {
        let loaded = load_in(Environment::Dev, FILE, &[("PORT", "5000")]).unwrap();
        let config = loaded.config;
        assert_eq!(config.port, 5000);
        assert_eq!(config.log_level, Level::Debug);
        assert_eq!(config.log_format, Format::Json);
        assert_eq!(config.api_base(), "/backend/v2");

        let prod = load_in(Environment::Prod, FILE, &[]).unwrap().config;
        assert_eq!((prod.port, prod.api_base()), (4000, "/api".to_string()));
        assert!(!prod.error_details);
    }

    #[test]
    fn mistakes_stop_dev_and_only_warn_in_prod() {
        let text = "prot = 4000\nlog.level = \"verbose\"\n[env.production]\nport = 1\n";
        let expected = [
            "CFG0001 Unknown setting `prot` (balise.toml:1); did you mean `port`?",
            "CFG0003 Unknown environment `production` (balise.toml:3); did you mean `prod`?",
            "CFG0002 `log.level` is `verbose` (balise.toml:2); expected `trace`, `debug`, `info`, `warn` or `error`",
        ];
        let errors = load_in(Environment::Dev, text, &[]).unwrap_err();
        assert_eq!(messages(&errors), expected);

        let loaded = load_in(Environment::Prod, text, &[]).unwrap();
        assert_eq!(messages(&loaded.warnings), expected);
        assert!(loaded.warnings.iter().all(|w| w.level == Level::Warn));
        assert_eq!(loaded.config.log_level, Level::Info);

        let check = load(
            &files(text),
            Environment::Prod,
            Environment::Prod,
            &[],
            Mode::Check,
        );
        assert_eq!(messages(&check.unwrap_err()), expected);
    }

    #[test]
    fn variables_are_named_in_messages_and_unknown_ones_are_reported() {
        let errors = load_in(
            Environment::Dev,
            "",
            &[
                ("BALISE_PORT", "abc"),
                ("BALISE_LOG_LEVL", "debug"),
                // The tools' own: not settings, never reported.
                ("BALISE_CLI", "/bin/balise-cli"),
                ("BALISE_DEV_PORT", "3000"),
            ],
        )
        .unwrap_err();
        assert_eq!(
            messages(&errors),
            [
                "CFG0001 Unknown setting `BALISE_LOG_LEVL` (variable BALISE_LOG_LEVL); did you mean `BALISE_LOG_LEVEL`?",
                "CFG0002 `port` is `abc` (variable BALISE_PORT); expected a port number from 1 to 65535",
            ]
        );
    }

    #[test]
    fn a_dev_build_never_runs_in_production() {
        let errors = load(
            &files(""),
            Environment::Prod,
            Environment::Dev,
            &[],
            Mode::Start,
        )
        .unwrap_err();
        assert_eq!(errors[0].code, Some(CFG0004));
        assert!(environment(Environment::Dev, &vars(&[("BALISE_ENV", "staging")])).is_err());
        assert_eq!(
            environment(Environment::Dev, &vars(&[("BALISE_ENV", "test")])).unwrap(),
            Environment::Test
        );
    }

    const SECRET: Setting = Setting {
        key: "database_url",
        kind: Kind::Secret,
        expected: "a URL",
        also: &["DATABASE_URL"],
        apply: |_, raw| raw.text().is_some_and(|url| url.contains("://")),
        value: |_| None,
    };

    fn load_secret(text: &'static str, variables: &[(&str, &str)]) -> Result<Loaded, Vec<Event>> {
        load_with(
            &[SECRET],
            &files(text),
            Environment::Prod,
            Environment::Prod,
            &vars(variables),
            Mode::Start,
        )
    }

    #[test]
    fn secrets_come_from_variables_or_files_and_are_never_shown() {
        assert!(load_secret("", &[("DATABASE_URL", "postgres://db")]).is_ok());

        let errors = load_secret("database_url = \"postgres://u:pw@db\"", &[]).unwrap_err();
        assert_eq!(errors[0].code, Some(CFG0005));
        assert!(!errors[0].message.contains("pw"), "{}", errors[0].message);

        let both = load_secret(
            "",
            &[("DATABASE_URL", "a://b"), ("DATABASE_URL_FILE", "/x")],
        );
        assert_eq!(both.unwrap_err()[0].code, Some(CFG0007));

        let missing = load_secret("", &[("DATABASE_URL_FILE", "/nonexistent/secret")]);
        assert_eq!(missing.unwrap_err()[0].code, Some(CFG0006));

        let invalid = load_secret("", &[("DATABASE_URL", "hunter2")]).unwrap_err();
        assert!(
            invalid[0].message.contains("`***`"),
            "{}",
            invalid[0].message
        );
    }
}
