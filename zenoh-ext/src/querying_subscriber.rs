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
use std::{
    collections::{btree_map, BTreeMap, VecDeque},
    convert::TryInto,
    future::{IntoFuture, Ready},
    mem::swap,
    sync::{Arc, Mutex},
    time::Duration,
};

use zenoh::{
    core::{Error, Resolvable, Resolve, Result as ZResult},
    handlers::{locked, DefaultHandler, IntoHandler},
    internal::zlock,
    key_expr::KeyExpr,
    prelude::Wait,
    query::{QueryConsolidation, QueryTarget, ReplyKeyExpr},
    sample::{Locality, Sample, SampleBuilder, TimestampBuilderTrait},
    session::{SessionDeclarations, SessionRef},
    subscriber::{Reliability, Subscriber},
    time::{new_timestamp, Timestamp},
};

use crate::ExtractSample;

/// The builder of [`FetchingSubscriber`], allowing to configure it.
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
pub struct QueryingSubscriberBuilder<'a, 'b, KeySpace, Handler> {
    pub(crate) session: SessionRef<'a>,
    pub(crate) key_expr: ZResult<KeyExpr<'b>>,
    pub(crate) key_space: KeySpace,
    pub(crate) reliability: Reliability,
    pub(crate) origin: Locality,
    pub(crate) query_selector: Option<ZResult<KeyExpr<'b>>>,
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
        Handler: IntoHandler<'static, Sample>,
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
        IntoSelector: TryInto<KeyExpr<'b>>,
        <IntoSelector as TryInto<KeyExpr<'b>>>::Error: Into<Error>,
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
    Handler: IntoHandler<'static, Sample>,
    Handler::Handler: Send,
{
    type To = ZResult<FetchingSubscriber<'a, Handler::Handler>>;
}

impl<KeySpace, Handler> Wait for QueryingSubscriberBuilder<'_, '_, KeySpace, Handler>
where
    KeySpace: Into<crate::KeySpace> + Clone,
    Handler: IntoHandler<'static, Sample> + Send,
    Handler::Handler: Send,
{
    fn wait(self) -> <Self as Resolvable>::To {
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
                .wait(),
                crate::KeySpace::Liveliness => session
                    .liveliness()
                    .get(key_expr)
                    .callback(cb)
                    .timeout(query_timeout)
                    .wait(),
            },
            handler: self.handler,
            phantom: std::marker::PhantomData,
        }
        .wait()
    }
}

impl<'a, KeySpace, Handler> IntoFuture for QueryingSubscriberBuilder<'a, '_, KeySpace, Handler>
where
    KeySpace: Into<crate::KeySpace> + Clone,
    Handler: IntoHandler<'static, Sample> + Send,
    Handler::Handler: Send,
{
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
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
        if let Some(ts) = sample.timestamp() {
            self.timstamped.entry(*ts).or_insert(sample);
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
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
pub struct FetchingSubscriberBuilder<
    'a,
    'b,
    KeySpace,
    Handler,
    Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()>,
    TryIntoSample,
> where
    TryIntoSample: ExtractSample,
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
    TryIntoSample: ExtractSample,
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
    TryIntoSample: ExtractSample,
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
        Handler: IntoHandler<'static, Sample>,
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
    TryIntoSample: ExtractSample,
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
    Handler: IntoHandler<'static, Sample>,
    Handler::Handler: Send,
    TryIntoSample: ExtractSample,
{
    type To = ZResult<FetchingSubscriber<'a, Handler::Handler>>;
}

impl<
        KeySpace,
        Handler,
        Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()> + Send + Sync,
        TryIntoSample,
    > Wait for FetchingSubscriberBuilder<'_, '_, KeySpace, Handler, Fetch, TryIntoSample>
where
    KeySpace: Into<crate::KeySpace>,
    Handler: IntoHandler<'static, Sample> + Send,
    Handler::Handler: Send,
    TryIntoSample: ExtractSample + Send + Sync,
{
    fn wait(self) -> <Self as Resolvable>::To {
        FetchingSubscriber::new(self.with_static_keys())
    }
}

impl<
        'a,
        KeySpace,
        Handler,
        Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()> + Send + Sync,
        TryIntoSample,
    > IntoFuture for FetchingSubscriberBuilder<'a, '_, KeySpace, Handler, Fetch, TryIntoSample>
where
    KeySpace: Into<crate::KeySpace>,
    Handler: IntoHandler<'static, Sample> + Send,
    Handler::Handler: Send,
    TryIntoSample: ExtractSample + Send + Sync,
{
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
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
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh::prelude::*;
/// use zenoh_ext::*;
///
/// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
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
pub struct FetchingSubscriber<'a, Handler> {
    subscriber: Subscriber<'a, ()>,
    callback: Arc<dyn Fn(Sample) + Send + Sync + 'static>,
    state: Arc<Mutex<InnerState>>,
    handler: Handler,
}

impl<Handler> std::ops::Deref for FetchingSubscriber<'_, Handler> {
    type Target = Handler;
    fn deref(&self) -> &Self::Target {
        &self.handler
    }
}

impl<Handler> std::ops::DerefMut for FetchingSubscriber<'_, Handler> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.handler
    }
}

