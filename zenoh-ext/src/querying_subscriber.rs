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
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use zenoh::{
    handlers::{locked, Callback, DefaultHandler, IntoHandler},
    internal::{zerror, zlock},
    key_expr::KeyExpr,
    pubsub::Subscriber,
    query::{QueryConsolidation, QueryTarget, Reply, ReplyKeyExpr, Selector},
    sample::{Locality, Sample, SampleBuilder},
    time::Timestamp,
    Error, Resolvable, Resolve, Result as ZResult, Session, Wait,
};

/// The space of keys to use in a [`FetchingSubscriber`].
#[zenoh_macros::unstable]
#[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
pub enum KeySpace {
    User,
    Liveliness,
}

/// The key space for user data.
#[zenoh_macros::unstable]
#[non_exhaustive]
#[derive(Debug, Clone, Copy)]
#[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
pub struct UserSpace;

#[allow(deprecated)]
impl From<UserSpace> for KeySpace {
    fn from(_: UserSpace) -> Self {
        KeySpace::User
    }
}

/// The key space for liveliness tokens.
#[zenoh_macros::unstable]
#[non_exhaustive]
#[derive(Debug, Clone, Copy)]
#[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
pub struct LivelinessSpace;

#[zenoh_macros::unstable]
#[allow(deprecated)]
impl From<LivelinessSpace> for KeySpace {
    #[zenoh_macros::unstable]
    fn from(_: LivelinessSpace) -> Self {
        KeySpace::Liveliness
    }
}

/// The builder of [`FetchingSubscriber`], allowing to configure it.
#[zenoh_macros::unstable]
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
#[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
pub struct QueryingSubscriberBuilder<'a, 'b, KeySpace, Handler, const BACKGROUND: bool = false> {
    pub(crate) session: &'a Session,
    pub(crate) key_expr: ZResult<KeyExpr<'b>>,
    pub(crate) key_space: KeySpace,
    pub(crate) origin: Locality,
    pub(crate) query_selector: Option<ZResult<Selector<'b>>>,
    pub(crate) query_target: QueryTarget,
    pub(crate) query_consolidation: QueryConsolidation,
    pub(crate) query_accept_replies: ReplyKeyExpr,
    pub(crate) query_timeout: Duration,
    pub(crate) handler: Handler,
}

#[zenoh_macros::unstable]
#[allow(deprecated)]
impl<'a, 'b, KeySpace> QueryingSubscriberBuilder<'a, 'b, KeySpace, DefaultHandler> {
    /// Add callback to [`FetchingSubscriber`].
    #[zenoh_macros::unstable]
    #[inline]
    #[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
    pub fn callback<F>(
        self,
        callback: F,
    ) -> QueryingSubscriberBuilder<'a, 'b, KeySpace, Callback<Sample>>
    where
        F: Fn(Sample) + Send + Sync + 'static,
    {
        self.with(Callback::from(callback))
    }

    /// Add callback to [`FetchingSubscriber`].
    ///
    /// Using this guarantees that your callback will never be called concurrently.
    /// If your callback is also accepted by the [`callback`](QueryingSubscriberBuilder::callback)
    /// method, we suggest you use it instead of `callback_mut`.
    ///
    /// Subscriber will not be undeclared when dropped, with the callback running
    /// in background until the session is closed.
    #[zenoh_macros::unstable]
    #[inline]
    #[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
    pub fn callback_mut<F>(
        self,
        callback: F,
    ) -> QueryingSubscriberBuilder<'a, 'b, KeySpace, Callback<Sample>>
    where
        F: FnMut(Sample) + Send + Sync + 'static,
    {
        self.callback(locked(callback))
    }

