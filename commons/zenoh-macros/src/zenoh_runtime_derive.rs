//
// Copyright (c) 2023 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//

//! ⚠️ WARNING ⚠️
//!
//! This crate is intended for Zenoh's internal use.
//!
//! [Click here for Zenoh's documentation](https://docs.rs/zenoh/latest/zenoh)
use proc_macro2::TokenStream;
use quote::{format_ident, quote, ToTokens};
use syn::{
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
    token::Comma,
    Data, DataEnum, DataStruct, DeriveInput, Expr, ExprLit, Fields, Ident, Lit, LitStr, Meta,
    MetaNameValue, Token, Variant,
};

struct SerdeAttribute {
    rename: LitStr,
}

impl Parse for SerdeAttribute {
    fn parse(tokens: ParseStream) -> syn::Result<Self> {
        let parsed = Punctuated::<MetaNameValue, Token![,]>::parse_terminated(tokens)?;
        for kv in parsed {
            if kv.path.is_ident("rename") {
                if let Expr::Lit(ExprLit {
                    lit: Lit::Str(str), ..
                }) = kv.value
                {
                    return Ok(SerdeAttribute { rename: str });
                }
            }
        }
        Err(syn::Error::new(
            tokens.span(),
            "Invalid rename detected, expect #[serde(rename = \"name\")]",
        ))
    }
}

fn parse_variants(
    variants: &Punctuated<Variant, Comma>,
) -> (Vec<&Ident>, Vec<TokenStream>, Vec<TokenStream>) {
    let mut name_vec = Vec::new();
    let mut alias_vec = Vec::new();
    let mut param_vec = Vec::new();

    for var in variants {
        let name = &var.ident;
        let alias = var
            .attrs
            .iter()
            .find(|attr| attr.path().is_ident("serde"))
            .map(|attr| {
                attr.parse_args::<SerdeAttribute>()
                    .map(|x| {
                        x.rename
                            .value()
                            .parse::<TokenStream>()
                            .expect("Failed to convert LitStr to Ident TokenStream")
                    })
                    .unwrap_or_else(syn::Error::into_compile_error)
            })
            .ok_or(syn::Error::new(
                var.span(),
                "#[serde(alias = \"name\")] is missing",
            ))
            .unwrap_or_else(syn::Error::into_compile_error);
        let param = var
            .attrs
            .iter()
            .find(|attr| attr.path().is_ident("param"))
            .map(|attr| match &attr.meta {
                Meta::List(list) => list.tokens.to_string().replace('=', ":").parse(),
                _ => panic!("Invalid"),
            })
            .unwrap_or("".parse())
            .unwrap();

        name_vec.push(name);
        alias_vec.push(alias);
        param_vec.push(param);
    }

    (name_vec, alias_vec, param_vec)
}

fn generate_declare_param(
    meta_param: &Ident,
    variant_names: &[&Ident],
    aliases: &Vec<TokenStream>,
    params: &[TokenStream],
) -> TokenStream {
    let default_param_of_variant: Vec<_> = variant_names
        .iter()
        .map(|name| format_ident!("DefaultParamOf{}", name))
        .collect();
    let params_with_default = params.iter().map(|x| {
        if x.to_string() != "" {
            quote!(#x, ..Default::default())
        } else {
            quote!(..Default::default())
        }
    });
    quote! {
        trait DefaultParam {
            fn param() -> #meta_param;
        }

        #(
            // Declare struct DefaultParamOf`#variant_name`
            #[derive(Debug, Clone, Copy)]
            struct #default_param_of_variant;

            // impl `DefaultParam` for `DefaultParamOf#variant_name`
            impl DefaultParam for #default_param_of_variant {
                fn param() -> #meta_param {
                    #meta_param {
                        #params_with_default
                    }
                }
            }
        )*

        // An internal helper struct for parsing the RuntimeParam
        #[derive(Deserialize, Debug, Clone, Copy)]
        #[serde(deny_unknown_fields)]
        struct AbstractRuntimeParam {
            #(
                #[serde(default)]
                #aliases: RuntimeParamHelper<#default_param_of_variant>,
            )*
        }

        // AbstractRuntimeParam => GlobalRuntimeParam, extract fields from AbstractRuntimeParam
        impl From<AbstractRuntimeParam> for GlobalRuntimeParam {
            fn from(value: AbstractRuntimeParam) -> Self {
                Self {
                    #(
                        #aliases: value.#aliases.into(),
                    )*
                }
            }
        }

        /// A global runtime parameter for zenoh runtimes
        // pub is needed within lazy_static
        pub struct GlobalRuntimeParam {
            #(
                #aliases: #meta_param,
            )*
        }

    }
}

