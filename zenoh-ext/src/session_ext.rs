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
use super::{PublicationCacheBuilder, QueryingSubscriberBuilder};
use std::convert::TryInto;
use std::fmt;
use std::ops::Deref;
use std::sync::Arc;
use zenoh::handlers::DefaultHandler;
use zenoh::prelude::KeyExpr;
use zenoh::Session;

#[derive(Clone)]
pub(crate) enum SessionRef<'a> {
    Borrow(&'a Session),
    Shared(Arc<Session>),
}

impl Deref for SessionRef<'_> {
    type Target = Session;

    fn deref(&self) -> &Self::Target {
        match self {
            SessionRef::Borrow(b) => b,
            SessionRef::Shared(s) => s,
        }
    }
}

impl fmt::Debug for SessionRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SessionRef::Borrow(b) => Session::fmt(b, f),
            SessionRef::Shared(s) => Session::fmt(s, f),
        }
    }
}

/// Some extensions to the [zenoh::Session](zenoh::Session)
pub trait SessionExt {
    /// Create a [QueryingSubscriber](super::QueryingSubscriber) with the given key expression.
    ///
    /// This operation returns a [`QueryingSubscriberBuilder`](QueryingSubscriberBuilder) that can be used to finely configure the subscriber.  
    /// As soon as built (calling `.wait()` or `.await` on the `QueryingSubscriberBuilder`), the `QueryingSubscriber`
    /// will issue a query on a given key expression (by default it uses the same key expression than it subscribes to).
    /// The results of the query will be merged with the received publications and made available in the receiver.
    /// Later on, new queries can be issued again, calling [`QueryingSubscriber::query()`](super::QueryingSubscriber::query()) or
    /// [`QueryingSubscriber::query_on()`](super::QueryingSubscriber::query_on()).
    ///
    /// A typical usage of the `QueryingSubscriber` is to retrieve publications that were made in the past, but stored in some zenoh Storage.
    ///
    /// # Arguments
    /// * `sub_key_expr` - The key expression to subscribe on (and to query on if not changed via the [`QueryingSubscriberBuilder`](QueryingSubscriberBuilder))
    ///
    /// # Examples
    /// ```no_run
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    /// use zenoh_ext::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let subscriber = session.declare_querying_subscriber("key/expr")
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// while let Ok(sample) = subscriber.recv_async().await {
    ///     println!("Received: {:?}", sample);
    /// }
    /// # })
    /// ```
    fn declare_querying_subscriber<'a, 'b, TryIntoKeyExpr>(
        &'a self,
        sub_key_expr: TryIntoKeyExpr,
    ) -> QueryingSubscriberBuilder<'a, 'b, DefaultHandler>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_core::Error>;

    fn declare_publication_cache<'a, 'b, 'c, TryIntoKeyExpr>(
        &'a self,
        pub_key_expr: TryIntoKeyExpr,
    ) -> PublicationCacheBuilder<'a, 'b, 'c>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_core::Error>;
}

impl SessionExt for Session {
    fn declare_querying_subscriber<'a, 'b, TryIntoKeyExpr>(
        &'a self,
        sub_key_expr: TryIntoKeyExpr,
    ) -> QueryingSubscriberBuilder<'a, 'b, DefaultHandler>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_core::Error>,
    {
        QueryingSubscriberBuilder::new(
            SessionRef::Borrow(self),
            sub_key_expr.try_into().map_err(Into::into),
        )
    }

    fn declare_publication_cache<'a, 'b, 'c, TryIntoKeyExpr>(
        &'a self,
        pub_key_expr: TryIntoKeyExpr,
    ) -> PublicationCacheBuilder<'a, 'b, 'c>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_core::Error>,
    {
        PublicationCacheBuilder::new(self, pub_key_expr.try_into().map_err(Into::into))
    }
}

impl SessionExt for Arc<Session> {
    fn declare_querying_subscriber<'a, 'b, TryIntoKeyExpr>(
        &'a self,
        sub_key_expr: TryIntoKeyExpr,
    ) -> QueryingSubscriberBuilder<'a, 'b, DefaultHandler>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_core::Error>,
    {
        QueryingSubscriberBuilder::new(
            SessionRef::Shared(self.clone()),
            sub_key_expr.try_into().map_err(Into::into),
        )
    }

    fn declare_publication_cache<'a, 'b, 'c, TryIntoKeyExpr>(
        &'a self,
        pub_key_expr: TryIntoKeyExpr,
    ) -> PublicationCacheBuilder<'a, 'b, 'c>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_core::Error>,
    {
        PublicationCacheBuilder::new(self, pub_key_expr.try_into().map_err(Into::into))
    }
}
