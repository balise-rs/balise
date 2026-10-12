//! What a handler can return, besides an `Output`: the status comes from the type.

use crate::error::Error;
use crate::handler::{IntoResponse, Request, Response};
use crate::output::{Output, OutputValue};

/// `201 Created`, with the created resource: `Ok(Created(order.into()))`.
pub struct Created<T>(pub T);

/// `204 No Content`: the request succeeded and there is nothing to send back.
pub struct NoContent;

/// `303 See Other`: the client goes to another page.
pub struct Redirect {
    location: String,
}

impl Redirect {
    /// Sends the client to `location`, such as `/orders`.
    pub fn to(location: impl Into<String>) -> Redirect {
        Redirect {
            location: location.into(),
        }
    }
}

fn json<T: OutputValue + ?Sized>(status: u16, value: &T) -> Response {
    let mut body = String::new();
    value.write_json(&mut body);
    Response::json(status, body)
}

impl<T: Output> IntoResponse for Created<T> {
    fn into_response(self, _: &Request) -> Response {
        json(201, &self.0)
    }
}

impl IntoResponse for NoContent {
    fn into_response(self, _: &Request) -> Response {
        Response {
            status: 204,
            headers: Vec::new(),
            body: Vec::new(),
            error: None,
        }
    }
}

impl IntoResponse for Redirect {
    fn into_response(self, _: &Request) -> Response {
        Response {
            status: 303,
            headers: vec![("location", self.location)],
            body: Vec::new(),
            error: None,
        }
    }
}

impl<T: Output> IntoResponse for Vec<T> {
    fn into_response(self, _: &Request) -> Response {
        json(200, &self)
    }
}

impl<T: IntoResponse> IntoResponse for Result<T, Error> {
    fn into_response(self, request: &Request) -> Response {
        match self {
            Ok(value) => value.into_response(request),
            Err(error) => error.respond(request),
        }
    }
}
