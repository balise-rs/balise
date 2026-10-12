//! Configuration, loaded once at startup and passed to whatever needs it.
//!
//! Every default lives here and nowhere else: no other file may hard-code an address or the
//! API prefix (a test checks it).

use crate::event::{Event, Level};
use crate::log::Format;

/// The application's configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// The address the server listens on.
    pub address: String,
    pub api: ApiConfig,
    /// The largest request body read, in bytes, for routes without files and without a
    /// `body_size` in `budgets.toml`.
    pub body_size: usize,
    /// Whether error responses show location, fix and cause, and keep a `5xx` as it is. On in
    /// dev only: they can reveal infrastructure details.
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

impl Default for Config {
    fn default() -> Config {
        Config {
            address: "0.0.0.0:3000".to_string(),
            api: ApiConfig {
                prefix: "/api".to_string(),
                version: None,
            },
            body_size: 2 * 1024 * 1024,
            error_details: false,
            log_format: Format::Text,
            log_level: Level::Info,
        }
    }
}

impl Config {
    /// Loads the configuration. For now, the defaults; `balise.toml` and the environment come
    /// later, behind the same function.
    #[allow(clippy::result_large_err)] // startup only: the size costs nothing
    pub fn load() -> Result<Config, Event> {
        Ok(Config::default())
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
}
