//! Per-route limits: body size, timeout and page sizes, deduced from each route's
//! declaration, the configuration, and the overrides of `budgets.toml`.
//!
//! | Limit | Route without files | Route with files | Override |
//! | --- | --- | --- | --- |
//! | body size | `limits.body_size` | the files' sizes, plus `limits.body_size` for the other fields | `body_size` |
//! | timeout | `limits.timeout` | plus the files' sizes at [`UPLOAD_RATE`] | `timeout` |
//! | page sizes | `pagination.*` | | `page(…)` on the route |
//!
//! `budgets.toml` is embedded in the binary by `balise::run!()`; it holds one section per
//! handler whose budgets differ from the defaults, and every problem in it stops the startup.

use std::collections::HashMap;
use std::time::Duration;

use toml::de::{DeTable, DeValue};

use crate::config::load::{coded_with_suggestion, line_of};
use crate::config::{Config, UPLOAD_RATE};
use crate::error_codes::{CFG0009, CFG0010, CFG0011};
use crate::event::{Event, Level};
use crate::page::PageLimits;
use crate::registry::Route;
use crate::text::{format_size, parse_duration, parse_size};

/// The limits of one route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// The largest body, in bytes.
    pub body_size: usize,
    /// The largest text of a multipart body, its files aside.
    pub text_size: usize,
    /// How long the request may take, reading its body included.
    pub timeout: Duration,
    /// For a route that reads `PageParams`.
    pub page: Option<PageLimits>,
}

impl Limits {
    /// The limits of a route that declares nothing.
    pub fn defaults(config: &Config) -> Limits {
        Limits {
            body_size: config.body_size,
            text_size: config.body_size,
            timeout: config.timeout,
            page: None,
        }
    }
}

impl Default for Limits {
    fn default() -> Limits {
        Limits::defaults(&Config::default())
    }
}

/// The limits of every route.
#[derive(Debug, Clone)]
pub struct RouteLimits {
    /// By the route's address: routes are statics.
    routes: HashMap<usize, Limits>,
    defaults: Limits,
}

impl RouteLimits {
    /// Combines each route's declaration with the configuration and `budgets.toml`.
    pub fn resolve(
        routes: &'static [Route],
        config: &Config,
        budgets: &str,
    ) -> Result<RouteLimits, Vec<Event>> {
        let names: Vec<String> = routes.iter().map(Route::handler_name).collect();
        let (budgets, mut problems) = read_budgets(budgets, &names);
        let mut resolved = HashMap::new();
        for route in routes {
            let budget = budgets.get(&route.handler_name());
            match limits_of(route, config, budget) {
                Ok(limits) => {
                    resolved.insert(address(route), limits);
                }
                Err(event) => problems.push(event),
            }
        }
        match problems.is_empty() {
            true => Ok(RouteLimits {
                routes: resolved,
                defaults: Limits::defaults(config),
            }),
            false => Err(problems),
        }
    }

    /// The limits of a route.
    pub fn get(&self, route: &Route) -> Limits {
        self.routes
            .get(&address(route))
            .copied()
            .unwrap_or(self.defaults)
    }
}

fn address(route: &Route) -> usize {
    route as *const Route as usize
}

/// One handler's section of `budgets.toml`.
#[derive(Debug, Default)]
struct Budget {
    body_size: Option<usize>,
    timeout: Option<Duration>,
    /// Where the section starts, for messages.
    line: usize,
}

#[allow(clippy::result_large_err)] // startup only: the size costs nothing
fn limits_of(route: &Route, config: &Config, budget: Option<&Budget>) -> Result<Limits, Event> {
    let files: u64 = route.files.iter().map(|spec| spec.total()).sum();
    let files_size = usize::try_from(files).unwrap_or(usize::MAX);
    let body_size = match budget.and_then(|b| b.body_size.map(|size| (size, b.line))) {
        Some((size, line)) if size < files_size => {
            let handler = route.handler_name();
            return Err(Event::coded(
                Level::Error,
                CFG0011,
                &[
                    ("handler", &handler),
                    ("budget", &format_size(size)),
                    ("files", &format_size(files_size)),
                    ("source", &format!("budgets.toml:{line}")),
                ],
            ));
        }
        Some((size, _)) => size,
        None if files > 0 => config.body_size.saturating_add(files_size),
        None => config.body_size,
    };
    let upload_time = Duration::from_secs(files.div_ceil(UPLOAD_RATE));
    let timeout = budget
        .and_then(|b| b.timeout)
        .unwrap_or(config.timeout + upload_time);
    let page = route.page.map(|declared| {
        let pagination = config.pagination;
        let max = declared
            .max
            .unwrap_or(pagination.max_per_page.max(declared.default.unwrap_or(0)));
        PageLimits {
            default: declared.default.unwrap_or(pagination.per_page.min(max)),
            max,
        }
    });
    Ok(Limits {
        body_size,
        text_size: config.body_size,
        timeout,
        page,
    })
}

