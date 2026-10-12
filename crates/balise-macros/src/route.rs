//! Route attributes: `#[get("/orders/{id}")]` and the other methods.

use balise_core::error_codes::{CMP0001, CMP0002, CMP0003, CMP0004, CMP0005, CMP0018};
use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote, quote_spanned};
use syn::parse::{ParseStream, Parser};
use syn::spanned::Spanned;
use syn::{FnArg, GenericArgument, ItemFn, LitInt, LitStr, PathArguments, ReturnType, Type};

use crate::error;

pub(crate) fn route(method: &str, attr: TokenStream, item: TokenStream) -> TokenStream {
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

/// What a route attribute holds: `"/orders/{id}"`, then `page(default = …, max = …)`.
struct Attribute {
    path: LitStr,
    page: Option<Page>,
}

/// `page(default = 500, max = 5000)`, either bound optional.
struct Page {
    span: Span,
    default: Option<u32>,
    max: Option<u32>,
}

/// Parses the attribute of `function`, every error with its code.
fn parse_attribute(function: &str, input: ParseStream) -> syn::Result<Attribute> {
    let path: LitStr = input.parse().map_err(|problem| {
        let reason = "the path must be a string literal";
        error(problem.span(), CMP0005, &[("reason", reason)], &[])
    })?;
    let invalid_page = |span: Span, reason: &str| {
        error(
            span,
            CMP0018,
            &[("function", function), ("reason", reason)],
            &[],
        )
    };
    let mut page = None;
    while !input.is_empty() {
        input.parse::<syn::Token![,]>()?;
        if input.is_empty() {
            break;
        }
        let name: syn::Ident = input.parse()?;
        if name != "page" {
            let reason = format!(
                "unknown option `{name}`: after the path, a route takes `page(default = …, max = …)`"
            );
            return Err(error(name.span(), CMP0005, &[("reason", &reason)], &[]));
        }
        if page.is_some() {
            return Err(invalid_page(name.span(), "it is declared twice"));
        }
        let content;
        syn::parenthesized!(content in input);
        let bounds = content.parse_terminated(
            |input: ParseStream| {
                let key: syn::Ident = input.parse()?;
                input.parse::<syn::Token![=]>()?;
                let value: LitInt = input.parse()?;
                Ok((key, value))
            },
            syn::Token![,],
        )?;
        let (mut default, mut max) = (None, None);
        for (key, value) in bounds {
            let slot = match key.to_string().as_str() {
                "default" => &mut default,
                "max" => &mut max,
                other => {
                    let reason = format!("unknown bound `{other}`; expected `default` or `max`");
                    return Err(invalid_page(key.span(), &reason));
                }
            };
            let number = value.base10_parse::<u32>().ok().filter(|n| *n > 0);
            let number = number
                .ok_or_else(|| invalid_page(value.span(), "bounds are whole numbers from 1"))?;
            if slot.replace(number).is_some() {
                let reason = format!("`{key}` is declared twice");
                return Err(invalid_page(key.span(), &reason));
            }
        }
        if default.is_none() && max.is_none() {
            return Err(invalid_page(
                name.span(),
                "it needs `default = …`, `max = …` or both",
            ));
        }
        if let (Some(default), Some(max)) = (default, max)
            && default > max
        {
            let reason = format!("`default` ({default}) is above `max` ({max})");
            return Err(invalid_page(name.span(), &reason));
        }
        page = Some(Page {
            span: name.span(),
            default,
            max,
        });
    }
    Ok(Attribute { path, page })
}

fn expand(method: &str, attr: TokenStream2, item: TokenStream2) -> syn::Result<TokenStream2> {
    let mut function: ItemFn = syn::parse2(item.clone()).map_err(|_| {
        error(
            item.span(),
            CMP0005,
            &[("reason", "a route attribute goes on a function")],
            &[],
        )
    })?;
    let function_name = function.sig.ident.to_string();
    let attribute = (|input: ParseStream| parse_attribute(&function_name, input)).parse2(attr)?;
    let path = attribute.path;
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
    // The `T` of `Body<T>`, whose files set the route's limits.
    let mut body_type = None;
    let mut reads_page = false;
    // The `T` of each `Query<T>`, which cannot have files.
    let mut queries = Vec::new();
    let mut path_values = 0;
    let mut types = Vec::new();
    // The `T` of each `State<T>`, checked at startup.
    let mut states = Vec::new();
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
            Some(("Body", _)) => {
                bodies.push(typed.ty.span());
                body_type = inner_value(&typed.ty);
            }
            Some(("Path", arity)) => path_values += arity,
            _ if is_query(&typed.ty) => queries.extend(inner_value(&typed.ty)),
            _ => {}
        }
        if let Type::Path(path) = &*typed.ty
            && path
                .path
                .segments
                .last()
                .is_some_and(|s| s.ident == "PageParams")
        {
            reads_page = true;
        }
        if let Some(state) = state_type(&typed.ty) {
            states.push(state);
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

    if let Some(page) = &attribute.page
        && !reads_page
    {
        return Err(error(
            page.span,
            CMP0018,
            &[
                ("function", &name_text),
                ("reason", "the handler does not read `page: PageParams`"),
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
    // `Body` comes last, whatever its place: a request that another parameter rejects is
    // answered before its body is read.
    let mut order: Vec<usize> = (0..types.len()).collect();
    order.sort_by_key(|i| matches!(last_segment(&types[*i]), Some(("Body", _))));
    let extractions: Vec<_> = order
        .iter()
        .map(|i| (&values[*i], &types[*i], &unwrap[*i]))
        .map(|(value, ty, unwrap)| {
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

    let files = match &body_type {
        Some(body) => quote_spanned!(body.span()=> <#body as #private::Input>::FILES),
        None => quote!(&[]),
    };
    let page = match (reads_page, &attribute.page) {
        (false, _) => quote!(::core::option::Option::None),
        (true, page) => {
            let bound = |bound: Option<u32>| match bound {
                Some(n) => quote!(::core::option::Option::Some(#n)),
                None => quote!(::core::option::Option::None),
            };
            let default = bound(page.as_ref().and_then(|p| p.default));
            let max = bound(page.as_ref().and_then(|p| p.max));
            quote!(::core::option::Option::Some(#private::PageDeclared {
                default: #default,
                max: #max,
            }))
        }
    };

    // Checked by `cargo check`, unlike an assertion inside a generic function.
    let no_files = queries.iter().map(|query| {
        quote_spanned! {query.span()=>
            const _: () = ::core::assert!(
                <#query as #private::Input>::FILES.is_empty(),
                "CMP0017: an `Input` with `File` fields is read with `Body<T>`, never `Query<T>`; more: balise explain CMP0017",
            );
        }
    });

    Ok(quote! {
        #function

        #(#no_files)*

        #[doc(hidden)]
        fn #invoke(mut request: #private::Request) -> #private::ResponseFuture {
            ::std::boxed::Box::pin(async move {
                #(#extractions)*
                #private::respond::<#output>(#name(#(#values),*).await, &request)
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
            states: &[#(#private::StateKey::of::<#states>),*],
            files: #files,
            page: #page,
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

fn is_query(ty: &Type) -> bool {
    matches!(ty, Type::Path(path) if path.path.segments.last().is_some_and(|s| s.ident == "Query"))
}

/// For `State<T>`, the `T`.
fn state_type(ty: &Type) -> Option<Type> {
    let Type::Path(path) = ty else { return None };
    let segment = path.path.segments.last()?;
    if segment.ident != "State" {
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
