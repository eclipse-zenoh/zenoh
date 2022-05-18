use std::ops::Deref;
use std::pin::Pin;
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
//;
use futures::Future;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use zenoh::prelude::*;
use zenoh::query::{QueryConsolidation, QueryTarget};
use zenoh::subscriber::{CallbackSubscriber, Reliability, SubMode};
use zenoh::time::Period;
use zenoh::Result as ZResult;
use zenoh_core::{zlock, SyncResolve};
use zenoh_sync::zready;

use crate::session_ext::SessionRef;

const MERGE_QUEUE_INITIAL_CAPCITY: usize = 32;

/// The builder of QueryingSubscriber, allowing to configure it.
#[derive(Clone)]
pub struct QueryingSubscriberBuilder<'a, 'b> {
    session: SessionRef<'a>,
    sub_key_expr: KeyExpr<'b>,
    reliability: Reliability,
    mode: SubMode,
    period: Option<Period>,
    query_key_expr: KeyExpr<'b>,
    query_value_selector: String,
    query_target: QueryTarget,
    query_consolidation: QueryConsolidation,
}

impl<'a, 'b> QueryingSubscriberBuilder<'a, 'b> {
    pub(crate) fn new(
        session: SessionRef<'a>,
        sub_key_expr: KeyExpr<'b>,
    ) -> QueryingSubscriberBuilder<'a, 'b> {
        // By default query all matching publication caches and storages
        let query_target = QueryTarget::All;

        // By default no query consolidation, to receive more than 1 sample per-resource
        // (in history of publications is available)
        let query_consolidation = QueryConsolidation::none();

        QueryingSubscriberBuilder {
            session,
            sub_key_expr: sub_key_expr.clone(),
            reliability: Reliability::default(),
            mode: SubMode::default(),
            period: None,
            query_key_expr: sub_key_expr,
            query_value_selector: "".into(),
            query_target,
            query_consolidation,
        }
    }

    /// Make the built QueryingSubscriber a [`CallbackQueryingSubscriber`](CallbackQueryingSubscriber).
    #[inline]
    pub fn callback<Callback>(
        self,
        callback: Callback,
    ) -> CallbackQueryingSubscriberBuilder<'a, 'b, Callback>
    where
        Callback: Fn(Sample) + Send + Sync + 'static,
    {
        CallbackQueryingSubscriberBuilder {
            builder: self,
            callback: Some(callback),
        }
    }

    /// Make the built QueryingSubscriber a [`CallbackQueryingSubscriber`](CallbackQueryingSubscriber).
    #[inline]
    pub fn callback_mut<CallbackMut>(
        self,
        callback: CallbackMut,
    ) -> CallbackQueryingSubscriberBuilder<'a, 'b, impl Fn(Sample) + Send + Sync + 'static>
    where
        CallbackMut: FnMut(Sample) + Send + Sync + 'static,
    {
        self.callback(locked(callback))
    }

    /// Make the built QueryingSubscriber a [`HandlerQueryingSubscriber`](HandlerQueryingSubscriber).
    #[inline]
    pub fn with<IntoHandler, Receiver>(
        self,
        handler: IntoHandler,
    ) -> HandlerQueryingSubscriberBuilder<'a, 'b, Receiver>
    where
        IntoHandler: zenoh::prelude::IntoHandler<Sample, Receiver>,
    {
        HandlerQueryingSubscriberBuilder {
            builder: self,
            handler: Some(handler.into_handler()),
        }
    }

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

    /// Change the subscription mode.
    #[inline]
    pub fn mode(mut self, mode: SubMode) -> Self {
        self.mode = mode;
        self
    }

    /// Change the subscription mode to Push.
    #[inline]
    pub fn push_mode(mut self) -> Self {
        self.mode = SubMode::Push;
        self.period = None;
        self
    }

    /// Change the subscription mode to Pull.
    #[inline]
    pub fn pull_mode(mut self) -> Self {
        self.mode = SubMode::Pull;
        self
    }

