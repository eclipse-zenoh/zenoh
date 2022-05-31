use std::convert::TryInto;
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
use std::ops::Deref;
use std::sync::{Arc, Mutex};
use zenoh::prelude::*;
use zenoh::query::{QueryConsolidation, QueryTarget};
use zenoh::subscriber::{CallbackSubscriber, Reliability, SubMode};
use zenoh::time::Period;
use zenoh::Result as ZResult;
use zenoh_core::{zerror, zlock, AsyncResolve, Resolvable, Resolve, SyncResolve};

use crate::session_ext::SessionRef;

const MERGE_QUEUE_INITIAL_CAPCITY: usize = 32;

/// The builder of QueryingSubscriber, allowing to configure it.
pub struct QueryingSubscriberBuilder<'a, 'b> {
    session: SessionRef<'a>,
    key_expr: ZResult<KeyExpr<'b>>,
    reliability: Reliability,
    mode: SubMode,
    period: Option<Period>,
    query_selector: Option<ZResult<Selector<'b>>>,
    query_target: QueryTarget,
    query_consolidation: QueryConsolidation,
}
impl Clone for QueryingSubscriberBuilder<'_, '_> {
    fn clone(&self) -> Self {
        Self {
            session: self.session.clone(),
            key_expr: match &self.key_expr {
                Ok(ke) => Ok(ke.clone()),
                Err(e) => Err(zerror!("Cloned KE Error {}", e).into()),
            },
            reliability: self.reliability,
            mode: self.mode,
            period: self.period,
            query_selector: match &self.query_selector {
                Some(Ok(s)) => Some(Ok(s.clone())),
                None => None,
                Some(Err(e)) => Some(Err(zerror!("Cloned Selector Error: {}", e).into())),
            },
            query_target: self.query_target,
            query_consolidation: self.query_consolidation,
        }
    }
}

