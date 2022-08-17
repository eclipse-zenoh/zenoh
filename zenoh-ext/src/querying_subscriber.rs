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
use std::collections::{btree_map, BTreeMap, VecDeque};
use std::convert::TryInto;
use std::mem::swap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use zenoh::handlers::{locked, DefaultHandler};
use zenoh::prelude::*;

use zenoh::query::{QueryConsolidation, QueryTarget};
use zenoh::subscriber::{Reliability, Subscriber};
use zenoh::time::Timestamp;
use zenoh::Result as ZResult;
use zenoh_core::{zlock, AsyncResolve, Resolvable, Resolve, SyncResolve};

use crate::session_ext::SessionRef;

/// The builder of QueryingSubscriber, allowing to configure it.
pub struct QueryingSubscriberBuilder<'a, 'b, Handler> {
    session: SessionRef<'a>,
    key_expr: ZResult<KeyExpr<'b>>,
    reliability: Reliability,
    query_selector: Option<ZResult<Selector<'b>>>,
    query_target: QueryTarget,
    query_consolidation: QueryConsolidation,
    query_timeout: Duration,
    handler: Handler,
}

impl<'a, 'b> QueryingSubscriberBuilder<'a, 'b, DefaultHandler> {
    pub(crate) fn new(
        session: SessionRef<'a>,
        key_expr: ZResult<KeyExpr<'b>>,
    ) -> QueryingSubscriberBuilder<'a, 'b, DefaultHandler> {
        // By default query all matching publication caches and storages
        let query_target = QueryTarget::All;

        // By default no query consolidation, to receive more than 1 sample per-resource
        // (in history of publications is available)
        let query_consolidation =
            QueryConsolidation::from_mode(zenoh::query::ConsolidationMode::None);

        QueryingSubscriberBuilder {
            session,
            key_expr,
            reliability: Reliability::default(),
            query_selector: None,
            query_target,
            query_consolidation,
            query_timeout: Duration::from_secs(10),
            handler: DefaultHandler,
        }
    }

    /// Make the built QueryingSubscriber a [`CallbackQueryingSubscriber`](CallbackQueryingSubscriber).
    #[inline]
    pub fn callback<Callback>(
        self,
        callback: Callback,
    ) -> QueryingSubscriberBuilder<'a, 'b, Callback>
    where
        Callback: Fn(Sample) + Send + Sync + 'static,
    {
        let QueryingSubscriberBuilder {
            session,
            key_expr,
            reliability,
            query_selector,
            query_target,
            query_consolidation,
            query_timeout,
            handler: _,
        } = self;
        QueryingSubscriberBuilder {
            session,
            key_expr,
            reliability,
            query_selector,
            query_target,
            query_consolidation,
            query_timeout,
            handler: callback,
        }
    }

    /// Make the built QueryingSubscriber a [`CallbackQueryingSubscriber`](CallbackQueryingSubscriber).
    ///
    /// Using this guarantees that your callback will never be called concurrently.
    /// If your callback is also accepted by the [`callback`](QueryingSubscriberBuilder::callback) method, we suggest you use it instead of `callback_mut`
    #[inline]
    pub fn callback_mut<CallbackMut>(
        self,
        callback: CallbackMut,
    ) -> QueryingSubscriberBuilder<'a, 'b, impl Fn(Sample) + Send + Sync + 'static>
    where
        CallbackMut: FnMut(Sample) + Send + Sync + 'static,
    {
        self.callback(locked(callback))
    }

    /// Make the built QueryingSubscriber a [`QueryingSubscriber`](QueryingSubscriber).
    #[inline]
    pub fn with<Handler>(self, handler: Handler) -> QueryingSubscriberBuilder<'a, 'b, Handler>
    where
        Handler: zenoh::prelude::IntoCallbackReceiverPair<'static, Sample>,
    {
        let QueryingSubscriberBuilder {
            session,
            key_expr,
            reliability,
            query_selector,
            query_target,
            query_consolidation,
            query_timeout,
            handler: _,
        } = self;
        QueryingSubscriberBuilder {
            session,
            key_expr,
            reliability,
            query_selector,
            query_target,
            query_consolidation,
            query_timeout,
            handler,
        }
    }
}
impl<'a, 'b, Handler> QueryingSubscriberBuilder<'a, 'b, Handler> {
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

