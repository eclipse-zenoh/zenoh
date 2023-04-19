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
use std::collections::{btree_map, BTreeMap, VecDeque};
use std::convert::TryInto;
use std::future::Ready;
use std::mem::swap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use zenoh::handlers::{locked, DefaultHandler};
use zenoh::prelude::r#async::*;
use zenoh::query::{QueryConsolidation, QueryTarget, ReplyKeyExpr};
use zenoh::subscriber::{Reliability, Subscriber};
use zenoh::time::Timestamp;
use zenoh::Result as ZResult;
use zenoh::SessionRef;
use zenoh_core::{zlock, AsyncResolve, Resolvable, SyncResolve};

/// The builder of [`FetchingSubscriber`], allowing to configure it.
pub struct QueryingSubscriberBuilder<'a, 'b, KeySpace, Handler> {
    pub(crate) session: SessionRef<'a>,
    pub(crate) key_expr: ZResult<KeyExpr<'b>>,
    pub(crate) key_space: KeySpace,
    pub(crate) reliability: Reliability,
    pub(crate) origin: Locality,
    pub(crate) query_selector: Option<ZResult<Selector<'b>>>,
    pub(crate) query_target: QueryTarget,
    pub(crate) query_consolidation: QueryConsolidation,
    pub(crate) query_accept_replies: ReplyKeyExpr,
    pub(crate) query_timeout: Duration,
    pub(crate) handler: Handler,
}

impl<'a, 'b, KeySpace> QueryingSubscriberBuilder<'a, 'b, KeySpace, DefaultHandler> {
    /// Add callback to [`FetchingSubscriber`].
    #[inline]
    pub fn callback<Callback>(
        self,
        callback: Callback,
    ) -> QueryingSubscriberBuilder<'a, 'b, KeySpace, Callback>
    where
        Callback: Fn(Sample) + Send + Sync + 'static,
    {
        let QueryingSubscriberBuilder {
            session,
            key_expr,
            key_space,
            reliability,
            origin,
            query_selector,
            query_target,
            query_consolidation,
            query_accept_replies,
            query_timeout,
            handler: _,
        } = self;
        QueryingSubscriberBuilder {
            session,
            key_expr,
            key_space,
            reliability,
            origin,
            query_selector,
            query_target,
            query_consolidation,
            query_accept_replies,
            query_timeout,
            handler: callback,
        }
    }

    /// Add callback to [`FetchingSubscriber`].
    ///
    /// Using this guarantees that your callback will never be called concurrently.
    /// If your callback is also accepted by the [`callback`](QueryingSubscriberBuilder::callback)
    /// method, we suggest you use it instead of `callback_mut`
    #[inline]
    pub fn callback_mut<CallbackMut>(
        self,
        callback: CallbackMut,
    ) -> QueryingSubscriberBuilder<'a, 'b, KeySpace, impl Fn(Sample) + Send + Sync + 'static>
    where
        CallbackMut: FnMut(Sample) + Send + Sync + 'static,
    {
        self.callback(locked(callback))
    }

    /// Use the given handler to recieve Samples.
    #[inline]
    pub fn with<Handler>(
        self,
        handler: Handler,
    ) -> QueryingSubscriberBuilder<'a, 'b, KeySpace, Handler>
    where
        Handler: zenoh::prelude::IntoCallbackReceiverPair<'static, Sample>,
    {
        let QueryingSubscriberBuilder {
            session,
            key_expr,
            key_space,
            reliability,
            origin,
            query_selector,
            query_target,
            query_consolidation,
            query_accept_replies,
            query_timeout,
            handler: _,
        } = self;
        QueryingSubscriberBuilder {
            session,
            key_expr,
            key_space,
            reliability,
            origin,
            query_selector,
            query_target,
            query_consolidation,
            query_accept_replies,
            query_timeout,
            handler,
        }
    }
}

impl<'a, 'b, Handler> QueryingSubscriberBuilder<'a, 'b, crate::UserSpace, Handler> {
    /// Change the subscription reliability.
    #[inline]
    pub fn reliability(mut self, reliability: Reliability) -> Self {
        self.reliability = reliability;
        self
    }

    /// Change the subscription reliability to Reliable.
    #[inline]
    pub fn reliable(mut self) -> Self {
        self.reliability = Reliability::Reliable;
        self
    }

    /// Change the subscription reliability to BestEffort.
    #[inline]
    pub fn best_effort(mut self) -> Self {
        self.reliability = Reliability::BestEffort;
        self
    }

