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
use quote::{quote, ToTokens};
use syn::{parse_macro_input, parse_quote, Attribute, Error, Item, LitStr, TraitItem};
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

/// An enumeration of items which can be annotated with `#[zenoh_macros::unstable]`, #[zenoh_macros::unstable]`, `#[zenoh_macros::internal]`
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
/// to mention it in dcoumentation for the crate items, but not to add `#[cfg(feature = "unstable")]` to every item.
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
        // See: https://doc.rust-lang.org/rustdoc/how-to-write-documentation.html#adding-a-warning-block
        let message = "<div class=\"warning\">This API has been marked as <strong>unstable</strong>: it works as advertised, but it may be changed in a future release.</div>";
        let note: Attribute = parse_quote!(#[doc = #message]);
        attrs.push(note);
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

mod zenoh_runtime_derive;
use syn::DeriveInput;
use zenoh_runtime_derive::{derive_generic_runtime_param, derive_register_param};

/// Make the underlying struct `Param` be generic over any `T` satifying a generated `trait DefaultParam { fn param() -> Param; }`
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