    /// Use the given handler to receive Samples.
    #[zenoh_macros::unstable]
    #[inline]
    #[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
    pub fn with<Handler>(
        self,
        handler: Handler,
    ) -> QueryingSubscriberBuilder<'a, 'b, KeySpace, Handler>
    where
        Handler: IntoHandler<Sample>,
    {
        let QueryingSubscriberBuilder {
            session,
            key_expr,
            key_space,
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

#[zenoh_macros::unstable]
#[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
#[allow(deprecated)]
impl<'a, 'b, KeySpace> QueryingSubscriberBuilder<'a, 'b, KeySpace, Callback<Sample>> {
    /// Make the subscriber to run in background until the session is closed.
    ///
    /// Background builder doesn't return a `FetchingSubscriber` object anymore.
    #[zenoh_macros::unstable]
    #[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
    pub fn background(self) -> QueryingSubscriberBuilder<'a, 'b, KeySpace, Callback<Sample>, true> {
        QueryingSubscriberBuilder {
            session: self.session,
            key_expr: self.key_expr,
            key_space: self.key_space,
            origin: self.origin,
            query_selector: self.query_selector,
            query_target: self.query_target,
            query_consolidation: self.query_consolidation,
            query_accept_replies: self.query_accept_replies,
            query_timeout: self.query_timeout,
            handler: self.handler,
        }
    }
}

#[zenoh_macros::unstable]
#[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
#[allow(deprecated)]
impl<'b, Handler, const BACKGROUND: bool>
    QueryingSubscriberBuilder<'_, 'b, UserSpace, Handler, BACKGROUND>
{
    ///
    ///
    /// Restrict the matching publications that will be receive by this [`Subscriber`]
    /// to the ones that have the given [`Locality`](Locality).
    #[zenoh_macros::unstable]
    #[inline]
    #[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
    pub fn allowed_origin(mut self, origin: Locality) -> Self {
        self.origin = origin;
        self
    }

    /// Change the selector to be used for queries.
    #[zenoh_macros::unstable]
    #[inline]
    #[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
    pub fn query_selector<IntoSelector>(mut self, query_selector: IntoSelector) -> Self
    where
        IntoSelector: TryInto<Selector<'b>>,
        <IntoSelector as TryInto<Selector<'b>>>::Error: Into<Error>,
    {
        self.query_selector = Some(query_selector.try_into().map_err(Into::into));
        self
    }

    /// Change the target to be used for queries.
    #[zenoh_macros::unstable]
    #[inline]
    #[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
    pub fn query_target(mut self, query_target: QueryTarget) -> Self {
        self.query_target = query_target;
        self
    }

    /// Change the consolidation mode to be used for queries.
    #[zenoh_macros::unstable]
    #[inline]
    #[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
    pub fn query_consolidation<QC: Into<QueryConsolidation>>(
        mut self,
        query_consolidation: QC,
    ) -> Self {
        self.query_consolidation = query_consolidation.into();
        self
    }

    /// Change the accepted replies for queries.
    #[zenoh_macros::unstable]
    #[inline]
    #[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
    pub fn query_accept_replies(mut self, accept_replies: ReplyKeyExpr) -> Self {
        self.query_accept_replies = accept_replies;
        self
    }
}

#[zenoh_macros::unstable]
#[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
#[allow(deprecated)]
impl<'a, 'b, KeySpace, Handler, const BACKGROUND: bool>
    QueryingSubscriberBuilder<'a, 'b, KeySpace, Handler, BACKGROUND>
{
    /// Change the timeout to be used for queries.
    #[zenoh_macros::unstable]
    #[inline]
    #[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
    pub fn query_timeout(mut self, query_timeout: Duration) -> Self {
        self.query_timeout = query_timeout;
        self
    }

    #[zenoh_macros::unstable]
    #[allow(clippy::type_complexity)]
    #[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
    fn into_fetching_subscriber_builder(
        self,
    ) -> ZResult<
        FetchingSubscriberBuilder<
            'a,
            'b,
            KeySpace,
            Handler,
            impl FnOnce(Box<dyn Fn(Reply) + Send + Sync>) -> ZResult<()>,
            Reply,
            BACKGROUND,
        >,
    >
    where
        KeySpace: Into<self::KeySpace> + Clone,
        Handler: IntoHandler<Sample>,
        Handler::Handler: Send,
    {
        let session = self.session.clone();
        let key_expr = self.key_expr?.into_owned();
        let key_space = self.key_space.clone().into();
        let query_selector = match self.query_selector {
            Some(s) => Some(s?.into_owned()),
            None => None,
        };
        let query_target = self.query_target;
        let query_consolidation = self.query_consolidation;
        let query_accept_replies = self.query_accept_replies;
        let query_timeout = self.query_timeout;
        Ok(FetchingSubscriberBuilder {
            session: self.session,
            key_expr: Ok(key_expr.clone()),
            key_space: self.key_space,
            origin: self.origin,
            fetch: move |cb| match key_space {
                self::KeySpace::User => match query_selector {
                    Some(s) => session.get(s),
                    None => session.get(key_expr),
                }
                .callback(cb)
                .target(query_target)
                .consolidation(query_consolidation)
                .accept_replies(query_accept_replies)
                .timeout(query_timeout)
                .wait(),
                self::KeySpace::Liveliness => session
                    .liveliness()
                    .get(key_expr)
                    .callback(cb)
                    .timeout(query_timeout)
                    .wait(),
            },
            handler: self.handler,
            phantom: std::marker::PhantomData,
        })
    }
}

#[zenoh_macros::unstable]
#[allow(deprecated)]
impl<KeySpace, Handler> Resolvable for QueryingSubscriberBuilder<'_, '_, KeySpace, Handler>
where
    Handler: IntoHandler<Sample>,
    Handler::Handler: Send,
{
    type To = ZResult<FetchingSubscriber<Handler::Handler>>;
}

#[zenoh_macros::unstable]
#[allow(deprecated)]
impl<KeySpace, Handler> Wait for QueryingSubscriberBuilder<'_, '_, KeySpace, Handler>
where
    KeySpace: Into<self::KeySpace> + Clone,
    Handler: IntoHandler<Sample> + Send,
    Handler::Handler: Send,
{
    #[zenoh_macros::unstable]
    fn wait(self) -> <Self as Resolvable>::To {
        self.into_fetching_subscriber_builder()?.wait()
    }
}

#[zenoh_macros::unstable]
#[allow(deprecated)]
impl<KeySpace, Handler> IntoFuture for QueryingSubscriberBuilder<'_, '_, KeySpace, Handler>
where
    KeySpace: Into<self::KeySpace> + Clone,
    Handler: IntoHandler<Sample> + Send,
    Handler::Handler: Send,
{
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    #[zenoh_macros::unstable]
    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

#[zenoh_macros::unstable]
#[allow(deprecated)]
impl<KeySpace> Resolvable for QueryingSubscriberBuilder<'_, '_, KeySpace, Callback<Sample>, true> {
    type To = ZResult<()>;
}

#[zenoh_macros::unstable]
#[allow(deprecated)]
impl<KeySpace> Wait for QueryingSubscriberBuilder<'_, '_, KeySpace, Callback<Sample>, true>
where
    KeySpace: Into<self::KeySpace> + Clone,
{
    #[zenoh_macros::unstable]
    fn wait(self) -> <Self as Resolvable>::To {
        self.into_fetching_subscriber_builder()?.wait()
    }
}

#[zenoh_macros::unstable]
#[allow(deprecated)]
impl<KeySpace> IntoFuture for QueryingSubscriberBuilder<'_, '_, KeySpace, Callback<Sample>, true>
where
    KeySpace: Into<self::KeySpace> + Clone,
{
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    #[zenoh_macros::unstable]
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
    timestamped: BTreeMap<Timestamp, Sample>,
}

impl MergeQueue {
    fn new() -> Self {
        MergeQueue {
            untimestamped: VecDeque::new(),
            timestamped: BTreeMap::new(),
        }
    }

    fn len(&self) -> usize {
        self.untimestamped.len() + self.timestamped.len()
    }

    fn push(&mut self, sample: Sample) {
        if let Some(ts) = sample.timestamp() {
            self.timestamped.entry(*ts).or_insert(sample);
        } else {
            self.untimestamped.push_back(sample);
        }
    }

    fn drain(&mut self) -> MergeQueueValues {
        let mut vec = VecDeque::new();
        let mut queue = BTreeMap::new();
        swap(&mut self.untimestamped, &mut vec);
        swap(&mut self.timestamped, &mut queue);
        MergeQueueValues {
            untimestamped: vec,
            timestamped: queue.into_values(),
        }
    }
}

struct MergeQueueValues {
    untimestamped: VecDeque<Sample>,
    timestamped: btree_map::IntoValues<Timestamp, Sample>,
}

impl Iterator for MergeQueueValues {
    type Item = Sample;
    fn next(&mut self) -> Option<Self::Item> {
        self.untimestamped
            .pop_front()
            .or_else(|| self.timestamped.next())
    }
}

struct InnerState {
    pending_fetches: u64,
    merge_queue: MergeQueue,
}

/// The builder of [`FetchingSubscriber`], allowing to configure it.
#[zenoh_macros::unstable]
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
#[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
#[allow(deprecated)]
pub struct FetchingSubscriberBuilder<
    'a,
    'b,
    KeySpace,
    Handler,
    Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()>,
    TryIntoSample,
    const BACKGROUND: bool = false,
> where
    TryIntoSample: ExtractSample,
{
    pub(crate) session: &'a Session,
    pub(crate) key_expr: ZResult<KeyExpr<'b>>,
    pub(crate) key_space: KeySpace,
    pub(crate) origin: Locality,
    pub(crate) fetch: Fetch,
    pub(crate) handler: Handler,
    pub(crate) phantom: std::marker::PhantomData<TryIntoSample>,
}

#[zenoh_macros::unstable]
#[allow(deprecated)]
impl<
        'a,
        KeySpace,
        Handler,
        Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()>,
        TryIntoSample,
        const BACKGROUND: bool,
    > FetchingSubscriberBuilder<'a, '_, KeySpace, Handler, Fetch, TryIntoSample, BACKGROUND>
where
    TryIntoSample: ExtractSample,
{
    #[zenoh_macros::unstable]
    fn with_static_keys(
        self,
    ) -> FetchingSubscriberBuilder<'a, 'static, KeySpace, Handler, Fetch, TryIntoSample> {
        FetchingSubscriberBuilder {
            session: self.session,
            key_expr: self.key_expr.map(|s| s.into_owned()),
            key_space: self.key_space,
            origin: self.origin,
            fetch: self.fetch,
            handler: self.handler,
            phantom: std::marker::PhantomData,
        }
    }
}

#[zenoh_macros::unstable]
#[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
#[allow(deprecated)]
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
    #[zenoh_macros::unstable]
    #[inline]
    #[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
    pub fn callback<F>(
        self,
        callback: F,
    ) -> FetchingSubscriberBuilder<'a, 'b, KeySpace, Callback<Sample>, Fetch, TryIntoSample>
    where
        F: Fn(Sample) + Send + Sync + 'static,
    {
        self.with(Callback::from(callback))
    }

    /// Add callback to [`FetchingSubscriber`].
    ///
    /// Using this guarantees that your callback will never be called concurrently.
    /// If your callback is also accepted by the [`callback`](FetchingSubscriberBuilder::callback)
    /// method, we suggest you use it instead of `callback_mut`.
    ///
    /// Subscriber will not be undeclared when dropped, with the callback running
    /// in background until the session is closed.
    #[zenoh_macros::unstable]
    #[inline]
    #[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
    pub fn callback_mut<F>(
        self,
        callback: F,
    ) -> FetchingSubscriberBuilder<'a, 'b, KeySpace, Callback<Sample>, Fetch, TryIntoSample>
    where
        F: FnMut(Sample) + Send + Sync + 'static,
    {
        self.callback(locked(callback))
    }

    /// Use the given handler to receive Samples.
    #[zenoh_macros::unstable]
    #[inline]
    #[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
    pub fn with<Handler>(
        self,
        handler: Handler,
    ) -> FetchingSubscriberBuilder<'a, 'b, KeySpace, Handler, Fetch, TryIntoSample>
    where
        Handler: IntoHandler<Sample>,
    {
        let FetchingSubscriberBuilder {
            session,
            key_expr,
            key_space,
            origin,
            fetch,
            handler: _,
            phantom,
        } = self;
        FetchingSubscriberBuilder {
            session,
            key_expr,
            key_space,
            origin,
            fetch,
            handler,
            phantom,
        }
    }
}

#[zenoh_macros::unstable]
#[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
#[allow(deprecated)]
impl<
        'a,
        'b,
        KeySpace,
        Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()>,
        TryIntoSample,
    > FetchingSubscriberBuilder<'a, 'b, KeySpace, Callback<Sample>, Fetch, TryIntoSample>
where
    TryIntoSample: ExtractSample,
{
    /// Make the subscriber to run in background until the session is closed.
    ///
    /// Background builder doesn't return a `FetchingSubscriber` object anymore.
    #[zenoh_macros::unstable]
    #[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
    pub fn background(
        self,
    ) -> FetchingSubscriberBuilder<'a, 'b, KeySpace, Callback<Sample>, Fetch, TryIntoSample, true>
    {
        FetchingSubscriberBuilder {
            session: self.session,
            key_expr: self.key_expr,
            key_space: self.key_space,
            origin: self.origin,
            fetch: self.fetch,
            handler: self.handler,
            phantom: self.phantom,
        }
    }
}

#[zenoh_macros::unstable]
#[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
#[allow(deprecated)]
impl<
        Handler,
        Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()>,
        TryIntoSample,
        const BACKGROUND: bool,
    > FetchingSubscriberBuilder<'_, '_, UserSpace, Handler, Fetch, TryIntoSample, BACKGROUND>
where
    TryIntoSample: ExtractSample,
{
    /// Restrict the matching publications that will be received by this [`FetchingSubscriber`]
    /// to the ones that have the given [`Locality`](Locality).
    #[zenoh_macros::unstable]
    #[inline]
    #[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
    pub fn allowed_origin(mut self, origin: Locality) -> Self {
        self.origin = origin;
        self
    }
}

#[zenoh_macros::unstable]
#[allow(deprecated)]
impl<
        KeySpace,
        Handler,
        Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()>,
        TryIntoSample,
    > Resolvable for FetchingSubscriberBuilder<'_, '_, KeySpace, Handler, Fetch, TryIntoSample>
where
    Handler: IntoHandler<Sample>,
    Handler::Handler: Send,
    TryIntoSample: ExtractSample,
{
    type To = ZResult<FetchingSubscriber<Handler::Handler>>;
}

#[zenoh_macros::unstable]
#[allow(deprecated)]
impl<
        KeySpace,
        Handler,
        Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()> + Send + Sync,
        TryIntoSample,
    > Wait for FetchingSubscriberBuilder<'_, '_, KeySpace, Handler, Fetch, TryIntoSample>
where
    KeySpace: Into<self::KeySpace>,
    Handler: IntoHandler<Sample> + Send,
    Handler::Handler: Send,
    TryIntoSample: ExtractSample + Send + Sync,
{
    #[zenoh_macros::unstable]
    fn wait(self) -> <Self as Resolvable>::To {
        FetchingSubscriber::new(self.with_static_keys())
    }
}

#[zenoh_macros::unstable]
#[allow(deprecated)]
impl<
        KeySpace,
        Handler,
        Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()> + Send + Sync,
        TryIntoSample,
    > IntoFuture for FetchingSubscriberBuilder<'_, '_, KeySpace, Handler, Fetch, TryIntoSample>
where
    KeySpace: Into<self::KeySpace>,
    Handler: IntoHandler<Sample> + Send,
    Handler::Handler: Send,
    TryIntoSample: ExtractSample + Send + Sync,
{
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    #[zenoh_macros::unstable]
    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

#[zenoh_macros::unstable]
#[allow(deprecated)]
impl<
        KeySpace,
        Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()>,
        TryIntoSample,
    > Resolvable
    for FetchingSubscriberBuilder<'_, '_, KeySpace, Callback<Sample>, Fetch, TryIntoSample, true>
where
    TryIntoSample: ExtractSample,
{
    type To = ZResult<()>;
}

#[zenoh_macros::unstable]
#[allow(deprecated)]
impl<
        KeySpace,
        Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()> + Send + Sync,
        TryIntoSample,
    > Wait
    for FetchingSubscriberBuilder<'_, '_, KeySpace, Callback<Sample>, Fetch, TryIntoSample, true>
where
    KeySpace: Into<self::KeySpace>,
    TryIntoSample: ExtractSample + Send + Sync,
{
    #[zenoh_macros::unstable]
    fn wait(self) -> <Self as Resolvable>::To {
        FetchingSubscriber::new(self.with_static_keys())?
            .subscriber
            .set_background(true);
        Ok(())
    }
}

#[zenoh_macros::unstable]
#[allow(deprecated)]
impl<
        KeySpace,
        Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()> + Send + Sync,
        TryIntoSample,
    > IntoFuture
    for FetchingSubscriberBuilder<'_, '_, KeySpace, Callback<Sample>, Fetch, TryIntoSample, true>
where
    KeySpace: Into<self::KeySpace>,
    TryIntoSample: ExtractSample + Send + Sync,
{
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    #[zenoh_macros::unstable]
    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

/// A Subscriber that will run the given user defined `fetch` function at startup.
///
/// The user defined `fetch` function should fetch some samples and return them through the callback function
/// (it could typically be a Session::get()). Those samples will be merged with the received publications and made available in the receiver.
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
#[zenoh_macros::unstable]
#[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
pub struct FetchingSubscriber<Handler> {
    subscriber: Subscriber<()>,
    callback: Callback<Sample>,
    state: Arc<Mutex<InnerState>>,
    handler: Handler,
}

#[zenoh_macros::unstable]
#[allow(deprecated)]
impl<Handler> std::ops::Deref for FetchingSubscriber<Handler> {
    type Target = Handler;
    #[zenoh_macros::unstable]
    fn deref(&self) -> &Self::Target {
        &self.handler
    }
}

#[zenoh_macros::unstable]
#[allow(deprecated)]
impl<Handler> std::ops::DerefMut for FetchingSubscriber<Handler> {
    #[zenoh_macros::unstable]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.handler
    }
}

#[zenoh_macros::unstable]
#[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
#[allow(deprecated)]
impl<Handler> FetchingSubscriber<Handler> {
    fn new<
        'a,
        KeySpace,
        InputHandler,
        Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()> + Send + Sync,
        TryIntoSample,
    >(
        conf: FetchingSubscriberBuilder<'a, 'a, KeySpace, InputHandler, Fetch, TryIntoSample>,
    ) -> ZResult<Self>
    where
        KeySpace: Into<self::KeySpace>,
        InputHandler: IntoHandler<Sample, Handler = Handler> + Send,
        TryIntoSample: ExtractSample + Send + Sync,
    {
        let session_id = conf.session.zid();

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
                    callback.call(s);
                } else {
                    tracing::trace!(
                        "Sample received while fetch in progress: push it to merge_queue"
                    );

                    // ensure the sample has a timestamp, thus it will always be sorted into the MergeQueue
                    // after any timestamped Sample possibly coming from a fetch reply.
                    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().into(); // UNIX_EPOCH is Returns a Timespec::zero(), Unwrap Should be permissible here
                    let timestamp = s
                        .timestamp()
                        .cloned()
                        .unwrap_or(Timestamp::new(now, session_id.into()));
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
            self::KeySpace::User => conf
                .session
                .declare_subscriber(&key_expr)
                .callback(sub_callback)
                .allowed_origin(conf.origin)
                .wait()?,
            self::KeySpace::Liveliness => conf
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

    /// Undeclare this [`FetchingSubscriber`]`.
    #[zenoh_macros::unstable]
    #[inline]
    #[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
    pub fn undeclare(self) -> impl Resolve<ZResult<()>> {
        self.subscriber.undeclare()
    }

    #[zenoh_macros::unstable]
    #[zenoh_macros::internal]
    #[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
    pub fn set_background(&mut self, background: bool) {
        self.subscriber.set_background(background)
    }

    /// Return the key expression of this FetchingSubscriber
    #[zenoh_macros::unstable]
    #[inline]
    #[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
    pub fn key_expr(&self) -> &KeyExpr<'static> {
        self.subscriber.key_expr()
    }

    /// Perform an additional `fetch`.
    ///
    /// The provided `fetch` function should fetch some samples and return them through the callback function
    /// (it could typically be a Session::get()). Those samples will be merged with the received publications and made available in the receiver.
    ///
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::Wait;
    /// use zenoh_ext::*;
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
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
    #[zenoh_macros::unstable]
    #[inline]
    #[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
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
    callback: Callback<Sample>,
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
                self.callback.call(s);
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
/// # use zenoh::Wait;
/// # use zenoh_ext::*;
/// #
/// # let session = zenoh::open(zenoh::Config::default()).await.unwrap();
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
#[zenoh_macros::unstable]
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
#[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
#[allow(deprecated)]
pub struct FetchBuilder<
    Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()>,
    TryIntoSample,
> where
    TryIntoSample: ExtractSample,
{
    fetch: Fetch,
    phantom: std::marker::PhantomData<TryIntoSample>,
    state: Arc<Mutex<InnerState>>,
    callback: Callback<Sample>,
}

#[zenoh_macros::unstable]
#[allow(deprecated)]
impl<Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()>, TryIntoSample>
    Resolvable for FetchBuilder<Fetch, TryIntoSample>
where
    TryIntoSample: ExtractSample,
{
    type To = ZResult<()>;
}

#[zenoh_macros::unstable]
#[allow(deprecated)]
impl<Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()>, TryIntoSample> Wait
    for FetchBuilder<Fetch, TryIntoSample>
where
    TryIntoSample: ExtractSample,
{
    #[zenoh_macros::unstable]
    fn wait(self) -> <Self as Resolvable>::To {
        let handler = register_handler(self.state, self.callback);
        run_fetch(self.fetch, handler)
    }
}

#[zenoh_macros::unstable]
#[allow(deprecated)]
impl<Fetch: FnOnce(Box<dyn Fn(TryIntoSample) + Send + Sync>) -> ZResult<()>, TryIntoSample>
    IntoFuture for FetchBuilder<Fetch, TryIntoSample>
where
    TryIntoSample: ExtractSample,
{
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    #[zenoh_macros::unstable]
    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

fn register_handler(state: Arc<Mutex<InnerState>>, callback: Callback<Sample>) -> RepliesHandler {
    zlock!(state).pending_fetches += 1;
    // pending fetches will be decremented in RepliesHandler drop()
    RepliesHandler { state, callback }
}

#[zenoh_macros::unstable]
#[allow(deprecated)]
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

/// [`ExtractSample`].
#[zenoh_macros::unstable]
#[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
#[allow(deprecated)]
pub trait ExtractSample {
    #[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
    fn extract(self) -> ZResult<Sample>;
}

#[allow(deprecated)]
impl ExtractSample for Reply {
    fn extract(self) -> ZResult<Sample> {
        self.into_result().map_err(|e| zerror!("{:?}", e).into())
    }
}