    /// Change the selector to be used for queries.
    #[inline]
    pub fn query_selector<IntoSelector>(mut self, query_selector: IntoSelector) -> Self
    where
        IntoSelector: TryInto<Selector<'b>>,
        <IntoSelector as TryInto<Selector<'b>>>::Error: Into<zenoh_core::Error>,
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

    /// Change the timeout to be used for queries.
    #[inline]
    pub fn query_timeout(mut self, query_timeout: Duration) -> Self {
        self.query_timeout = query_timeout;
        self
    }

    fn with_static_keys(self) -> QueryingSubscriberBuilder<'a, 'static, Handler> {
        QueryingSubscriberBuilder {
            session: self.session,
            key_expr: self.key_expr.map(|s| s.into_owned()),
            reliability: self.reliability,
            query_selector: self.query_selector.map(|s| s.map(|s| s.into_owned())),
            query_target: self.query_target,
            query_consolidation: self.query_consolidation,
            query_timeout: self.query_timeout,
            handler: self.handler,
        }
    }
}

impl<'a, 'b, Handler: IntoCallbackReceiverPair<'static, Sample>> Resolvable
    for QueryingSubscriberBuilder<'a, 'b, Handler>
{
    type Output = ZResult<QueryingSubscriber<'a, Handler::Receiver>>;
}

impl<Handler: IntoCallbackReceiverPair<'static, Sample>> AsyncResolve
    for QueryingSubscriberBuilder<'_, '_, Handler>
where
    Handler::Receiver: Send,
{
    type Future = futures::future::Ready<Self::Output>;
    fn res_async(self) -> Self::Future {
        futures::future::ready(self.res_sync())
    }
}

impl<Handler: IntoCallbackReceiverPair<'static, Sample>> SyncResolve
    for QueryingSubscriberBuilder<'_, '_, Handler>
where
    Handler::Receiver: Send,
{
    fn res_sync(self) -> Self::Output {
        QueryingSubscriber::new(self.with_static_keys())
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
    pending_queries: u64,
    merge_queue: MergeQueue,
}

pub struct QueryingSubscriber<'a, Receiver> {
    session: SessionRef<'a>,
    query_key_expr: KeyExpr<'a>,
    query_value_selector: String,
    query_target: QueryTarget,
    query_consolidation: QueryConsolidation,
    query_timeout: Duration,
    _subscriber: Subscriber<'a, ()>,
    callback: Arc<dyn Fn(Sample) + Send + Sync + 'static>,
    state: Arc<Mutex<InnerState>>,
    receiver: Receiver,
}
impl<Receiver> std::ops::Deref for QueryingSubscriber<'_, Receiver> {
    type Target = Receiver;
    fn deref(&self) -> &Self::Target {
        &self.receiver
    }
}
impl<Receiver> std::ops::DerefMut for QueryingSubscriber<'_, Receiver> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.receiver
    }
}

