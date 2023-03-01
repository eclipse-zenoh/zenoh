use std::time::Duration;

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
use flume::r#async::RecvStream;
use futures::stream::{Forward, Map};
use zenoh::{
    prelude::Sample,
    query::{QueryConsolidation, QueryTarget},
    subscriber::{PushMode, Subscriber, SubscriberBuilder},
};

use crate::QueryingSubscriberBuilder;

/// Allows writing `subscriber.forward(receiver)` instead of `subscriber.stream().map(Ok).forward(publisher)`
pub trait SubscriberForward<'a, S> {
    type Output;
    fn forward(&'a mut self, sink: S) -> Self::Output;
}
impl<'a, S> SubscriberForward<'a, S> for Subscriber<'_, flume::Receiver<Sample>>
where
    S: futures::sink::Sink<Sample>,
{
    type Output = Forward<Map<RecvStream<'a, Sample>, fn(Sample) -> Result<Sample, S::Error>>, S>;
    fn forward(&'a mut self, sink: S) -> Self::Output {
        futures::StreamExt::forward(futures::StreamExt::map(self.receiver.stream(), Ok), sink)
    }
}

/// Some extensions to the [zenoh::SubscriberBuilder](zenoh::SubscriberBuilder)
pub trait SubscriberBuilderExt<'a, 'b, Handler> {
    /// Create a [QueryingSubscriber](super::QueryingSubscriber).
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
    /// # Examples
    /// ```no_run
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    /// use zenoh_ext::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let subscriber = session
    ///     .declare_subscriber("key/expr")
    ///     .querying()
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// while let Ok(sample) = subscriber.recv_async().await {
    ///     println!("Received: {:?}", sample);
    /// }
    /// # })
    /// ```
    fn querying(self) -> QueryingSubscriberBuilder<'a, 'b, Handler>;
}

impl<'a, 'b, Handler> SubscriberBuilderExt<'a, 'b, Handler>
    for SubscriberBuilder<'a, 'b, PushMode, Handler>
{
    fn querying(self) -> QueryingSubscriberBuilder<'a, 'b, Handler> {
        QueryingSubscriberBuilder {
            session: self.session,
            key_expr: self.key_expr,
            reliability: self.reliability,
            origin: self.origin,
            query_selector: None,
            // By default query all matching publication caches and storages
            query_target: QueryTarget::All,
            // By default no query consolidation, to receive more than 1 sample per-resource
            // (if history of publications is available)
            query_consolidation: QueryConsolidation::from(zenoh::query::ConsolidationMode::None),
            query_timeout: Duration::from_secs(10),
            handler: self.handler,
        }
    }
}