    /// Change the subscription period.
    #[inline]
    pub fn period(mut self, period: Option<Period>) -> Self {
        self.period = period;
        self
    }

    /// Change the selector to be used for queries.
    #[inline]
    pub fn query_selector<IntoSelector>(mut self, query_selector: IntoSelector) -> Self
    where
        IntoSelector: Into<Selector<'b>>,
    {
        let selector = query_selector.into();
        self.query_key_expr = selector.key_selector.to_owned();
        self.query_value_selector = selector.value_selector.to_string();
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
    pub fn query_consolidation(mut self, query_consolidation: QueryConsolidation) -> Self {
        self.query_consolidation = query_consolidation;
        self
    }

    fn with_static_keys(self) -> QueryingSubscriberBuilder<'a, 'static> {
        QueryingSubscriberBuilder {
            session: self.session,
            sub_key_expr: self.sub_key_expr.to_owned(),
            reliability: self.reliability,
            mode: self.mode,
            period: self.period,
            query_key_expr: self.query_key_expr.to_owned(),
            query_value_selector: "".to_string(),
            query_target: self.query_target,
            query_consolidation: self.query_consolidation,
        }
    }
}

impl<'a, 'b> Future for QueryingSubscriberBuilder<'a, 'b> {
    type Output = ZResult<HandlerQueryingSubscriber<'a, flume::Receiver<Sample>>>;

    #[inline]
    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let (callback, receiver) = flume::bounded(256).into_handler();
        Poll::Ready(
            CallbackQueryingSubscriber::new(self.clone().with_static_keys(), callback).map(
                |subscriber| HandlerQueryingSubscriber {
                    subscriber,
                    receiver,
                },
            ),
        )
    }
}

impl<'a, 'b> ZFuture for QueryingSubscriberBuilder<'a, 'b> {
    #[inline]
    fn wait(self) -> ZResult<HandlerQueryingSubscriber<'a, flume::Receiver<Sample>>> {
        let (callback, receiver) = flume::bounded(256).into_handler();
        CallbackQueryingSubscriber::new(self.with_static_keys(), callback).map(|subscriber| {
            HandlerQueryingSubscriber {
                subscriber,
                receiver,
            }
        })
    }
}

/// The builder of QueryingSubscriber, allowing to configure it.
#[derive(Clone)]
#[must_use = "ZFutures do nothing unless you `.wait()`, `.await` or poll them"]
pub struct CallbackQueryingSubscriberBuilder<'a, 'b, Callback> {
    builder: QueryingSubscriberBuilder<'a, 'b>,
    callback: Option<Callback>,
}

impl<'a, 'b, Callback> Future for CallbackQueryingSubscriberBuilder<'a, 'b, Callback>
where
    Callback: Fn(Sample) + Unpin + Send + Sync + 'static,
{
    type Output = ZResult<CallbackQueryingSubscriber<'a>>;

    #[inline]
    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Ready(CallbackQueryingSubscriber::new(
            self.builder.clone().with_static_keys(),
            Box::new(self.callback.take().unwrap()),
        ))
    }
}

impl<'a, 'b, Callback> ZFuture for CallbackQueryingSubscriberBuilder<'a, 'b, Callback>
where
    Callback: Fn(Sample) + Unpin + Send + Sync + 'static,
{
    #[inline]
    fn wait(mut self) -> ZResult<CallbackQueryingSubscriber<'a>> {
        CallbackQueryingSubscriber::new(
            self.builder.with_static_keys(),
            Box::new(self.callback.take().unwrap()),
        )
    }
}

impl<'a, 'b, Callback> CallbackQueryingSubscriberBuilder<'a, 'b, Callback> {
    /// Change the subscription reliability.
    #[inline]
    pub fn reliability(mut self, reliability: Reliability) -> Self {
        self.builder.reliability = reliability;
        self
    }

    /// Change the subscription reliability to Reliable.
    #[inline]
    pub fn reliable(mut self) -> Self {
        self.builder.reliability = Reliability::Reliable;
        self
    }

