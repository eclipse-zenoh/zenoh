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
use super::{PublicationCacheBuilder, QueryingSubscriberBuilder};
use zenoh::prelude::KeyExpr;
use zenoh::Session;

/// Some extensions to the [zenoh::Session](zenoh::Session)
pub trait SessionExt {
    /// Declare a [QueryingSubscriber](super::QueryingSubscriber) for the given resource key.
    ///
    /// This operation returns a [QueryingSubscriberBuilder](QueryingSubscriberBuilder) that can be used to finely configure the subscriber.  
    /// As soon as built (calling `.wait()` or `.await` on the QueryingSubscriberBuilder), the QueryingSubscriber
    /// will issue a query on a given resource key (by default it uses the same resource key than it subscribes to).
    /// The results of the query will be merged with the received publications and made available in the receiver.
    /// Later on, new queries can be issued again, calling [QueryingSubscriber::query()](super::QueryingSubscriber::query()) or
    /// [QueryingSubscriber::query_on()](super::QueryingSubscriber::query_on()).
    ///
    /// A typical usage of the QueryingSubscriber is to retrieve publications that were made in the past, but stored in some zenoh Storage.
    ///
    /// # Arguments
    /// * `sub_key_expr` - The resource key to subscribe (and to query on if not changed via the [QueryingSubscriberBuilder](QueryingSubscriberBuilder))
    ///
    /// # Examples
    /// ```no_run
    /// # async_std::task::block_on(async {
    /// use futures::prelude::*;
    /// use zenoh::prelude::*;
    /// use zenoh_ext::*;
    ///
    /// let session = zenoh::open(config::peer()).await.unwrap();
    /// let mut subscriber = session.subscribe_with_query("/resource/name").await.unwrap();
    /// while let Some(sample) = subscriber.receiver().next().await {
    ///     println!("Received : {:?}", sample);
    /// }
    /// # })
    /// ```
    fn subscribe_with_query<'a, 'b, IntoKeyExpr>(
        &'a self,
        sub_key_expr: IntoKeyExpr,
    ) -> QueryingSubscriberBuilder<'a, 'b>
    where
        IntoKeyExpr: Into<KeyExpr<'b>>;

    fn publishing_with_cache<'a, 'b, IntoKeyExpr>(
        &'a self,
        pub_key_expr: IntoKeyExpr,
    ) -> PublicationCacheBuilder<'a, 'b>
    where
        IntoKeyExpr: Into<KeyExpr<'b>>;
}

impl SessionExt for Session {
    fn subscribe_with_query<'a, 'b, IntoKeyExpr>(
        &'a self,
        sub_key_expr: IntoKeyExpr,
    ) -> QueryingSubscriberBuilder<'a, 'b>
    where
        IntoKeyExpr: Into<KeyExpr<'b>>,
    {
        QueryingSubscriberBuilder::new(self, sub_key_expr.into())
    }

    fn publishing_with_cache<'a, 'b, IntoKeyExpr>(
        &'a self,
        pub_key_expr: IntoKeyExpr,
    ) -> PublicationCacheBuilder<'a, 'b>
    where
        IntoKeyExpr: Into<KeyExpr<'b>>,
    {
        PublicationCacheBuilder::new(self, pub_key_expr.into())
    }
}
