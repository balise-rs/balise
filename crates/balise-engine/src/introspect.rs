//! The commands of a dev build that read the app itself, for the CLI: `routes` and
//! `why <METHOD> <path>`. They answer in JSON on stdout, which the CLI renders; a problem
//! that stops them is `{"error":{"code":…,"message":…,"fix":…}}`, as the CLI writes its own.

use balise_core::config::load::{self, Files, Mode};
use balise_core::config::{Config, Environment};
use balise_core::event::{Event, Level};
use balise_core::handler::Method;
use balise_core::json::{Number, Value, write_value};
use balise_core::limits::{Limits, RouteLimits};
use balise_core::registry::{self, Route};
use balise_core::router::{Router, Why};

/// `routes`: every route, with its handler and limits.
pub fn routes(files: Files, build: Environment, variables: &[(String, String)]) -> i32 {
    let (router, limits) = match load_router(files, build, variables) {
        Ok(loaded) => loaded,
        Err(event) => return failed(&event),
    };
    let mut routes: Vec<&Route> = registry::routes().iter().collect();
    routes.sort_by_key(|route| (route.path, route.method));
    let routes = routes
        .into_iter()
        .map(|route| describe(route, router.base(), &limits.get(route)))
        .collect();
    print(&Value::Object(vec![
        ("base".into(), text(router.base())),
        ("routes".into(), Value::Array(routes)),
    ]));
    0
}

/// `why <METHOD> <path>`: the route a request reaches, or why none does.
pub fn why(
    files: Files,
    build: Environment,
    variables: &[(String, String)],
    method: &str,
    path: &str,
) -> i32 {
    let (router, limits) = match load_router(files, build, variables) {
        Ok(loaded) => loaded,
        Err(event) => return failed(&event),
    };
    let method = method.to_ascii_uppercase();
    let path = path.split(['?', '#']).next().unwrap_or(path);
    let path = match path.starts_with('/') {
        true => path.to_string(),
        false => format!("/{path}"),
    };
    let mut fields = vec![
        ("method".into(), text(&method)),
        ("path".into(), text(&path)),
    ];
    match router.why(Method::parse(&method), &path) {
        Why::Found { route, params } => {
            fields.push(("result".into(), text("found")));
            fields.push((
                "route".into(),
                describe(route, router.base(), &limits.get(route)),
            ));
            fields.push((
                "params".into(),
                Value::Object(
                    params
                        .into_iter()
                        .map(|(name, value)| (name, Value::String(value)))
                        .collect(),
                ),
            ));
        }
        Why::MethodNotAllowed { allowed } => {
            fields.push(("result".into(), text("method_not_allowed")));
            fields.push(("code".into(), text("HTTP0008")));
            fields.push((
                "allowed".into(),
                Value::Array(allowed.iter().map(|m| text(m.as_str())).collect()),
            ));
        }
        Why::NotFound { instead, near } => {
            fields.push(("result".into(), text("not_found")));
            fields.push(("code".into(), text("HTTP0003")));
            fields.push((
                "instead".into(),
                match instead {
                    Some((path, reason)) => Value::Object(vec![
                        ("path".into(), Value::String(path)),
                        ("reason".into(), Value::String(reason)),
                    ]),
                    None => Value::Null,
                },
            ));
            let near = near
                .into_iter()
                .map(|near| {
                    let routes = near
                        .routes
                        .iter()
                        .map(|route| {
                            Value::Object(vec![
                                ("method".into(), text(route.method.as_str())),
                                ("handler".into(), text(&route.handler_name())),
                                ("location".into(), text(&route.location())),
                            ])
                        })
                        .collect();
                    Value::Object(vec![
                        ("path".into(), Value::String(near.path)),
                        ("reason".into(), Value::String(near.reason)),
                        ("routes".into(), Value::Array(routes)),
                    ])
                })
                .collect();
            fields.push(("near".into(), Value::Array(near)));
        }
    }
    print(&Value::Object(fields));
    0
}

/// The router and limits the app would serve with, from the same configuration.
fn load_router(
    files: Files,
    build: Environment,
    variables: &[(String, String)],
) -> Result<(Router, RouteLimits), Event> {
    let first = |events: Vec<Event>| {
        let none = || Event::new(Level::Error, "The configuration cannot be read");
        events.into_iter().next().unwrap_or_else(none)
    };
    let environment = load::environment(build, variables).map_err(|event| *event)?;
    let config: Config = load::load(&files, environment, build, variables, Mode::Start)
        .map_err(first)?
        .config;
    let limits = RouteLimits::resolve(registry::routes(), &config, files.budgets).map_err(first)?;
    let router = Router::build(registry::routes(), &config.api_base())?;
    Ok((router, limits))
}

fn describe(route: &Route, base: &str, limits: &Limits) -> Value {
    let count = |n: u64| Value::Number(Number::Unsigned(n));
    let page = match limits.page {
        Some(page) => Value::Object(vec![
            ("default".into(), count(page.default.into())),
            ("max".into(), count(page.max.into())),
        ]),
        None => Value::Null,
    };
    Value::Object(vec![
        ("method".into(), text(route.method.as_str())),
        ("path".into(), text(&format!("{base}{}", route.path))),
        ("handler".into(), text(&route.handler_name())),
        ("location".into(), text(&route.location())),
        ("body_size".into(), count(limits.body_size as u64)),
        (
            "timeout_ms".into(),
            count(limits.timeout.as_millis() as u64),
        ),
        ("page".into(), page),
    ])
}

/// What stops the command, as the CLI writes its errors.
fn failed(event: &Event) -> i32 {
    let code = event.code.map_or("EXT0001", |code| code.code());
    print(&Value::Object(vec![(
        "error".into(),
        Value::Object(vec![
            ("code".into(), text(code)),
            ("message".into(), text(&event.message)),
            ("fix".into(), text(event.fix.as_deref().unwrap_or_default())),
        ]),
    )]));
    1
}

fn text(text: &str) -> Value {
    Value::String(text.to_string())
}

fn print(value: &Value) {
    let mut out = String::new();
    write_value(&mut out, value);
    println!("{out}");
}
