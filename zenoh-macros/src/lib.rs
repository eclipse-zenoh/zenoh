//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse::Parse, DeriveInput, Expr, Token};

#[allow(clippy::large_enum_variant)]
enum IntKeyAttribute {
    Skip,
    Key {
        key: Expr,
        to_cowstr: Expr,
        from_toowned_string: Expr,
        direct: bool,
    },
}
mod kw {
    syn::custom_keyword!(skip);
    syn::custom_keyword!(into);
    syn::custom_keyword!(from);
    syn::custom_keyword!(direct);
}
impl Parse for IntKeyAttribute {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut to_cowstr = None;
        let mut from_toowned_string = None;
        let mut direct = false;
        if input.parse::<kw::skip>().is_ok() {
            return Ok(IntKeyAttribute::Skip);
        }
        let key = input.parse()?;
        while input.parse::<Token![,]>().is_ok() {
            if input.parse::<kw::direct>().is_ok() {
                direct = true;
            } else if input.parse::<kw::into>().is_ok() {
                input.parse::<Token![=]>()?;
                to_cowstr = Some(input.parse()?);
            } else {
                input.parse::<kw::from>()?;
                input.parse::<Token![=]>()?;
                from_toowned_string = Some(input.parse()?);
            }
        }
        Ok(IntKeyAttribute::Key {
            key,
            to_cowstr: to_cowstr.expect("`into = <expr>` required"),
            from_toowned_string: from_toowned_string.expect("`from = <expr>` required"),
            direct,
        })
    }
}

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

#[proc_macro_derive(IntKeyMapLike, attributes(intkey))]
pub fn derive_intkey_maplike(tokens: TokenStream) -> TokenStream {
    unzip_n::unzip_n!(4);
    unzip_n::unzip_n!(5);
    unzip_n::unzip_n!(6);
    let input = syn::parse_macro_input!(tokens as DeriveInput);
    let ident = input.ident;
    let fields = if let syn::Data::Struct(data) = input.data {
        data.fields
    } else {
        panic!("This derive only works for structures")
    };
    let (gets, sets, recgets, recsets, keys) = fields
        .into_iter()
        .filter_map(|f| {
            let attr: Option<IntKeyAttribute> = f
                .attrs
                .into_iter()
                .filter_map(|attr| {
                    attr.path
                        .is_ident("intkey")
                        .then(|| attr.parse_args().unwrap())
                })
                .next();
            let fid = f
                .ident
                .expect("Fields are required to be named for this derive");
            match attr {
                Some(IntKeyAttribute::Skip) => None,
                None => Some((
                    None,
                    None,
                    Some(quote! {
                        self.#fid.iget(key)
                    }),
                    Some(quote! {self.#fid.iset(key, value)}),
                    None,
                )),
                Some(IntKeyAttribute::Key {
                    key,
                    to_cowstr,
                    from_toowned_string,
                    direct,
                }) => Some((
                    Some(quote! {#key => (#to_cowstr)(&self.#fid)}),
                    Some(if !direct {
                        let set_id = quote::format_ident!("set_{}", &fid);
                        quote! {
                        #key => {if let Some(value) = (#from_toowned_string)(value) {match self.#set_id(value) {Ok(_) => Ok(()), Err(_) => Err(())}} else {Err(())}}
                        }
                    } else {
                        quote! {#key => if let Some(value) = (#from_toowned_string)(value) {self.#fid = (#from_toowned_string)(value); Ok(())} else {Err(())}}
                    }),
                    None,
                    None,
                    Some(key)
                )),
            }
        })
        .unzip_n_vec();
    let gets = gets.into_iter().flatten();
    let sets = sets.into_iter().flatten();
    let mut recgets = recgets.into_iter().flatten();
    let first_recget = recgets.next().unwrap_or(quote! {None});
    let mut recsets = recsets.into_iter().flatten();
    let first_recset = recsets.next().unwrap_or(quote! {Err(())});
    let keys = keys.into_iter().flatten();
    (quote! {
        #[automatically_derived]
        impl zenoh_util::properties::config::IntKeyMapLike for #ident {
            fn iget(&self, key: u64) -> Option<std::borrow::Cow<'_, str>> {
                match key {
                    #(#gets,)*
                    _ => {#first_recget#(.or_else(||#recgets))*}
                }
            }
            fn iset<S: AsRef<str>>(&mut self, key: u64, value: S) -> std::result::Result<(), ()> {
                let value: &str = value.as_ref();
                match key {
                    #(#sets,)*
                    _ => {#first_recset#(.or_else(|_|#recsets))*}
                }
            }
            type Keys = Vec<u64>;
            fn ikeys(&self) -> Self::Keys {
                let mut keys = Vec::new();
                #(keys.push(#keys);)*
                keys
            }
        }
    })
    .into()
}
