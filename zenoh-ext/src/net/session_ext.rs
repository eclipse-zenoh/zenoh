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
use super::QueryingSubscriberBuilder;
use zenoh::net::{ResKey, Session};

/// Some extensions to the [zenoh::net::Session](zenoh::net::Session)
pub trait SessionExt {
    /// Declare a [QueryingSubscriber](super::QueryingSubscriber) for the given resource key.
    ///
    /// This operation returns a [QueryingSubscriberBuilder](QueryingSubscriberBuilder) that can be used to finely configure the subscriber.  
    /// As soon as built (calling `.wait()` or `.await` on the QueryingSubscriberBuilder), the QueryingSubscriber
    /// will issue a query on a given resource key (by default it uses the same resource key than it subscribes to).
    /// The results of the query will be merged with the received publications and made available in the receiver.
    /// Later on, new queries can be issued again, calling [QueryingSubscriber::do_query()](super::QueryingSubscriber::do_query()).
    ///
    /// A typical usage of the QueryingSubscriber is to retrieve publications that were made in the past, but stored in some zenoh Storage.
    ///
    /// # Arguments
    /// * `sub_reskey` - The resource key to subscribe (and to query on if not changed via the [QueryingSubscriberBuilder](QueryingSubscriberBuilder))
    ///
    /// # Examples
    /// ```no_run
    /// # async_std::task::block_on(async {
    /// use zenoh::net::*;
    /// use zenoh_ext::net::*;
    /// use futures::prelude::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// let mut subscriber = session.declare_querying_subscriber(&"/resource/name".into()).await.unwrap();
    /// while let Some(sample) = subscriber.receiver().next().await {
    ///     println!("Received : {:?}", sample);
    /// }
    /// # })
    /// ```
    fn declare_querying_subscriber(&self, sub_reskey: &ResKey) -> QueryingSubscriberBuilder<'_>;
}

impl SessionExt for Session {
    fn declare_querying_subscriber(&self, sub_reskey: &ResKey) -> QueryingSubscriberBuilder<'_> {
        QueryingSubscriberBuilder::new(self, sub_reskey)
    }
}
