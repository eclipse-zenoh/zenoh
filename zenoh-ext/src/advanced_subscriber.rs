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
use std::{future::IntoFuture, str::FromStr};

use zenoh::{
    config::ZenohId,
    handlers::{Callback, IntoHandler},
    key_expr::KeyExpr,
    query::{ConsolidationMode, Selector},
    sample::{Locality, Sample, SampleKind},
    session::{EntityGlobalId, EntityId},
    Resolvable, Resolve, Session, Wait,
};
use zenoh_util::{Timed, TimedEvent, Timer};
#[zenoh_macros::unstable]
use {
    async_trait::async_trait,
    std::collections::hash_map::Entry,
    std::collections::HashMap,
    std::convert::TryFrom,
    std::future::Ready,
    std::sync::{Arc, Mutex},
    std::time::Duration,
    uhlc::ID,
    zenoh::handlers::{locked, DefaultHandler},
    zenoh::internal::zlock,
    zenoh::pubsub::Subscriber,
    zenoh::query::{QueryTarget, Reply, ReplyKeyExpr},
    zenoh::time::Timestamp,
    zenoh::Result as ZResult,
};

use crate::advanced_cache::{ke_liveliness, KE_PREFIX, KE_STAR, KE_UHLC};

/// The builder of AdvancedSubscriber, allowing to configure it.
#[zenoh_macros::unstable]
pub struct AdvancedSubscriberBuilder<'a, 'b, Handler, const BACKGROUND: bool = false> {
    pub(crate) session: &'a Session,
    pub(crate) key_expr: ZResult<KeyExpr<'b>>,
    pub(crate) origin: Locality,
    pub(crate) retransmission: bool,
    pub(crate) query_target: QueryTarget,
    pub(crate) query_timeout: Duration,
    pub(crate) period: Option<Duration>,
    pub(crate) history: bool,
    pub(crate) liveliness: bool,
    pub(crate) handler: Handler,
}

#[zenoh_macros::unstable]
impl<'a, 'b, Handler> AdvancedSubscriberBuilder<'a, 'b, Handler> {
    pub(crate) fn new(
        session: &'a Session,
        key_expr: ZResult<KeyExpr<'b>>,
        origin: Locality,
        handler: Handler,
    ) -> Self {
        AdvancedSubscriberBuilder {
            session,
            key_expr,
            origin,
            handler,
            retransmission: false,
            query_target: QueryTarget::All,
            query_timeout: Duration::from_secs(10),
            history: false,
            liveliness: false,
            period: None,
        }
    }
}

#[zenoh_macros::unstable]
impl<'a, 'b> AdvancedSubscriberBuilder<'a, 'b, DefaultHandler> {
    /// Add callback to AdvancedSubscriber.
    #[inline]
    pub fn callback<Callback>(
        self,
        callback: Callback,
    ) -> AdvancedSubscriberBuilder<'a, 'b, Callback>
    where
        Callback: Fn(Sample) + Send + Sync + 'static,
    {
        AdvancedSubscriberBuilder {
            session: self.session,
            key_expr: self.key_expr.map(|s| s.into_owned()),
            origin: self.origin,
            retransmission: self.retransmission,
            query_target: self.query_target,
            query_timeout: self.query_timeout,
            period: self.period,
            history: self.history,
            liveliness: self.liveliness,
            handler: callback,
        }
    }

    /// Add callback to `AdvancedSubscriber`.
    ///
    /// Using this guarantees that your callback will never be called concurrently.
    /// If your callback is also accepted by the [`callback`](AdvancedSubscriberBuilder::callback) method, we suggest you use it instead of `callback_mut`
    #[inline]
    pub fn callback_mut<CallbackMut>(
        self,
        callback: CallbackMut,
    ) -> AdvancedSubscriberBuilder<'a, 'b, impl Fn(Sample) + Send + Sync + 'static>
    where
        CallbackMut: FnMut(Sample) + Send + Sync + 'static,
    {
        self.callback(locked(callback))
    }

