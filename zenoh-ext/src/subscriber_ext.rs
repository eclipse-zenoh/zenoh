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
use std::time::Duration;

use futures::stream::{Forward, Map};
use zenoh::{
    handlers::{fifo, FifoChannelHandler},
    liveliness::LivelinessSubscriberBuilder,
    pubsub::{Subscriber, SubscriberBuilder},
    query::{QueryConsolidation, QueryTarget, ReplyKeyExpr},
    sample::{Locality, Sample},
    Result as ZResult,
};

use crate::{
    querying_subscriber::QueryingSubscriberBuilder, ExtractSample, FetchingSubscriberBuilder,
};

/// Allows writing `subscriber.forward(receiver)` instead of `subscriber.stream().map(Ok).forward(publisher)`
#[zenoh_macros::unstable]
pub trait SubscriberForward<'a, S> {
    type Output;
    fn forward(&'a mut self, sink: S) -> Self::Output;
}
impl<'a, S> SubscriberForward<'a, S> for Subscriber<FifoChannelHandler<Sample>>
where
    S: futures::sink::Sink<Sample>,
{
    type Output =
        Forward<Map<fifo::RecvStream<'a, Sample>, fn(Sample) -> Result<Sample, S::Error>>, S>;
    fn forward(&'a mut self, sink: S) -> Self::Output {
        futures::StreamExt::forward(futures::StreamExt::map(self.stream(), Ok), sink)
    }
}

/// Some extensions to the [`zenoh::subscriber::SubscriberBuilder`](zenoh::pubsub::SubscriberBuilder)
#[zenoh_macros::unstable]
pub trait SubscriberBuilderExt<'a, 'b, Handler> {
    type KeySpace;

    /// Create a [`FetchingSubscriber`](super::FetchingSubscriber).
    ///
    /// This operation returns a [`FetchingSubscriberBuilder`](FetchingSubscriberBuilder) that can be used to finely configure the subscriber.
    /// As soon as built (calling `.wait()` or `.await` on the `FetchingSubscriberBuilder`), the `FetchingSubscriber`
    /// will run the given `fetch` function. The user defined `fetch` function should fetch some samples and return them
    /// through the callback function. Those samples will be merged with the received publications and made available in the receiver.
    /// Later on, new fetches can be performed again, calling [`FetchingSubscriber::fetch()`](super::FetchingSubscriber::fetch()).
    ///
    /// A typical usage of the `FetchingSubscriber` is to retrieve publications that were made in the past, but stored in some zenoh Storage.
    ///
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::Wait;
    /// use zenoh_ext::*;
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let subscriber = session
    ///     .declare_subscriber("key/expr")
    ///     .fetching( |cb| {
    ///         session
    ///             .get("key/expr")
    ///             .callback(cb)
    ///             .wait()
    ///     })
    ///     .await
    ///     .unwrap();
    /// while let Ok(sample) = subscriber.recv_async().await {
    ///     println!("Received: {:?}", sample);
    /// }
    /// # }
    /// ```
    fn fetching<
        Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()>,
        TryIntoSample,
    >(
        self,
        fetch: Fetch,
    ) -> FetchingSubscriberBuilder<'a, 'b, Self::KeySpace, Handler, Fetch, TryIntoSample>
    where
        TryIntoSample: ExtractSample;

    /// Create a [`FetchingSubscriber`](super::FetchingSubscriber) that will perform a query (`session.get()`) as it's
    /// initial fetch.
    ///
    /// This operation returns a [`QueryingSubscriberBuilder`](QueryingSubscriberBuilder) that can be used to finely configure the subscriber.
    /// As soon as built (calling `.wait()` or `.await` on the `QueryingSubscriberBuilder`), the `FetchingSubscriber`
    /// will issue a query on a given key expression (by default it uses the same key expression than it subscribes to).
    /// The results of the query will be merged with the received publications and made available in the receiver.
    /// Later on, new fetches can be performed again, calling [`FetchingSubscriber::fetch()`](super::FetchingSubscriber::fetch()).
    ///
    /// A typical usage of the `FetchingSubscriber` is to retrieve publications that were made in the past, but stored in some zenoh Storage.
    ///
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh_ext::*;
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let subscriber = session
    ///     .declare_subscriber("key/expr")
    ///     .querying()
    ///     .await
    ///     .unwrap();
    /// while let Ok(sample) = subscriber.recv_async().await {
    ///     println!("Received: {:?}", sample);
    /// }
    /// # }
    /// ```
    fn querying(self) -> QueryingSubscriberBuilder<'a, 'b, Self::KeySpace, Handler>;
}

