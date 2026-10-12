//! Procedural macros: route attributes and derives.
//!
//! The generated code only names items of `::balise::__private`, so that the facade stays the
//! single dependency of a project. Every error is a registered `CMP` code, with its fix.

use balise_core::error_codes::ErrorCode;
use proc_macro::TokenStream;
use proc_macro2::Span;

mod app_error;
mod input;
mod output;
mod route;
mod run;
mod state;

use route::route;

/// `#[get("/orders/{id}")]`: a handler for `GET` requests on this path.
#[proc_macro_attribute]
pub fn get(attr: TokenStream, item: TokenStream) -> TokenStream {
    route("Get", attr, item)
}

/// `#[post("/orders")]`: a handler for `POST` requests on this path.
#[proc_macro_attribute]
pub fn post(attr: TokenStream, item: TokenStream) -> TokenStream {
    route("Post", attr, item)
}

/// `#[put("/orders/{id}")]`: a handler for `PUT` requests on this path.
#[proc_macro_attribute]
pub fn put(attr: TokenStream, item: TokenStream) -> TokenStream {
    route("Put", attr, item)
}

/// `#[patch("/orders/{id}")]`: a handler for `PATCH` requests on this path.
#[proc_macro_attribute]
pub fn patch(attr: TokenStream, item: TokenStream) -> TokenStream {
    route("Patch", attr, item)
}

/// `#[delete("/orders/{id}")]`: a handler for `DELETE` requests on this path.
#[proc_macro_attribute]
pub fn delete(attr: TokenStream, item: TokenStream) -> TokenStream {
    route("Delete", attr, item)
}

/// `#[derive(Input)]`: the struct is read from a JSON body (`Body<T>`) or a query string
/// (`Query<T>`), with the rules declared on its fields.
#[proc_macro_derive(Input, attributes(input))]
pub fn derive_input(item: TokenStream) -> TokenStream {
    derive(item, input::expand)
}

/// `#[derive(Output)]`: the struct is sent as JSON when a handler returns it.
#[proc_macro_derive(Output, attributes(output))]
pub fn derive_output(item: TokenStream) -> TokenStream {
    derive(item, output::expand)
}

/// `#[derive(AppError)]`: an application's errors, each variant with its status, code,
/// message and fix. `?` converts them into `balise::Error`.
#[proc_macro_derive(AppError, attributes(error))]
pub fn derive_app_error(item: TokenStream) -> TokenStream {
    derive(item, app_error::expand)
}

/// `#[balise::state]`: a function that creates one value of shared state at startup, read
/// with `State<T>` by handlers and tasks.
#[proc_macro_attribute]
pub fn state(attr: TokenStream, item: TokenStream) -> TokenStream {
    let item = proc_macro2::TokenStream::from(item);
    match state::expand(attr.into(), item.clone()) {
        Ok(tokens) => tokens.into(),
        // Keep the function, so the only error shown is ours.
        Err(error) => {
            let error = error.to_compile_error();
            quote::quote!(#error #item).into()
        }
    }
}

/// Embeds the project's files and starts the app. Called by `balise::run!()`.
#[doc(hidden)]
#[proc_macro]
pub fn __run(input: TokenStream) -> TokenStream {
    let input = proc_macro2::TokenStream::from(input);
    let result = match input.is_empty() {
        true => run::expand(),
        false => Err(syn::Error::new_spanned(
            input,
            "`balise::run!()` takes no arguments",
        )),
    };
    result.unwrap_or_else(syn::Error::into_compile_error).into()
}

fn derive(
    item: TokenStream,
    expand: fn(syn::DeriveInput) -> syn::Result<proc_macro2::TokenStream>,
) -> TokenStream {
    syn::parse(item)
        .and_then(expand)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// A compile error with the code's message, fix and how to learn more.
fn error(
    span: Span,
    code: &ErrorCode,
    message_args: &[(&str, &str)],
    fix_args: &[(&str, &str)],
) -> syn::Error {
    syn::Error::new(span, code.report(message_args, fix_args))
}
