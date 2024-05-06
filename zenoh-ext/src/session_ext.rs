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
pub trait SessionExt<'s, 'a> {
    fn declare_publication_cache<'b, 'c, TryIntoKeyExpr>(
        &'s self,
        pub_key_expr: TryIntoKeyExpr,
    ) -> PublicationCacheBuilder<'a, 'b, 'c>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>;
}

impl<'s, 'a> SessionExt<'s, 'a> for SessionRef<'a> {
    fn declare_publication_cache<'b, 'c, TryIntoKeyExpr>(
        &'s self,
        pub_key_expr: TryIntoKeyExpr,
    ) -> PublicationCacheBuilder<'a, 'b, 'c>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>,
    {
        PublicationCacheBuilder::new(self.clone(), pub_key_expr.try_into().map_err(Into::into))
    }
}

impl<'a> SessionExt<'a, 'a> for Session {
    fn declare_publication_cache<'b, 'c, TryIntoKeyExpr>(
        &'a self,
        pub_key_expr: TryIntoKeyExpr,
    ) -> PublicationCacheBuilder<'a, 'b, 'c>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>,
    {
        SessionRef::Borrow(self).declare_publication_cache(pub_key_expr)
    }
}

impl<'s> SessionExt<'s, 'static> for Arc<Session> {
    /// Examples:
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::r#async::*;
    /// use zenoh::config::ModeDependentValue::Unique;
    /// use zenoh_ext::SessionExt;
    ///
    /// let mut config = config::default();
    /// config.timestamping.set_enabled(Some(Unique(true)));
    /// let session = zenoh::open(config).res().await.unwrap().into_arc();
    /// let publication_cache = session.declare_publication_cache("key/expression").res().await.unwrap();
    /// tokio::task::spawn(async move {
    ///     publication_cache.key_expr();
    /// }).await;
    /// # }
    /// ```
    fn declare_publication_cache<'b, 'c, TryIntoKeyExpr>(
        &'s self,
        pub_key_expr: TryIntoKeyExpr,
    ) -> PublicationCacheBuilder<'static, 'b, 'c>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>,
    {
        SessionRef::Shared(self.clone()).declare_publication_cache(pub_key_expr)
    }
}
