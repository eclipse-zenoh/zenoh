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
//! [Click here for Zenoh's documentation](../zenoh/index.html)
use proc_macro::TokenStream;
use quote::quote;
use zenoh_keyexpr::format::{
    macro_support::{self, SegmentBuilder},
    KeFormat,
};

const RUSTC_VERSION: &str = include_str!(concat!(env!("OUT_DIR"), "/version.rs"));

#[proc_macro]
pub fn rustc_version_release(_tokens: TokenStream) -> TokenStream {
    let release = RUSTC_VERSION
        .split('\n')
        .filter_map(|l| {
            let line = l.trim();
            if line.is_empty() {
                None
            } else {
                Some(line)
            }
        })
        .find_map(|l| l.strip_prefix("release: "))
        .unwrap();
    let commit = RUSTC_VERSION
        .split('\n')
        .filter_map(|l| {
            let line = l.trim();
            if line.is_empty() {
                None
            } else {
                Some(line)
            }
        })
        .find_map(|l| l.strip_prefix("commit-hash: "))
        .unwrap();
    (quote! {(#release, #commit)}).into()
}

#[proc_macro_attribute]
pub fn unstable(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let item = proc_macro2::TokenStream::from(item);
    TokenStream::from(quote! {
        #[cfg(feature = "unstable")]
        /// <div class="stab unstable">
        ///   <span class="emoji">🔬</span>
        ///   This API has been marked as unstable: it works as advertised, but we may change it in a future release.
        ///   To use it, you must enable zenoh's <code>unstable</code> feature flag.
        /// </div>
        ///
        #item
    })
}

fn keformat_support(source: &str) -> proc_macro2::TokenStream {
    let format = match KeFormat::new(&source) {
        Ok(format) => format,
        Err(e) => panic!("{}", e),
    };
    let specs = unsafe { macro_support::specs(&format) };
    let len = specs.len();
    let setters = specs.iter().map(|spec| {
        let id = &source[spec.spec_start..(spec.spec_start + spec.id_end as usize)];
        let set_id = quote::format_ident!("{}", id);
        quote! {
            pub fn #set_id <S: ::core::fmt::Display>(&mut self, value: S) -> Result<&mut Self, ::zenoh::key_expr::format::FormatSetError> {
                match self.0.set(#id, value) {
                    Ok(_) => Ok(self),
                    Err(e) => Err(e)
                }
            }
        }
    });
    let getters = specs.iter().map(|spec| {
        let id = &source[spec.spec_start..(spec.spec_start + spec.id_end as usize)];
        let get_id = quote::format_ident!("{}", id);
        quote! {
            pub fn #get_id (&self) -> Option<& ::zenoh::key_expr::keyexpr> {
                unsafe {self._0.get(#id).unwrap_unchecked()}
            }
        }
    });
    let segments = specs.iter().map(|spec| {
        let SegmentBuilder {
            segment_start,
            prefix_end,
            spec_start,
            id_end,
            pattern_end,
            spec_end,
            segment_end,
        } = spec;
        quote! {
            ::zenoh::key_expr::format::macro_support::SegmentBuilder {
                segment_start: #segment_start,
                prefix_end: #prefix_end,
                spec_start: #spec_start,
                id_end: #id_end,
                pattern_end: #pattern_end,
                spec_end: #spec_end,
                segment_end: #segment_end,
            },
        }
    });

    let format_doc = format!("The `{source}` format, as a zero-sized-type.");
    let formatter_doc = format!("And instance of a formatter for `{source}`.");

    quote! {
            use ::zenoh::Result as ZResult;
            const FORMAT_INNER: ::zenoh::key_expr::format::KeFormat<'static, [::zenoh::key_expr::format::Segment<'static>; #len]> = unsafe {
                ::zenoh::key_expr::format::macro_support::const_new(#source, [#(#segments)*])
            };
            #[doc = #format_doc]
            #[derive(Copy, Clone, Hash)]
            pub struct Format;

            #[doc = #formatter_doc]
            #[derive(Clone)]
            pub struct Formatter(::zenoh::key_expr::format::KeFormatter<'static, [::zenoh::key_expr::format::Segment<'static>; #len]>);
            impl ::core::fmt::Debug for Format {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Debug::fmt(&FORMAT_INNER, f)
                }
            }
            impl ::core::fmt::Debug for Formatter {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Debug::fmt(&self.0, f)
                }
            }
            impl ::core::fmt::Display for Format {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Display::fmt(&FORMAT_INNER, f)
                }
            }
            impl ::core::ops::Deref for Format {
                type Target = ::zenoh::key_expr::format::KeFormat<'static, [::zenoh::key_expr::format::Segment<'static>; #len]>;
                fn deref(&self) -> &Self::Target {&FORMAT_INNER}
            }
            impl ::core::ops::Deref for Formatter {
                type Target = ::zenoh::key_expr::format::KeFormatter<'static, [::zenoh::key_expr::format::Segment<'static>; #len]>;
                fn deref(&self) -> &Self::Target {&self.0}
            }
            impl ::core::ops::DerefMut for Formatter {
                fn deref_mut(&mut self) -> &mut Self::Target {&mut self.0}
            }
            impl Formatter {
                #(#setters)*
            }
            pub struct Parsed<'s>{_0: ::zenoh::key_expr::format::Parsed<'s, [::zenoh::key_expr::format::Segment<'s>; #len]>}
            impl<'s> ::core::ops::Deref for Parsed<'s> {
                type Target = ::zenoh::key_expr::format::Parsed<'s, [::zenoh::key_expr::format::Segment<'s>; #len]>;
                fn deref(&self) -> &Self::Target {&self._0}
            }
            impl Parsed<'_> {
                #(#getters)*
            }
            impl Format {
                pub fn formatter() -> Formatter {
                    Formatter(Format.formatter())
                }
                pub fn parse<'s>(target: &'s ::zenoh::key_expr::keyexpr) -> ZResult<Parsed<'s>> {
                    Ok(Parsed{_0: Format.parse(target)?})
                }
                pub fn into_inner(self) -> ::zenoh::key_expr::format::KeFormat<'static, [::zenoh::key_expr::format::Segment<'static>; #len]> {
                    FORMAT_INNER
                }
            }
            pub fn formatter() -> Formatter {
                Format::formatter()
            }
            pub fn parse<'s>(target: &'s ::zenoh::key_expr::keyexpr) -> ZResult<Parsed<'s>> {
                Format::parse(target)
            }
    }
}