    /// Restrict the matching publications that will be receive by this [`Subscriber`]
    /// to the ones that have the given [`Locality`](zenoh::prelude::Locality).
    #[zenoh_macros::unstable]
    #[inline]
    pub fn allowed_origin(mut self, origin: Locality) -> Self {
        self.origin = origin;
        self
    }

    /// Change the selector to be used for queries.
    #[inline]
    pub fn query_selector<IntoSelector>(mut self, query_selector: IntoSelector) -> Self
    where
        IntoSelector: TryInto<Selector<'b>>,
        <IntoSelector as TryInto<Selector<'b>>>::Error: Into<zenoh_result::Error>,
    {
        self.query_selector = Some(query_selector.try_into().map_err(Into::into));
        self
    }

    /// Change the target to be used for queries.
    #[inline]
    pub fn query_target(mut self, query_target: QueryTarget) -> Self {
        self.query_target = query_target;
        self
    }

    /// Change the consolidation mode to be used for queries.
    #[inline]
    pub fn query_consolidation<QC: Into<QueryConsolidation>>(
        mut self,
        query_consolidation: QC,
    ) -> Self {
        self.query_consolidation = query_consolidation.into();
        self
    }

    /// Change the accepted replies for queries.
    #[inline]
    pub fn query_accept_replies(mut self, accept_replies: ReplyKeyExpr) -> Self {
        self.query_accept_replies = accept_replies;
        self
    }
}

impl<'a, 'b, KeySpace, Handler> QueryingSubscriberBuilder<'a, 'b, KeySpace, Handler> {
    /// Change the timeout to be used for queries.
    #[inline]
    pub fn query_timeout(mut self, query_timeout: Duration) -> Self {
        self.query_timeout = query_timeout;
        self
    }
}

impl<'a, KeySpace, Handler> Resolvable for QueryingSubscriberBuilder<'a, '_, KeySpace, Handler>
where
    Handler: IntoCallbackReceiverPair<'static, Sample>,
    Handler::Receiver: Send,
{
    type To = ZResult<FetchingSubscriber<'a, Handler::Receiver>>;
}

impl<KeySpace, Handler> SyncResolve for QueryingSubscriberBuilder<'_, '_, KeySpace, Handler>
where
    KeySpace: Into<crate::KeySpace> + Clone,
    Handler: IntoCallbackReceiverPair<'static, Sample> + Send,
    Handler::Receiver: Send,
{
    fn res_sync(self) -> <Self as Resolvable>::To {
        let session = self.session.clone();
        let key_expr = self.key_expr?;
        let key_space = self.key_space.clone().into();
        let query_selector = match self.query_selector {
            Some(s) => Some(s?),
            None => None,
        };
        let query_target = self.query_target;
        let query_consolidation = self.query_consolidation;
        let query_accept_replies = self.query_accept_replies;
        let query_timeout = self.query_timeout;
        FetchingSubscriberBuilder {
            session: self.session,
            key_expr: Ok(key_expr.clone()),
            key_space: self.key_space,
            reliability: self.reliability,
            origin: self.origin,
            fetch: |cb| match key_space {
                crate::KeySpace::User => match query_selector {
                    Some(s) => session.get(s),
                    None => session.get(key_expr),
                }
                .callback(cb)
                .target(query_target)
                .consolidation(query_consolidation)
                .accept_replies(query_accept_replies)
                .timeout(query_timeout)
                .res_sync(),
                crate::KeySpace::Liveliness => session
                    .liveliness()
                    .get(key_expr)
                    .callback(cb)
                    .timeout(query_timeout)
                    .res_sync(),
            },
            handler: self.handler,
            phantom: std::marker::PhantomData,
        }
        .res_sync()
    }
}

impl<'a, KeySpace, Handler> AsyncResolve for QueryingSubscriberBuilder<'a, '_, KeySpace, Handler>
where
    KeySpace: Into<crate::KeySpace> + Clone,
    Handler: IntoCallbackReceiverPair<'static, Sample> + Send,
    Handler::Receiver: Send,
{
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}

// Collects samples in their Timestamp order, if any,
// and ignores repeating samples with duplicate timestamps.
// Samples without Timestamps are kept in a separate Vector,
// and are considered as older than any sample with Timestamp.
struct MergeQueue {
    untimestamped: VecDeque<Sample>,
    timstamped: BTreeMap<Timestamp, Sample>,
}