impl<'a, Receiver> QueryingSubscriber<'a, Receiver> {
    fn new<Handler>(conf: QueryingSubscriberBuilder<'a, 'a, Handler>) -> ZResult<Self>
    where
        Handler: IntoCallbackReceiverPair<'static, Sample, Receiver = Receiver>,
    {
        let state = Arc::new(Mutex::new(InnerState {
            pending_queries: 0,
            merge_queue: MergeQueue::new(),
        }));
        let (callback, receiver) = conf.handler.into_cb_receiver_pair();

        let sub_callback = {
            let state = state.clone();
            let callback = callback.clone();
            move |mut s| {
                let state = &mut zlock!(state);
                if state.pending_queries == 0 {
                    callback(s);
                } else {
                    log::trace!("Sample received while query in progress: push it to merge_queue");
                    // ensure the sample has a timestamp, thus it will always be sorted into the MergeQueue
                    // after any timestamped Sample possibly coming from a query reply.
                    s.ensure_timestamp();
                    state.merge_queue.push(s);
                }
            }
        };

        let key_expr = conf.key_expr?;
        let (key_selector, value_selector) = match conf.query_selector {
            Some(Ok(s)) => s,
            Some(Err(e)) => return Err(e),
            None => key_expr.clone().with_value_selector(""),
        }
        .split();

        // declare subscriber at first
        let subscriber = match conf.session.clone() {
            SessionRef::Borrow(session) => session
                .declare_subscriber(&key_expr)
                .callback(sub_callback)
                .reliability(conf.reliability)
                .res_sync()?,
            SessionRef::Shared(session) => session
                .declare_subscriber(&key_expr)
                .callback(sub_callback)
                .reliability(conf.reliability)
                .res_sync()?,
        };

        let mut query_subscriber = QueryingSubscriber {
            session: conf.session,
            query_key_expr: key_selector,
            query_value_selector: value_selector.into_owned(),
            query_target: conf.query_target,
            query_consolidation: conf.query_consolidation,
            query_timeout: conf.query_timeout,
            _subscriber: subscriber,
            callback,
            state,
            receiver,
        };

        // start query
        query_subscriber.query().res_sync()?;

        Ok(query_subscriber)
    }

    /// Close this QueryingSubscriber
    #[inline]
    pub fn close(self) -> impl Resolve<ZResult<()>> + 'a {
        self._subscriber.undeclare()
    }

    /// Issue a new query using the configured selector.
    #[inline]
    pub fn query(&mut self) -> impl Resolve<ZResult<()>> + '_ {
        self.query_on_selector(
            self.query_key_expr
                .clone()
                .with_owned_value_selector(self.query_value_selector.to_owned()),
            self.query_target,
            self.query_consolidation,
            self.query_timeout,
        )
    }

    /// Issue a new query on the specified selector.
    #[inline]
    pub fn query_on<'c, IntoSelector>(
        &'c mut self,
        selector: IntoSelector,
        target: QueryTarget,
        consolidation: QueryConsolidation,
        timeout: Duration,
    ) -> impl Resolve<ZResult<()>> + 'c
    where
        IntoSelector: Into<Selector<'c>>,
    {
        self.query_on_selector(selector.into(), target, consolidation, timeout)
    }

    #[inline]
    fn query_on_selector<'c>(
        &'c mut self,
        selector: Selector<'c>,
        target: QueryTarget,
        consolidation: QueryConsolidation,
        timeout: Duration,
    ) -> impl Resolve<ZResult<()>> + 'c {
        zlock!(self.state).pending_queries += 1;
        // pending queries will be decremented in RepliesHandler drop()
        let handler = RepliesHandler {
            state: self.state.clone(),
            callback: self.callback.clone(),
        };

        log::debug!("Start query on {}", selector);
        self.session
            .get(selector)
            .target(target)
            .consolidation(consolidation)
            .timeout(timeout)
            .callback(move |r| {
                let mut state = zlock!(handler.state);
                match r.sample {
                    Ok(s) => {
                        log::trace!("Reply received: push it to merge_queue");
                        state.merge_queue.push(s)
                    }
                    Err(v) => log::debug!("Received error {}", v),
                }
            })
    }
}

struct RepliesHandler {
    state: Arc<Mutex<InnerState>>,
    callback: Arc<dyn Fn(Sample) + Send + Sync>,
}

impl Drop for RepliesHandler {
    fn drop(&mut self) {
        let mut state = zlock!(self.state);
        state.pending_queries -= 1;
        log::trace!(
            "Query done - {} queries still in progress",
            state.pending_queries
        );
        if state.pending_queries == 0 {
            log::debug!(
                "All queries done. Replies and live publications merged - {} samples to propagate",
                state.merge_queue.len()
            );
            for s in state.merge_queue.drain() {
                (self.callback)(s);
            }
        }
    }
}