/// The sections of `budgets.toml`, by handler, and every problem in it, in the file's order.
fn read_budgets(text: &str, handlers: &[String]) -> (HashMap<String, Budget>, Vec<Event>) {
    let source = |offset: usize| format!("budgets.toml:{}", line_of(text, offset));
    let invalid = |problem: &str, offset: usize| {
        Event::coded(
            Level::Error,
            CFG0010,
            &[("problem", problem), ("source", &source(offset))],
        )
    };
    // `balise::run!()` already checked the syntax at compile time.
    let table = match DeTable::parse(text) {
        Ok(table) => table,
        Err(error) => {
            let offset = error.span().map_or(0, |span| span.start);
            return (HashMap::new(), vec![invalid(error.message(), offset)]);
        }
    };
    let names: Vec<&str> = handlers.iter().map(String::as_str).collect();
    let mut problems = Vec::new();
    let mut budgets = HashMap::new();
    for (key, value) in table.get_ref() {
        let handler = key.get_ref().to_string();
        let offset = key.span().start;
        let DeValue::Table(section) = value.get_ref() else {
            let problem = format!("`{handler}` must be a section: `[\"{handler}\"]`");
            problems.push((offset, invalid(&problem, offset)));
            continue;
        };
        if !handlers.contains(&handler) {
            let event = coded_with_suggestion(
                CFG0009,
                &[("handler", &handler), ("source", &source(offset))],
                &handler,
                &names,
            );
            problems.push((offset, event));
            continue;
        }
        let mut budget = Budget {
            line: line_of(text, offset),
            ..Budget::default()
        };
        for (name, value) in section {
            let name_text = name.get_ref().as_ref();
            let offset = name.span().start;
            let text_value = match value.get_ref() {
                DeValue::String(text) => Some(text.as_ref()),
                _ => None,
            };
            match name_text {
                "body_size" => match text_value.and_then(parse_size) {
                    Some(size) if size > 0 => budget.body_size = Some(size),
                    _ => {
                        let problem =
                            format!("`body_size` of `{handler}` must be a size such as \"20MB\"");
                        problems.push((offset, invalid(&problem, offset)));
                    }
                },
                "timeout" => match text_value.and_then(parse_duration) {
                    Some(timeout) if !timeout.is_zero() => budget.timeout = Some(timeout),
                    _ => {
                        let problem =
                            format!("`timeout` of `{handler}` must be a duration such as \"2m\"");
                        problems.push((offset, invalid(&problem, offset)));
                    }
                },
                other => {
                    let problem = format!(
                        "Unknown budget `{other}` for `{handler}`; expected `body_size` or `timeout`"
                    );
                    problems.push((offset, invalid(&problem, offset)));
                }
            }
        }
        budgets.insert(handler, budget);
    }
    problems.sort_by_key(|(offset, _)| *offset);
    (
        budgets,
        problems.into_iter().map(|(_, event)| event).collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file::FileSpec;
    use crate::handler::{Method, Request, ResponseFuture};
    use crate::page::PageDeclared;

    fn invoke(_: Request) -> ResponseFuture {
        unreachable!("never called")
    }

    static VIDEO: [FileSpec; 1] = [FileSpec {
        field: "video",
        max_size: 100 * 1024 * 1024,
        types: &["video/mp4"],
        max_count: 2,
    }];

    static ROUTES: [Route; 3] = [
        Route {
            method: Method::Post,
            path: "/imports",
            module: "app::imports",
            function: "create",
            file: "src/imports/mod.rs",
            line: 4,
            invoke,
            states: &[],
            files: &[],
            page: None,
        },
        Route {
            method: Method::Post,
            path: "/videos",
            module: "app::videos",
            function: "upload",
            file: "src/videos/mod.rs",
            line: 9,
            invoke,
            states: &[],
            files: &VIDEO,
            page: None,
        },
        Route {
            method: Method::Get,
            path: "/exports",
            module: "app::exports",
            function: "list",
            file: "src/exports/mod.rs",
            line: 2,
            invoke,
            states: &[],
            files: &[],
            page: Some(PageDeclared {
                default: Some(500),
                max: None,
            }),
        },
    ];

    fn messages(budgets: &str) -> Vec<String> {
        RouteLimits::resolve(&ROUTES, &Config::default(), budgets)
            .unwrap_err()
            .into_iter()
            .map(|event| format!("{} {}", event.code.unwrap().code(), event.message))
            .collect()
    }

    #[test]
    fn limits_follow_files_pages_and_budgets() {
        let config = Config::default();
        let budgets = "[\"imports::create\"]\nbody_size = \"20MB\"\ntimeout = \"2m\"\n";
        let limits = RouteLimits::resolve(&ROUTES, &config, budgets).unwrap();
        let import = limits.get(&ROUTES[0]);
        assert_eq!(import.body_size, 20 * 1024 * 1024);
        assert_eq!(import.timeout, Duration::from_secs(120));
        let upload = limits.get(&ROUTES[1]);
        assert_eq!(upload.body_size, config.body_size + 200 * 1024 * 1024);
        assert_eq!(upload.timeout, config.timeout + Duration::from_secs(200));
        let export = limits.get(&ROUTES[2]);
        assert_eq!(
            export.page,
            Some(PageLimits {
                default: 500,
                max: 500
            })
        );
    }

    #[test]
    fn every_problem_in_budgets_is_reported() {
        let budgets = "[\"imports::creat\"]\nbody_size = \"1MB\"\n\n[\"exports::list\"]\nbody_sise = \"1MB\"\ntimeout = 30\n\n[\"videos::upload\"]\nbody_size = \"10MB\"\n";
        assert_eq!(
            messages(budgets),
            [
                "CFG0009 `[\"imports::creat\"]` names no handler (budgets.toml:1); did you mean `imports::create`?",
                "CFG0010 Unknown budget `body_sise` for `exports::list`; expected `body_size` or `timeout` (budgets.toml:5)",
                "CFG0010 `timeout` of `exports::list` must be a duration such as \"2m\" (budgets.toml:6)",
                "CFG0011 `body_size` of `videos::upload` is 10 MB, less than the 200 MB its `File` fields declare (budgets.toml:8)",
            ]
        );
    }
}
