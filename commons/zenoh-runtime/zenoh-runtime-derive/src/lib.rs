use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
    token::Comma,
    Data, DataEnum, DataStruct, DeriveInput, Expr, ExprLit, Fields, Ident, Lit, LitStr, Meta,
    MetaNameValue, Token, Variant,
};

struct SerdeAttribute {
    alias: LitStr,
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
                    return Ok(SerdeAttribute { alias: str });
                }
            }
        }
        Err(syn::Error::new(
            tokens.span(),
            "Invalid alias detected, expect #[serde(rename = \"name\")]",
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
                        x.alias
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

            impl DefaultParam for #helper_names {
                fn param() -> RuntimeParam {
                    RuntimeParam {
                        #params_with_default
                    }
                }
            }
        )*

        // pub is needed within lazy_static
        #[derive(Deserialize, Debug, Clone, Copy)]
        #[serde(deny_unknown_fields)]
        pub struct AbstractRuntimeParam {
            #(
                #[serde(default)]
                #aliases: RuntimeParamHelper<#helper_names>,
            )*
        }

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
        pub struct GlobalRuntimeParam {
            #(
                #aliases: RuntimeParam,
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
    let (variant_names, aliases, params) = parse_variants(&variants);
    let declare_param_quote = declare_param(&variant_names, &aliases, &params);
    let aliases_string = aliases.iter().map(|x| x.to_string());

    let expanded = quote! {

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
            // pub static ref ZRUNTIME_CONFIG: AbstractRuntimeParam<'static> = ron::from_str(&ZENOH_RUNTIME_ENV_STRING).unwrap();
        }

        #declare_param_quote

        impl #enum_name {
            /// Create an iterator from enum
            pub fn iter() -> impl Iterator<Item = #enum_name> {
                [#(#variant_names,)*].into_iter()
            }

            /// Initialize the tokio runtime according to the given config
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

        impl RuntimeParamTrait for #enum_name {

            fn param(&self) -> &RuntimeParam {
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
    let helper_name = format_ident!("{}Helper", &input.ident);

    quote! {

        use std::marker::PhantomData;

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

        impl<T> From<RuntimeParam> for #helper_name<T>
        where
            T: DefaultParam,
        {
            fn from(value: RuntimeParam) -> Self {
                let RuntimeParam { #(#field_names,)* } = value;
                Self {
                    #(#field_names,)*
                    _phantom: PhantomData::default(),
                }
            }
        }

        impl<T> From<#helper_name<T>> for RuntimeParam
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

    }
    .into()
}