impl<'a, 'b> QueryingSubscriberBuilder<'a, 'b> {
    pub(crate) fn new(
        session: SessionRef<'a>,
        key_expr: ZResult<KeyExpr<'b>>,
    ) -> QueryingSubscriberBuilder<'a, 'b> {
        // By default query all matching publication caches and storages
        let query_target = QueryTarget::All;

        // By default no query consolidation, to receive more than 1 sample per-resource
        // (in history of publications is available)
        let query_consolidation = QueryConsolidation::none();

        QueryingSubscriberBuilder {
            session,
            key_expr,
            reliability: Reliability::default(),
            mode: SubMode::default(),
            period: None,
            query_selector: None,
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
            callback,
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
            handler: handler.into_handler(),
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
    pub fn query_consolidation(mut self, query_consolidation: QueryConsolidation) -> Self {
        self.query_consolidation = query_consolidation;
        self
    }

    fn with_static_keys(self) -> QueryingSubscriberBuilder<'a, 'static> {
        QueryingSubscriberBuilder {
            session: self.session,
            key_expr: self.key_expr.map(|s| s.into_owned()),
            reliability: self.reliability,
            mode: self.mode,
            period: self.period,
            query_selector: self.query_selector.map(|s| s.map(|s| s.to_owned())),
            query_target: self.query_target,
            query_consolidation: self.query_consolidation,
        }
    }
}

impl<'a, 'b> Resolvable for QueryingSubscriberBuilder<'a, 'b> {
    type Output = ZResult<HandlerQueryingSubscriber<'a, flume::Receiver<Sample>>>;
}

impl AsyncResolve for QueryingSubscriberBuilder<'_, '_> {
    type Future = futures::future::Ready<Self::Output>;
    fn res_async(self) -> Self::Future {
        futures::future::ready(self.res_sync())
    }
}

impl SyncResolve for QueryingSubscriberBuilder<'_, '_> {
    #[inline]
    fn res_sync(self) -> Self::Output {
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
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
pub struct CallbackQueryingSubscriberBuilder<'a, 'b, Callback> {
    builder: QueryingSubscriberBuilder<'a, 'b>,
    callback: Callback,
}

impl<'a, 'b, Callback> Resolvable for CallbackQueryingSubscriberBuilder<'a, 'b, Callback>
where
    Callback: Fn(Sample) + Unpin + Send + Sync + 'static,
{
    type Output = ZResult<CallbackQueryingSubscriber<'a>>;
}

impl<Callback> AsyncResolve for CallbackQueryingSubscriberBuilder<'_, '_, Callback>
where
    Callback: 'static + Fn(Sample) + Send + Sync + Unpin,
{
    type Future = futures::future::Ready<Self::Output>;

    fn res_async(self) -> Self::Future {
        futures::future::ready(self.res_sync())
    }
}

impl<Callback> SyncResolve for CallbackQueryingSubscriberBuilder<'_, '_, Callback>
where
    Callback: Fn(Sample) + Unpin + Send + Sync + 'static,
{
    #[inline]
    fn res_sync(self) -> Self::Output {
        CallbackQueryingSubscriber::new(self.builder.with_static_keys(), Box::new(self.callback))
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
        self.builder = self.builder.query_selector(query_selector);
        self
    }

    /// Change the target to be used for queries.
    #[inline]
    pub fn query_target(mut self, query_target: QueryTarget) -> Self {
        self.builder = self.builder.query_target(query_target);
        self
    }

    /// Change the consolidation mode to be used for queries.
    #[inline]
    pub fn query_consolidation(mut self, query_consolidation: QueryConsolidation) -> Self {
        self.builder = self.builder.query_consolidation(query_consolidation);
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
    _subscriber: CallbackSubscriber<'a>,
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
                    log::trace!("Sample received while query in progress: push it to merge_queue");
                    state.merge_queue.push(s);
                }
            }
        };

        let key_expr = conf.key_expr?;
        let Selector {
            key_selector,
            value_selector,
        } = match conf.query_selector {
            Some(Ok(s)) => s,
            Some(Err(e)) => return Err(e),
            None => Selector {
                key_selector: key_expr.clone(),
                value_selector: std::borrow::Cow::Borrowed(""),
            },
        };

        // declare subscriber at first
        let subscriber = match conf.session.clone() {
            SessionRef::Borrow(session) => session
                .subscribe(&key_expr)
                .callback(sub_callback)
                .reliability(conf.reliability)
                .mode(conf.mode)
                .period(conf.period)
                .res_sync()?,
            SessionRef::Shared(session) => session
                .subscribe(&key_expr)
                .callback(sub_callback)
                .reliability(conf.reliability)
                .mode(conf.mode)
                .period(conf.period)
                .res_sync()?,
        };

        let mut query_subscriber = CallbackQueryingSubscriber {
            session: conf.session,
            query_key_expr: key_selector,
            query_value_selector: value_selector.into_owned(),
            query_target: conf.query_target,
            query_consolidation: conf.query_consolidation,
            _subscriber: subscriber,
            callback,
            state,
        };

        // start query
        query_subscriber.query().res_sync()?;

        Ok(query_subscriber)
    }

    /// Close this QueryingSubscriber
    #[inline]
    pub fn close(self) -> impl Resolve<ZResult<()>> + 'a {
        struct Undeclare<'a> {
            inner: Option<CallbackQueryingSubscriber<'a>>,
        }
        impl Resolvable for Undeclare<'_> {
            type Output = ZResult<()>;
        }
        impl AsyncResolve for Undeclare<'_> {
            type Future = futures::future::Ready<Self::Output>;
            fn res_async(self) -> Self::Future {
                futures::future::ready(self.res_sync())
            }
        }
        impl SyncResolve for Undeclare<'_> {
            fn res_sync(mut self) -> Self::Output {
                self.inner.take().unwrap().close().res_sync()
            }
        }
        Undeclare { inner: Some(self) }
    }

    /// Issue a new query using the configured selector.
    #[inline]
    pub fn query(&mut self) -> impl Resolve<ZResult<()>> + '_ {
        self.query_on_selector(
            Selector {
                key_selector: self.query_key_expr.clone(),
                value_selector: self.query_value_selector.clone().into(),
            },
            self.query_target,
            self.query_consolidation,
        )
    }

    /// Issue a new query on the specified selector.
    #[inline]
    pub fn query_on<'c, IntoSelector>(
        &'c mut self,
        selector: IntoSelector,
        target: QueryTarget,
        consolidation: QueryConsolidation,
    ) -> impl Resolve<ZResult<()>> + 'c
    where
        IntoSelector: Into<Selector<'c>>,
    {
        self.query_on_selector(selector.into(), target, consolidation)
    }

    #[inline]
    fn query_on_selector<'c>(
        &'c mut self,
        selector: Selector<'c>,
        target: QueryTarget,
        consolidation: QueryConsolidation,
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
            state
                .merge_queue
                .sort_by_key(|sample| *sample.get_timestamp().unwrap());
            state
                .merge_queue
                .dedup_by_key(|sample| *sample.get_timestamp().unwrap());
            log::debug!(
                "All queries done. Replies and live publications merged - {} samples to propagate",
                state.merge_queue.len()
            );
            for s in state.merge_queue.drain(..) {
                (self.callback)(s);
            }
        }
    }
}

