//! The router: a tree of path segments, built once at startup and never modified.
//!
//! A static segment wins over a parameter: `/orders/new` is tried before `/orders/{id}`. When
//! a path matches several routes, the first one, in that priority order, that accepts the
//! method is chosen; when none does, the answer is `405` with the allowed methods.

use std::collections::BTreeMap;

use crate::error_codes::HTTP0010;
use crate::event::{Event, Level};
use crate::handler::Method;
use crate::registry::Route;

/// Served by Balise at the root, outside the API base, for orchestrators: `/health` while
/// the process runs, `/ready` while it accepts traffic.
pub const HEALTH_PATHS: [&str; 2] = ["/health", "/ready"];

/// The routes of the application, ready to match requests.
#[derive(Debug)]
pub struct Router {
    base: String,
    root: Node,
}

#[derive(Debug, Default)]
struct Node {
    statics: BTreeMap<String, Node>,
    param: Option<Box<Node>>,
    routes: BTreeMap<Method, &'static Route>,
}

/// What the router found for a request.
#[derive(Debug)]
pub enum Match {
    Found {
        route: &'static Route,
        /// Path parameters, named as in the matched route.
        params: Vec<(String, String)>,
        /// The path without the API base.
        path: String,
    },
    /// The path exists, not with this method.
    MethodNotAllowed {
        allowed: Vec<Method>,
    },
    NotFound,
}

impl Router {
    /// Builds the router, or the event naming two handlers that claim the same route.
    #[allow(clippy::result_large_err)] // startup only: the size costs nothing
    pub fn build(routes: &'static [Route], base: &str) -> Result<Router, Event> {
        let base = base.trim_end_matches('/');
        let mut root = Node::default();
        for route in routes {
            let full = format!("{base}{}", route.path);
            if route.method == Method::Get && HEALTH_PATHS.contains(&full.as_str()) {
                return Err(Event::coded(
                    Level::Error,
                    HTTP0010,
                    &[
                        ("method", route.method.as_str()),
                        ("path", &full),
                        (
                            "first",
                            &format!("{} ({})", route.handler_name(), route.location()),
                        ),
                        ("second", "Balise's health check"),
                    ],
                ));
            }
            let mut node = &mut root;
            for segment in segments(route.path) {
                node = if is_param(segment) {
                    node.param.get_or_insert_with(Box::default)
                } else {
                    node.statics.entry(segment.to_string()).or_default()
                };
            }
            if let Some(existing) = node.routes.insert(route.method, route) {
                return Err(duplicate(existing, route));
            }
        }
        Ok(Router {
            base: base.to_string(),
            root,
        })
    }

    /// Finds the route for a method and a full path, API base included.
    pub fn find(&self, method: Option<Method>, full_path: &str) -> Match {
        let Some(path) = full_path.strip_prefix(&self.base) else {
            return Match::NotFound;
        };
        if !(path.is_empty() || path.starts_with('/')) {
            return Match::NotFound;
        }
        let path = if path.is_empty() { "/" } else { path };
        let parts: Vec<&str> = segments(path).collect();

        let mut candidates = Vec::new();
        collect(&self.root, &parts, &mut Vec::new(), &mut candidates);
        if candidates.is_empty() {
            return Match::NotFound;
        }
        if let Some(method) = method {
            for (node, values) in &candidates {
                if let Some(route) = node.routes.get(&method) {
                    let names = segments(route.path).filter(|s| is_param(s)).map(param_name);
                    return Match::Found {
                        route,
                        params: names
                            .map(String::from)
                            .zip(values.iter().cloned())
                            .collect(),
                        path: path.to_string(),
                    };
                }
            }
        }
        let mut allowed: Vec<Method> = candidates
            .iter()
            .flat_map(|(node, _)| node.routes.keys().copied())
            .collect();
        allowed.sort();
        allowed.dedup();
        Match::MethodNotAllowed { allowed }
    }
}

/// Every node matching `parts` that has routes, static segments first, with the values of the
/// parameters on the way.
fn collect<'a>(
    node: &'a Node,
    parts: &[&str],
    values: &mut Vec<String>,
    found: &mut Vec<(&'a Node, Vec<String>)>,
) {
    let Some((first, rest)) = parts.split_first() else {
        if !node.routes.is_empty() {
            found.push((node, values.clone()));
        }
        return;
    };
    if let Some(next) = node.statics.get(*first) {
        collect(next, rest, values, found);
    }
    if let Some(next) = &node.param
        && !first.is_empty()
    {
        values.push(crate::text::percent_decode(first));
        collect(next, rest, values, found);
        values.pop();
    }
}

