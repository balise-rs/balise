//! Extractors: what a handler declares as parameters.
//!
//! `Path`, `Query` and `Body` give the value itself: the route macro unwraps them, so that in
//! the handler's body `id: Path<OrderId>` is an `OrderId`. `RequestId` is a service and stays
//! as it is.

use std::fmt;
use std::future::Future;

use crate::error::Error;
use crate::error_codes::{
    ErrorCode, HTTP0010, VAL0001, VAL0002, VAL0003, VAL0004, VAL0005, VAL0006, VAL0007,
};
use crate::handler::{FromRequest, Request};
use crate::input::{Fields, Input};
use crate::json;

/// A field that did not pass, with the code of its cause: `quantity`, `VAL0006`,
/// `must be at most 100`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldError {
    pub field: String,
    pub code: &'static ErrorCode,
    pub message: String,
}

impl FieldError {
    /// A required field is absent or `null` (`VAL0004`).
    pub fn missing(field: &str) -> FieldError {
        FieldError::new(field, VAL0004, &[])
    }

    /// A value cannot be read as the declared type (`VAL0005`): `expected` and `found` read
    /// like `a whole number` and `text`.
    pub fn wrong_type(field: &str, expected: &str, found: &str) -> FieldError {
        FieldError::new(field, VAL0005, &[("expected", expected), ("found", found)])
    }

    /// A value breaks a declared rule (`VAL0006`): `rule` reads like `must be at most 100`.
    pub fn rule(field: &str, rule: &str) -> FieldError {
        FieldError::new(field, VAL0006, &[("rule", rule)])
    }

    /// A field the `Input` does not declare (`VAL0007`), with the closest declared name.
    pub fn unknown(field: &str, closest: Option<&str>) -> FieldError {
        let suggestion = closest
            .map(|name| format!("; did you mean `{name}`?"))
            .unwrap_or_default();
        FieldError::new(field, VAL0007, &[("suggestion", &suggestion)])
    }

    fn new(field: &str, code: &'static ErrorCode, args: &[(&str, &str)]) -> FieldError {
        FieldError {
            field: field.to_string(),
            code,
            message: code.format_message(args),
        }
    }
}

/// Every field that did not pass, so that all of them can be fixed at once.
pub type FieldErrors = Vec<FieldError>;

/// Path parameters: `Path<OrderId>` for one, `Path<(ShopId, OrderId)>` for several, in order.
pub struct Path<T>(T);

/// The query string, read into an `Input`.
pub struct Query<T>(T);

/// The body, read into an `Input`: JSON, or `multipart/form-data` when it has `File` fields.
/// It is read after every other parameter, so a request they reject is never read.
pub struct Body<T>(T);

macro_rules! unwrap_value {
    ($($wrapper:ident),*) => {$(
        impl<T> $wrapper<T> {
            /// The value itself. Called by the code the route macro generates.
            #[doc(hidden)]
            pub fn __into_value(self) -> T {
                self.0
            }
        }
    )*};
}

unwrap_value!(Path, Query, Body);

/// A value read from one path segment.
pub trait FromParam: Sized {
    /// What the value must look like, as error messages say it: `a whole number`.
    const EXPECTED: &'static str;
    fn from_param(text: &str) -> Option<Self>;
}

impl FromParam for String {
    const EXPECTED: &'static str = "text";
    fn from_param(text: &str) -> Option<Self> {
        Some(text.to_string())
    }
}

impl FromParam for bool {
    const EXPECTED: &'static str = "boolean (`true` or `false`)";
    fn from_param(text: &str) -> Option<Self> {
        text.parse().ok()
    }
}

macro_rules! integer_param {
    ($($t:ty),*) => {$(
        impl FromParam for $t {
            const EXPECTED: &'static str = "whole number";
            fn from_param(text: &str) -> Option<Self> {
                text.parse().ok()
            }
        }
    )*};
}

integer_param!(i16, i32, i64, u8, u16, u32, u64);

/// What a `Path` reads: one parameter, or a tuple of them.
pub trait PathValue: Sized {
    fn from_params(request: &mut Request) -> Result<Self, Error>;
}

impl<T: FromParam> PathValue for T {
    fn from_params(request: &mut Request) -> Result<Self, Error> {
        next_param(request)
    }
}

macro_rules! tuple_path {
    ($($name:ident),*) => {
        impl<$($name: FromParam),*> PathValue for ($($name,)*) {
            fn from_params(request: &mut Request) -> Result<Self, Error> {
                Ok(($(next_param::<$name>(request)?,)*))
            }
        }
    };
}

tuple_path!(A, B);
tuple_path!(A, B, C);
tuple_path!(A, B, C, D);

fn next_param<T: FromParam>(request: &mut Request) -> Result<T, Error> {
    // The route macro checks that there is one `Path` value per parameter.
    let (name, value) = if request.params.is_empty() {
        (String::new(), String::new())
    } else {
        request.params.remove(0)
    };
    T::from_param(&value).ok_or_else(|| {
        Error::coded(
            404,
            HTTP0010,
            &[
                ("name", &name),
                ("expected", T::EXPECTED),
                ("value", &value),
            ],
        )
    })
}