    /// Make the built AdvancedSubscriber an [`AdvancedSubscriber`](AdvancedSubscriber).
    #[inline]
    pub fn with<Handler>(self, handler: Handler) -> AdvancedSubscriberBuilder<'a, 'b, Handler>
    where
        Handler: IntoHandler<Sample>,
    {
        AdvancedSubscriberBuilder {
            session: self.session,
            key_expr: self.key_expr.map(|s| s.into_owned()),
            origin: self.origin,
            retransmission: self.retransmission,
            query_target: self.query_target,
            query_timeout: self.query_timeout,
            period: self.period,
            history: self.history,
            liveliness: self.liveliness,
            handler,
        }
    }
}

#[zenoh_macros::unstable]
impl<'a, 'b, Handler> AdvancedSubscriberBuilder<'a, 'b, Handler> {
    /// Restrict the matching publications that will be receive by this [`Subscriber`]
    /// to the ones that have the given [`Locality`](crate::prelude::Locality).
    #[zenoh_macros::unstable]
    #[inline]
    pub fn allowed_origin(mut self, origin: Locality) -> Self {
        self.origin = origin;
        self
    }

    /// Ask for retransmission of detected lost Samples.
    ///
    /// Retransmission can only be achieved by Publishers that also activate retransmission.
    #[zenoh_macros::unstable]
    #[inline]
    pub fn retransmission(mut self) -> Self {
        self.retransmission = true;
        self
    }

    // /// Change the target to be used for queries.

    // #[inline]
    // pub fn query_target(mut self, query_target: QueryTarget) -> Self {
    //     self.query_target = query_target;
    //     self
    // }

    /// Change the timeout to be used for queries (history, retransmission).
    #[zenoh_macros::unstable]
    #[inline]
    pub fn query_timeout(mut self, query_timeout: Duration) -> Self {
        self.query_timeout = query_timeout;
        self
    }

    /// Enable periodic queries for not yet received Samples and specify their period.
    ///
    /// This allows to retrieve the last Sample(s) if the last Sample(s) is/are lost.
    /// So it is useful for sporadic publications but useless for periodic publications
    /// with a period smaller or equal to this period.
    /// Retransmission can only be achieved by Publishers that also activate retransmission.
    #[zenoh_macros::unstable]
    #[inline]
    pub fn periodic_queries(mut self, period: Option<Duration>) -> Self {
        self.period = period;
        self
    }

    /// Enable query for historical data.
    ///
    /// History can only be retransmitted by Publishers that also activate history.
    #[zenoh_macros::unstable]
    #[inline]
    pub fn history(mut self) -> Self {
        self.history = true;
        self
    }

    /// Enable detection of late joiner publishers and query for their historical data.
    ///
    /// Let joiner detectiopn can only be achieved for Publishers that also activate late_joiner.
    /// History can only be retransmitted by Publishers that also activate history.
    #[zenoh_macros::unstable]
    #[inline]
    pub fn late_joiner(mut self) -> Self {
        self.liveliness = true;
        self
    }

    fn with_static_keys(self) -> AdvancedSubscriberBuilder<'a, 'static, Handler> {
        AdvancedSubscriberBuilder {
            session: self.session,
            key_expr: self.key_expr.map(|s| s.into_owned()),
            origin: self.origin,
            retransmission: self.retransmission,
            query_target: self.query_target,
            query_timeout: self.query_timeout,
            period: self.period,
            history: self.history,
            liveliness: self.liveliness,
            handler: self.handler,
        }
    }
}

impl<Handler> Resolvable for AdvancedSubscriberBuilder<'_, '_, Handler>
where
    Handler: IntoHandler<Sample>,
    Handler::Handler: Send,
{
    type To = ZResult<AdvancedSubscriber<Handler::Handler>>;
}

