//! Page sizes: what a paginated route reads from its query string, within its limits.
//!
//! Pages hold `pagination.per_page` items by default and `pagination.max_per_page` at most;
//! a route that needs other limits declares them: `#[get("/exports", page(default = 500,
//! max = 5000))]`. The pages themselves (`Page<T>`, cursors) come with the ORM.

use std::future::Future;

use crate::error::Error;
use crate::extract::{FieldError, query_pairs, validation_failed};
use crate::handler::{FromRequest, Request};

/// The query keys `PageParams` reads. `Query<T>` leaves them to it on a paginated route.
pub(crate) const KEYS: [&str; 3] = ["per_page", "cursor", "page"];

/// What a route declares with `page(default = …, max = …)`. Recorded by the route macro.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PageDeclared {
    pub default: Option<u32>,
    pub max: Option<u32>,
}

/// A route's page sizes, once its declaration and the configuration are combined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageLimits {
    pub default: u32,
    pub max: u32,
}

/// Which page a client asks for, and how large: `?per_page=50&cursor=…`, or `?page=3` for
/// numbered pages. Read with `page: PageParams`, and passed to the query that pages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageParams {
    per_page: u32,
    cursor: Option<String>,
    page: Option<u32>,
}

impl PageParams {
    /// Items per page: what the client asked for, lowered to the route's maximum, or the
    /// route's default.
    pub fn per_page(&self) -> u32 {
        self.per_page
    }

    /// Where the next page starts, for cursor pagination.
    pub fn cursor(&self) -> Option<&str> {
        self.cursor.as_deref()
    }

    /// The page number from 1, for numbered pagination.
    pub fn page(&self) -> Option<u32> {
        self.page
    }

    fn read(query: &str, limits: PageLimits) -> Result<PageParams, Error> {
        let pairs = query_pairs(query);
        let get = |key: &str| {
            pairs
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.as_str())
        };
        let mut errors = Vec::new();
        let mut number = |key: &str| -> Option<u32> {
            let text = get(key)?;
            match text.parse::<u32>() {
                Ok(0) => {
                    errors.push(FieldError::rule(key, "must be at least 1"));
                    None
                }
                Ok(n) => Some(n),
                Err(_) => {
                    let found = format!("`{text}`");
                    errors.push(FieldError::wrong_type(key, "a whole number from 1", &found));
                    None
                }
            }
        };
        let per_page = number("per_page").map_or(limits.default, |n| n.min(limits.max));
        let page = number("page");
        let cursor = get("cursor")
            .filter(|cursor| !cursor.is_empty())
            .map(str::to_string);
        if cursor.is_some() && page.is_some() {
            errors.push(FieldError::rule("page", "cannot be sent with `cursor`"));
        }
        match errors.is_empty() {
            true => Ok(PageParams {
                per_page,
                cursor,
                page,
            }),
            false => Err(validation_failed(errors)),
        }
    }
}

impl FromRequest for PageParams {
    fn from_request(request: &mut Request) -> impl Future<Output = Result<Self, Error>> + Send {
        // The route macro records every route that reads `PageParams`, so its limits are set.
        let limits = request.limits.page.unwrap_or_else(|| {
            let pagination = crate::config::Config::default().pagination;
            PageLimits {
                default: pagination.per_page,
                max: pagination.max_per_page,
            }
        });
        let value = PageParams::read(&request.query, limits);
        async move { value }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMITS: PageLimits = PageLimits {
        default: 20,
        max: 100,
    };

    #[test]
    fn page_sizes_follow_the_route_limits() {
        let params = PageParams::read("", LIMITS).unwrap();
        assert_eq!(
            (params.per_page(), params.cursor(), params.page()),
            (20, None, None)
        );
        let params = PageParams::read("per_page=500&cursor=abc", LIMITS).unwrap();
        assert_eq!((params.per_page(), params.cursor()), (100, Some("abc")));
        assert_eq!(PageParams::read("page=3", LIMITS).unwrap().page(), Some(3));
    }

    #[test]
    fn invalid_values_are_field_errors() {
        let error = PageParams::read("per_page=0&page=x", LIMITS).unwrap_err();
        assert_eq!(error.status(), 422);
        assert_eq!(error.message(), "Invalid fields: per_page, page");
        let error = PageParams::read("page=2&cursor=abc", LIMITS).unwrap_err();
        assert_eq!(error.message(), "Invalid fields: page");
    }
}