    /// Change the subscription reliability to BestEffort.
    #[inline]
    pub fn best_effort(mut self) -> Self {
        self.builder.reliability = Reliability::BestEffort;
        self
    }

    /// Change the subscription mode.
    #[inline]
    pub fn mode(mut self, mode: SubMode) -> Self {
        self.builder.mode = mode;
        self
    }

    /// Change the subscription mode to Push.
    #[inline]
    pub fn push_mode(mut self) -> Self {
        self.builder.mode = SubMode::Push;
        self.builder.period = None;
        self
    }

    /// Change the subscription mode to Pull.
    #[inline]
    pub fn pull_mode(mut self) -> Self {
        self.builder.mode = SubMode::Pull;
        self
    }

    /// Change the subscription period.
    #[inline]
    pub fn period(mut self, period: Option<Period>) -> Self {
        self.builder.period = period;
        self
    }

    /// Change the selector to be used for queries.
    #[inline]
    pub fn query_selector<IntoSelector>(mut self, query_selector: IntoSelector) -> Self
    where
        IntoSelector: Into<Selector<'b>>,
    {
        let selector = query_selector.into();
        self.builder.query_key_expr = selector.key_selector.to_owned();
        self.builder.query_value_selector = selector.value_selector.to_string();
        self
    }

    /// Change the target to be used for queries.
    #[inline]
    pub fn query_target(mut self, query_target: QueryTarget) -> Self {
        self.builder.query_target = query_target;
        self
    }

    /// Change the consolidation mode to be used for queries.
    #[inline]
    pub fn query_consolidation(mut self, query_consolidation: QueryConsolidation) -> Self {
        self.builder.query_consolidation = query_consolidation;
        self
    }
}

struct InnerState {
    pending_queries: u64,
    merge_queue: Vec<Sample>,
}

pub struct CallbackQueryingSubscriber<'a> {
    session: SessionRef<'a>,
    query_key_expr: KeyExpr<'a>,
    query_value_selector: String,
    query_target: QueryTarget,
    query_consolidation: QueryConsolidation,
    subscriber: CallbackSubscriber<'a>,
    callback: Arc<dyn Fn(Sample) + Send + Sync>,
    state: Arc<Mutex<InnerState>>,
}

impl<'a> CallbackQueryingSubscriber<'a> {
    fn new(
        conf: QueryingSubscriberBuilder<'a, 'a>,
        callback: Callback<Sample>,
    ) -> ZResult<CallbackQueryingSubscriber<'a>> {
        let state = Arc::new(Mutex::new(InnerState {
            pending_queries: 0,
            merge_queue: Vec::with_capacity(MERGE_QUEUE_INITIAL_CAPCITY),
        }));
        let callback: Arc<dyn Fn(Sample) + Send + Sync> = callback.into();

        let sub_callback = {
            let state = state.clone();
            let callback = callback.clone();
            move |s| {
                let state = &mut zlock!(state);
                if state.pending_queries == 0 {
                    callback(s);
                } else {
                    state.merge_queue.push(s);
                }
            }
        };

        // declare subscriber at first
        let subscriber = match conf.session.clone() {
            SessionRef::Borrow(session) => session
                .subscribe(&conf.sub_key_expr)
                .callback(sub_callback)
                .reliability(conf.reliability)
                .mode(conf.mode)
                .period(conf.period)
                .res_sync()?,
            SessionRef::Shared(session) => session
                .subscribe(&conf.sub_key_expr)
                .callback(sub_callback)
                .reliability(conf.reliability)
                .mode(conf.mode)
                .period(conf.period)
                .res_sync()?,
        };

        let mut query_subscriber = CallbackQueryingSubscriber {
            session: conf.session,
            query_key_expr: conf.query_key_expr,
            query_value_selector: conf.query_value_selector,
            query_target: conf.query_target,
            query_consolidation: conf.query_consolidation,
            subscriber,
            callback,
            state,
        };

        // start query
        query_subscriber.query().wait()?;

