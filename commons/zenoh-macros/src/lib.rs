//
// Copyright (c) 2022 ZettaScale Technology
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

#[proc_macro]
pub fn keformat(tokens: TokenStream) -> TokenStream {
    let lit = syn::parse::<syn::LitStr>(tokens).unwrap();
    let source = lit.value();
    let len = source.split("${").count() - 1;
    // use zenoh_protocol::core::key_expr::format::KeFormat;
    // let format = match KeFormat::new(&source) {
    //     Ok(format) => format,
    //     Err(e) => panic!("{}", e),
    // };
    // let specs = format.specs().collect::<Vec<_>>();
    // let len = specs.len();
    quote!{
        {
            const SOURCE: &'static str = #lit;
            /// The `#lit` format, as a structure.
            pub struct Format<'a>(::zenoh::key_expr::format::KeFormat<'a, [::zenoh::key_expr::format::Segment<'a>; #len]>);
            impl<'a> ::core::ops::Deref for Format<'a> {
                type Target = ::zenoh::key_expr::format::KeFormat<'a, [::zenoh::key_expr::format::Segment<'a>; #len]>;
                fn deref(&self) -> &Self::Target {&self.0}
            }
            Format(unsafe{::zenoh::key_expr::format::KeFormat::noalloc_new::<#len>(SOURCE).unwrap_unchecked()})
        }
    }.into()
}