impl MergeQueue {
    fn new() -> Self {
        MergeQueue {
            untimestamped: VecDeque::new(),
            timstamped: BTreeMap::new(),
        }
    }

    fn len(&self) -> usize {
        self.untimestamped.len() + self.timstamped.len()
    }

    fn push(&mut self, sample: Sample) {
        if let Some(ts) = sample.timestamp {
            self.timstamped.entry(ts).or_insert(sample);
        } else {
            self.untimestamped.push_back(sample);
        }
    }

    fn drain(&mut self) -> MergeQueueValues {
        let mut vec = VecDeque::new();
        let mut queue = BTreeMap::new();
        swap(&mut self.untimestamped, &mut vec);
        swap(&mut self.timstamped, &mut queue);
        MergeQueueValues {
            untimestamped: vec,
            timstamped: queue.into_values(),
        }
    }
}

struct MergeQueueValues {
    untimestamped: VecDeque<Sample>,
    timstamped: btree_map::IntoValues<Timestamp, Sample>,
}

impl Iterator for MergeQueueValues {
    type Item = Sample;
    fn next(&mut self) -> Option<Self::Item> {
        self.untimestamped
            .pop_front()
            .or_else(|| self.timstamped.next())
    }
}

struct InnerState {
    pending_fetches: u64,
    merge_queue: MergeQueue,
}

/// The builder of [`FetchingSubscriber`], allowing to configure it.
pub struct FetchingSubscriberBuilder<
    'a,
    'b,
    KeySpace,
    Handler,
    Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()>,
    TryIntoSample,
> where
    TryIntoSample: TryInto<Sample>,
    <TryIntoSample as TryInto<Sample>>::Error: Into<zenoh_core::Error>,
{
    pub(crate) session: SessionRef<'a>,
    pub(crate) key_expr: ZResult<KeyExpr<'b>>,
    pub(crate) key_space: KeySpace,
    pub(crate) reliability: Reliability,
    pub(crate) origin: Locality,
    pub(crate) fetch: Fetch,
    pub(crate) handler: Handler,
    pub(crate) phantom: std::marker::PhantomData<TryIntoSample>,
}

impl<
        'a,
        'b,
        KeySpace,
        Handler,
        Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()>,
        TryIntoSample,
    > FetchingSubscriberBuilder<'a, 'b, KeySpace, Handler, Fetch, TryIntoSample>
where
    TryIntoSample: TryInto<Sample>,
    <TryIntoSample as TryInto<Sample>>::Error: Into<zenoh_core::Error>,
{
    fn with_static_keys(
        self,
    ) -> FetchingSubscriberBuilder<'a, 'static, KeySpace, Handler, Fetch, TryIntoSample> {
        FetchingSubscriberBuilder {
            session: self.session,
            key_expr: self.key_expr.map(|s| s.into_owned()),
            key_space: self.key_space,
            reliability: self.reliability,
            origin: self.origin,
            fetch: self.fetch,
            handler: self.handler,
            phantom: std::marker::PhantomData,
        }
    }
}

impl<
        'a,
        'b,
        KeySpace,
        Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()>,
        TryIntoSample,
    > FetchingSubscriberBuilder<'a, 'b, KeySpace, DefaultHandler, Fetch, TryIntoSample>
