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
use zenoh::Session;

/// Some extensions to the [`zenoh::Session`](zenoh::Session)
pub trait SessionExt {
    fn declare_publication_cache<'a, 'b, 'c, TryIntoKeyExpr>(
        &'a self,
        pub_key_expr: TryIntoKeyExpr,
    ) -> PublicationCacheBuilder<'a, 'b, 'c>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>;
}

impl SessionExt for Session {
    fn declare_publication_cache<'a, 'b, 'c, TryIntoKeyExpr>(
        &'a self,
        pub_key_expr: TryIntoKeyExpr,
    ) -> PublicationCacheBuilder<'a, 'b, 'c>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>,
    {
        PublicationCacheBuilder::new(self, pub_key_expr.try_into().map_err(Into::into))
    }
}

impl SessionExt for Arc<Session> {
    fn declare_publication_cache<'a, 'b, 'c, TryIntoKeyExpr>(
        &'a self,
        pub_key_expr: TryIntoKeyExpr,
    ) -> PublicationCacheBuilder<'a, 'b, 'c>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>,
    {
        PublicationCacheBuilder::new(self, pub_key_expr.try_into().map_err(Into::into))
    }
}