impl<Handler> Wait for AdvancedSubscriberBuilder<'_, '_, Handler>
where
    Handler: IntoHandler<Sample> + Send,
    Handler::Handler: Send,
{
    fn wait(self) -> <Self as Resolvable>::To {
        AdvancedSubscriber::new(self.with_static_keys())
    }
}

impl<Handler> IntoFuture for AdvancedSubscriberBuilder<'_, '_, Handler>
where
    Handler: IntoHandler<Sample> + Send,
    Handler::Handler: Send,
{
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

#[zenoh_macros::unstable]
struct State {
    global_pending_queries: u64,
    sequenced_states: HashMap<EntityGlobalId, SourceState<u32>>,
    timestamped_states: HashMap<ID, SourceState<Timestamp>>,
}

#[zenoh_macros::unstable]
struct SourceState<T> {
    last_delivered: Option<T>,
    pending_queries: u64,
    pending_samples: HashMap<T, Sample>,
}

#[zenoh_macros::unstable]
pub struct AdvancedSubscriber<Receiver> {
    _subscriber: Subscriber<()>,
    receiver: Receiver,
    _liveliness_subscriber: Option<Subscriber<()>>,
}

#[zenoh_macros::unstable]
impl<Receiver> std::ops::Deref for AdvancedSubscriber<Receiver> {
    type Target = Receiver;
    fn deref(&self) -> &Self::Target {
        &self.receiver
    }
}

#[zenoh_macros::unstable]
impl<Receiver> std::ops::DerefMut for AdvancedSubscriber<Receiver> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.receiver
    }
}

#[zenoh_macros::unstable]
fn handle_sample(states: &mut State, sample: Sample, callback: &Callback<Sample>) -> bool {
    if let (Some(source_id), Some(source_sn)) = (
        sample.source_info().source_id(),
        sample.source_info().source_sn(),
    ) {
        let entry = states.sequenced_states.entry(*source_id);
        let new = matches!(&entry, Entry::Vacant(_));
        let state = entry.or_insert(SourceState::<u32> {
            last_delivered: None,
            pending_queries: 0,
            pending_samples: HashMap::new(),
        });
        if states.global_pending_queries != 0 {
            state.pending_samples.insert(source_sn, sample);
        } else if state.last_delivered.is_some() && source_sn != state.last_delivered.unwrap() + 1 {
            if source_sn > state.last_delivered.unwrap() {
                state.pending_samples.insert(source_sn, sample);
            }
        } else {
            callback.call(sample);
            let mut last_seq_num = source_sn;
            state.last_delivered = Some(last_seq_num);
            while let Some(s) = state.pending_samples.remove(&(last_seq_num + 1)) {
                callback.call(s);
                last_seq_num += 1;
                state.last_delivered = Some(last_seq_num);
            }
        }
        new
    } else if let Some(timestamp) = sample.timestamp() {
        let entry = states.timestamped_states.entry(*timestamp.get_id());
        let state = entry.or_insert(SourceState::<Timestamp> {
            last_delivered: None,
            pending_queries: 0,
            pending_samples: HashMap::new(),
        });
        if state.last_delivered.map(|t| t < *timestamp).unwrap_or(true) {
            if states.global_pending_queries == 0 && state.pending_queries == 0 {
                state.last_delivered = Some(*timestamp);
                callback.call(sample);
            } else {
                state.pending_samples.entry(*timestamp).or_insert(sample);
            }
        }
        false
    } else {
        callback.call(sample);
        false
    }
}

#[zenoh_macros::unstable]
fn seq_num_range(start: Option<u32>, end: Option<u32>) -> String {
    match (start, end) {
        (Some(start), Some(end)) => format!("_sn={}..{}", start, end),
        (Some(start), None) => format!("_sn={}..", start),
        (None, Some(end)) => format!("_sn=..{}", end),
        (None, None) => "_sn=..".to_string(),
    }
}

