//! What a handler receives and returns, independent of the HTTP library underneath.
//!
//! The route macro turns each handler into a [`HandlerFn`]: it extracts every parameter with
//! [`FromRequest`], calls the handler, and turns its result into a [`Response`] with
//! [`IntoResponse`]. Extractors and response types implement these traits.

use std::future::Future;
use std::pin::Pin;

/// The parts of a request that handlers can read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub method: Method,
    /// The path without the API base: `/orders/ord_01JB…`.
    pub path: String,
    /// Path parameters, in the order they appear in the route: `("id", "ord_01JB…")`.
    pub params: Vec<(String, String)>,
}

/// A response, before it is written to the connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(&'static str, String)>,
    pub body: Vec<u8>,
}

impl Response {
    /// A JSON response with this status.
    pub fn json(status: u16, body: String) -> Response {
        Response {
            status,
            headers: vec![("content-type", "application/json".to_string())],
            body: body.into_bytes(),
        }
    }
}

/// The HTTP methods a route can declare.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Method {
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
pub trait FromRequest: Sized {
    /// The value, or the response to send instead (a `422`, a `401`…).
    fn from_request(request: &mut Request) -> impl Future<Output = Result<Self, Response>> + Send;
}

/// A handler's result, turned into a response.
pub trait IntoResponse {
    fn into_response(self) -> Response;
}