impl<'a, 'b, Handler> SubscriberBuilderExt<'a, 'b, Handler> for SubscriberBuilder<'a, 'b, Handler> {
    type KeySpace = crate::UserSpace;

    /// Create a [`FetchingSubscriber`](super::FetchingSubscriber).
    ///
    /// This operation returns a [`FetchingSubscriberBuilder`](FetchingSubscriberBuilder) that can be used to finely configure the subscriber.
    /// As soon as built (calling `.wait()` or `.await` on the `FetchingSubscriberBuilder`), the `FetchingSubscriber`
    /// will run the given `fetch` function. The user defined `fetch` function should fetch some samples and return them
    /// through the callback function. Those samples will be merged with the received publications and made available in the receiver.
    /// Later on, new fetches can be performed again, calling [`FetchingSubscriber::fetch()`](super::FetchingSubscriber::fetch()).
    ///
    /// A typical usage of the `FetchingSubscriber` is to retrieve publications that were made in the past, but stored in some zenoh Storage.
    ///
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::Wait;
    /// use zenoh_ext::*;
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let subscriber = session
    ///     .declare_subscriber("key/expr")
    ///     .fetching( |cb| {
    ///         session
    ///             .get("key/expr")
    ///             .callback(cb)
    ///             .wait()
    ///     })
    ///     .await
    ///     .unwrap();
    /// while let Ok(sample) = subscriber.recv_async().await {
    ///     println!("Received: {:?}", sample);
    /// }
    /// # }
    /// ```
    fn fetching<
        Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()>,
        TryIntoSample,
    >(
        self,
        fetch: Fetch,
    ) -> FetchingSubscriberBuilder<'a, 'b, Self::KeySpace, Handler, Fetch, TryIntoSample>
    where
        TryIntoSample: ExtractSample,
    {
        FetchingSubscriberBuilder {
            session: self.session,
            key_expr: self.key_expr,
            key_space: crate::UserSpace,
            origin: self.origin,
            fetch,
            handler: self.handler,
            phantom: std::marker::PhantomData,
        }
    }

    /// Create a [`FetchingSubscriber`](super::FetchingSubscriber) that will perform a query (`session.get()`) as it's
    /// initial fetch.
    ///
    /// This operation returns a [`QueryingSubscriberBuilder`](QueryingSubscriberBuilder) that can be used to finely configure the subscriber.
    /// As soon as built (calling `.wait()` or `.await` on the `QueryingSubscriberBuilder`), the `FetchingSubscriber`
    /// will issue a query on a given key expression (by default it uses the same key expression than it subscribes to).
    /// The results of the query will be merged with the received publications and made available in the receiver.
    /// Later on, new fetches can be performed again, calling [`FetchingSubscriber::fetch()`](super::FetchingSubscriber::fetch()).
    ///
    /// A typical usage of the `FetchingSubscriber` is to retrieve publications that were made in the past, but stored in some zenoh Storage.
    ///
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh_ext::*;
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let subscriber = session
    ///     .declare_subscriber("key/expr")
    ///     .querying()
    ///     .await
    ///     .unwrap();
    /// while let Ok(sample) = subscriber.recv_async().await {
    ///     println!("Received: {:?}", sample);
    /// }
    /// # }
    /// ```
    fn querying(self) -> QueryingSubscriberBuilder<'a, 'b, Self::KeySpace, Handler> {
        QueryingSubscriberBuilder {
            session: self.session,
            key_expr: self.key_expr,
            key_space: crate::UserSpace,
            origin: self.origin,
            query_selector: None,
            // By default query all matching publication caches and storages
            query_target: QueryTarget::All,
            // By default no query consolidation, to receive more than 1 sample per-resource
            // (if history of publications is available)
            query_consolidation: QueryConsolidation::from(zenoh::query::ConsolidationMode::None),
            query_accept_replies: ReplyKeyExpr::default(),
            query_timeout: Duration::from_secs(10),
            handler: self.handler,
        }
    }
}

