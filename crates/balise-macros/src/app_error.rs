//! `#[derive(AppError)]`: an application's own errors, with their codes, converted into
//! `balise::Error` by `?`.
//!
//! Each variant registers its code, so that `balise explain ORD0001` finds it and two enums
//! cannot declare the same code. The conversion keeps where the error was raised.

use std::collections::HashSet;

use balise_core::error_codes::{CMP0011, RESERVED_PREFIXES};
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Expr, Fields, Lit, LitInt, LitStr};

use crate::error;

pub(crate) fn expand(input: DeriveInput) -> syn::Result<TokenStream> {
    let name = &input.ident;
    if !input.generics.params.is_empty() {
        return Err(invalid(
            input.generics.span(),
            "it cannot have generics or lifetimes",
        ));
    }
    let Data::Enum(data) = &input.data else {
        return Err(invalid(
            name.span(),
            "it derives on an enum, one variant per error",
        ));
    };

    let private = quote!(::balise::__private);
    let mut seen = HashSet::new();
    let mut registrations = Vec::new();
    let mut arms = Vec::new();
    for variant in &data.variants {
        let ident = &variant.ident;
        let declared = Declared::parse(variant)?;
        if !seen.insert(declared.code.value()) {
            return Err(invalid(
                declared.code.span(),
                &format!("`{}` is used by two variants", declared.code.value()),
            ));
        }
        let bindings: Vec<_> = match &variant.fields {
            Fields::Unit => Vec::new(),
            Fields::Named(fields) => fields
                .named
                .iter()
                .filter_map(|f| f.ident.clone())
                .collect(),
            Fields::Unnamed(_) => {
                return Err(invalid(
                    variant.fields.span(),
                    "a variant has no fields or named fields: `TooMany { max: u32 }`",
                ));
            }
        };

        let code_static = format_ident!("__BALISE_CODE_{}", declared.code.value());
        let registration = format_ident!("__BALISE_APP_CODE_{}", declared.code.value());
        let code = &declared.code;
        let title = format!("{name}::{ident}");
        let (message, fix) = (&declared.message, &declared.fix);
        let explanation = doc_comment(variant);
        registrations.push(quote! {
            static #code_static: #private::ErrorCode =
                #private::ErrorCode::app(#code, #title, #message, #fix, #explanation);
            #private::__register_app_code!(#registration, &#code_static);
        });

        let status = &declared.status;
        let pattern = match &variant.fields {
            Fields::Unit => quote!(#name::#ident),
            _ => quote!(#name::#ident { #(#bindings),* }),
        };
        arms.push(quote! {
            #pattern => #private::Error::app(
                #status,
                &#code_static,
                ::std::format!(#message),
                ::std::format!(#fix),
            ),
        });
    }

    Ok(quote! {
        const _: () = {
            #(#registrations)*

            impl ::core::convert::From<#name> for #private::Error {
                #[track_caller]
                #[allow(unused_variables)] // a field may be used by the message only, or not at all
                fn from(error: #name) -> Self {
                    match error {
                        #(#arms)*
                    }
                }
            }
        };
    })
}

/// What a variant's `#[error(…)]` declares.
struct Declared {
    status: LitInt,
    code: LitStr,
    message: LitStr,
    fix: LitStr,
}

impl Declared {
    fn parse(variant: &syn::Variant) -> syn::Result<Declared> {
        let mut attrs = variant.attrs.iter().filter(|a| a.path().is_ident("error"));
        let Some(attr) = attrs.next() else {
            return Err(invalid(
                variant.ident.span(),
                &format!("`{}` has no `#[error(…)]` attribute", variant.ident),
            ));
        };
        if let Some(second) = attrs.next() {
            return Err(invalid(
                second.span(),
                "a variant has one `#[error(…)]` attribute",
            ));
        }
        let (mut status, mut code, mut message, mut fix) = (None, None, None, None);
        attr.parse_nested_meta(|meta| {
            let key = meta
                .path
                .get_ident()
                .map(|i| i.to_string())
                .unwrap_or_default();
            let value: Expr = meta.value()?.parse()?;
            let Expr::Lit(syn::ExprLit { lit, .. }) = value else {
                return Err(invalid(value.span(), &format!("`{key}` takes a literal")));
            };
            match (key.as_str(), lit) {
                ("status", Lit::Int(lit)) => status = Some(lit),
                ("code", Lit::Str(lit)) => code = Some(lit),
                ("message", Lit::Str(lit)) => message = Some(lit),
                ("fix", Lit::Str(lit)) => fix = Some(lit),
                ("status" | "code" | "message" | "fix", lit) => {
                    return Err(invalid(lit.span(), &format!("`{key}` has the wrong type")));
                }
                _ => {
                    return Err(invalid(
                        meta.path.span(),
                        &format!(
                            "unknown key `{key}`; expected `status`, `code`, `message`, `fix`"
                        ),
                    ));
                }
            }
            Ok(())
        })?;
        let missing = |key: &str| {
            invalid(
                attr.meta.span(),
                &format!("`{}` has no `{key}`", variant.ident),
            )
        };
        let declared = Declared {
            status: status.ok_or_else(|| missing("status"))?,
            code: code.ok_or_else(|| missing("code"))?,
            message: message.ok_or_else(|| missing("message"))?,
            fix: fix.ok_or_else(|| missing("fix"))?,
        };
        let status: u16 = declared.status.base10_parse()?;
        if !(400..500).contains(&status) {
            return Err(invalid(
                declared.status.span(),
                &format!("the status is {status}; an application error is a 4xx"),
            ));
        }
        check_code(&declared.code)?;
        Ok(declared)
    }
}

/// A prefix of 2 to 4 uppercase letters that is not Balise's, then four digits.
fn check_code(code: &LitStr) -> syn::Result<()> {
    let text = code.value();
    let prefix_len = text.bytes().take_while(u8::is_ascii_uppercase).count();
    let (prefix, digits) = text.split_at(prefix_len);
    let shaped = (2..=4).contains(&prefix.len())
        && digits.len() == 4
        && digits.bytes().all(|b| b.is_ascii_digit());
    if !shaped {
        return Err(invalid(
            code.span(),
            &format!("`{text}` is not 2 to 4 uppercase letters and four digits, like `ORD0001`"),
        ));
    }
    if RESERVED_PREFIXES.contains(&prefix) {
        return Err(invalid(
            code.span(),
            &format!("the prefix `{prefix}` is Balise's; use one of your own, like `ORD`"),
        ));
    }
    Ok(())
}

/// The variant's doc comment, the explanation shown by `balise explain`.
fn doc_comment(variant: &syn::Variant) -> String {
    let lines: Vec<String> = variant
        .attrs
        .iter()
        .filter(|a| a.path().is_ident("doc"))
        .filter_map(|a| match &a.meta {
            syn::Meta::NameValue(nv) => match &nv.value {
                Expr::Lit(syn::ExprLit {
                    lit: Lit::Str(s), ..
                }) => Some(
                    s.value()
                        .strip_prefix(' ')
                        .unwrap_or(&s.value())
                        .to_string(),
                ),
                _ => None,
            },
            _ => None,
        })
        .collect();
    lines.join("\n")
}

fn invalid(span: Span, reason: &str) -> syn::Error {
    error(span, CMP0011, &[("reason", reason)], &[])
}