#[zenoh_macros::unstable]
#[derive(Clone)]
struct PeriodicQuery {
    source_id: Option<EntityGlobalId>,
    statesref: Arc<Mutex<State>>,
    key_expr: KeyExpr<'static>,
    session: Session,
    query_target: QueryTarget,
    query_timeout: Duration,
    callback: Callback<Sample>,
}

#[zenoh_macros::unstable]
impl PeriodicQuery {
    fn with_source_id(mut self, source_id: EntityGlobalId) -> Self {
        self.source_id = Some(source_id);
        self
    }
}

#[zenoh_macros::unstable]
#[async_trait]
impl Timed for PeriodicQuery {
    async fn run(&mut self) {
        let mut lock = zlock!(self.statesref);
        let states = &mut *lock;
        if let Some(source_id) = &self.source_id {
            if let Some(state) = states.sequenced_states.get_mut(source_id) {
                state.pending_queries += 1;
                let query_expr = KE_PREFIX
                    / &source_id.zid().into_keyexpr()
                    / &KeyExpr::try_from(source_id.eid().to_string()).unwrap()
                    / &self.key_expr;
                let seq_num_range = seq_num_range(Some(state.last_delivered.unwrap() + 1), None);
                drop(lock);
                let handler = SequencedRepliesHandler {
                    source_id: *source_id,
                    statesref: self.statesref.clone(),
                    callback: self.callback.clone(),
                };
                let _ = self
                    .session
                    .get(Selector::from((query_expr, seq_num_range)))
                    .callback({
                        let key_expr = self.key_expr.clone().into_owned();
                        move |r: Reply| {
                            if let Ok(s) = r.into_result() {
                                if key_expr.intersects(s.key_expr()) {
                                    let states = &mut *zlock!(handler.statesref);
                                    handle_sample(states, s, &handler.callback);
                                }
                            }
                        }
                    })
                    .consolidation(ConsolidationMode::None)
                    .accept_replies(ReplyKeyExpr::Any)
                    .target(self.query_target)
                    .timeout(self.query_timeout)
                    .wait();
            }
        }
    }
}