        Ok(query_subscriber)
    }

    /// Close this QueryingSubscriber
    #[inline]
    pub fn close(self) -> impl ZFuture<Output = ZResult<()>> {
        zready(self.subscriber.undeclare().res_sync())
    }

    /// Issue a new query using the configured selector.
    #[inline]
    pub fn query(&mut self) -> impl ZFuture<Output = ZResult<()>> {
        self.query_on_selector(
            Selector {
                key_selector: self.query_key_expr.clone(),
                value_selector: self.query_value_selector.clone().into(),
            },
            self.query_target.clone(),
            self.query_consolidation.clone(),
        )
    }

    /// Issue a new query on the specified selector.
    #[inline]
    pub fn query_on<'c, IntoSelector>(
        &mut self,
        selector: IntoSelector,
        target: QueryTarget,
        consolidation: QueryConsolidation,
    ) -> impl ZFuture<Output = ZResult<()>>
    where
        IntoSelector: Into<Selector<'c>>,
    {
        self.query_on_selector(selector.into(), target, consolidation)
    }

    #[inline]
    fn query_on_selector(
        &mut self,
        selector: Selector,
        target: QueryTarget,
        consolidation: QueryConsolidation,
    ) -> impl ZFuture<Output = ZResult<()>> {
        zlock!(self.state).pending_queries += 1;
        // pending queries will be decremented in RepliesHandler drop()
        let handler = RepliesHandler {
            state: self.state.clone(),
            callback: self.callback.clone(),
        };

        log::debug!("Start query on {}", selector);
        zready(
            self.session
                .get(selector)
                .target(target)
                .consolidation(consolidation)
                .callback(move |r| {
                    let mut state = zlock!(handler.state);
                    match r.sample {
                        Ok(s) => state.merge_queue.push(s),
                        Err(v) => log::debug!("Received error {}", v),
                    }
                })
                .wait(),
        )
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
        if state.pending_queries == 0 {
            state
                .merge_queue
                .sort_by_key(|sample| *sample.get_timestamp().unwrap());
            state
                .merge_queue
                .dedup_by_key(|sample| *sample.get_timestamp().unwrap());
            log::debug!(
                "Merged received publications - {} samples to propagate",
                state.merge_queue.len()
            );
            for s in state.merge_queue.drain(..) {
                (self.callback)(s);
            }
        }
    }
}

#[must_use = "ZFutures do nothing unless you `.wait()`, `.await` or poll them"]
pub struct HandlerQueryingSubscriberBuilder<'a, 'b, Receiver> {
    builder: QueryingSubscriberBuilder<'a, 'b>,
    handler: Option<zenoh::prelude::Handler<Sample, Receiver>>,
}

impl<'a, 'b, Receiver> std::future::Future for HandlerQueryingSubscriberBuilder<'a, 'b, Receiver>
where
    Receiver: Unpin,
{
    type Output = ZResult<HandlerQueryingSubscriber<'a, Receiver>>;

    #[inline]
    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut async_std::task::Context<'_>,
    ) -> std::task::Poll<<Self as ::std::future::Future>::Output> {
        let (callback, receiver) = self.handler.take().unwrap();

        std::task::Poll::Ready(
            CallbackQueryingSubscriber::new(self.builder.clone().with_static_keys(), callback).map(
                |subscriber| HandlerQueryingSubscriber {
                    subscriber,
                    receiver,
                },
            ),
        )
    }
}

impl<'a, 'b, Receiver> zenoh_sync::ZFuture for HandlerQueryingSubscriberBuilder<'a, 'b, Receiver>
where
    Receiver: Send + Sync + Unpin,
{
    #[inline]
    fn wait(mut self) -> Self::Output {
        let (callback, receiver) = self.handler.take().unwrap();

        CallbackQueryingSubscriber::new(self.builder.with_static_keys(), callback).map(
            |subscriber| HandlerQueryingSubscriber {
                subscriber,
                receiver,
            },
        )
    }
}

impl<'a, 'b, Receiver> HandlerQueryingSubscriberBuilder<'a, 'b, Receiver> {
    /// Change the subscription reliability.
    #[inline]
    pub fn reliability(mut self, reliability: Reliability) -> Self {
        self.builder.reliability = reliability;
        self
    }

