//! Procedural macros: route attributes and derives.
//!
//! The generated code only names items of `::balise::__private`, so that the facade stays the
//! single dependency of a project. Every error is a registered `CMP` code, with its fix.

use balise_core::error_codes::{CMP0001, CMP0002, CMP0003, CMP0004, CMP0005, ErrorCode};
use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{FnArg, GenericArgument, ItemFn, LitStr, PathArguments, ReturnType, Type};

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

fn route(method: &str, attr: TokenStream, item: TokenStream) -> TokenStream {
    let item = TokenStream2::from(item);
    match expand(method, attr.into(), item.clone()) {
        Ok(tokens) => tokens.into(),
        // Keep the function, so the only error shown is ours.
        Err(error) => {
            let error = error.to_compile_error();
            quote!(#error #item).into()
        }
    }
}

fn expand(method: &str, attr: TokenStream2, item: TokenStream2) -> syn::Result<TokenStream2> {
    let path: LitStr = syn::parse2(attr.clone()).map_err(|_| {
        error(
            attr.span(),
            CMP0005,
            &[("reason", "the path must be a string literal")],
            &[],
        )
    })?;
    let mut function: ItemFn = syn::parse2(item.clone()).map_err(|_| {
        error(
            item.span(),
            CMP0005,
            &[("reason", "a route attribute goes on a function")],
            &[],
        )
    })?;
    let name = &function.sig.ident;
    let name_text = name.to_string();
    let args = [("function", name_text.as_str())];

    let params = route_params(&path.value()).map_err(|reason| {
        error(
            path.span(),
            CMP0004,
            &[("path", &path.value()), ("reason", &reason)],
            &[],
        )
    })?;

    if function.sig.asyncness.is_none() {
        return Err(error(function.sig.fn_token.span(), CMP0001, &args, &args));
    }

    let mut bodies = Vec::new();
    let mut path_values = 0;
    let mut types = Vec::new();
    // `Path`, `Query` and `Body` give the value itself: true for each parameter to unwrap.
    let mut unwrap = Vec::new();
    for input in &function.sig.inputs {
        let FnArg::Typed(typed) = input else {
            return Err(error(
                input.span(),
                CMP0005,
                &[("reason", "a handler is a free function, without `self`")],
                &[],
            ));
        };
        match last_segment(&typed.ty) {
            Some(("Body", _)) => bodies.push(typed.ty.span()),
            Some(("Path", arity)) => path_values += arity,
            _ => {}
        }
        types.push(typed.ty.clone());
        unwrap.push(inner_value(&typed.ty).is_some());
    }
    if bodies.len() > 1 {
        let count = bodies.len().to_string();
        return Err(error(
            bodies[1],
            CMP0002,
            &[("function", &name_text), ("count", &count)],
            &[],
        ));
    }
    if path_values != params.len() {
        return Err(error(
            function.sig.ident.span(),
            CMP0003,
            &[
                ("function", &name_text),
                ("path", &path.value()),
                ("found", &path_values.to_string()),
                ("expected", &params.len().to_string()),
            ],
            &[],
        ));
    }

    // The handler as compiled takes the values themselves: `id: Path<OrderId>` becomes
    // `id: OrderId`, so its body never writes `*id` or `.into_inner()`.
    for input in function.sig.inputs.iter_mut() {
        if let FnArg::Typed(typed) = input
            && let Some(inner) = inner_value(&typed.ty)
        {
            *typed.ty = inner;
        }
    }

    let invoke = format_ident!("__balise_invoke_{}", name);
    let route = format_ident!("__BALISE_ROUTE_{}", name_text.to_uppercase());
    let method = format_ident!("{}", method);
    let values: Vec<_> = (0..types.len())
        .map(|i| format_ident!("__value_{}", i))
        .collect();
    let private = quote!(::balise::__private);
    let extractions: Vec<_> = values
        .iter()
        .zip(&types)
        .zip(&unwrap)
        .map(|((value, ty), unwrap)| {
            let unwrap = match unwrap {
                true => quote!(let #value = #value.__into_value();),
                false => quote!(),
            };
            quote_spanned! {ty.span()=>
                let #value = match ::balise::__private::extract::<#ty>(&mut request).await {
                    ::std::result::Result::Ok(value) => value,
                    ::std::result::Result::Err(response) => return response,
                };
                #unwrap
            }
        })
        .collect();
    let output = match &function.sig.output {
        ReturnType::Default => quote_spanned!(function.sig.ident.span()=> ()),
        ReturnType::Type(_, ty) => quote!(#ty),
    };

    Ok(quote! {
        #function

        #[doc(hidden)]
        fn #invoke(mut request: #private::Request) -> #private::ResponseFuture {
            ::std::boxed::Box::pin(async move {
                #(#extractions)*
                #private::respond::<#output>(#name(#(#values),*).await)
            })
        }

        #private::__register_route!(#route, #private::Route {
            method: #private::Method::#method,
            path: #path,
            module: ::core::module_path!(),
            function: #name_text,
            file: ::core::file!(),
            line: ::core::line!(),
            invoke: #invoke,
        });
    })
}

