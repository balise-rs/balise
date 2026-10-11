//! `#[derive(Output)]`: writes a struct as JSON, fields in declaration order.

use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::DeriveInput;
use syn::ext::IdentExt;
use syn::spanned::Spanned;

use crate::input::{named_fields, unknown};

pub(crate) fn expand(input: DeriveInput) -> syn::Result<TokenStream> {
    let name = &input.ident;
    let fields = named_fields(&input, "Output")?;

    let mut from = None;
    for attr in input.attrs.iter().filter(|a| a.path().is_ident("output")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("from") {
                from = Some(meta.value()?.parse::<syn::Path>()?);
                Ok(())
            } else {
                Err(unknown(&meta, "Output", "`from = …`"))
            }
        })?;
    }

    let private = quote!(::balise::__private);
    let writes = fields.iter().enumerate().map(|(i, field)| {
        let ident = field.ident.as_ref().expect("named fields");
        let json_name = ident.unraw().to_string();
        let first = i == 0;
        quote_spanned! {field.ty.span()=>
            #private::write_field(__balise_out, #first, #json_name, &self.#ident);
        }
    });

    // `#[output(from = Order)]`: `order.into()` copies the fields of the same name.
    let from = from.map(|source| {
        let copies = fields.iter().map(|field| {
            let ident = field.ident.as_ref().expect("named fields");
            quote_spanned!(ident.span()=> #ident: __balise_source.#ident)
        });
        quote! {
            impl ::core::convert::From<#source> for #name {
                fn from(__balise_source: #source) -> Self {
                    Self { #(#copies),* }
                }
            }
        }
    });

    Ok(quote! {
        impl #private::Output for #name {
            fn write_json(&self, __balise_out: &mut ::std::string::String) {
                __balise_out.push('{');
                #(#writes)*
                __balise_out.push('}');
            }
        }
        #from
    })
}
