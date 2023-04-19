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
use flume::r#async::RecvStream;
use futures::stream::{Forward, Map};
use std::{convert::TryInto, time::Duration};
use zenoh::query::ReplyKeyExpr;
use zenoh::sample::Locality;
use zenoh::Result as ZResult;
use zenoh::{
    liveliness::LivelinessSubscriberBuilder,
    prelude::Sample,
    query::{QueryConsolidation, QueryTarget},
    subscriber::{PushMode, Reliability, Subscriber, SubscriberBuilder},
};

use crate::{querying_subscriber::QueryingSubscriberBuilder, FetchingSubscriberBuilder};

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

/// Some extensions to the [`zenoh::subscriber::SubscriberBuilder`](zenoh::subscriber::SubscriberBuilder)
pub trait SubscriberBuilderExt<'a, 'b, Handler> {
    type KeySpace;

    /// Create a [`FetchingSubscriber`](super::FetchingSubscriber).
    ///
    /// This operation returns a [`FetchingSubscriberBuilder`](FetchingSubscriberBuilder) that can be used to finely configure the subscriber.  
    /// As soon as built (calling `.wait()` or `.await` on the `FetchingSubscriberBuilder`), the `FetchingSubscriber`
    /// will run the given `fetch` funtion. The user defined `fetch` funtion should fetch some samples and return them
    /// through the callback funtion. Those samples will be merged with the received publications and made available in the receiver.
    /// Later on, new fetches can be performed again, calling [`FetchingSubscriber::fetch()`](super::FetchingSubscriber::fetch()).
    ///
    /// A typical usage of the `FetchingSubscriber` is to retrieve publications that were made in the past, but stored in some zenoh Storage.
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
    ///     .fetching( |cb| {
    ///         use zenoh::prelude::sync::SyncResolve;
    ///         session
    ///             .get("key/expr")
    ///             .callback(cb)
    ///             .res_sync()
    ///     })
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// while let Ok(sample) = subscriber.recv_async().await {
    ///     println!("Received: {:?}", sample);
    /// }
    /// # })
    /// ```
    fn fetching<
        Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()>,
        TryIntoSample,
    >(
        self,
        fetch: Fetch,
    ) -> FetchingSubscriberBuilder<'a, 'b, Self::KeySpace, Handler, Fetch, TryIntoSample>
    where
        TryIntoSample: TryInto<Sample>,
        <TryIntoSample as TryInto<Sample>>::Error: Into<zenoh_core::Error>;

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
    fn querying(self) -> QueryingSubscriberBuilder<'a, 'b, Self::KeySpace, Handler>;
}

impl<'a, 'b, Handler> SubscriberBuilderExt<'a, 'b, Handler>
    for SubscriberBuilder<'a, 'b, PushMode, Handler>
{
    type KeySpace = crate::UserSpace;

    /// Create a [`FetchingSubscriber`](super::FetchingSubscriber).
    ///
    /// This operation returns a [`FetchingSubscriberBuilder`](FetchingSubscriberBuilder) that can be used to finely configure the subscriber.  
    /// As soon as built (calling `.wait()` or `.await` on the `FetchingSubscriberBuilder`), the `FetchingSubscriber`
    /// will run the given `fetch` funtion. The user defined `fetch` funtion should fetch some samples and return them
    /// through the callback funtion. Those samples will be merged with the received publications and made available in the receiver.
    /// Later on, new fetches can be performed again, calling [`FetchingSubscriber::fetch()`](super::FetchingSubscriber::fetch()).
    ///
    /// A typical usage of the `FetchingSubscriber` is to retrieve publications that were made in the past, but stored in some zenoh Storage.
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
    ///     .fetching( |cb| {
    ///         use zenoh::prelude::sync::SyncResolve;
    ///         session
    ///             .get("key/expr")
    ///             .callback(cb)
    ///             .res_sync()
    ///     })
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// while let Ok(sample) = subscriber.recv_async().await {
    ///     println!("Received: {:?}", sample);
    /// }
    /// # })
    /// ```
    fn fetching<
        Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()>,
        TryIntoSample,
    >(
        self,
        fetch: Fetch,
    ) -> FetchingSubscriberBuilder<'a, 'b, Self::KeySpace, Handler, Fetch, TryIntoSample>
    where
        TryIntoSample: TryInto<Sample>,
        <TryIntoSample as TryInto<Sample>>::Error: Into<zenoh_core::Error>,
    {
        FetchingSubscriberBuilder {
            session: self.session,
            key_expr: self.key_expr,
            key_space: crate::UserSpace,
            reliability: self.reliability,
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
    fn querying(self) -> QueryingSubscriberBuilder<'a, 'b, Self::KeySpace, Handler> {
        QueryingSubscriberBuilder {
            session: self.session,
            key_expr: self.key_expr,
            key_space: crate::UserSpace,
            reliability: self.reliability,
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
    /// will run the given `fetch` funtion. The user defined `fetch` funtion should fetch some samples and return them
    /// through the callback funtion. Those samples will be merged with the received publications and made available in the receiver.
    /// Later on, new fetches can be performed again, calling [`FetchingSubscriber::fetch()`](super::FetchingSubscriber::fetch()).
    ///
    /// A typical usage of the fetching liveliness subscriber is to retrieve existing liveliness tokens while susbcribing to
    /// new liveness changes.
    ///
    /// # Examples
    /// ```no_run
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    /// use zenoh_ext::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let subscriber = session
    ///     .liveliness()
    ///     .declare_subscriber("key/expr")
    ///     .fetching( |cb| {
    ///         use zenoh::prelude::sync::SyncResolve;
    ///         session
    ///             .liveliness()
    ///             .get("key/expr")
    ///             .callback(cb)
    ///             .res_sync()
    ///     })
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// while let Ok(sample) = subscriber.recv_async().await {
    ///     println!("Received: {:?}", sample);
    /// }
    /// # })
    /// ```
    fn fetching<
        Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()>,
        TryIntoSample,
    >(
        self,
        fetch: Fetch,
    ) -> FetchingSubscriberBuilder<'a, 'b, Self::KeySpace, Handler, Fetch, TryIntoSample>
    where
        TryIntoSample: TryInto<Sample>,
        <TryIntoSample as TryInto<Sample>>::Error: Into<zenoh_core::Error>,
    {
        FetchingSubscriberBuilder {
            session: self.session,
            key_expr: self.key_expr,
            key_space: crate::LivelinessSpace,
            reliability: Reliability::default(),
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
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    /// use zenoh_ext::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let subscriber = session
    ///     .liveliness()
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
    fn querying(self) -> QueryingSubscriberBuilder<'a, 'b, Self::KeySpace, Handler> {
        QueryingSubscriberBuilder {
            session: self.session,
            key_expr: self.key_expr,
            key_space: crate::LivelinessSpace,
            reliability: Reliability::default(),
            origin: Locality::default(),
            query_selector: None,
            query_target: QueryTarget::default(),
            query_consolidation: QueryConsolidation::default(),
            query_accept_replies: ReplyKeyExpr::MatchingQuery,
            query_timeout: Duration::from_secs(10),
            handler: self.handler,
        }
    }
}