struct FormatDeclaration {
    vis: syn::Visibility,
    name: syn::Ident,
    lit: syn::LitStr,
}
impl syn::parse::Parse for FormatDeclaration {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let vis = input.parse()?;
        let name = input.parse()?;
        let _: syn::Token!(:) = input.parse()?;
        let lit = input.parse()?;
        Ok(FormatDeclaration { vis, name, lit })
    }
}
struct FormatDeclarations(syn::punctuated::Punctuated<FormatDeclaration, syn::Token!(,)>);
impl syn::parse::Parse for FormatDeclarations {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(Self(input.parse_terminated(FormatDeclaration::parse)?))
    }
}

/// Create format modules from a format specification.
///
/// `kedefine!($($vis $ident: $lit),*)` will validate each `$lit` to be a valid KeFormat, and declare a module called `$ident` with `$vis` visibility at its call-site for each format.
///  The modules contain the following elements:
/// - `Format`, a zero-sized type that represents your format.
/// - `formatter()`, a function that constructs a `Formatter` specialized for your format:
///     - for every spec in your format, `Formatter` will have a method named after the spec's `id` that lets you set a value for that field of your format. These methods will return `Result<&mut Formatter, FormatError>`.
/// - `parse(target: &keyexpr) -> ZResult<Parsed<'_>>` will parse the provided key expression according to your format. Just like `KeFormat::parse`, parsing is lazy: each field will match the smallest subsection of your `target` that is included in its pattern.
///     - like `Formatter`, `Parsed` will have a method named after each spec's `id` that returns `Option<&keyexpr>`. That `Option` will only be `None` if the spec's format was `**` and matched a sequence of 0 chunks.
#[proc_macro]
pub fn kedefine(tokens: TokenStream) -> TokenStream {
    let declarations: FormatDeclarations = syn::parse(tokens).unwrap();
    let content = declarations.0.into_iter().map(|FormatDeclaration { vis, name, lit }|
    {
    let source = lit.value();
    let docstring = format!(
        r"The module associated with the `{source}` format, it contains:
- `Format`, a zero-sized type that represents your format.
- `formatter()`, a function that constructs a `Formatter` specialized for your format:
    - for every spec in your format, `Formatter` will have a method named after the spec's `id` that lets you set a value for that field of your format. These methods will return `Result<&mut Formatter, FormatError>`.
- `parse(target: &keyexpr) -> ZResult<Parsed<'_>>` will parse the provided key expression according to your format. Just like `KeFormat::parse`, parsing is lazy: each field will match the smallest subsection of your `target` that is included in its pattern.
    - like `Formatter`, `Parsed` will have a method named after each spec's `id` that returns `Option<&keyexpr>`. That `Option` will only be `None` if the spec's format was `**` and matched a sequence of 0 chunks."
    );
    let support = keformat_support(&source);
    quote! {
        #[doc = #docstring]
        #vis mod #name{
            #support
        }
    }});
    quote!(#(#content)*).into()
}