#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
pub struct HandlerQueryingSubscriberBuilder<'a, 'b, Receiver> {
    builder: QueryingSubscriberBuilder<'a, 'b>,
    handler: zenoh::prelude::Handler<Sample, Receiver>,
}

impl<'a, 'b, Receiver> Resolvable for HandlerQueryingSubscriberBuilder<'a, 'b, Receiver> {
    type Output = ZResult<HandlerQueryingSubscriber<'a, Receiver>>;
}

impl<Receiver> AsyncResolve for HandlerQueryingSubscriberBuilder<'_, '_, Receiver>
where
    Receiver: Send,
{
    type Future = futures::future::Ready<Self::Output>;
    fn res_async(self) -> Self::Future {
        futures::future::ready(self.res_sync())
    }
}

impl<'a, 'b, Receiver> SyncResolve for HandlerQueryingSubscriberBuilder<'a, 'b, Receiver>
where
    Receiver: Send,
{
    #[inline]
    fn res_sync(self) -> Self::Output {
        let (callback, receiver) = self.handler;
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
        self.builder = self.builder.reliability(reliability);
        self
    }

    /// Change the subscription reliability to `Reliable`.
    #[inline]
    pub fn reliable(mut self) -> Self {
        self.builder = self.builder.reliable();
        self
    }

    /// Change the subscription reliability to `BestEffort`.
    #[inline]
    pub fn best_effort(mut self) -> Self {
        self.builder = self.builder.best_effort();
        self
    }

    /// Change the subscription mode.
    #[inline]
    pub fn mode(mut self, mode: SubMode) -> Self {
        self.builder = self.builder.mode(mode);
        self
    }

    /// Change the subscription mode to Push.
    #[inline]
    pub fn push_mode(mut self) -> Self {
        self.builder = self.builder.push_mode();
        self
    }

    /// Change the subscription mode to Pull.
    #[inline]
    pub fn pull_mode(mut self) -> Self {
        self.builder = self.builder.pull_mode();
        self
    }

    /// Change the subscription period.
    #[inline]
    pub fn period(mut self, period: Option<Period>) -> Self {
        self.builder = self.builder.period(period);
        self
    }

    /// Change the selector to be used for queries.
    #[inline]
    pub fn query_selector<IntoSelector>(mut self, query_selector: IntoSelector) -> Self
    where
        IntoSelector: Into<Selector<'b>>,
    {
        self.builder = self.builder.query_selector(query_selector);
        self
    }

    /// Change the target to be used for queries.
    #[inline]
    pub fn query_target(mut self, query_target: QueryTarget) -> Self {
        self.builder = self.builder.query_target(query_target);
        self
    }

    /// Change the consolidation mode to be used for queries.
    #[inline]
    pub fn query_consolidation(mut self, query_consolidation: QueryConsolidation) -> Self {
        self.builder = self.builder.query_consolidation(query_consolidation);
        self
    }
}

pub struct HandlerQueryingSubscriber<'a, Receiver> {
    pub subscriber: CallbackQueryingSubscriber<'a>,
    pub receiver: Receiver,
}

impl<'a, Receiver> HandlerQueryingSubscriber<'a, Receiver> {
    /// Close a [`HandlerQueryingSubscriber`](HandlerQueryingSubscriber)
    #[inline]
    pub fn close(self) -> impl Resolve<ZResult<()>> + 'a {
        self.subscriber.close()
    }

    /// Issue a new query using the configured selector.
    #[inline]
    pub fn query(&mut self) -> impl Resolve<ZResult<()>> + '_ {
        self.subscriber.query()
    }

    /// Issue a new query on the specified selector.
    #[inline]
    pub fn query_on<'c, IntoSelector>(
        &'c mut self,
        selector: IntoSelector,
        target: QueryTarget,
        consolidation: QueryConsolidation,
    ) -> impl Resolve<ZResult<()>> + 'c
    where
        IntoSelector: Into<Selector<'c>> + 'c,
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
    pub fn forward<'s, E: 's, S>(
        &'s mut self,
        sink: S,
    ) -> futures::stream::Forward<
        impl futures::TryStream<Ok = Sample, Error = E, Item = Result<Sample, E>> + 's,
        S,
    >
    where
        S: futures::sink::Sink<Sample, Error = E>,
    {
        futures::StreamExt::forward(futures::StreamExt::map(self.receiver.stream(), Ok), sink)
    }
}