    /// Change the subscription reliability to `Reliable`.
    #[inline]
    pub fn reliable(mut self) -> Self {
        self.builder.reliability = Reliability::Reliable;
        self
    }

    /// Change the subscription reliability to `BestEffort`.
    #[inline]
    pub fn best_effort(mut self) -> Self {
        self.builder.reliability = Reliability::BestEffort;
        self
    }

    /// Change the subscription mode.
    #[inline]
    pub fn mode(mut self, mode: SubMode) -> Self {
        self.builder.mode = mode;
        self
    }

    /// Change the subscription mode to Push.
    #[inline]
    pub fn push_mode(mut self) -> Self {
        self.builder.mode = SubMode::Push;
        self.builder.period = None;
        self
    }

    /// Change the subscription mode to Pull.
    #[inline]
    pub fn pull_mode(mut self) -> Self {
        self.builder.mode = SubMode::Pull;
        self
    }

    /// Change the subscription period.
    #[inline]
    pub fn period(mut self, period: Option<Period>) -> Self {
        self.builder.period = period;
        self
    }

    /// Change the selector to be used for queries.
    #[inline]
    pub fn query_selector<IntoSelector>(mut self, query_selector: IntoSelector) -> Self
    where
        IntoSelector: Into<Selector<'b>>,
    {
        let selector = query_selector.into();
        self.builder.query_key_expr = selector.key_selector.to_owned();
        self.builder.query_value_selector = selector.value_selector.to_string();
        self
    }

    /// Change the target to be used for queries.
    #[inline]
    pub fn query_target(mut self, query_target: QueryTarget) -> Self {
        self.builder.query_target = query_target;
        self
    }

    /// Change the consolidation mode to be used for queries.
    #[inline]
    pub fn query_consolidation(mut self, query_consolidation: QueryConsolidation) -> Self {
        self.builder.query_consolidation = query_consolidation;
        self
    }
}

pub struct HandlerQueryingSubscriber<'a, Receiver> {
    pub subscriber: CallbackQueryingSubscriber<'a>,
    pub receiver: Receiver,
}

impl<Receiver> HandlerQueryingSubscriber<'_, Receiver> {
    /// Close a [`HandlerQueryingSubscriber`](HandlerQueryingSubscriber)
    #[inline]
    #[must_use = "ZFutures do nothing unless you `.wait()`, `.await` or poll them"]
    pub fn close(self) -> impl ZFuture<Output = ZResult<()>> {
        self.subscriber.close()
    }

    /// Issue a new query using the configured selector.
    #[inline]
    pub fn query(&mut self) -> impl ZFuture<Output = ZResult<()>> {
        self.subscriber.query()
    }

    /// Issue a new query on the specified selector.
    #[inline]
    pub fn query_on<'c, IntoSelector>(
        &mut self,
        selector: IntoSelector,
        target: QueryTarget,
        consolidation: QueryConsolidation,
    ) -> impl ZFuture<Output = ZResult<()>>
    where
        IntoSelector: Into<Selector<'c>>,
    {
        self.subscriber.query_on(selector, target, consolidation)
    }
}

impl<Receiver> Deref for HandlerQueryingSubscriber<'_, Receiver> {
    type Target = Receiver;

    fn deref(&self) -> &Self::Target {
        &self.receiver
    }
}

impl HandlerQueryingSubscriber<'_, flume::Receiver<Sample>> {
    pub fn forward<'selflifetime, E: 'selflifetime, S>(
        &'selflifetime mut self,
        sink: S,
    ) -> futures::stream::Forward<
        impl futures::TryStream<Ok = Sample, Error = E, Item = Result<Sample, E>> + 'selflifetime,
        S,
    >
    where
        S: futures::sink::Sink<Sample, Error = E>,
    {
        futures::StreamExt::forward(futures::StreamExt::map(self.receiver.stream(), Ok), sink)
    }
}
