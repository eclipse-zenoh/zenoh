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
use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use syn::{parse_macro_input, parse_quote, Attribute, Error, Item, ItemImpl, LitStr, TraitItem};
use zenoh_keyexpr::{
    format::{
        macro_support::{self, SegmentBuilder},
        KeFormat,
    },
    key_expr::keyexpr,
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

/// An enumeration of items which can be annotated with `#[zenoh_macros::unstable_doc]`, #[zenoh_macros::unstable]`, `#[zenoh_macros::internal]`
#[allow(clippy::large_enum_variant)]
enum AnnotableItem {
    /// Wrapper around [`syn::Item`].
    Item(Item),
    /// Wrapper around [`syn::TraitItem`].
    TraitItem(TraitItem),
}

macro_rules! parse_annotable_item {
    ($tokens:ident) => {{
        let item: Item = parse_macro_input!($tokens as Item);

        if matches!(item, Item::Verbatim(_)) {
            let tokens = TokenStream::from(item.to_token_stream());
            let trait_item: TraitItem = parse_macro_input!(tokens as TraitItem);

            if matches!(trait_item, TraitItem::Verbatim(_)) {
                Err(Error::new_spanned(
                    trait_item,
                    "the `unstable` proc-macro attribute only supports items and trait items",
                ))
            } else {
                Ok(AnnotableItem::TraitItem(trait_item))
            }
        } else {
            Ok(AnnotableItem::Item(item))
        }
    }};
}

impl AnnotableItem {
    /// Mutably borrows the attribute list of this item.
    fn attributes_mut(&mut self) -> Result<&mut Vec<Attribute>, Error> {
        match self {
            AnnotableItem::Item(item) => match item {
                Item::Const(item) => Ok(&mut item.attrs),
                Item::Enum(item) => Ok(&mut item.attrs),
                Item::ExternCrate(item) => Ok(&mut item.attrs),
                Item::Fn(item) => Ok(&mut item.attrs),
                Item::ForeignMod(item) => Ok(&mut item.attrs),
                Item::Impl(item) => Ok(&mut item.attrs),
                Item::Macro(item) => Ok(&mut item.attrs),
                Item::Mod(item) => Ok(&mut item.attrs),
                Item::Static(item) => Ok(&mut item.attrs),
                Item::Struct(item) => Ok(&mut item.attrs),
                Item::Trait(item) => Ok(&mut item.attrs),
                Item::TraitAlias(item) => Ok(&mut item.attrs),
                Item::Type(item) => Ok(&mut item.attrs),
                Item::Union(item) => Ok(&mut item.attrs),
                Item::Use(item) => Ok(&mut item.attrs),
                other => Err(Error::new_spanned(
                    other,
                    "item is not supported by the `unstable` or `internal` proc-macro attribute",
                )),
            },
            AnnotableItem::TraitItem(trait_item) => match trait_item {
                TraitItem::Const(trait_item) => Ok(&mut trait_item.attrs),
                TraitItem::Fn(trait_item) => Ok(&mut trait_item.attrs),
                TraitItem::Type(trait_item) => Ok(&mut trait_item.attrs),
                TraitItem::Macro(trait_item) => Ok(&mut trait_item.attrs),
                other => Err(Error::new_spanned(
                    other,
                    "item is not supported by the `unstable` or `internal` proc-macro attribute",
                )),
            },
        }
    }

    /// Converts this item to a `proc_macro2::TokenStream`.
    fn to_token_stream(&self) -> proc_macro2::TokenStream {
        match self {
            AnnotableItem::Item(item) => item.to_token_stream(),
            AnnotableItem::TraitItem(trait_item) => trait_item.to_token_stream(),
        }
    }
}

#[proc_macro_attribute]
/// Adds only piece of documentation about the item being unstable but no unstable attribute itself.
/// This is useful when the whole crate is supposed to be used in unstable mode only, it makes sense
/// to mention it in documentation for the crate items, but not to add `#[cfg(feature = "unstable")]` to every item.
pub fn unstable_doc(_attr: TokenStream, tokens: TokenStream) -> TokenStream {
    let mut item = match parse_annotable_item!(tokens) {
        Ok(item) => item,
        Err(err) => return err.into_compile_error().into(),
    };

    let attrs = match item.attributes_mut() {
        Ok(attrs) => attrs,
        Err(err) => return err.into_compile_error().into(),
    };

    if attrs.iter().any(is_doc_attribute) {
        let mut pushed = false;
        let oldattrs = std::mem::take(attrs);
        for attr in oldattrs {
            if is_doc_attribute(&attr) && !pushed {
                attrs.push(attr);
                // See: https://doc.rust-lang.org/rustdoc/how-to-write-documentation.html#adding-a-warning-block
                let message = "<div class=\"warning\">This API has been marked as <strong>unstable</strong>: it works as advertised, but it may be changed in a future release.</div>";
                let note: Attribute = parse_quote!(#[doc = #message]);
                attrs.push(note);
                pushed = true;
            } else {
                attrs.push(attr);
            }
        }
    }

    TokenStream::from(item.to_token_stream())
}

#[proc_macro_attribute]
/// Adds a `#[cfg(feature = "unstable")]` attribute to the item and appends piece of documentation about the item being unstable.
pub fn unstable(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    let tokens = unstable_doc(attr, tokens);
    let mut item = match parse_annotable_item!(tokens) {
        Ok(item) => item,
        Err(err) => return err.into_compile_error().into(),
    };

    let attrs = match item.attributes_mut() {
        Ok(attrs) => attrs,
        Err(err) => return err.into_compile_error().into(),
    };

    let feature_gate: Attribute = parse_quote!(#[cfg(feature = "unstable")]);
    attrs.push(feature_gate);

    TokenStream::from(item.to_token_stream())
}

// FIXME(fuzzypixelz): refactor `unstable` macro to accept arguments
#[proc_macro_attribute]
pub fn internal_config(args: TokenStream, tokens: TokenStream) -> TokenStream {
    let tokens = unstable_doc(args, tokens);
    let mut item = match parse_annotable_item!(tokens) {
        Ok(item) => item,
        Err(err) => return err.into_compile_error().into(),
    };

    let attrs = match item.attributes_mut() {
        Ok(attrs) => attrs,
        Err(err) => return err.into_compile_error().into(),
    };

    let feature_gate: Attribute = parse_quote!(#[cfg(feature = "internal_config")]);
    let hide_doc: Attribute = parse_quote!(#[doc(hidden)]);
    attrs.push(feature_gate);
    attrs.push(hide_doc);

    TokenStream::from(item.to_token_stream())
}

#[proc_macro_attribute]
/// Adds a `#[cfg(feature = "internal")]` and `#[doc(hidden)]` attributes to the item.
pub fn internal(_attr: TokenStream, tokens: TokenStream) -> TokenStream {
    let mut item = match parse_annotable_item!(tokens) {
        Ok(item) => item,
        Err(err) => return err.into_compile_error().into(),
    };

    let attrs = match item.attributes_mut() {
        Ok(attrs) => attrs,
        Err(err) => return err.into_compile_error().into(),
    };

    let feature_gate: Attribute = parse_quote!(#[cfg(feature = "internal")]);
    let hide_doc: Attribute = parse_quote!(#[doc(hidden)]);
    attrs.push(feature_gate);
    attrs.push(hide_doc);

    TokenStream::from(item.to_token_stream())
}

/// Returns `true` if the attribute is a `#[doc = "..."]` attribute.
fn is_doc_attribute(attr: &Attribute) -> bool {
    attr.path()
        .get_ident()
        .is_some_and(|ident| &ident.to_string() == "doc")
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
        let source = &source[spec.spec_start..spec.spec_end];
        let id = &source[..(spec.id_end as usize)];
        let get_id = quote::format_ident!("{}", id);
        let pattern = unsafe {
            keyexpr::from_str_unchecked(if spec.pattern_end != u16::MAX {
                &source[(spec.id_end as usize + 1)..(spec.spec_start + spec.pattern_end as usize)]
            } else {
                &source[(spec.id_end as usize + 1)..]
            })
        };
        let doc = format!("Get the parsed value for `{id}`.\n\nThis value is guaranteed to be a valid key expression intersecting with `{pattern}`");
        if pattern.as_bytes() == b"**" {
            quote! {
                #[doc = #doc]
                /// Since the pattern is `**`, this may return `None` if the pattern didn't consume any chunks.
                pub fn #get_id (&self) -> Option<& ::zenoh::key_expr::keyexpr> {
                    unsafe {
                        let s =self._0.get(#id).unwrap_unchecked();
                        (!s.is_empty()).then(|| ::zenoh::key_expr::keyexpr::from_str_unchecked(s))
                    }
                }
            }
        } else {
            quote! {
                #[doc = #doc]
                pub fn #get_id (&self) -> &::zenoh::key_expr::keyexpr {
                    unsafe {::zenoh::key_expr::keyexpr::from_str_unchecked(self._0.get(#id).unwrap_unchecked())}
                }
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
        Ok(Self(input.parse_terminated(
            FormatDeclaration::parse,
            syn::Token![,],
        )?))
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
///     - like `Formatter`, `Parsed` will have a method named after each spec's `id` that returns `&keyexpr`; except for specs whose pattern was `**`, these will return an `Option<&keyexpr>`, where `None` signifies that the pattern was matched by an empty list of chunks.
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
    - like `Formatter`, `Parsed` will have a method named after each spec's `id` that returns `&keyexpr`; except for specs whose pattern was `**`, these will return an `Option<&keyexpr>`, where `None` signifies that the pattern was matched by an empty list of chunks."
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
                .parse_terminated(syn::Expr::parse, syn::Token![,])?
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

/// Equivalent to [`keyexpr::new`](zenoh_keyexpr::keyexpr::new), but the check is run at compile-time and will throw a compile error in case of failure.
#[proc_macro]
pub fn ke(tokens: TokenStream) -> TokenStream {
    let value: LitStr = syn::parse(tokens).unwrap();
    let ke = value.value();
    match zenoh_keyexpr::keyexpr::new(&ke) {
        Ok(_) => quote!(unsafe { zenoh::key_expr::keyexpr::from_str_unchecked(#ke)}).into(),
        Err(e) => panic!("{}", e),
    }
}

/// Equivalent to [`nonwild_keyexpr::new`](zenoh_keyexpr::nonwild_keyexpr::new), but the check is run at compile-time and will throw a compile error in case of failure.
#[proc_macro]
pub fn nonwild_ke(tokens: TokenStream) -> TokenStream {
    let value: LitStr = syn::parse(tokens).unwrap();
    let ke = value.value();
    match zenoh_keyexpr::nonwild_keyexpr::new(&ke) {
        Ok(_) => quote!(unsafe { zenoh::key_expr::nonwild_keyexpr::from_str_unchecked(#ke)}).into(),
        Err(e) => panic!("{}", e),
    }
}

mod zenoh_runtime_derive;
use syn::DeriveInput;
use zenoh_runtime_derive::{derive_generic_runtime_param, derive_register_param};

/// Make the underlying struct `Param` be generic over any `T` satisfying a generated `trait DefaultParam { fn param() -> Param; }`
/// ```rust,ignore
/// #[derive(GenericRuntimeParam)]
/// struct Param {
///    ...
/// }
/// ```
#[proc_macro_derive(GenericRuntimeParam)]
pub fn generic_runtime_param(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input: DeriveInput = syn::parse_macro_input!(input);
    derive_generic_runtime_param(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Register the input `Enum` with the struct `Param` specified in the param attribute
/// ```rust,ignore
/// #[derive(RegisterParam)]
/// #[param(Param)]
/// enum Enum {
///    ...
/// }
/// ```
#[proc_macro_derive(RegisterParam, attributes(alias, param))]
pub fn register_param(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input: DeriveInput = syn::parse_macro_input!(input);
    derive_register_param(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Macro `#[internal_trait]` should precede
/// `impl Trait for Struct { ... }`
///
/// This macro wraps the implementations of "internal" tratis.
///
/// These traits are used to group set of functions which should be implemented
/// together and with the same prototype. E.g. `QoSBuilderTrait` provides set of
/// setters (`congestion_control`, `priority`, `express`) and we should not
/// forget to implement all these setters for each entity which supports
/// QoS functionality.
///
/// The traits mechanism is a good way to group functions. But additional traits
/// adds extra burden to end user who have to import it every time.
///
/// The macro `internal_trait` solves this problem by adding
/// methods with same names as in trait to structure implementation itself,
/// making them available to user without additional trait import.
///
#[proc_macro_attribute]
pub fn internal_trait(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemImpl);
    let trait_path = &input.trait_.as_ref().unwrap().1;
    let struct_path = &input.self_ty;
    let generics = &input.generics;
    // let struct_lifetime = get_type_path_lifetime(struct_path);

    let mut struct_methods = quote! {};
    for item_fn in input.items.iter() {
        if let syn::ImplItem::Fn(method) = item_fn {
            let method_name = &method.sig.ident;
            let method_generic_params = &method.sig.generics.params;
            let method_generic_params = if method_generic_params.is_empty() {
                quote! {}
            } else {
                quote! {<#method_generic_params>}
            };
            let method_args = &method.sig.inputs;
            let method_output = &method.sig.output;
            let where_clause = &method.sig.generics.where_clause;
            let mut method_call_args = quote! {};
            for arg in method_args.iter() {
                match arg {
                    syn::FnArg::Receiver(_) => {
                        method_call_args.extend(quote! { self, });
                    }
                    syn::FnArg::Typed(pat_type) => {
                        let pat = &pat_type.pat;
                        method_call_args.extend(quote! { #pat, });
                    }
                }
            }
            let mut attributes = quote! {};
            for attr in &method.attrs {
                attributes.extend(quote! {
                    #attr
                });
            }
            // call corresponding trait method from struct method
            struct_methods.extend(quote! {
                #attributes
                pub fn #method_name #method_generic_params (#method_args) #method_output #where_clause {
                    <#struct_path as #trait_path>::#method_name(#method_call_args)
                }
            });
        }
    }
    let struct_methods_output = quote! {
        impl #generics #struct_path {
            #struct_methods
        }
    };
    (quote! {
        #input
        #struct_methods_output
    })
    .into()
}
