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
use super::PublicationCacheBuilder;
use std::convert::TryInto;
use std::sync::Arc;
use zenoh::prelude::KeyExpr;
use zenoh::{Session, SessionRef};

/// Some extensions to the [`zenoh::Session`](zenoh::Session)
pub trait SessionExt {
    type PublicationCacheBuilder<'a, 'b, 'c>
    where
        Self: 'a;
    fn declare_publication_cache<'a, 'b, 'c, TryIntoKeyExpr>(
        &'a self,
        pub_key_expr: TryIntoKeyExpr,
    ) -> Self::PublicationCacheBuilder<'a, 'b, 'c>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>;
}

impl SessionExt for Session {
    type PublicationCacheBuilder<'a, 'b, 'c> = PublicationCacheBuilder<'a, 'b, 'c>;
    fn declare_publication_cache<'a, 'b, 'c, TryIntoKeyExpr>(
        &'a self,
        pub_key_expr: TryIntoKeyExpr,
    ) -> PublicationCacheBuilder<'a, 'b, 'c>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>,
    {
        PublicationCacheBuilder::new(
            SessionRef::Borrow(self),
            pub_key_expr.try_into().map_err(Into::into),
        )
    }
}

impl<T: ArcSessionExt + 'static> SessionExt for T {
    type PublicationCacheBuilder<'a, 'b, 'c> = PublicationCacheBuilder<'static, 'b, 'c>;
    fn declare_publication_cache<'a, 'b, 'c, TryIntoKeyExpr>(
        &'a self,
        pub_key_expr: TryIntoKeyExpr,
    ) -> Self::PublicationCacheBuilder<'a, 'b, 'c>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>,
    {
        ArcSessionExt::declare_publication_cache(self, pub_key_expr)
    }
}

pub trait ArcSessionExt {
    fn declare_publication_cache<'b, 'c, TryIntoKeyExpr>(
        &self,
        pub_key_expr: TryIntoKeyExpr,
    ) -> PublicationCacheBuilder<'static, 'b, 'c>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>;
}

impl ArcSessionExt for Arc<Session> {
    /// Examples:
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    /// use zenoh::config::ModeDependentValue::Unique;
    /// use zenoh_ext::ArcSessionExt;
    ///
    /// let mut config = config::default();
    /// config.timestamping.set_enabled(Some(Unique(true)));
    /// let session = zenoh::open(config).res().await.unwrap().into_arc();
    /// let publication_cache = session.declare_publication_cache("key/expression").res().await.unwrap();
    /// async_std::task::spawn(async move {
    ///     publication_cache.key_expr();
    /// }).await;
    /// # })
    /// ```
    fn declare_publication_cache<'b, 'c, TryIntoKeyExpr>(
        &self,
        pub_key_expr: TryIntoKeyExpr,
    ) -> PublicationCacheBuilder<'static, 'b, 'c>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>,
    {
        PublicationCacheBuilder::new(
            SessionRef::Shared(self.clone()),
            pub_key_expr.try_into().map_err(Into::into),
        )
    }
}
