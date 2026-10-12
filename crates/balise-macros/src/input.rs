//! `#[derive(Input)]`: reads a struct from a JSON body or a query string, with its rules.
//!
//! The generated code reads each field through `Fields`, checks its rules, and gathers every
//! error before answering, so that a client can fix all of them at once.

use balise_core::error_codes::{CMP0008, CMP0017};
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, quote_spanned};
use syn::ext::IdentExt;
use syn::meta::ParseNestedMeta;
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Expr, Fields, Ident, Lit, LitInt, LitStr};

use crate::error;

const FIELD_ATTRIBUTES: &str = "`min`, `max`, `length(min = …, max = …)`, `email`, `url`, `one_of(\"a\", \"b\")`, `default = …`, `secret`, and for files `max_size = …`, `types(…)`, `max_count = …`";
const STRUCT_ATTRIBUTES: &str = "`model = …`, `allow_unknown_fields`";

pub(crate) fn expand(input: DeriveInput) -> syn::Result<TokenStream> {
    let name = &input.ident;
    let fields = named_fields(&input, "Input")?;

    let mut model = None;
    let mut allow_unknown = false;
    for attr in input.attrs.iter().filter(|a| a.path().is_ident("input")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("model") {
                model = Some(meta.value()?.parse::<syn::Path>()?);
            } else if meta.path.is_ident("allow_unknown_fields") {
                allow_unknown = true;
            } else {
                return Err(unknown(&meta, "Input", STRUCT_ATTRIBUTES));
            }
            Ok(())
        })?;
    }

    let private = quote!(::balise::__private);
    let mut reads = Vec::new();
    let mut locals = Vec::new();
    let mut idents = Vec::new();
    let mut names = Vec::new();
    let mut files = Vec::new();
    for field in fields {
        let ident = field.ident.as_ref().expect("named fields");
        let ty = &field.ty;
        let json_name = ident.unraw().to_string();
        let local = format_ident!("__balise_{}", json_name);
        let rules = FieldRules::parse(field)?;
        let file = rules.file(field, &json_name)?;

        let read = match &rules.default {
            Some(default) => {
                let default = match default {
                    Expr::Lit(lit) if matches!(lit.lit, Lit::Str(_)) => {
                        quote_spanned!(default.span()=> ::std::string::String::from(#default))
                    }
                    _ => quote!(#default),
                };
                quote_spanned!(ty.span()=> __balise_fields.read_or::<#ty>(#json_name, || #default))
            }
            None if file.is_some() => {
                quote_spanned!(ty.span()=> __balise_fields.read_file::<#ty>(#json_name))
            }
            None => quote_spanned!(ty.span()=> __balise_fields.read::<#ty>(#json_name)),
        };
        let inner = match file {
            Some(_) => quote!(#private::FileField::inner),
            None => quote!(#private::InputField::inner),
        };
        if let Some(spec) = file {
            files.push(spec);
        }
        let checks = rules.checks(&private);
        // Without rules, nothing reads the value: a type Balise cannot read is reported once.
        let checks = (!checks.is_empty()).then(|| {
            quote! {
                if let ::core::option::Option::Some(__balise_value) =
                    #local.as_ref().and_then(#inner)
                {
                    #(__balise_fields.check(#json_name, #checks);)*
                }
            }
        });
        let secret = rules
            .secret
            .then(|| quote!(__balise_fields.secret(#json_name);));
        reads.push(quote! {
            #secret
            let #local = #read;
            #checks
        });
        locals.push(local);
        idents.push(ident);
        names.push(json_name);
    }

    // `#[input(model = …)]` takes effect with the ORM; until then, the path is only checked.
    let model = model.map(|path| {
        quote! {
            const _: () = {
                let _ = ::core::marker::PhantomData::<#path>;
            };
        }
    });

    Ok(quote! {
        impl #private::Input for #name {
            const FILES: &'static [#private::FileSpec] = &[#(#files),*];

            fn from_fields(
                mut __balise_fields: #private::Fields,
            ) -> ::core::result::Result<Self, #private::FieldErrors> {
                #(#reads)*
                let __balise_errors = __balise_fields.finish(&[#(#names),*], #allow_unknown);
                match (#(#locals,)*) {
                    (#(::core::option::Option::Some(#locals),)*) if __balise_errors.is_empty() => {
                        ::core::result::Result::Ok(Self { #(#idents: #locals),* })
                    }
                    _ => ::core::result::Result::Err(__balise_errors),
                }
            }
        }
        #model
    })
}

/// The fields of a struct with named fields and no generics, or a `CMP0008` error.
pub(crate) fn named_fields<'a>(
    input: &'a DeriveInput,
    derive: &str,
) -> syn::Result<&'a syn::punctuated::Punctuated<syn::Field, syn::token::Comma>> {
    if !input.generics.params.is_empty() {
        return Err(invalid(
            input.generics.span(),
            derive,
            "it cannot have generics or lifetimes",
        ));
    }
    match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => Ok(&fields.named),
            _ => Err(invalid(
                input.ident.span(),
                derive,
                "its fields must have names, like `struct CreateOrder { quantity: u32 }`",
            )),
        },
        _ => Err(invalid(
            input.ident.span(),
            derive,
            "it derives on a struct only",
        )),
    }
}

pub(crate) fn invalid(span: Span, derive: &str, reason: &str) -> syn::Error {
    error(
        span,
        CMP0008,
        &[("derive", derive), ("reason", reason)],
        &[],
    )
}

pub(crate) fn unknown(meta: &ParseNestedMeta, derive: &str, expected: &str) -> syn::Error {
    let name = meta
        .path
        .get_ident()
        .map(|i| i.to_string())
        .unwrap_or_default();
    let reason = format!("unknown attribute `{name}`; expected one of {expected}");
    invalid(meta.path.span(), derive, &reason)
}

/// The rules declared on one field, each with the span of its attribute.
#[derive(Default)]
struct FieldRules {
    min: Option<(Ident, Lit)>,
    max: Option<(Ident, Lit)>,
    length: Option<(Ident, Option<LitInt>, Option<LitInt>)>,
    email: Option<Ident>,
    url: Option<Ident>,
    one_of: Option<(Ident, Vec<LitStr>)>,
    default: Option<Expr>,
    secret: bool,
    max_size: Option<(Ident, LitStr)>,
    types: Option<(Ident, Vec<LitStr>)>,
    max_count: Option<(Ident, LitInt)>,
}

impl FieldRules {
    fn parse(field: &syn::Field) -> syn::Result<FieldRules> {
        let mut rules = FieldRules::default();
        for attr in field.attrs.iter().filter(|a| a.path().is_ident("input")) {
            attr.parse_nested_meta(|meta| {
                let Some(ident) = meta.path.get_ident().cloned() else {
                    return Err(unknown(&meta, "Input", FIELD_ATTRIBUTES));
                };
                let twice = || {
                    invalid(
                        ident.span(),
                        "Input",
                        &format!("`{ident}` is declared twice"),
                    )
                };
                match ident.to_string().as_str() {
                    "min" | "max" => {
                        let bound: Lit = meta.value()?.parse()?;
                        let slot = if ident == "min" {
                            &mut rules.min
                        } else {
                            &mut rules.max
                        };
                        if slot.replace((ident.clone(), bound)).is_some() {
                            return Err(twice());
                        }
                    }
                    "length" => {
                        let (mut min, mut max) = (None, None);
                        meta.parse_nested_meta(|bound| {
                            if bound.path.is_ident("min") {
                                min = Some(bound.value()?.parse::<LitInt>()?);
                            } else if bound.path.is_ident("max") {
                                max = Some(bound.value()?.parse::<LitInt>()?);
                            } else {
                                return Err(invalid(
                                    bound.path.span(),
                                    "Input",
                                    "`length` takes `min = …` and `max = …`",
                                ));
                            }
                            Ok(())
                        })?;
                        if min.is_none() && max.is_none() {
                            return Err(invalid(
                                ident.span(),
                                "Input",
                                "`length` needs `min = …`, `max = …` or both",
                            ));
                        }
                        if rules.length.replace((ident.clone(), min, max)).is_some() {
                            return Err(twice());
                        }
                    }
                    "email" | "url" => {
                        let slot = if ident == "email" {
                            &mut rules.email
                        } else {
                            &mut rules.url
                        };
                        if slot.replace(ident.clone()).is_some() {
                            return Err(twice());
                        }
                    }
                    "one_of" => {
                        let content;
                        syn::parenthesized!(content in meta.input);
                        let values: Vec<LitStr> = content
                            .parse_terminated(|input| input.parse::<LitStr>(), syn::Token![,])?
                            .into_iter()
                            .collect();
                        if values.is_empty() {
                            return Err(invalid(
                                ident.span(),
                                "Input",
                                "`one_of` needs at least one value: `one_of(\"a\", \"b\")`",
                            ));
                        }
                        if rules.one_of.replace((ident.clone(), values)).is_some() {
                            return Err(twice());
                        }
                    }
                    "secret" => rules.secret = true,
                    "max_size" => {
                        let size: LitStr = meta.value()?.parse()?;
                        if rules.max_size.replace((ident.clone(), size)).is_some() {
                            return Err(twice());
                        }
                    }
                    "max_count" => {
                        let count: LitInt = meta.value()?.parse()?;
                        if rules.max_count.replace((ident.clone(), count)).is_some() {
                            return Err(twice());
                        }
                    }
                    "types" => {
                        let content;
                        syn::parenthesized!(content in meta.input);
                        let values: Vec<LitStr> = content
                            .parse_terminated(|input| input.parse::<LitStr>(), syn::Token![,])?
                            .into_iter()
                            .collect();
                        if rules.types.replace((ident.clone(), values)).is_some() {
                            return Err(twice());
                        }
                    }
                    "default" => {
                        let value: Expr = meta.value()?.parse()?;
                        if rules.default.replace(value).is_some() {
                            return Err(twice());
                        }
                    }
                    _ => return Err(unknown(&meta, "Input", FIELD_ATTRIBUTES)),
                }
                Ok(())
            })?;
        }
        Ok(rules)
    }

    /// For a field that holds files, its `FileSpec`; `None` for other fields. Everything is
    /// checked here, so that a wrong declaration never reaches the app.
    fn file(&self, field: &syn::Field, name: &str) -> syn::Result<Option<TokenStream>> {
        let declared = self.max_size.is_some() || self.types.is_some() || self.max_count.is_some();
        let shape = file_shape(&field.ty);
        let invalid = |span: Span, reason: &str| {
            error(span, CMP0017, &[("field", name), ("reason", reason)], &[])
        };
        if !declared {
            return match shape {
                Some(_) => Err(invalid(
                    field.ty.span(),
                    "a file field declares its `max_size` and `types`",
                )),
                None => Ok(None),
            };
        }
        let Some(shape) = shape else {
            return Err(invalid(
                field.ty.span(),
                "`max_size`, `types` and `max_count` go on a field of type `File`, `Option<File>` or `Vec<File>`",
            ));
        };
        let Some((_, size)) = &self.max_size else {
            return Err(invalid(
                field.ty.span(),
                "it needs `max_size = \"…\"`, such as `\"5MB\"`",
            ));
        };
        let max_size = balise_core::text::parse_size(&size.value())
            .filter(|size| *size > 0)
            .ok_or_else(|| {
                invalid(
                    size.span(),
                    "`max_size` is a size such as `\"512KB\"`, `\"5MB\"` or `\"2GB\"`",
                )
            })? as u64;
        let Some((types_ident, types)) = &self.types else {
            return Err(invalid(
                field.ty.span(),
                "it needs `types(…)`, such as `types(\"image/png\", \"image/jpeg\")`",
            ));
        };
        if types.is_empty() {
            return Err(invalid(
                types_ident.span(),
                "`types(…)` lists at least one type",
            ));
        }
        for mime in types {
            if !balise_core::file::is_known(&mime.value()) {
                let known = balise_core::file::known_types().join(", ");
                let reason = format!(
                    "Balise cannot recognize `{}` by its content; supported types: {known}",
                    mime.value()
                );
                return Err(invalid(mime.span(), &reason));
            }
        }
        let max_count: u32 = match (shape, &self.max_count) {
            (FileShape::Many, Some((_, count))) => match count.base10_parse::<u32>() {
                Ok(count) if count > 0 => count,
                _ => {
                    return Err(invalid(
                        count.span(),
                        "`max_count` is a whole number from 1",
                    ));
                }
            },
            (FileShape::Many, None) => {
                return Err(invalid(
                    field.ty.span(),
                    "a `Vec<File>` declares how many files it takes: `max_count = 4`",
                ));
            }
            (FileShape::One, Some((ident, _))) => {
                return Err(invalid(
                    ident.span(),
                    "`max_count` goes on a `Vec<File>`; a `File` takes one file",
                ));
            }
            (FileShape::One, None) => 1,
        };
        let rules = [
            self.min.as_ref().map(|(i, _)| i),
            self.max.as_ref().map(|(i, _)| i),
            self.email.as_ref(),
            self.url.as_ref(),
            self.one_of.as_ref().map(|(i, _)| i),
        ];
        if let Some(ident) = rules.into_iter().flatten().next() {
            let reason = format!("`{ident}` does not apply to files");
            return Err(invalid(ident.span(), &reason));
        }
        if let Some(default) = &self.default {
            return Err(invalid(
                default.span(),
                "a file field has no default; use `Option<File>`",
            ));
        }
        let private = quote!(::balise::__private);
        Ok(Some(quote! {
            #private::FileSpec {
                field: #name,
                max_size: #max_size,
                types: &[#(#types),*],
                max_count: #max_count,
            }
        }))
    }

    /// One call per rule, each spanned on its attribute so that a rule on the wrong type
    /// points at the rule.
    fn checks(&self, private: &TokenStream) -> Vec<TokenStream> {
        let mut checks = Vec::new();
        let rules = quote!(#private::rules);
        if let Some((ident, bound)) = &self.min {
            checks.push(quote_spanned!(ident.span()=> #rules::min(__balise_value, #bound)));
        }
        if let Some((ident, bound)) = &self.max {
            checks.push(quote_spanned!(ident.span()=> #rules::max(__balise_value, #bound)));
        }
        if let Some((ident, min, max)) = &self.length {
            let bound = |b: &Option<LitInt>| match b {
                Some(b) => quote!(::core::option::Option::Some(#b)),
                None => quote!(::core::option::Option::None),
            };
            let (min, max) = (bound(min), bound(max));
            checks.push(quote_spanned!(ident.span()=> #rules::length(__balise_value, #min, #max)));
        }
        if let Some(ident) = &self.email {
            checks.push(quote_spanned!(ident.span()=> #rules::email(__balise_value)));
        }
        if let Some(ident) = &self.url {
            checks.push(quote_spanned!(ident.span()=> #rules::url(__balise_value)));
        }
        if let Some((ident, values)) = &self.one_of {
            checks.push(
                quote_spanned!(ident.span()=> #rules::one_of(__balise_value, &[#(#values),*])),
            );
        }
        checks
    }
}

/// How many files a field type holds, read from its syntax.
#[derive(Clone, Copy)]
enum FileShape {
    /// `File` or `Option<File>`.
    One,
    /// `Vec<File>`.
    Many,
}

fn file_shape(ty: &syn::Type) -> Option<FileShape> {
    let syn::Type::Path(path) = ty else {
        return None;
    };
    let segment = path.path.segments.last()?;
    if segment.ident == "File" {
        return Some(FileShape::One);
    }
    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    let Some(syn::GenericArgument::Type(inner)) = args.args.first() else {
        return None;
    };
    match (segment.ident.to_string().as_str(), file_shape(inner)) {
        ("Option", Some(FileShape::One)) => Some(FileShape::One),
        ("Vec", Some(FileShape::One)) => Some(FileShape::Many),
        _ => None,
    }
}