/// The segments of a path: `/orders/{id}` gives `orders` and `{id}`; `/` gives none.
fn segments(path: &str) -> impl Iterator<Item = &str> {
    let path = path.strip_prefix('/').unwrap_or(path);
    path.split('/').filter(move |_| !path.is_empty())
}

fn is_param(segment: &str) -> bool {
    segment.starts_with('{') && segment.ends_with('}')
}

fn param_name(segment: &str) -> &str {
    &segment[1..segment.len() - 1]
}

fn duplicate(first: &Route, second: &Route) -> Event {
    Event::coded(
        Level::Error,
        HTTP0010,
        &[
            ("method", second.method.as_str()),
            ("path", second.path),
            (
                "first",
                &format!("{} ({})", first.handler_name(), first.location()),
            ),
            (
                "second",
                &format!("{} ({})", second.handler_name(), second.location()),
            ),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handler::{Request, Response, ResponseFuture};

    fn invoke(_: Request) -> ResponseFuture {
        Box::pin(async { Response::json(200, String::new()) })
    }

    macro_rules! routes {
        ($($method:ident $path:literal $function:literal),* $(,)?) => {{
            let routes: Vec<Route> = vec![$(Route {
                method: Method::$method,
                path: $path,
                module: "app::orders",
                function: $function,
                file: "src/orders/mod.rs",
                line: 1,
                invoke,
                states: &[],
                files: &[],
                page: None,
            }),*];
            Box::leak(routes.into_boxed_slice())
        }};
    }

    fn found(router: &Router, method: Method, path: &str) -> (String, Vec<(String, String)>) {
        match router.find(Some(method), path) {
            Match::Found { route, params, .. } => (route.handler_name(), params),
            other => panic!("{path}: {other:?}"),
        }
    }

    #[test]
    fn matches_static_and_parameter_segments_under_the_base() {
        let router = Router::build(
            routes![Get "/orders" "list", Get "/orders/{id}" "get", Get "/orders/new" "new"],
            "/api",
        )
        .unwrap();
        assert_eq!(found(&router, Method::Get, "/api/orders").0, "orders::list");
        assert_eq!(
            found(&router, Method::Get, "/api/orders/new").0,
            "orders::new"
        );
        assert_eq!(
            found(&router, Method::Get, "/api/orders/ord%5F1"),
            (
                "orders::get".to_string(),
                vec![("id".to_string(), "ord_1".to_string())]
            )
        );
        for path in ["/orders", "/apiary/orders", "/api/orders/"] {
            assert!(
                matches!(router.find(Some(Method::Get), path), Match::NotFound),
                "{path}"
            );
        }
    }

    #[test]
    fn falls_back_to_a_parameter_when_the_static_branch_does_not_match() {
        let router = Router::build(
            routes![Get "/orders/new" "new", Get "/orders/{id}/lines" "lines"],
            "/api",
        )
        .unwrap();
        assert_eq!(
            found(&router, Method::Get, "/api/orders/new/lines").0,
            "orders::lines"
        );
    }

    #[test]
    fn a_path_with_another_method_is_not_allowed() {
        let router = Router::build(
            routes![Get "/orders" "list", Post "/orders" "create"],
            "/api",
        )
        .unwrap();
        for method in [Some(Method::Delete), None] {
            assert!(matches!(
                router.find(method, "/api/orders"),
                Match::MethodNotAllowed { allowed } if allowed == [Method::Get, Method::Post]
            ));
        }
    }

    #[test]
    fn two_handlers_for_the_same_route_fail_with_both_names() {
        let error = Router::build(
            routes![Get "/orders/{id}" "get", Get "/orders/{order_id}" "show"],
            "/api",
        )
        .unwrap_err();
        assert_eq!(error.code, Some(HTTP0010));
        assert_eq!(
            error.message,
            "Two handlers for GET /orders/{order_id}: orders::get (src/orders/mod.rs:1) and orders::show (src/orders/mod.rs:1)"
        );
    }

    #[test]
    fn the_root_path_is_the_base_itself() {
        let router = Router::build(routes![Get "/" "home"], "/api/v1").unwrap();
        assert_eq!(found(&router, Method::Get, "/api/v1").0, "orders::home");
        assert_eq!(found(&router, Method::Get, "/api/v1/").0, "orders::home");
    }

    #[test]
    fn health_paths_belong_to_balise() {
        let error = Router::build(routes![Get "/health" "health"], "").unwrap_err();
        assert_eq!(
            error.message,
            "Two handlers for GET /health: orders::health (src/orders/mod.rs:1) and Balise's health check"
        );
        assert!(Router::build(routes![Get "/health" "health"], "/api").is_ok());
    }
}
