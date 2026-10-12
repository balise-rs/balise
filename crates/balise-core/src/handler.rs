//! What a handler receives and returns, independent of the HTTP library underneath.
//!
//! The route macro turns each handler into a [`HandlerFn`]: it extracts every parameter with
//! [`FromRequest`], calls the handler, and turns its result into a [`Response`] with
//! [`IntoResponse`]. Extractors and response types implement these traits.

use std::future::Future;
use std::pin::Pin;

use crate::error::Error;

/// The parts of a request that handlers can read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Request {
    pub method: Method,
    /// The path without the API base: `/orders/ord_01JB…`.
    pub path: String,
    /// Path parameters not read yet, in the order they appear in the route: `("id", "ord_01JB…")`.
    pub params: Vec<(String, String)>,
    /// The query string, without `?`.
    pub query: String,
    /// Header names in lowercase.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub request_id: String,
    /// Whether error responses show location, fix and cause (dev only).
    pub error_details: bool,
}

impl Request {
    /// The first value of a header, by its lowercase name.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

/// A response, before it is written to the connection.
#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(&'static str, String)>,
    pub body: Vec<u8>,
    /// The error this response answers, for the logs; never sent.
    pub error: Option<Error>,
}

impl Response {
    /// A JSON response with this status.
    pub fn json(status: u16, body: String) -> Response {
        Response {
            status,
            headers: vec![("content-type", "application/json".to_string())],
            body: body.into_bytes(),
            error: None,
        }
    }
}

/// The HTTP methods a route can declare.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Method {
    #[default]
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
        }
    }

    /// The method named `name`, if a route can declare it.
    pub fn parse(name: &str) -> Option<Method> {
        match name {
            "GET" => Some(Method::Get),
            "POST" => Some(Method::Post),
            "PUT" => Some(Method::Put),
            "PATCH" => Some(Method::Patch),
            "DELETE" => Some(Method::Delete),
            _ => None,
        }
    }
}

/// A boxed future that returns a response.
pub type ResponseFuture = Pin<Box<dyn Future<Output = Response> + Send>>;

/// A handler once its parameters and result are erased: what the router stores.
pub type HandlerFn = fn(Request) -> ResponseFuture;

/// A handler parameter that Balise builds from the request.
#[diagnostic::on_unimplemented(
    message = "CMP0006: `{Self}` cannot be a handler parameter",
    label = "Balise cannot provide this parameter",
    note = "fix: Use `Path<T>`, `Query<T>`, `Body<T>`, `RequestId`, or a service such as `Db` or `CurrentUser`.",
    note = "more: balise explain CMP0006"
)]
pub trait FromRequest: Sized {
    /// The value, or the error to answer instead (a `422`, a `401`…).
    fn from_request(request: &mut Request) -> impl Future<Output = Result<Self, Error>> + Send;
}

/// Extracts one parameter. The route macro calls it once per parameter, so that a type
/// Balise cannot provide is reported once, on the parameter itself.
pub async fn extract<T: FromRequest>(request: &mut Request) -> Result<T, Response> {
    match T::from_request(request).await {
        Ok(value) => Ok(value),
        Err(error) => Err(error.respond(request)),
    }
}

/// Turns a handler's result into a response, reported on the return type when it cannot.
pub fn respond<T: IntoResponse>(value: T, request: &Request) -> Response {
    value.into_response(request)
}

/// A handler's result, turned into a response.
#[diagnostic::on_unimplemented(
    message = "CMP0007: `{Self}` cannot be returned from a handler",
    label = "not a response type",
    note = "fix: Return a struct that derives `Output`, a `Vec` of them, `Created<T>`, `NoContent` or `Redirect`, or a `Result` of one of these.",
    note = "more: balise explain CMP0007"
)]
pub trait IntoResponse {
    /// The request gives errors their request ID and how much detail they show.
    fn into_response(self, request: &Request) -> Response;
}