/// The parameter names of a route path, or why the path is invalid.
fn route_params(path: &str) -> Result<Vec<String>, String> {
    if !path.starts_with('/') {
        return Err("it must start with `/`".into());
    }
    if path == "/" {
        return Ok(Vec::new());
    }
    if path.ends_with('/') {
        return Err("it must not end with `/`".into());
    }
    if path.contains(['?', '#']) {
        return Err("query strings and fragments are not part of a route".into());
    }
    let mut names = Vec::new();
    for segment in path[1..].split('/') {
        if segment.is_empty() {
            return Err("it has an empty segment (`//`)".into());
        }
        let opens = segment.contains('{');
        let closes = segment.contains('}');
        if !opens && !closes {
            continue;
        }
        let Some(name) = segment.strip_prefix('{').and_then(|s| s.strip_suffix('}')) else {
            return Err(format!(
                "`{segment}`: a parameter is a whole segment, such as `{{id}}`"
            ));
        };
        let snake = !name.is_empty()
            && name.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        if !snake {
            return Err(format!("`{{{name}}}`: parameter names are snake_case"));
        }
        if names.iter().any(|n| n == name) {
            return Err(format!("`{{{name}}}` appears twice"));
        }
        names.push(name.to_string());
    }
    Ok(names)
}

/// The last segment of a type path, with the number of path values it reads when it is
/// `Path<T>` (the arity of a tuple, 1 otherwise).
fn last_segment(ty: &Type) -> Option<(&'static str, usize)> {
    let Type::Path(path) = ty else { return None };
    let segment = path.path.segments.last()?;
    let name = match segment.ident.to_string().as_str() {
        "Body" => "Body",
        "Path" => "Path",
        _ => return None,
    };
    let arity = match &segment.arguments {
        PathArguments::AngleBracketed(args) => match args.args.first() {
            Some(GenericArgument::Type(Type::Tuple(tuple))) => tuple.elems.len(),
            _ => 1,
        },
        _ => 1,
    };
    Some((name, arity))
}

/// For `Path<T>`, `Query<T>` and `Body<T>`, the `T` the handler receives.
fn inner_value(ty: &Type) -> Option<Type> {
    let Type::Path(path) = ty else { return None };
    let segment = path.path.segments.last()?;
    if !matches!(
        segment.ident.to_string().as_str(),
        "Path" | "Query" | "Body"
    ) {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    match args.args.first()? {
        GenericArgument::Type(inner) => Some(inner.clone()),
        _ => None,
    }
}

/// A compile error with the code's message, fix and how to learn more.
fn error(
    span: Span,
    code: &ErrorCode,
    message_args: &[(&str, &str)],
    fix_args: &[(&str, &str)],
) -> syn::Error {
    syn::Error::new(
        span,
        format!(
            "{}: {}\n  fix: {}\n  more: balise explain {}",
            code.code(),
            code.format_message(message_args),
            code.format_fix(fix_args),
            code.code()
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::route_params;

    #[test]
    fn route_paths_are_checked() {
        assert_eq!(route_params("/"), Ok(vec![]));
        assert_eq!(
            route_params("/orders/{id}/lines/{line_id}"),
            Ok(vec!["id".into(), "line_id".into()])
        );
        for bad in [
            "orders",
            "/orders/",
            "/orders//x",
            "/orders?x",
            "/orders/id-{id}",
            "/orders/{Id}",
            "/a/{x}/b/{x}",
            "/orders/{}",
        ] {
            assert!(route_params(bad).is_err(), "{bad}");
        }
    }
}