impl<T: PathValue + Send> FromRequest for Path<T> {
    fn from_request(request: &mut Request) -> impl Future<Output = Result<Self, Error>> + Send {
        let value = T::from_params(request).map(Path);
        async move { value }
    }
}

impl<T: Input + Send> FromRequest for Query<T> {
    fn from_request(request: &mut Request) -> impl Future<Output = Result<Self, Error>> + Send {
        let mut pairs = query_pairs(&request.query);
        // On a paginated route, the page keys belong to `PageParams`.
        if request.limits.page.is_some() {
            pairs.retain(|(key, _)| !crate::page::KEYS.contains(&key.as_str()));
        }
        let value = T::from_fields(Fields::from_query(pairs))
            .map(Query)
            .map_err(validation_failed);
        async move { value }
    }
}

impl<T: Input + Send> FromRequest for Body<T> {
    async fn from_request(request: &mut Request) -> Result<Self, Error> {
        read_body(request).await.map(Body)
    }
}

const JSON: &str = "application/json";

/// Reads the body as JSON, or as multipart when the `Input` has files.
async fn read_body<T: Input>(request: &mut Request) -> Result<T, Error> {
    let content_type = request.header("content-type").map(str::to_string);
    let essence = content_type
        .as_deref()
        .map(|value| value.split(';').next().unwrap_or_default().trim());
    let unsupported = |expected: &str| {
        let found = essence.unwrap_or_default();
        let expected = format!("`{expected}`");
        Error::coded(
            415,
            VAL0003,
            &[("content_type", found), ("expected", &expected)],
        )
    };
    #[cfg(feature = "runtime")]
    if !T::FILES.is_empty() {
        const MULTIPART: &str = "multipart/form-data";
        let content_type = match essence {
            Some(essence) if essence.eq_ignore_ascii_case(MULTIPART) => content_type.as_deref(),
            _ => return Err(unsupported(MULTIPART)),
        };
        let text_size = request.limits.text_size;
        let fields = crate::multipart::read(
            &mut request.body,
            content_type.unwrap_or_default(),
            T::FILES,
            text_size,
        )
        .await?;
        return T::from_fields(fields).map_err(validation_failed);
    }
    if let Some(essence) = essence
        && !essence.eq_ignore_ascii_case(JSON)
    {
        return Err(unsupported(JSON));
    }
    let bytes = request.body.bytes().await?;
    let value =
        json::parse(&bytes).map_err(|cause| Error::coded(400, VAL0002, &[("cause", &cause)]))?;
    let fields = Fields::from_json(value).map_err(|kind| {
        let cause = format!("expected an object, found {kind}");
        Error::coded(400, VAL0002, &[("cause", &cause)])
    })?;
    T::from_fields(fields).map_err(validation_failed)
}

pub(crate) fn validation_failed(fields: FieldErrors) -> Error {
    let names: Vec<&str> = fields.iter().map(|f| f.field.as_str()).collect();
    Error::coded(422, VAL0001, &[("names", &names.join(", "))]).with_fields(fields)
}

/// `a=1&b=x%20y` gives `("a", "1")` and `("b", "x y")`; `+` is a space.
pub(crate) fn query_pairs(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            let decode = |text: &str| crate::text::percent_decode(&text.replace('+', " "));
            (decode(key), decode(value))
        })
        .collect()
}

/// The request's identifier, as it appears in logs: `req_7f3a2c`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestId(String);

impl RequestId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RequestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromRequest for RequestId {
    fn from_request(request: &mut Request) -> impl Future<Output = Result<Self, Error>> + Send {
        let id = RequestId(request.request_id.clone());
        async move { Ok(id) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(params: &[(&str, &str)]) -> Request {
        Request {
            params: params
                .iter()
                .map(|(n, v)| (n.to_string(), v.to_string()))
                .collect(),
            request_id: "req_000001".into(),
            ..Request::default()
        }
    }

    #[test]
    fn path_values_are_read_in_order_and_parsed() {
        let mut req = request(&[("shop_id", "7"), ("id", "ord_1")]);
        let (shop, order) = <(u32, String)>::from_params(&mut req).unwrap();
        assert_eq!((shop, order.as_str()), (7, "ord_1"));
    }

    #[test]
    fn an_invalid_path_value_is_a_404_naming_it() {
        let mut req = request(&[("id", "abc")]);
        let response = u32::from_params(&mut req)
            .unwrap_err()
            .render("req_000001", false);
        assert_eq!(response.status, 404);
        assert_eq!(
            String::from_utf8(response.body).unwrap(),
            r#"{"error":{"code":"HTTP0010","message":"`id` in the path is not a valid whole number: `abc`","request_id":"req_000001"}}"#
        );
    }

    #[test]
    fn query_strings_are_decoded() {
        assert_eq!(
            query_pairs("status=pending&note=a+b%21&flag"),
            [
                ("status".into(), "pending".into()),
                ("note".into(), "a b!".into()),
                ("flag".into(), String::new())
            ]
        );
    }
}