where
    TryIntoSample: TryInto<Sample>,
    <TryIntoSample as TryInto<Sample>>::Error: Into<zenoh_core::Error>,
{
    /// Add callback to [`FetchingSubscriber`].
    #[inline]
    pub fn callback<Callback>(
        self,
        callback: Callback,
    ) -> FetchingSubscriberBuilder<'a, 'b, KeySpace, Callback, Fetch, TryIntoSample>
    where
        Callback: Fn(Sample) + Send + Sync + 'static,
    {
        let FetchingSubscriberBuilder {
            session,
            key_expr,
            key_space,
            reliability,
            origin,
            fetch,
            handler: _,
            phantom,
        } = self;
        FetchingSubscriberBuilder {
            session,
            key_expr,
            key_space,
            reliability,
            origin,
            fetch,
            handler: callback,
            phantom,
        }
    }

    /// Add callback to [`FetchingSubscriber`].
    ///
    /// Using this guarantees that your callback will never be called concurrently.
    /// If your callback is also accepted by the [`callback`](FetchingSubscriberBuilder::callback)
    /// method, we suggest you use it instead of `callback_mut`
    #[inline]
    pub fn callback_mut<CallbackMut>(
        self,
        callback: CallbackMut,
    ) -> FetchingSubscriberBuilder<
        'a,
        'b,
        KeySpace,
        impl Fn(Sample) + Send + Sync + 'static,
        Fetch,
        TryIntoSample,
    >
    where
        CallbackMut: FnMut(Sample) + Send + Sync + 'static,
    {
        self.callback(locked(callback))
    }

    /// Use the given handler to receive Samples.
    #[inline]
    pub fn with<Handler>(
        self,
        handler: Handler,
    ) -> FetchingSubscriberBuilder<'a, 'b, KeySpace, Handler, Fetch, TryIntoSample>
    where
        Handler: zenoh::prelude::IntoCallbackReceiverPair<'static, Sample>,
    {
        let FetchingSubscriberBuilder {
            session,
            key_expr,
            key_space,
            reliability,
            origin,
            fetch,
            handler: _,
            phantom,
        } = self;
        FetchingSubscriberBuilder {
            session,
            key_expr,
            key_space,
            reliability,
            origin,
            fetch,
            handler,
            phantom,
        }
    }
}

impl<
        'a,
        'b,
        Handler,
        Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()>,
        TryIntoSample,
    > FetchingSubscriberBuilder<'a, 'b, crate::UserSpace, Handler, Fetch, TryIntoSample>
where
    TryIntoSample: TryInto<Sample>,
    <TryIntoSample as TryInto<Sample>>::Error: Into<zenoh_core::Error>,
{
    /// Change the subscription reliability.
    #[inline]
    pub fn reliability(mut self, reliability: Reliability) -> Self {
        self.reliability = reliability;
        self
    }

    /// Change the subscription reliability to Reliable.
    #[inline]
    pub fn reliable(mut self) -> Self {
        self.reliability = Reliability::Reliable;
        self
    }

    /// Change the subscription reliability to BestEffort.
    #[inline]
    pub fn best_effort(mut self) -> Self {
        self.reliability = Reliability::BestEffort;
        self
    }

    /// Restrict the matching publications that will be receive by this [`FetchingSubscriber`]
    /// to the ones that have the given [`Locality`](zenoh::prelude::Locality).
    #[zenoh_macros::unstable]
    #[inline]
    pub fn allowed_origin(mut self, origin: Locality) -> Self {
        self.origin = origin;
        self
    }
}

impl<
        'a,
        KeySpace,
        Handler,
        Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()>,
        TryIntoSample,
    > Resolvable for FetchingSubscriberBuilder<'a, '_, KeySpace, Handler, Fetch, TryIntoSample>
where
    Handler: IntoCallbackReceiverPair<'static, Sample>,
    Handler::Receiver: Send,
    TryIntoSample: TryInto<Sample>,
    <TryIntoSample as TryInto<Sample>>::Error: Into<zenoh_core::Error>,
{
    type To = ZResult<FetchingSubscriber<'a, Handler::Receiver>>;
}

impl<
        KeySpace,
        Handler,
        Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()> + Send + Sync,
        TryIntoSample,
    > SyncResolve for FetchingSubscriberBuilder<'_, '_, KeySpace, Handler, Fetch, TryIntoSample>
where
    KeySpace: Into<crate::KeySpace>,
    Handler: IntoCallbackReceiverPair<'static, Sample> + Send,
    Handler::Receiver: Send,
    TryIntoSample: TryInto<Sample> + Send + Sync,
    <TryIntoSample as TryInto<Sample>>::Error: Into<zenoh_core::Error>,
{
    fn res_sync(self) -> <Self as Resolvable>::To {
        FetchingSubscriber::new(self.with_static_keys())
    }
}

impl<
        'a,
        KeySpace,
        Handler,
        Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()> + Send + Sync,
        TryIntoSample,
    > AsyncResolve for FetchingSubscriberBuilder<'a, '_, KeySpace, Handler, Fetch, TryIntoSample>
where
    KeySpace: Into<crate::KeySpace>,
    Handler: IntoCallbackReceiverPair<'static, Sample> + Send,
    Handler::Receiver: Send,
    TryIntoSample: TryInto<Sample> + Send + Sync,
    <TryIntoSample as TryInto<Sample>>::Error: Into<zenoh_core::Error>,
{
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}

