//! Configuration, loaded once at startup and passed to whatever needs it.
//!
//! Every default lives here and nowhere else: no other file may hard-code an address, the API
//! prefix or a limit (a test checks it). Values come, from highest to lowest priority, from
//! environment variables, the `[env.<name>]` section of `balise.toml`, its top level, and
//! these defaults. Reading and checking them is in [`load`].

pub mod load;

use std::time::Duration;

use crate::event::Level;
use crate::log::Format;

pub use load::{Files, Loaded, Mode};

/// The application's configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub environment: Environment,
    /// The interface the server listens on.
    pub host: String,
    pub port: u16,
    pub api: ApiConfig,
    /// The largest request body read, in bytes, for routes without files and without a
    /// `body_size` in `budgets.toml`.
    pub body_size: usize,
    /// How long a request may take, for routes without files and without a `timeout` in
    /// `budgets.toml`.
    pub timeout: Duration,
    /// How long a shutdown waits for requests and tasks in progress.
    pub shutdown_timeout: Duration,
    /// Where `File::store` puts files, and uploads wait in `.uploads` meanwhile.
    pub storage_dir: String,
    pub pagination: PaginationConfig,
    /// Whether error responses show location, fix and cause, and keep a `5xx` as it is.
    /// Follows the environment, never set directly: on in dev and test only.
    pub error_details: bool,
    /// How logs are written: text for a terminal, JSON lines for a collector.
    pub log_format: Format,
    /// Events below this level are not written.
    pub log_level: Level,
}

/// Where the API is served.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiConfig {
    /// The prefix of every route, such as `/api`.
    pub prefix: String,
    /// When set, routes are served under `{prefix}/v{version}`.
    pub version: Option<u32>,
}

/// Page sizes, for routes that do not declare theirs with `page(…)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaginationConfig {
    /// Items per page when the client does not ask.
    pub per_page: u32,
    /// The most a client can ask for; a larger `per_page` is lowered to it.
    pub max_per_page: u32,
}

/// The slowest upload a route with files is expected to receive, in bytes per second: its
/// timeout grows by the time its declared files take at this rate.
pub const UPLOAD_RATE: u64 = 1024 * 1024;

/// Where the application runs. Chosen by `BALISE_ENV`, or by how the binary was built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Environment {
    Dev,
    Test,
    Prod,
}

impl Environment {
    pub const ALL: [Environment; 3] = [Environment::Dev, Environment::Test, Environment::Prod];

    pub fn name(self) -> &'static str {
        match self {
            Environment::Dev => "dev",
            Environment::Test => "test",
            Environment::Prod => "prod",
        }
    }

    pub fn parse(name: &str) -> Option<Environment> {
        Environment::ALL.into_iter().find(|env| env.name() == name)
    }
}

impl Default for Config {
    /// The defaults of production. [`Config::defaults`] gives those of another environment.
    fn default() -> Config {
        Config::defaults(Environment::Prod)
    }
}

impl Config {
    /// Balise's defaults in this environment.
    pub fn defaults(environment: Environment) -> Config {
        Config {
            environment,
            host: "0.0.0.0".to_string(),
            port: 3000,
            api: ApiConfig {
                prefix: "/api".to_string(),
                version: None,
            },
            body_size: 2 * 1024 * 1024,
            timeout: Duration::from_secs(30),
            shutdown_timeout: Duration::from_secs(10),
            storage_dir: "storage".to_string(),
            pagination: PaginationConfig {
                per_page: 20,
                max_per_page: 100,
            },
            error_details: environment != Environment::Prod,
            log_format: match environment {
                Environment::Dev => Format::Text,
                Environment::Test | Environment::Prod => Format::Json,
            },
            log_level: Level::Info,
        }
    }

    /// The address to listen on: `0.0.0.0:3000`, or `[::1]:3000` for an IPv6 host.
    pub fn address(&self) -> String {
        match self.host.contains(':') {
            true => format!("[{}]:{}", self.host, self.port),
            false => format!("{}:{}", self.host, self.port),
        }
    }

    /// The base path of every route: `/api`, or `/api/v1` with a version.
    pub fn api_base(&self) -> String {
        let prefix = self.api.prefix.trim_end_matches('/');
        match self.api.version {
            Some(version) => format!("{prefix}/v{version}"),
            None => prefix.to_string(),
        }
    }
}

/// How much a wrong value of a setting matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A wrong value has a safe default: in production, the default is used with a warning.
    Safe,
    /// Security or data: a wrong value always stops the startup.
    Critical,
    /// Never written in `balise.toml`: from an environment variable, or a file named by
    /// `<VARIABLE>_FILE`. Its value never appears in a message.
    Secret,
}

/// One setting: its key in `balise.toml`, and how to read it.
pub struct Setting {
    /// `log.level`. Its variable is `BALISE_LOG_LEVEL`.
    pub key: &'static str,
    pub kind: Kind,
    /// What a valid value looks like, as messages say it.
    pub expected: &'static str,
    /// Other variables read for it, after `BALISE_…`: `PORT`.
    pub also: &'static [&'static str],
    /// Sets the value, or says it is not valid.
    pub apply: fn(&mut Config, &load::Raw) -> bool,
}