#[zenoh_macros::unstable]
impl<Handler> AdvancedSubscriber<Handler> {
    fn new<H>(conf: AdvancedSubscriberBuilder<'_, '_, H>) -> ZResult<Self>
    where
        H: IntoHandler<Sample, Handler = Handler> + Send,
    {
        let statesref = Arc::new(Mutex::new(State {
            sequenced_states: HashMap::new(),
            timestamped_states: HashMap::new(),
            global_pending_queries: if conf.history { 1 } else { 0 },
        }));
        let (callback, receiver) = conf.handler.into_handler();
        let key_expr = conf.key_expr?;
        let retransmission = conf.retransmission;
        let query_target = conf.query_target;
        let query_timeout = conf.query_timeout;
        let session = conf.session.clone();
        let periodic_query = conf.period.map(|period| {
            (
                Arc::new(Timer::new(false)),
                period,
                PeriodicQuery {
                    source_id: None,
                    statesref: statesref.clone(),
                    key_expr: key_expr.clone().into_owned(),
                    session,
                    query_target,
                    query_timeout,
                    callback: callback.clone(),
                },
            )
        });

        let sub_callback = {
            let statesref = statesref.clone();
            let session = conf.session.clone();
            let callback = callback.clone();
            let key_expr = key_expr.clone().into_owned();
            let periodic_query = periodic_query.clone();

            move |s: Sample| {
                let mut lock = zlock!(statesref);
                let states = &mut *lock;
                let source_id = s.source_info().source_id().cloned();
                let new = handle_sample(states, s, &callback);

                if let Some(source_id) = source_id {
                    if new {
                        if let Some((timer, period, query)) = periodic_query.as_ref() {
                            timer.add(TimedEvent::periodic(
                                *period,
                                query.clone().with_source_id(source_id),
                            ))
                        }
                    }

                    if let Some(state) = states.sequenced_states.get_mut(&source_id) {
                        if retransmission
                            && state.pending_queries == 0
                            && !state.pending_samples.is_empty()
                        {
                            state.pending_queries += 1;
                            let query_expr = KE_PREFIX
                                / &source_id.zid().into_keyexpr()
                                / &KeyExpr::try_from(source_id.eid().to_string()).unwrap()
                                / &key_expr;
                            let seq_num_range =
                                seq_num_range(Some(state.last_delivered.unwrap() + 1), None);
                            drop(lock);
                            let handler = SequencedRepliesHandler {
                                source_id,
                                statesref: statesref.clone(),
                                callback: callback.clone(),
                            };
                            let _ = session
                                .get(Selector::from((query_expr, seq_num_range)))
                                .callback({
                                    let key_expr = key_expr.clone().into_owned();
                                    move |r: Reply| {
                                        if let Ok(s) = r.into_result() {
                                            if key_expr.intersects(s.key_expr()) {
                                                let states = &mut *zlock!(handler.statesref);
                                                handle_sample(states, s, &handler.callback);
                                            }
                                        }
                                    }
                                })
                                .consolidation(ConsolidationMode::None)
                                .accept_replies(ReplyKeyExpr::Any)
                                .target(query_target)
                                .timeout(query_timeout)
                                .wait();
                        }
                    }
                }
            }
        };

        let subscriber = conf
            .session
            .declare_subscriber(&key_expr)
            .callback(sub_callback)
            .allowed_origin(conf.origin)
            .wait()?;

        if conf.history {
            let handler = InitialRepliesHandler {
                statesref: statesref.clone(),
                callback: callback.clone(),
                periodic_query: periodic_query.clone(),
            };
            let _ = conf
                .session
                .get(Selector::from((
                    KE_PREFIX / KE_STAR / KE_STAR / &key_expr,
                    "0..",
                )))
                .callback({
                    let key_expr = key_expr.clone().into_owned();
                    move |r: Reply| {
                        if let Ok(s) = r.into_result() {
                            if key_expr.intersects(s.key_expr()) {
                                let states = &mut *zlock!(handler.statesref);
                                handle_sample(states, s, &handler.callback);
                            }
                        }
                    }
                })
                .consolidation(ConsolidationMode::None)
                .accept_replies(ReplyKeyExpr::Any)
                .target(query_target)
                .timeout(query_timeout)
                .wait();
        }

        let liveliness_subscriber = if conf.history && conf.liveliness {
            let live_callback = {
                let session = conf.session.clone();
                let statesref = statesref.clone();
                let key_expr = key_expr.clone().into_owned();
                move |s: Sample| {
                    if s.kind() == SampleKind::Put {
                        if let Ok(parsed) = ke_liveliness::parse(s.key_expr().as_keyexpr()) {
                            if let Ok(zid) = ZenohId::from_str(parsed.zid().as_str()) {
                                // TODO : If we already have a state associated to this discovered source
                                // we should query with the appropriate range to avoid unnecessary retransmissions
                                if parsed.eid() == KE_UHLC {
                                    let states = &mut *zlock!(statesref);
                                    let entry = states.timestamped_states.entry(ID::from(zid));
                                    let state = entry.or_insert(SourceState::<Timestamp> {
                                        last_delivered: None,
                                        pending_queries: 0,
                                        pending_samples: HashMap::new(),
                                    });
                                    state.pending_queries += 1;

                                    let handler = TimestampedRepliesHandler {
                                        id: ID::from(zid),
                                        statesref: statesref.clone(),
                                        callback: callback.clone(),
                                    };
                                    let _ = session
                                        .get(Selector::from((s.key_expr(), "0..")))
                                        .callback({
                                            let key_expr = key_expr.clone().into_owned();
                                            move |r: Reply| {
                                                if let Ok(s) = r.into_result() {
                                                    if key_expr.intersects(s.key_expr()) {
                                                        let states =
                                                            &mut *zlock!(handler.statesref);
                                                        handle_sample(states, s, &handler.callback);
                                                    }
                                                }
                                            }
                                        })
                                        .consolidation(ConsolidationMode::None)
                                        .accept_replies(ReplyKeyExpr::Any)
                                        .target(query_target)
                                        .timeout(query_timeout)
                                        .wait();
                                } else if let Ok(eid) = EntityId::from_str(parsed.eid().as_str()) {
                                    let source_id = EntityGlobalId::new(zid, eid);
                                    let states = &mut *zlock!(statesref);
                                    let entry = states.sequenced_states.entry(source_id);
                                    let new = matches!(&entry, Entry::Vacant(_));
                                    let state = entry.or_insert(SourceState::<u32> {
                                        last_delivered: None,
                                        pending_queries: 0,
                                        pending_samples: HashMap::new(),
                                    });
                                    state.pending_queries += 1;

                                    let handler = SequencedRepliesHandler {
                                        source_id,
                                        statesref: statesref.clone(),
                                        callback: callback.clone(),
                                    };
                                    let _ = session
                                        .get(Selector::from((s.key_expr(), "0..")))
                                        .callback({
                                            let key_expr = key_expr.clone().into_owned();
                                            move |r: Reply| {
                                                if let Ok(s) = r.into_result() {
                                                    if key_expr.intersects(s.key_expr()) {
                                                        let states =
                                                            &mut *zlock!(handler.statesref);
                                                        handle_sample(states, s, &handler.callback);
                                                    }
                                                }
                                            }
                                        })
                                        .consolidation(ConsolidationMode::None)
                                        .accept_replies(ReplyKeyExpr::Any)
                                        .target(query_target)
                                        .timeout(query_timeout)
                                        .wait();

                                    if new {
                                        if let Some((timer, period, query)) =
                                            periodic_query.as_ref()
                                        {
                                            timer.add(TimedEvent::periodic(
                                                *period,
                                                query.clone().with_source_id(source_id),
                                            ))
                                        }
                                    }
                                }
                            } else {
                                let states = &mut *zlock!(statesref);
                                states.global_pending_queries += 1;

                                let handler = InitialRepliesHandler {
                                    statesref: statesref.clone(),
                                    periodic_query: None,
                                    callback: callback.clone(),
                                };
                                let _ = session
                                    .get(Selector::from((s.key_expr(), "0..")))
                                    .callback({
                                        let key_expr = key_expr.clone().into_owned();
                                        move |r: Reply| {
                                            if let Ok(s) = r.into_result() {
                                                if key_expr.intersects(s.key_expr()) {
                                                    let states = &mut *zlock!(handler.statesref);
                                                    handle_sample(states, s, &handler.callback);
                                                }
                                            }
                                        }
                                    })
                                    .consolidation(ConsolidationMode::None)
                                    .accept_replies(ReplyKeyExpr::Any)
                                    .target(query_target)
                                    .timeout(query_timeout)
                                    .wait();
                            }
                        } else {
                            tracing::warn!(
                                "Received malformed liveliness token key expression: {}",
                                s.key_expr()
                            );
                        }
                    }
                }
            };

            Some(
                conf
                .session
                .liveliness()
                .declare_subscriber(KE_PREFIX / KE_STAR / KE_STAR / &key_expr)
                // .declare_subscriber(keformat!(ke_liveliness_all::formatter(), zid = 0, eid = 0, remaining = key_expr).unwrap())
                .history(true)
                .callback(live_callback)
                .wait()?,
            )
        } else {
            None
        };

        let reliable_subscriber = AdvancedSubscriber {
            _subscriber: subscriber,
            receiver,
            _liveliness_subscriber: liveliness_subscriber,
        };

        Ok(reliable_subscriber)
    }

