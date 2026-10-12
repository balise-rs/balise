//! `#[balise::state]`: a function that creates one value of shared state at startup.

use balise_core::error_codes::CMP0015;
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{GenericArgument, ItemFn, PathArguments, ReturnType, Type};

use crate::error;

pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    if !attr.is_empty() {
        return Err(invalid(attr.span(), "the attribute takes no arguments"));
    }
    let function: ItemFn =
        syn::parse2(item.clone()).map_err(|_| invalid(item.span(), "it goes on a function"))?;
    let name = &function.sig.ident;
    if let Some(input) = function.sig.inputs.first() {
        return Err(invalid(
            input.span(),
            "it takes no parameters; read other state with `State<T>` where it is used",
        ));
    }
    if !function.sig.generics.params.is_empty() {
        return Err(invalid(
            function.sig.generics.span(),
            "it cannot have generics",
        ));
    }
    let ReturnType::Type(_, output) = &function.sig.output else {
        return Err(invalid(
            name.span(),
            "it returns the value it creates: `fn name() -> Type`",
        ));
    };
    // `Result<T>` or `Result<T, E>`: the state is `T`, and an error stops the startup.
    let (value_type, fallible) = match result_value(output) {
        Some(inner) => (inner, true),
        None => ((**output).clone(), false),
    };

    let private = quote!(::balise::__private);
    let call = match function.sig.asyncness.is_some() {
        true => quote!(#name().await),
        false => quote!(#name()),
    };
    let value = match fallible {
        true => quote! {
            match #call {
                ::core::result::Result::Ok(value) => value,
                ::core::result::Result::Err(error) => {
                    return ::core::result::Result::Err(::core::convert::Into::into(error));
                }
            }
        },
        false => call,
    };
    let create = format_ident!("__balise_create_{}", name);
    let registration = format_ident!("__BALISE_STATE_{}", name.to_string().to_uppercase());
    let name_text = name.to_string();
    let erase = quote_spanned!(value_type.span()=> #private::erase::<#value_type>);

    Ok(quote! {
        #function

        #[doc(hidden)]
        fn #create() -> #private::StateFuture {
            ::std::boxed::Box::pin(async {
                ::core::result::Result::Ok(#erase(#value))
            })
        }

        #private::__register_state!(#registration, #private::StateInit {
            key: #private::StateKey::of::<#value_type>,
            function: ::core::concat!(::core::module_path!(), "::", #name_text),
            file: ::core::file!(),
            line: ::core::line!(),
            create: #create,
        });
    })
}

/// For `Result<T>` or `Result<T, E>`, the `T`.
fn result_value(ty: &Type) -> Option<Type> {
    let Type::Path(path) = ty else { return None };
    let segment = path.path.segments.last()?;
    if segment.ident != "Result" {
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

fn invalid(span: Span, reason: &str) -> syn::Error {
    error(span, CMP0015, &[("reason", reason)], &[])
}