/// The settings of the core. Modules add theirs.
pub const SETTINGS: &[Setting] = &[
    Setting {
        key: "host",
        kind: Kind::Safe,
        expected: "an IP address or a host name, such as `0.0.0.0`",
        also: &[],
        apply: |config, raw| match raw.text() {
            Some(host) if !host.is_empty() && !host.contains(char::is_whitespace) => {
                config.host = host.to_string();
                true
            }
            _ => false,
        },
    },
    Setting {
        key: "port",
        kind: Kind::Safe,
        expected: "a port number from 1 to 65535",
        also: &["PORT"],
        apply: |config, raw| match raw.integer().and_then(|n| u16::try_from(n).ok()) {
            Some(port) if port > 0 => {
                config.port = port;
                true
            }
            _ => false,
        },
    },
    Setting {
        key: "api.prefix",
        kind: Kind::Safe,
        expected: "a path that starts with `/`, such as `/api`",
        also: &[],
        apply: |config, raw| match raw.text() {
            Some(prefix) if prefix.starts_with('/') && !prefix.contains(['?', '#', ' ']) => {
                config.api.prefix = prefix.to_string();
                true
            }
            _ => false,
        },
    },
    Setting {
        key: "api.version",
        kind: Kind::Safe,
        expected: "a whole number from 1, such as `1` for `/api/v1`",
        also: &[],
        apply: |config, raw| match raw.integer().and_then(|n| u32::try_from(n).ok()) {
            Some(version) if version > 0 => {
                config.api.version = Some(version);
                true
            }
            _ => false,
        },
    },
    Setting {
        key: "limits.body_size",
        kind: Kind::Safe,
        expected: "a size such as `2MB` or `512KB`",
        also: &[],
        apply: |config, raw| match raw.text().and_then(crate::text::parse_size) {
            Some(size) if size > 0 => {
                config.body_size = size;
                true
            }
            _ => false,
        },
    },
    Setting {
        key: "limits.timeout",
        kind: Kind::Safe,
        expected: "a duration such as `30s`, `2m` or `500ms`",
        also: &[],
        apply: |config, raw| match raw.text().and_then(crate::text::parse_duration) {
            Some(timeout) if !timeout.is_zero() => {
                config.timeout = timeout;
                true
            }
            _ => false,
        },
    },
    Setting {
        key: "shutdown.timeout",
        kind: Kind::Safe,
        expected: "a duration such as `10s` or `500ms`",
        also: &[],
        apply: |config, raw| match raw.text().and_then(crate::text::parse_duration) {
            Some(timeout) => {
                config.shutdown_timeout = timeout;
                true
            }
            _ => false,
        },
    },
    Setting {
        key: "storage.dir",
        kind: Kind::Critical,
        expected: "a folder path, such as `storage` or `/data/storage`",
        also: &[],
        apply: |config, raw| match raw.text() {
            Some(dir) if !dir.trim().is_empty() => {
                config.storage_dir = dir.to_string();
                true
            }
            _ => false,
        },
    },
    Setting {
        key: "pagination.per_page",
        kind: Kind::Safe,
        expected: "a whole number from 1, such as `20`",
        also: &[],
        apply: |config, raw| match raw.integer().and_then(|n| u32::try_from(n).ok()) {
            Some(n) if n > 0 => {
                config.pagination.per_page = n;
                true
            }
            _ => false,
        },
    },
    Setting {
        key: "pagination.max_per_page",
        kind: Kind::Safe,
        expected: "a whole number from 1, such as `100`",
        also: &[],
        apply: |config, raw| match raw.integer().and_then(|n| u32::try_from(n).ok()) {
            Some(n) if n > 0 => {
                config.pagination.max_per_page = n;
                true
            }
            _ => false,
        },
    },
    Setting {
        key: "log.format",
        kind: Kind::Safe,
        expected: "`text` or `json`",
        also: &[],
        apply: |config, raw| {
            config.log_format = match raw.text() {
                Some("text") => Format::Text,
                Some("json") => Format::Json,
                _ => return false,
            };
            true
        },
    },
    Setting {
        key: "log.level",
        kind: Kind::Safe,
        expected: "`trace`, `debug`, `info`, `warn` or `error`",
        also: &[],
        apply: |config, raw| {
            config.log_level = match raw.text() {
                Some("trace") => Level::Trace,
                Some("debug") => Level::Debug,
                Some("info") => Level::Info,
                Some("warn") => Level::Warn,
                Some("error") => Level::Error,
                _ => return false,
            };
            true
        },
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_api_base_includes_the_version_when_there_is_one() {
        let mut config = Config::default();
        assert_eq!(config.api_base(), "/api");
        config.api.version = Some(2);
        assert_eq!(config.api_base(), "/api/v2");
        config.api.prefix = "/backend/".to_string();
        assert_eq!(config.api_base(), "/backend/v2");
    }

    #[test]
    fn defaults_follow_the_environment() {
        let dev = Config::defaults(Environment::Dev);
        let prod = Config::defaults(Environment::Prod);
        assert!(dev.error_details && !prod.error_details);
        assert_eq!(
            (dev.log_format, prod.log_format),
            (Format::Text, Format::Json)
        );
        assert_eq!(prod.address(), "0.0.0.0:3000");
    }
}