    /// Close this AdvancedSubscriber
    #[inline]
    pub fn close(self) -> impl Resolve<ZResult<()>> {
        self._subscriber.undeclare()
    }
}

#[zenoh_macros::unstable]
#[inline]
fn flush_sequenced_source(state: &mut SourceState<u32>, callback: &Callback<Sample>) {
    if state.pending_queries == 0 && !state.pending_samples.is_empty() {
        if state.last_delivered.is_some() {
            tracing::error!("Sample missed: unable to retrieve some missing samples.");
        }
        let mut pending_samples = state
            .pending_samples
            .drain()
            .collect::<Vec<(u32, Sample)>>();
        pending_samples.sort_by_key(|(k, _s)| *k);
        for (seq_num, sample) in pending_samples {
            if state
                .last_delivered
                .map(|last| seq_num > last)
                .unwrap_or(true)
            {
                state.last_delivered = Some(seq_num);
                callback.call(sample);
            }
        }
    }
}

#[zenoh_macros::unstable]
#[inline]
fn flush_timestamped_source(state: &mut SourceState<Timestamp>, callback: &Callback<Sample>) {
    if state.pending_queries == 0 && !state.pending_samples.is_empty() {
        let mut pending_samples = state
            .pending_samples
            .drain()
            .collect::<Vec<(Timestamp, Sample)>>();
        pending_samples.sort_by_key(|(k, _s)| *k);
        for (timestamp, sample) in pending_samples {
            if state
                .last_delivered
                .map(|last| timestamp > last)
                .unwrap_or(true)
            {
                state.last_delivered = Some(timestamp);
                callback.call(sample);
            }
        }
    }
}