impl<'a, 'b, Handler> SubscriberBuilderExt<'a, 'b, Handler>
    for LivelinessSubscriberBuilder<'a, 'b, Handler>
{
    type KeySpace = crate::LivelinessSpace;

    /// Create a fetching liveliness subscriber ([`FetchingSubscriber`](super::FetchingSubscriber)).
    ///
    /// This operation returns a [`FetchingSubscriberBuilder`](FetchingSubscriberBuilder) that can be used to finely configure the subscriber.
    /// As soon as built (calling `.wait()` or `.await` on the `FetchingSubscriberBuilder`), the `FetchingSubscriber`
    /// will run the given `fetch` function. The user defined `fetch` function should fetch some samples and return them
    /// through the callback function. Those samples will be merged with the received publications and made available in the receiver.
    /// Later on, new fetches can be performed again, calling [`FetchingSubscriber::fetch()`](super::FetchingSubscriber::fetch()).
    ///
    /// A typical usage of the fetching liveliness subscriber is to retrieve existing liveliness tokens while susbcribing to
    /// new liveness changes.
    ///
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::Wait;
    /// use zenoh_ext::*;
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let subscriber = session
    ///     .liveliness()
    ///     .declare_subscriber("key/expr")
    ///     .fetching( |cb| {
    ///         session
    ///             .liveliness()
    ///             .get("key/expr")
    ///             .callback(cb)
    ///             .wait()
    ///     })
    ///     .await
    ///     .unwrap();
    /// while let Ok(sample) = subscriber.recv_async().await {
    ///     println!("Received: {:?}", sample);
    /// }
    /// # }
    /// ```
    fn fetching<
        Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()>,
        TryIntoSample,
    >(
        self,
        fetch: Fetch,
    ) -> FetchingSubscriberBuilder<'a, 'b, Self::KeySpace, Handler, Fetch, TryIntoSample>
    where
        TryIntoSample: ExtractSample,
    {
        FetchingSubscriberBuilder {
            session: self.session,
            key_expr: self.key_expr,
            key_space: crate::LivelinessSpace,
            origin: Locality::default(),
            fetch,
            handler: self.handler,
            phantom: std::marker::PhantomData,
        }
    }

    /// Create a fetching liveliness subscriber ([`FetchingSubscriber`](super::FetchingSubscriber)) that will perform a
    /// liveliness query (`session.liveliness().get()`) as it's initial fetch.
    ///
    /// This operation returns a [`QueryingSubscriberBuilder`](QueryingSubscriberBuilder) that can be used to finely configure the subscriber.
    /// As soon as built (calling `.wait()` or `.await` on the `QueryingSubscriberBuilder`), the `FetchingSubscriber`
    /// will issue a liveliness query on a given key expression (by default it uses the same key expression than it subscribes to).
    /// The results of the query will be merged with the received publications and made available in the receiver.
    /// Later on, new fetches can be performed again, calling [`FetchingSubscriber::fetch()`](super::FetchingSubscriber::fetch()).
    ///
    /// A typical usage of the fetching liveliness subscriber is to retrieve existing liveliness tokens while susbcribing to
    /// new liveness changes.
    ///
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh_ext::*;
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let subscriber = session
    ///     .liveliness()
    ///     .declare_subscriber("key/expr")
    ///     .querying()
    ///     .await
    ///     .unwrap();
    /// while let Ok(sample) = subscriber.recv_async().await {
    ///     println!("Received: {:?}", sample);
    /// }
    /// # }
    /// ```
    fn querying(self) -> QueryingSubscriberBuilder<'a, 'b, Self::KeySpace, Handler> {
        QueryingSubscriberBuilder {
            session: self.session,
            key_expr: self.key_expr,
            key_space: crate::LivelinessSpace,
            origin: Locality::default(),
            query_selector: None,
            query_target: QueryTarget::DEFAULT,
            query_consolidation: QueryConsolidation::DEFAULT,
            query_accept_replies: ReplyKeyExpr::MatchingQuery,
            query_timeout: Duration::from_secs(10),
            handler: self.handler,
        }
    }
}