struct FormatUsage {
    id: syn::Expr,
    assigns: Vec<(syn::Expr, syn::Expr)>,
}
impl syn::parse::Parse for FormatUsage {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let id = input.parse()?;
        let mut assigns = Vec::new();
        if !input.is_empty() {
            input.parse::<syn::Token!(,)>()?;
        }
        assigns.extend(
            input
                .parse_terminated::<_, syn::Token!(,)>(syn::Expr::parse)?
                .into_iter()
                .map(|a| match a {
                    syn::Expr::Assign(a) => (*a.left, *a.right),
                    a => (a.clone(), a),
                }),
        );
        Ok(FormatUsage { id, assigns })
    }
}

/// Write a set of values into a `Formatter`, stopping as soon as a value doesn't fit the specification for its field.
/// Contrary to `keformat` doesn't build the Formatter into a Key Expression.
///
/// `kewrite!($formatter, $($ident [= $expr]),*)` will attempt to write `$expr` into their respective `$ident` fields for `$formatter`.  
/// `$formatter` must be an expression that dereferences to `&mut Formatter`.  
/// `$expr` must resolve to a value that implements `core::fmt::Display`.  
/// `$expr` defaults to `$ident` if omitted.
///
/// This macro always results in an expression that resolves to `Result<&mut Formatter, FormatSetError>`.
#[proc_macro]
pub fn kewrite(tokens: TokenStream) -> TokenStream {
    let FormatUsage { id, assigns } = syn::parse(tokens).unwrap();
    let mut sets = None;
    for (l, r) in assigns.iter().rev() {
        if let Some(set) = sets {
            sets = Some(quote!(.#l(#r).and_then(|x| x #set)));
        } else {
            sets = Some(quote!(.#l(#r)));
        }
    }
    quote!(#id #sets).into()
}

/// Write a set of values into a `Formatter` and then builds it into an `OwnedKeyExpr`, stopping as soon as a value doesn't fit the specification for its field.
///
/// `keformat!($formatter, $($ident [= $expr]),*)` will attempt to write `$expr` into their respective `$ident` fields for `$formatter`.  
/// `$formatter` must be an expression that dereferences to `&mut Formatter`.  
/// `$expr` must resolve to a value that implements `core::fmt::Display`.  
/// `$expr` defaults to `$ident` if omitted.
///
/// This macro always results in an expression that resolves to `ZResult<OwnedKeyExpr>`, and leaves `$formatter` in its written state.
#[proc_macro]
pub fn keformat(tokens: TokenStream) -> TokenStream {
    let formatted: proc_macro2::TokenStream = kewrite(tokens).into();
    quote!(match #formatted {
        Ok(ok) => ok.build(),
        Err(e) => Err(e.into()),
    })
    .into()
}