#[zenoh_macros::unstable]
#[derive(Clone)]
struct InitialRepliesHandler {
    statesref: Arc<Mutex<State>>,
    periodic_query: Option<(Arc<Timer>, Duration, PeriodicQuery)>,
    callback: Callback<Sample>,
}

#[zenoh_macros::unstable]
impl Drop for InitialRepliesHandler {
    fn drop(&mut self) {
        let states = &mut *zlock!(self.statesref);
        states.global_pending_queries = states.global_pending_queries.saturating_sub(1);

        if states.global_pending_queries == 0 {
            for (source_id, state) in states.sequenced_states.iter_mut() {
                flush_sequenced_source(state, &self.callback);
                if let Some((timer, period, query)) = self.periodic_query.as_ref() {
                    timer.add(TimedEvent::periodic(
                        *period,
                        query.clone().with_source_id(*source_id),
                    ))
                }
            }
            for state in states.timestamped_states.values_mut() {
                flush_timestamped_source(state, &self.callback);
            }
        }
    }
}

#[zenoh_macros::unstable]
#[derive(Clone)]
struct SequencedRepliesHandler {
    source_id: EntityGlobalId,
    statesref: Arc<Mutex<State>>,
    callback: Callback<Sample>,
}

#[zenoh_macros::unstable]
impl Drop for SequencedRepliesHandler {
    fn drop(&mut self) {
        let states = &mut *zlock!(self.statesref);
        if let Some(state) = states.sequenced_states.get_mut(&self.source_id) {
            state.pending_queries = state.pending_queries.saturating_sub(1);
            if states.global_pending_queries == 0 {
                flush_sequenced_source(state, &self.callback)
            }
        }
    }
}

#[zenoh_macros::unstable]
#[derive(Clone)]
struct TimestampedRepliesHandler {
    id: ID,
    statesref: Arc<Mutex<State>>,
    callback: Callback<Sample>,
}

#[zenoh_macros::unstable]
impl Drop for TimestampedRepliesHandler {
    fn drop(&mut self) {
        let states = &mut *zlock!(self.statesref);
        if let Some(state) = states.timestamped_states.get_mut(&self.id) {
            state.pending_queries = state.pending_queries.saturating_sub(1);
            if states.global_pending_queries == 0 {
                flush_timestamped_source(state, &self.callback);
            }
        }
    }
}