/// A Subscriber that will run the given user defined `fetch` funtion at startup.
///
/// The user defined `fetch` funtion should fetch some samples and return them through the callback funtion
/// (it could typically be a Session::get()). Those samples will be merged with the received publications and made available in the receiver.
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
pub struct FetchingSubscriber<'a, Receiver> {
    subscriber: Subscriber<'a, ()>,
    callback: Arc<dyn Fn(Sample) + Send + Sync + 'static>,
    state: Arc<Mutex<InnerState>>,
    receiver: Receiver,
}

impl<Receiver> std::ops::Deref for FetchingSubscriber<'_, Receiver> {
    type Target = Receiver;
    fn deref(&self) -> &Self::Target {
        &self.receiver
    }
}

impl<Receiver> std::ops::DerefMut for FetchingSubscriber<'_, Receiver> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.receiver
    }
}

impl<'a, Receiver> FetchingSubscriber<'a, Receiver> {
    fn new<
        KeySpace,
        Handler,
        Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()> + Send + Sync,
        TryIntoSample,
    >(
        conf: FetchingSubscriberBuilder<'a, 'a, KeySpace, Handler, Fetch, TryIntoSample>,
    ) -> ZResult<Self>
    where
        KeySpace: Into<crate::KeySpace>,
        Handler: IntoCallbackReceiverPair<'static, Sample, Receiver = Receiver> + Send,
        TryIntoSample: TryInto<Sample> + Send + Sync,
        <TryIntoSample as TryInto<Sample>>::Error: Into<zenoh_core::Error>,
    {
        let state = Arc::new(Mutex::new(InnerState {
            pending_fetches: 0,
            merge_queue: MergeQueue::new(),
        }));
        let (callback, receiver) = conf.handler.into_cb_receiver_pair();

        let sub_callback = {
            let state = state.clone();
            let callback = callback.clone();
            move |mut s| {
                let state = &mut zlock!(state);
                if state.pending_fetches == 0 {
                    callback(s);
                } else {
                    log::trace!("Sample received while fetch in progress: push it to merge_queue");
                    // ensure the sample has a timestamp, thus it will always be sorted into the MergeQueue
                    // after any timestamped Sample possibly coming from a fetch reply.
                    s.ensure_timestamp();
                    state.merge_queue.push(s);
                }
            }
        };

        let key_expr = conf.key_expr?;

        // declare subscriber at first
        let subscriber = match conf.session.clone() {
            SessionRef::Borrow(session) => match conf.key_space.into() {
                crate::KeySpace::User => session
                    .declare_subscriber(&key_expr)
                    .callback(sub_callback)
                    .reliability(conf.reliability)
                    .allowed_origin(conf.origin)
                    .res_sync()?,
                crate::KeySpace::Liveliness => session
                    .liveliness()
                    .declare_subscriber(&key_expr)
                    .callback(sub_callback)
                    .res_sync()?,
            },
            SessionRef::Shared(session) => match conf.key_space.into() {
                crate::KeySpace::User => session
                    .declare_subscriber(&key_expr)
                    .callback(sub_callback)
                    .reliability(conf.reliability)
                    .allowed_origin(conf.origin)
                    .res_sync()?,
                crate::KeySpace::Liveliness => session
                    .liveliness()
                    .declare_subscriber(&key_expr)
                    .callback(sub_callback)
                    .res_sync()?,
            },
        };

        let mut fetch_subscriber = FetchingSubscriber {
            subscriber,
            callback,
            state,
            receiver,
        };

        // start fetch
        fetch_subscriber.fetch(conf.fetch).res_sync()?;

        Ok(fetch_subscriber)
    }

    /// Close this FetchingSubscriber
    #[inline]
    pub fn close(self) -> impl Resolve<ZResult<()>> + 'a {
        self.subscriber.undeclare()
    }

    /// Return the key expression of this FetchingSubscriber
    #[inline]
    pub fn key_expr(&self) -> &KeyExpr<'static> {
        self.subscriber.key_expr()
    }

    /// Perform an additional `fetch`.
    ///
    /// The provided `fetch` funtion should fetch some samples and return them through the callback funtion
    /// (it could typically be a Session::get()). Those samples will be merged with the received publications and made available in the receiver.
    ///
    /// # Examples
    /// ```no_run
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    /// use zenoh_ext::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let mut subscriber = session
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
    ///
    /// // perform an additional fetch
    /// subscriber
    ///     .fetch( |cb| {
    ///         use zenoh::prelude::sync::SyncResolve;
    ///         session
    ///             .get("key/expr")
    ///             .callback(cb)
    ///             .res_sync()
    ///     })
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// # })
    /// ```
    #[inline]
    pub fn fetch<
        Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()> + Send + Sync,
        TryIntoSample,
    >(
        &mut self,
        fetch: Fetch,
    ) -> impl Resolve<ZResult<()>>
    where
        TryIntoSample: TryInto<Sample> + Send + Sync,
        <TryIntoSample as TryInto<Sample>>::Error: Into<zenoh_core::Error>,
    {
        FetchBuilder {
            fetch,
            phantom: std::marker::PhantomData,
            state: self.state.clone(),
            callback: self.callback.clone(),
        }
    }
}

