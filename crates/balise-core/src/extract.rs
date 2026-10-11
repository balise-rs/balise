//! Extractors: what a handler declares as parameters.
//!
//! `Path`, `Query` and `Body` give the value itself: the route macro unwraps them, so that in
//! the handler's body `id: Path<OrderId>` is an `OrderId`. `RequestId` is a service and stays
//! as it is.

use std::fmt;
use std::future::Future;

use crate::error_codes::{
    ErrorCode, HTTP0011, VAL0001, VAL0002, VAL0003, VAL0004, VAL0005, VAL0006,
};
use crate::handler::{FromRequest, Request, Response};
use crate::json::{self, Value};

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

/// A struct that derives `Input`: built from a JSON body or a query string, with its rules.
pub trait Input: Sized {
    fn from_json(value: Value) -> Result<Self, FieldErrors>;
    /// Query string values are text: numbers and booleans are read from it.
    fn from_query(pairs: Vec<(String, String)>) -> Result<Self, FieldErrors>;
}

/// Path parameters: `Path<OrderId>` for one, `Path<(ShopId, OrderId)>` for several, in order.
pub struct Path<T>(T);

/// The query string, read into an `Input`.
pub struct Query<T>(T);

/// The JSON body, read into an `Input`.
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
    fn from_params(request: &mut Request) -> Result<Self, Response>;
}

impl<T: FromParam> PathValue for T {
    fn from_params(request: &mut Request) -> Result<Self, Response> {
        next_param(request)
    }
}

macro_rules! tuple_path {
    ($($name:ident),*) => {
        impl<$($name: FromParam),*> PathValue for ($($name,)*) {
            fn from_params(request: &mut Request) -> Result<Self, Response> {
                Ok(($(next_param::<$name>(request)?,)*))
            }
        }
    };
}

tuple_path!(A, B);
tuple_path!(A, B, C);
tuple_path!(A, B, C, D);

fn next_param<T: FromParam>(request: &mut Request) -> Result<T, Response> {
    // The route macro checks that there is one `Path` value per parameter.
    let (name, value) = if request.params.is_empty() {
        (String::new(), String::new())
    } else {
        request.params.remove(0)
    };
    T::from_param(&value).ok_or_else(|| {
        let message = HTTP0011.format_message(&[
            ("name", &name),
            ("expected", T::EXPECTED),
            ("value", &value),
        ]);
        Response::error(404, HTTP0011, &message, &[], &request.request_id)
    })
}

impl<T: PathValue + Send> FromRequest for Path<T> {
    fn from_request(request: &mut Request) -> impl Future<Output = Result<Self, Response>> + Send {
        let value = T::from_params(request).map(Path);
        async move { value }
    }
}

impl<T: Input + Send> FromRequest for Query<T> {
    fn from_request(request: &mut Request) -> impl Future<Output = Result<Self, Response>> + Send {
        let value = T::from_query(query_pairs(&request.query))
            .map(Query)
            .map_err(|fields| validation_failed(&fields, &request.request_id));
        async move { value }
    }
}

impl<T: Input + Send> FromRequest for Body<T> {
    fn from_request(request: &mut Request) -> impl Future<Output = Result<Self, Response>> + Send {
        let value = read_body(request).map(Body);
        async move { value }
    }
}

fn read_body<T: Input>(request: &Request) -> Result<T, Response> {
    let id = &request.request_id;
    if let Some(content_type) = request.header("content-type") {
        let essence = content_type.split(';').next().unwrap_or_default().trim();
        if !essence.eq_ignore_ascii_case("application/json") {
            let message = VAL0003.format_message(&[("content_type", essence)]);
            return Err(Response::error(415, VAL0003, &message, &[], id));
        }
    }
    let value = json::parse(&request.body).map_err(|cause| {
        let message = VAL0002.format_message(&[("cause", &cause)]);
        Response::error(400, VAL0002, &message, &[], id)
    })?;
    T::from_json(value).map_err(|fields| validation_failed(&fields, id))
}

fn validation_failed(fields: &FieldErrors, request_id: &str) -> Response {
    let names: Vec<&str> = fields.iter().map(|f| f.field.as_str()).collect();
    let message = VAL0001.format_message(&[("names", &names.join(", "))]);
    Response::error(422, VAL0001, &message, fields, request_id)
}

/// `a=1&b=x%20y` gives `("a", "1")` and `("b", "x y")`; `+` is a space.
fn query_pairs(query: &str) -> Vec<(String, String)> {
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
    fn from_request(request: &mut Request) -> impl Future<Output = Result<Self, Response>> + Send {
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
        let response = u32::from_params(&mut req).unwrap_err();
        assert_eq!(response.status, 404);
        assert_eq!(
            String::from_utf8(response.body).unwrap(),
            r#"{"error":{"code":"HTTP0011","message":"`id` in the path is not a valid whole number: `abc`","request_id":"req_000001"}}"#
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