impl<'a, Handler> FetchingSubscriber<'a, Handler> {
    fn new<
        KeySpace,
        InputHandler,
        Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()> + Send + Sync,
        TryIntoSample,
    >(
        conf: FetchingSubscriberBuilder<'a, 'a, KeySpace, InputHandler, Fetch, TryIntoSample>,
    ) -> ZResult<Self>
    where
        KeySpace: Into<crate::KeySpace>,
        InputHandler: IntoHandler<'static, Sample, Handler = Handler> + Send,
        TryIntoSample: ExtractSample + Send + Sync,
    {
        let zid = conf.session.zid();
        let state = Arc::new(Mutex::new(InnerState {
            pending_fetches: 0,
            merge_queue: MergeQueue::new(),
        }));
        let (callback, receiver) = conf.handler.into_handler();

        let sub_callback = {
            let state = state.clone();
            let callback = callback.clone();
            move |s| {
                let state = &mut zlock!(state);
                if state.pending_fetches == 0 {
                    callback(s);
                } else {
                    tracing::trace!(
                        "Sample received while fetch in progress: push it to merge_queue"
                    );
                    // ensure the sample has a timestamp, thus it will always be sorted into the MergeQueue
                    // after any timestamped Sample possibly coming from a fetch reply.
                    let timestamp = s.timestamp().cloned().unwrap_or(new_timestamp(zid));
                    state
                        .merge_queue
                        .push(SampleBuilder::from(s).timestamp(timestamp).into());
                }
            }
        };

        let key_expr = conf.key_expr?;

        // register fetch handler
        let handler = register_handler(state.clone(), callback.clone());
        // declare subscriber
        let subscriber = match conf.key_space.into() {
            crate::KeySpace::User => conf
                .session
                .declare_subscriber(&key_expr)
                .callback(sub_callback)
                .reliability(conf.reliability)
                .allowed_origin(conf.origin)
                .wait()?,
            crate::KeySpace::Liveliness => conf
                .session
                .liveliness()
                .declare_subscriber(&key_expr)
                .callback(sub_callback)
                .wait()?,
        };

        let fetch_subscriber = FetchingSubscriber {
            subscriber,
            callback,
            state,
            handler: receiver,
        };

        // run fetch
        run_fetch(conf.fetch, handler)?;

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
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::*;
    /// use zenoh_ext::*;
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// let mut subscriber = session
    ///     .declare_subscriber("key/expr")
    ///     .fetching( |cb| {
    ///         session
    ///             .get("key/expr")
    ///             .callback(cb)
    ///             .wait()
    ///     })
    ///     .await
    ///     .unwrap();
    ///
    /// // perform an additional fetch
    /// subscriber
    ///     .fetch( |cb| {
    ///         session
    ///             .get("key/expr")
    ///             .callback(cb)
    ///             .wait()
    ///     })
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
    #[inline]
    pub fn fetch<
        Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()> + Send + Sync,
        TryIntoSample,
    >(
        &self,
        fetch: Fetch,
    ) -> impl Resolve<ZResult<()>>
    where
        TryIntoSample: ExtractSample + Send + Sync,
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
        tracing::trace!(
            "Fetch done - {} fetches still in progress",
            state.pending_fetches
        );
        if state.pending_fetches == 0 {
            tracing::debug!(
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
/// # #[tokio::main]
/// # async fn main() {
/// # use zenoh::prelude::*;
/// # use zenoh_ext::*;
/// #
/// # let session = zenoh::open(zenoh::config::peer()).await.unwrap();
/// # let mut fetching_subscriber = session
/// #     .declare_subscriber("key/expr")
/// #     .fetching( |cb| {
/// #         session
/// #             .get("key/expr")
/// #             .callback(cb)
/// #            .wait()
/// #     })
/// #     .await
/// #     .unwrap();
/// #
/// fetching_subscriber
///     .fetch( |cb| {
///         session
///             .get("key/expr")
///             .callback(cb)
///             .wait()
///     })
///     .await
///     .unwrap();
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
pub struct FetchBuilder<
    Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()>,
    TryIntoSample,
> where
    TryIntoSample: ExtractSample,
{
    fetch: Fetch,
    phantom: std::marker::PhantomData<TryIntoSample>,
    state: Arc<Mutex<InnerState>>,
    callback: Arc<dyn Fn(Sample) + Send + Sync>,
}

impl<Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()>, TryIntoSample>
    Resolvable for FetchBuilder<Fetch, TryIntoSample>
where
    TryIntoSample: ExtractSample,
{
    type To = ZResult<()>;
}

impl<Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()>, TryIntoSample> Wait
    for FetchBuilder<Fetch, TryIntoSample>
where
    TryIntoSample: ExtractSample,
{
    fn wait(self) -> <Self as Resolvable>::To {
        let handler = register_handler(self.state, self.callback);
        run_fetch(self.fetch, handler)
    }
}

impl<Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()>, TryIntoSample>
    IntoFuture for FetchBuilder<Fetch, TryIntoSample>
where
    TryIntoSample: ExtractSample,
{
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

fn register_handler(
    state: Arc<Mutex<InnerState>>,
    callback: Arc<dyn Fn(Sample) + Send + Sync>,
) -> RepliesHandler {
    zlock!(state).pending_fetches += 1;
    // pending fetches will be decremented in RepliesHandler drop()
    RepliesHandler { state, callback }
}

fn run_fetch<
    Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()>,
    TryIntoSample,
>(
    fetch: Fetch,
    handler: RepliesHandler,
) -> ZResult<()>
where
    TryIntoSample: ExtractSample,
{
    tracing::debug!("Fetch data for FetchingSubscriber");
    (fetch)(Box::new(move |s: TryIntoSample| match s.extract() {
        Ok(s) => {
            let mut state = zlock!(handler.state);
            tracing::trace!("Fetched sample received: push it to merge_queue");
            state.merge_queue.push(s);
        }
        Err(e) => tracing::debug!("Received error fetching data: {}", e),
    }))
}
