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

use zenoh::{key_expr::KeyExpr, session::Session, Error};

#[allow(deprecated)]
use super::PublicationCacheBuilder;

/// Some extensions to the [`zenoh::Session`](zenoh::Session)
#[zenoh_macros::unstable]
#[allow(deprecated)]
pub trait SessionExt {
    // REVIEW(fuzzypixelz): this doc test is the only one to use the programmatic configuration API..
    /// Declare a [`PublicationCache`](crate::PublicationCache).
    ///
    /// Examples:
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh_ext::SessionExt;
    /// use zenoh_config::ModeDependentValue::Unique;
    ///
    ///
    /// let mut config = zenoh::Config::default();
    /// config.timestamping.set_enabled(Some(Unique(true)));
    /// let session = zenoh::open(config).await.unwrap();
    /// let publication_cache = session.declare_publication_cache("key/expression").await.unwrap();
    /// tokio::task::spawn(async move {
    ///     publication_cache.key_expr();
    /// }).await;
    /// # }
    /// ```
    #[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
    #[zenoh_macros::unstable]
    fn declare_publication_cache<'a, 'b, 'c, TryIntoKeyExpr>(
        &'a self,
        pub_key_expr: TryIntoKeyExpr,
    ) -> PublicationCacheBuilder<'a, 'b, 'c>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<Error>;
}

#[allow(deprecated)]
impl SessionExt for Session {
    #[zenoh_macros::unstable]
    fn declare_publication_cache<'a, 'b, 'c, TryIntoKeyExpr>(
        &'a self,
        pub_key_expr: TryIntoKeyExpr,
    ) -> PublicationCacheBuilder<'a, 'b, 'c>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<Error>,
    {
        PublicationCacheBuilder::new(self, pub_key_expr.try_into().map_err(Into::into))
    }
}