struct RepliesHandler {
    state: Arc<Mutex<InnerState>>,
    callback: Arc<dyn Fn(Sample) + Send + Sync>,
}

impl Drop for RepliesHandler {
    fn drop(&mut self) {
        let mut state = zlock!(self.state);
        state.pending_fetches -= 1;
        log::trace!(
            "Fetch done - {} fetches still in progress",
            state.pending_fetches
        );
        if state.pending_fetches == 0 {
            log::debug!(
                "All fetches done. Replies and live publications merged - {} samples to propagate",
                state.merge_queue.len()
            );
            for s in state.merge_queue.drain() {
                (self.callback)(s);
            }
        }
    }
}

/// The builder returned by [`FetchingSubscriber::fetch`](FetchingSubscriber::fetch).
///
/// # Examples
/// ```no_run
/// # async_std::task::block_on(async {
/// # use zenoh::prelude::r#async::*;
/// # use zenoh_ext::*;
/// #
/// # let session = zenoh::open(config::peer()).res().await.unwrap();
/// # let mut fetching_subscriber = session
/// #     .declare_subscriber("key/expr")
/// #     .fetching( |cb| {
/// #         use zenoh::prelude::sync::SyncResolve;
/// #         session
/// #             .get("key/expr")
/// #             .callback(cb)
/// #            .res_sync()
/// #     })
/// #     .res()
/// #     .await
/// #     .unwrap();
/// #
/// fetching_subscriber
///     .fetch( |cb| {
///         use zenoh::prelude::sync::SyncResolve;
///         session
///             .get("key/expr")
///             .callback(cb)
///             .res_sync()
///     })
///     .res()
///     .await
///     .unwrap();
/// # })
/// ```
pub struct FetchBuilder<
    Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()>,
    TryIntoSample,
> where
    TryIntoSample: TryInto<Sample>,
    <TryIntoSample as TryInto<Sample>>::Error: Into<zenoh_core::Error>,
{
    fetch: Fetch,
    phantom: std::marker::PhantomData<TryIntoSample>,
    state: Arc<Mutex<InnerState>>,
    callback: Arc<dyn Fn(Sample) + Send + Sync>,
}

impl<Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()>, TryIntoSample>
    Resolvable for FetchBuilder<Fetch, TryIntoSample>
where
    TryIntoSample: TryInto<Sample>,
    <TryIntoSample as TryInto<Sample>>::Error: Into<zenoh_core::Error>,
{
    type To = ZResult<()>;
}

impl<Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()>, TryIntoSample>
    SyncResolve for FetchBuilder<Fetch, TryIntoSample>
where
    TryIntoSample: TryInto<Sample>,
    <TryIntoSample as TryInto<Sample>>::Error: Into<zenoh_core::Error>,
{
    fn res_sync(self) -> <Self as Resolvable>::To {
        zlock!(self.state).pending_fetches += 1;
        // pending fetches will be decremented in RepliesHandler drop()
        let handler = RepliesHandler {
            state: self.state,
            callback: self.callback,
        };

        log::debug!("Fetch");
        (self.fetch)(Box::new(move |s: TryIntoSample| match s.try_into() {
            Ok(s) => {
                let mut state = zlock!(handler.state);
                log::trace!("Fetched sample received: push it to merge_queue");
                state.merge_queue.push(s);
            }
            Err(e) => log::debug!("Received error fetching data: {}", e.into()),
        }))
    }
}

impl<Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()>, TryIntoSample>
    AsyncResolve for FetchBuilder<Fetch, TryIntoSample>
where
    TryIntoSample: TryInto<Sample>,
    <TryIntoSample as TryInto<Sample>>::Error: Into<zenoh_core::Error>,
{
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}
