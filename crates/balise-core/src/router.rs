//! The router: a tree of path segments, built once at startup and never modified.
//!
//! A static segment wins over a parameter: `/orders/new` is tried before `/orders/{id}`. When
//! a path matches several routes, the first one, in that priority order, that accepts the
//! method is chosen; when none does, the answer is `405` with the allowed methods.

use std::collections::BTreeMap;

use crate::error_codes::{HTTP0009, HTTP0014};
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
    routes: &'static [Route],
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
                    HTTP0014,
                    &[
                        ("path", &full),
                        ("handler", &route.handler_name()),
                        ("location", &route.location()),
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
            routes,
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

/// Why a request reaches a route or not, for `balise why`.
#[derive(Debug)]
pub enum Why {
    Found {
        route: &'static Route,
        params: Vec<(String, String)>,
    },
    MethodNotAllowed {
        allowed: Vec<Method>,
    },
    NotFound {
        /// A path that would work, when the mistake is a known one: the API base forgotten,
        /// a trailing `/`.
        instead: Option<(String, String)>,
        /// The routes that almost match, with what differs.
        near: Vec<Near>,
    },
}

/// A path of routes that almost matches a request's.
#[derive(Debug)]
pub struct Near {
    /// The routes of this path, one per method.
    pub routes: Vec<&'static Route>,
    /// The full path: `/api/orders/{id}`.
    pub path: String,
    pub reason: String,
}

impl Router {
    /// The API base: `/api`.
    pub fn base(&self) -> &str {
        &self.base
    }

    /// What [`find`](Self::find) answers for a request, and why.
    pub fn why(&self, method: Option<Method>, full_path: &str) -> Why {
        match self.find(method, full_path) {
            Match::Found { route, params, .. } => return Why::Found { route, params },
            Match::MethodNotAllowed { allowed } => return Why::MethodNotAllowed { allowed },
            Match::NotFound => {}
        }
        let works = |path: &str| !matches!(self.find(method, path), Match::NotFound);
        let instead = if !self.base.is_empty() && !full_path.starts_with(&self.base) {
            let path = format!("{}{full_path}", self.base);
            works(&path).then(|| {
                let reason = format!("every route is under `{}`", self.base);
                (path, reason)
            })
        } else if full_path.len() > 1 && full_path.ends_with('/') {
            let path = full_path.trim_end_matches('/').to_string();
            works(&path).then(|| (path, "routes have no trailing `/`".to_string()))
        } else {
            None
        };
        let path = match full_path.starts_with(&self.base) {
            true => full_path.to_string(),
            false => format!("{}{full_path}", self.base),
        };
        let parts: Vec<&str> = segments(&path).collect();
        let mut near: Vec<(usize, usize, Near)> = self
            .routes
            .iter()
            .filter_map(|route| {
                let full = format!("{}{}", self.base, route.path);
                let expected: Vec<&str> = segments(&full).collect();
                let same = expected
                    .iter()
                    .zip(&parts)
                    .take_while(|(e, p)| *e == *p || (is_param(e) && !p.is_empty()))
                    .count();
                // Only routes that share more than the base, or whose next segment is a typo
                // away: `ordres` for `orders`.
                let base_segments = segments(&self.base).count();
                let typo = match (expected.get(same), parts.get(same)) {
                    (Some(e), Some(p)) => !is_param(e) && crate::text::closest(p, &[e]).is_some(),
                    _ => false,
                };
                if same <= base_segments && !typo {
                    return None;
                }
                let reason = match (expected.get(same), parts.get(same)) {
                    (Some(e), Some(p)) => format!("`{p}` is not `{e}`"),
                    (Some(_), None) => {
                        let more = expected.len() - parts.len();
                        format!("it has {} more", plural(more, "segment"))
                    }
                    (None, Some(_)) => {
                        let fewer = parts.len() - expected.len();
                        format!("it ends {} earlier", plural(fewer, "segment"))
                    }
                    (None, None) => return None,
                };
                let distance = expected.len().abs_diff(parts.len());
                let near = Near {
                    routes: vec![route],
                    path: full,
                    reason,
                };
                Some((same, distance, near))
            })
            .collect();
        near.sort_by(|a, b| (b.0, a.1, &a.2.path).cmp(&(a.0, b.1, &b.2.path)));
        // One entry per path; none when a known mistake explains it all.
        let mut paths: Vec<Near> = Vec::new();
        for (_, _, near) in near {
            match paths.iter_mut().find(|other| other.path == near.path) {
                Some(other) => other.routes.extend(near.routes),
                None => paths.push(near),
            }
        }
        for near in &mut paths {
            near.routes.sort_by_key(|route| route.method);
        }
        paths.truncate(3);
        if instead.is_some() {
            paths.clear();
        }
        Why::NotFound {
            instead,
            near: paths,
        }
    }
}

fn plural(count: usize, word: &str) -> String {
    match count {
        1 => format!("one {word}"),
        _ => format!("{count} {word}s"),
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
        HTTP0009,
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
        assert_eq!(error.code, Some(HTTP0009));
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
    fn why_names_the_routes_that_almost_match() {
        let router = Router::build(
            routes![Get "/orders" "list", Get "/orders/{id}" "get", Get "/orders/{id}/lines" "lines", Post "/orders" "create"],
            "/api",
        )
        .unwrap();
        assert!(matches!(
            router.why(Some(Method::Get), "/api/orders/7"),
            Why::Found { route, params } if route.function == "get" && params == [("id".to_string(), "7".to_string())]
        ));
        assert!(matches!(
            router.why(Some(Method::Delete), "/api/orders"),
            Why::MethodNotAllowed { allowed } if allowed == [Method::Get, Method::Post]
        ));
        let Why::NotFound { instead, near } = router.why(Some(Method::Get), "/orders/7") else {
            panic!()
        };
        assert_eq!(
            instead,
            Some((
                "/api/orders/7".to_string(),
                "every route is under `/api`".to_string()
            ))
        );
        assert!(near.is_empty());
        let Why::NotFound { instead, near } = router.why(Some(Method::Get), "/api/ordres/7") else {
            panic!()
        };
        assert_eq!(instead, None);
        assert_eq!(near[0].reason, "`ordres` is not `orders`");
        let Why::NotFound { near, .. } = router.why(Some(Method::Get), "/api/orders/7/items")
        else {
            panic!()
        };
        let reasons: Vec<(&str, &str)> = near
            .iter()
            .map(|n| (n.path.as_str(), n.reason.as_str()))
            .collect();
        assert_eq!(
            reasons,
            [
                ("/api/orders/{id}/lines", "`items` is not `lines`"),
                ("/api/orders/{id}", "it ends one segment earlier"),
                ("/api/orders", "it ends 2 segments earlier"),
            ]
        );
        let Why::NotFound { instead, .. } = router.why(Some(Method::Get), "/api/orders/") else {
            panic!()
        };
        assert_eq!(instead.unwrap().1, "routes have no trailing `/`");
    }

    #[test]
    fn health_paths_belong_to_balise() {
        let error = Router::build(routes![Get "/health" "health"], "").unwrap_err();
        assert_eq!(
            error.message,
            "`/health` is reserved for Balise's health checks; `orders::health` cannot use it (src/orders/mod.rs:1)"
        );
        assert!(Router::build(routes![Get "/health" "health"], "/api").is_ok());
    }
}
