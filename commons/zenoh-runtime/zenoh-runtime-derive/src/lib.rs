use proc_macro2::TokenStream;
use quote::{format_ident, quote, ToTokens};
use syn::{
    punctuated::Punctuated, token::Comma, Data, DataEnum, DataStruct, DeriveInput, Fields, Ident,
    Meta, Variant,
};

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
            .find(|attr| attr.path().is_ident("alias"))
            .map(|attr| match &attr.meta {
                Meta::List(list) => list.tokens.to_token_stream(),
                _ => panic!("Invalid"),
            })
            .unwrap_or(name.to_string().to_lowercase().parse().unwrap());
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

fn declare_param(
    names: &[&Ident],
    aliases: &Vec<TokenStream>,
    params: &[TokenStream],
) -> TokenStream {
    let helper_names: Vec<_> = names
        .iter()
        .map(|name| format_ident!("Default{}", name))
        .collect();
    let params_with_default = params.iter().map(|x| {
        if x.to_string() != "" {
            quote!(#x, ..Default::default())
        } else {
            quote!(..Default::default())
        }
    });
    quote! {
        #(
            #[derive(Debug, Clone, Copy)]
            struct #helper_names;

            impl<'a> DefaultParam<'a> for #helper_names {
                fn param() -> RuntimeParam<'a> {
                    RuntimeParam {
                        #params_with_default
                    }
                }
            }
        )*

        // pub is needed within lazy_static
        #[derive(Serialize, Deserialize, Debug, Clone, Copy)]
        #[serde(deny_unknown_fields)]
        pub struct GenericRuntimeConfig<'a> {
            #(
                #[serde(borrow, default)]
                #aliases: GenericRuntimeParam<'a, #helper_names>,
            )*
        }
    }
}

#[proc_macro_derive(ConfigureZRuntime, attributes(alias, param))]
pub fn enum_iter(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input: DeriveInput = syn::parse_macro_input!(input);
    let enum_name = &input.ident;
    let variants = match input.data {
        Data::Enum(DataEnum { variants, .. }) => variants,
        _ => unimplemented!("Only enum is supported."),
    };
    let (names, aliases, params) = parse_variants(&variants);
    let declare_param_quote = declare_param(&names, &aliases, &params);
    let aliases_string = aliases.iter().map(|x| x.to_string());

    let expanded = quote! {

        lazy_static! {
            // We need to hold the reference of ZENOH_RUNTIME_ENV_STRING to prevent the issue of
            // "returning a value referencing data owned by the current function"
            pub static ref ZENOH_RUNTIME_ENV_STRING: String = env::var(ZENOH_RUNTIME_ENV).unwrap_or("()".to_string());
            pub static ref ZRUNTIME_CONFIG: GenericRuntimeConfig<'static> = ron::from_str(&ZENOH_RUNTIME_ENV_STRING).unwrap();
        }

        #declare_param_quote

        impl #enum_name {
            fn iter() -> impl Iterator<Item = #enum_name> {
                use #enum_name::*;
                [#(#names,)*].into_iter()
            }

            fn init(&self) -> Result<Runtime> {
                use #enum_name::*;

                // let env_str = env::var(ZENOH_RUNTIME_THREADS_ENV).unwrap_or("()".to_string());
                // let config: GenericRuntimeConfig = ron::from_str(&env_str).unwrap();
                match self {
                    #(
                        #names => {
                            let param: RuntimeParam = ZRUNTIME_CONFIG.#aliases.into();
                            param.build(#names)
                        },
                    )*
                }
            }
        }

        impl std::fmt::Display for #enum_name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                use #enum_name::*;
                match self {
                    #(
                        #names => {
                            write!(f, #aliases_string)
                        },
                    )*

                }
            }
        }
    };
    proc_macro::TokenStream::from(expanded)
}

#[proc_macro_derive(GenericRuntimeParam)]
pub fn build_generic_runtime_param(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input: DeriveInput = syn::parse_macro_input!(input);
    let fields = match &input.data {
        Data::Struct(DataStruct {
            fields: Fields::Named(fields),
            ..
        }) => &fields.named,
        _ => panic!("Expected a struct with named fields"),
    };
    let field_names: Vec<_> = fields.iter().map(|field| &field.ident).collect();
    let field_types: Vec<_> = fields.iter().map(|field| &field.ty).collect();
    let struct_name = format_ident!("Generic{}", &input.ident);

    quote! {

        use std::marker::PhantomData;

        #[derive(Serialize, Deserialize, Debug, Clone, Copy)]
        #[serde(deny_unknown_fields, default)]
        struct #struct_name<'a, T>
        where
            T: DefaultParam<'a>,
        {
            #(
                #field_names: #field_types,
            )*
            #[serde(skip)]
            _phantom: PhantomData<&'a T>,
        }

        impl<'a, T> From<RuntimeParam<'a>> for #struct_name<'a, T>
        where
            T: DefaultParam<'a>,
        {
            fn from(value: RuntimeParam<'a>) -> Self {
                let RuntimeParam { #(#field_names,)* } = value;
                Self {
                    #(#field_names,)*
                    _phantom: PhantomData::default(),
                }
            }
        }

        impl<'a, T> From<#struct_name<'a, T>> for RuntimeParam<'a>
        where
            T: DefaultParam<'a>,
        {
            fn from(value: #struct_name<'a, T>) -> Self {
                let #struct_name { #(#field_names,)* .. } = value;
                Self {
                    #(#field_names,)*
                }
            }
        }

        impl<'a, T: DefaultParam<'a>> Default for #struct_name<'a, T> {
            fn default() -> Self {
                T::param().into()
            }
        }

    }
    .into()
}