pub(crate) fn derive_register_param(input: DeriveInput) -> Result<TokenStream, syn::Error> {
    // enum representing the runtime
    let enum_name = &input.ident;

    // Parse the parameter to register, called meta_param
    let attr = input
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("param"))
        .ok_or(syn::Error::new(
            input.span(),
            "Expected attribute #[param(StructName)] is missing.",
        ))?;
    let meta_param = match &attr.meta {
        Meta::List(list) => syn::parse2::<Ident>(list.tokens.to_token_stream()),
        _ => Err(syn::Error::new(
            attr.span(),
            format!("Failed to parse #[param({})]", attr.to_token_stream()),
        )),
    }?;

    // Parse the variants and associated parameters of the enum
    let variants = match input.data {
        Data::Enum(DataEnum { variants, .. }) => variants,
        _ => unimplemented!("Only enum is supported."),
    };
    let (variant_names, aliases, params) = parse_variants(&variants);
    let declare_param_quote =
        generate_declare_param(&meta_param, &variant_names, &aliases, &params);

    let tokens = quote! {

        use ron::{extensions::Extensions, options::Options};
        use #enum_name::*;

        lazy_static! {
            // We need to hold the reference of ZENOH_RUNTIME_ENV_STRING to prevent the issue of
            // "returning a value referencing data owned by the current function"
            pub static ref ZENOH_RUNTIME_ENV_STRING: String = env::var(ZENOH_RUNTIME_ENV).unwrap_or("()".to_string());
            pub static ref ZRUNTIME_PARAM: GlobalRuntimeParam = Options::default()
                .with_default_extension(Extensions::IMPLICIT_SOME)
                .from_str::<AbstractRuntimeParam>(&ZENOH_RUNTIME_ENV_STRING)
                .unwrap()
                .into();
        }

        #declare_param_quote

        impl #enum_name {
            #[doc = concat!("Create an iterator from ", stringify!(#enum_name))]
            pub fn iter() -> impl Iterator<Item = #enum_name> {
                [#(#variant_names,)*].into_iter()
            }

            #[doc = "Initialize the tokio runtime according to the given config"]
            fn init(&self) -> Result<Runtime> {
                match self {
                    #(
                        #variant_names => {
                            ZRUNTIME_PARAM.#aliases.build(#variant_names)
                        },
                    )*
                }
            }

        }

        #[doc = concat!(
            "Borrow the underlying `",
            stringify!(#meta_param),
            "` from ",
            stringify!(#enum_name)
        )]
        impl Borrow<#meta_param> for #enum_name {
            fn borrow(&self) -> &#meta_param {
                match self {
                    #(
                        #variant_names => {
                            &ZRUNTIME_PARAM.#aliases
                        },
                    )*
                }
            }
        }
        impl std::fmt::Display for #enum_name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    #(
                        #variant_names => {
                            write!(f, stringify!(#aliases))
                        },
                    )*

                }
            }
        }
    };
    Ok(tokens)
}

pub(crate) fn derive_generic_runtime_param(input: DeriveInput) -> Result<TokenStream, syn::Error> {
    let meta_param = &input.ident;
    let fields = match &input.data {
        Data::Struct(DataStruct {
            fields: Fields::Named(fields),
            ..
        }) => &fields.named,
        _ => {
            return Err(syn::Error::new(
                input.span(),
                "Expected a struct with named fields",
            ))
        }
    };
    let field_names: Vec<_> = fields.iter().map(|field| &field.ident).collect();
    let field_types: Vec<_> = fields.iter().map(|field| &field.ty).collect();
    let helper_name = format_ident!("{}Helper", meta_param);

    let tokens = quote! {

        use std::marker::PhantomData;

        // Declare a helper struct to be generic over any T implementing DefaultParam
        #[derive(Deserialize, Debug, Clone, Copy)]
        #[serde(deny_unknown_fields, default)]
        struct #helper_name<T>
        where
            T: DefaultParam,
        {
            #(
                #field_names: #field_types,
            )*
            #[serde(skip)]
            _phantom: PhantomData<T>,
        }

        impl<T> From<#meta_param> for #helper_name<T>
        where
            T: DefaultParam,
        {
            fn from(value: #meta_param) -> Self {
                let #meta_param { #(#field_names,)* } = value;
                Self {
                    #(#field_names,)*
                    _phantom: PhantomData,
                }
            }
        }

        impl<T> From<#helper_name<T>> for #meta_param
        where
            T: DefaultParam,
        {
            fn from(value: #helper_name<T>) -> Self {
                let #helper_name { #(#field_names,)* .. } = value;
                Self {
                    #(#field_names,)*
                }
            }
        }

        impl<T> Default for #helper_name<T>
        where
            T: DefaultParam,
        {
            fn default() -> Self {
                T::param().into()
            }
        }

    };

    Ok(tokens)
}
