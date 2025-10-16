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
use std::{collections::BTreeMap, future::IntoFuture, str::FromStr};

use zenoh::{
    config::ZenohId,
    handlers::{Callback, CallbackParameter, IntoHandler},
    key_expr::KeyExpr,
    liveliness::{LivelinessSubscriberBuilder, LivelinessToken},
    pubsub::SubscriberBuilder,
    query::{
        ConsolidationMode, Parameters, Selector, TimeBound, TimeExpr, TimeRange, ZenohParameters,
    },
    sample::{Locality, Sample, SampleKind, SourceSn},
    session::{EntityGlobalId, EntityId},
    Resolvable, Resolve, Session, Wait, KE_ADV_PREFIX, KE_EMPTY, KE_PUB, KE_STAR, KE_STARSTAR,
    KE_SUB,
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
    zenoh::internal::{runtime::ZRuntime, zlock},
    zenoh::pubsub::Subscriber,
    zenoh::query::{QueryTarget, Reply, ReplyKeyExpr},
    zenoh::time::Timestamp,
    zenoh::Result as ZResult,
};

use crate::{
    advanced_cache::{ke_liveliness, KE_UHLC},
    z_deserialize,
};

#[derive(Debug, Default, Clone)]
/// Configure query for historical data for [`history`](crate::AdvancedSubscriberBuilder::history) method.
#[zenoh_macros::unstable]
pub struct HistoryConfig {
    liveliness: bool,
    sample_depth: Option<usize>,
    age: Option<f64>,
}

#[zenoh_macros::unstable]
impl HistoryConfig {
    /// Enable detection of late joiner publishers and query for their historical data.
    ///
    /// Late joiner detection can only be achieved for [`AdvancedPublishers`](crate::AdvancedPublisher) that enable publisher_detection.
    /// History can only be retransmitted by [`AdvancedPublishers`](crate::AdvancedPublisher) that enable [`cache`](crate::AdvancedPublisherBuilder::cache).
    #[inline]
    #[zenoh_macros::unstable]
    pub fn detect_late_publishers(mut self) -> Self {
        self.liveliness = true;
        self
    }

    /// Specify how many samples to query for each resource.
    #[zenoh_macros::unstable]
    pub fn max_samples(mut self, depth: usize) -> Self {
        self.sample_depth = Some(depth);
        self
    }

    /// Specify the maximum age of samples to query.
    #[zenoh_macros::unstable]
    pub fn max_age(mut self, seconds: f64) -> Self {
        self.age = Some(seconds);
        self
    }
}

#[derive(Debug, Default, Clone, Copy)]
/// Configure retransmission.
#[zenoh_macros::unstable]
pub struct RecoveryConfig<const CONFIGURED: bool = true> {
    periodic_queries: Option<Duration>,
    heartbeat: bool,
}

#[zenoh_macros::unstable]
impl RecoveryConfig<false> {
    /// Enable periodic queries for not yet received Samples and specify their period.
    ///
    /// This allows retrieving the last Sample(s) if the last Sample(s) is/are lost.
    /// So it is useful for sporadic publications but useless for periodic publications
    /// with a period smaller or equal to this period.
    /// Retransmission can only be achieved by [`AdvancedPublishers`](crate::AdvancedPublisher)
    /// that enable [`cache`](crate::AdvancedPublisherBuilder::cache) and
    /// [`sample_miss_detection`](crate::AdvancedPublisherBuilder::sample_miss_detection).
    #[zenoh_macros::unstable]
    #[inline]
    pub fn periodic_queries(self, period: Duration) -> RecoveryConfig<true> {
        RecoveryConfig {
            periodic_queries: Some(period),
            heartbeat: false,
        }
    }

    /// Subscribe to heartbeats of [`AdvancedPublishers`](crate::AdvancedPublisher).
    ///
    /// This allows receiving the last published Sample's sequence number and check for misses.
    /// Heartbeat subscriber must be paired with [`AdvancedPublishers`](crate::AdvancedPublisher)
    /// that enable [`cache`](crate::AdvancedPublisherBuilder::cache) and
    /// [`sample_miss_detection`](crate::AdvancedPublisherBuilder::sample_miss_detection) with
    /// [`heartbeat`](crate::advanced_publisher::MissDetectionConfig::heartbeat) or
    /// [`sporadic_heartbeat`](crate::advanced_publisher::MissDetectionConfig::sporadic_heartbeat).
    #[zenoh_macros::unstable]
    #[inline]
    pub fn heartbeat(self) -> RecoveryConfig<true> {
        RecoveryConfig {
            periodic_queries: None,
            heartbeat: true,
        }
    }
}

/// The builder of an [`AdvancedSubscriber`], allowing to configure it.
#[zenoh_macros::unstable]
pub struct AdvancedSubscriberBuilder<'a, 'b, 'c, Handler, const BACKGROUND: bool = false> {
    pub(crate) session: &'a Session,
    pub(crate) key_expr: ZResult<KeyExpr<'b>>,
    pub(crate) origin: Locality,
    pub(crate) retransmission: Option<RecoveryConfig>,
    pub(crate) query_target: QueryTarget,
    pub(crate) query_timeout: Duration,
    pub(crate) history: Option<HistoryConfig>,
    pub(crate) liveliness: bool,
    pub(crate) meta_key_expr: Option<ZResult<KeyExpr<'c>>>,
    pub(crate) handler: Handler,
}

#[zenoh_macros::unstable]
impl<'a, 'b, Handler> AdvancedSubscriberBuilder<'a, 'b, '_, Handler> {
    #[zenoh_macros::unstable]
    pub(crate) fn new(builder: SubscriberBuilder<'a, 'b, Handler>) -> Self {
        AdvancedSubscriberBuilder {
            session: builder.session,
            key_expr: builder.key_expr,
            origin: builder.origin,
            handler: builder.handler,
            retransmission: None,
            query_target: QueryTarget::All,
            query_timeout: Duration::from_secs(10),
            history: None,
            liveliness: false,
            meta_key_expr: None,
        }
    }
}

#[zenoh_macros::unstable]
impl<'a, 'b, 'c> AdvancedSubscriberBuilder<'a, 'b, 'c, DefaultHandler> {
    /// Add callback to AdvancedSubscriber.
    #[inline]
    #[zenoh_macros::unstable]
    pub fn callback<F>(self, callback: F) -> AdvancedSubscriberBuilder<'a, 'b, 'c, Callback<Sample>>
    where
        F: Fn(Sample) + Send + Sync + 'static,
    {
        self.with(Callback::from(callback))
    }

    /// Add callback to `AdvancedSubscriber`.
    ///
    /// Using this guarantees that your callback will never be called concurrently.
    /// If your callback is also accepted by the [`callback`](AdvancedSubscriberBuilder::callback) method, we suggest you use it instead of `callback_mut`
    #[inline]
    #[zenoh_macros::unstable]
    pub fn callback_mut<F>(
        self,
        callback: F,
    ) -> AdvancedSubscriberBuilder<'a, 'b, 'c, Callback<Sample>>
    where
        F: FnMut(Sample) + Send + Sync + 'static,
    {
        self.callback(locked(callback))
    }

    /// Make the built AdvancedSubscriber an [`AdvancedSubscriber`](AdvancedSubscriber).
    #[inline]
    #[zenoh_macros::unstable]
    pub fn with<Handler>(self, handler: Handler) -> AdvancedSubscriberBuilder<'a, 'b, 'c, Handler>
    where
        Handler: IntoHandler<Sample>,
    {
        AdvancedSubscriberBuilder {
            session: self.session,
            key_expr: self.key_expr,
            origin: self.origin,
            retransmission: self.retransmission,
            query_target: self.query_target,
            query_timeout: self.query_timeout,
            history: self.history,
            liveliness: self.liveliness,
            meta_key_expr: self.meta_key_expr,
            handler,
        }
    }
}

#[zenoh_macros::unstable]
impl<'a, 'b, 'c> AdvancedSubscriberBuilder<'a, 'b, 'c, Callback<Sample>> {
    /// Make the subscriber run in background until the session is closed.
    ///
    /// Background builder doesn't return a `AdvancedSubscriber` object anymore.
    pub fn background(self) -> AdvancedSubscriberBuilder<'a, 'b, 'c, Callback<Sample>, true> {
        AdvancedSubscriberBuilder {
            session: self.session,
            key_expr: self.key_expr,
            origin: self.origin,
            retransmission: self.retransmission,
            query_target: self.query_target,
            query_timeout: self.query_timeout,
            history: self.history,
            liveliness: self.liveliness,
            meta_key_expr: self.meta_key_expr,
            handler: self.handler,
        }
    }
}

#[zenoh_macros::unstable]
impl<'a, 'c, Handler, const BACKGROUND: bool>
    AdvancedSubscriberBuilder<'a, '_, 'c, Handler, BACKGROUND>
{
    /// Restrict the matching publications that will be received by this [`Subscriber`]
    /// to the ones that have the given [`Locality`](crate::prelude::Locality).
    #[zenoh_macros::unstable]
    #[inline]
    pub fn allowed_origin(mut self, origin: Locality) -> Self {
        self.origin = origin;
        self
    }

    /// Ask for retransmission of detected lost Samples.
    ///
    /// Retransmission can only be achieved by [`AdvancedPublishers`](crate::AdvancedPublisher)
    /// that enable [`cache`](crate::AdvancedPublisherBuilder::cache) and
    /// [`sample_miss_detection`](crate::AdvancedPublisherBuilder::sample_miss_detection).
    #[zenoh_macros::unstable]
    #[inline]
    pub fn recovery(mut self, conf: RecoveryConfig) -> Self {
        self.retransmission = Some(conf);
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

    /// Enable query for historical data.
    ///
    /// History can only be retransmitted by [`AdvancedPublishers`](crate::AdvancedPublisher) that enable [`cache`](crate::AdvancedPublisherBuilder::cache).
    #[zenoh_macros::unstable]
    #[inline]
    pub fn history(mut self, config: HistoryConfig) -> Self {
        self.history = Some(config);
        self
    }

    /// Allow this subscriber to be detected through liveliness.
    #[zenoh_macros::unstable]
    pub fn subscriber_detection(mut self) -> Self {
        self.liveliness = true;
        self
    }

    /// A key expression added to the liveliness token key expression.
    /// It can be used to convey metadata.
    #[zenoh_macros::unstable]
    pub fn subscriber_detection_metadata<TryIntoKeyExpr>(mut self, meta: TryIntoKeyExpr) -> Self
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'c>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'c>>>::Error: Into<zenoh::Error>,
    {
        self.meta_key_expr = Some(meta.try_into().map_err(Into::into));
        self
    }

    #[zenoh_macros::unstable]
    fn with_static_keys(self) -> AdvancedSubscriberBuilder<'a, 'static, 'static, Handler> {
        AdvancedSubscriberBuilder {
            session: self.session,
            key_expr: self.key_expr.map(|s| s.into_owned()),
            origin: self.origin,
            retransmission: self.retransmission,
            query_target: self.query_target,
            query_timeout: self.query_timeout,
            history: self.history,
            liveliness: self.liveliness,
            meta_key_expr: self.meta_key_expr.map(|s| s.map(|s| s.into_owned())),
            handler: self.handler,
        }
    }
}

#[zenoh_macros::unstable]
impl<Handler> Resolvable for AdvancedSubscriberBuilder<'_, '_, '_, Handler>
where
    Handler: IntoHandler<Sample>,
    Handler::Handler: Send,
{
    type To = ZResult<AdvancedSubscriber<Handler::Handler>>;
}

#[zenoh_macros::unstable]
impl<Handler> Wait for AdvancedSubscriberBuilder<'_, '_, '_, Handler>
where
    Handler: IntoHandler<Sample> + Send,
    Handler::Handler: Send,
{
    #[zenoh_macros::unstable]
    fn wait(self) -> <Self as Resolvable>::To {
        AdvancedSubscriber::new(self.with_static_keys())
    }
}

#[zenoh_macros::unstable]
impl<Handler> IntoFuture for AdvancedSubscriberBuilder<'_, '_, '_, Handler>
where
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
impl Resolvable for AdvancedSubscriberBuilder<'_, '_, '_, Callback<Sample>, true> {
    type To = ZResult<()>;
}

#[zenoh_macros::unstable]
impl Wait for AdvancedSubscriberBuilder<'_, '_, '_, Callback<Sample>, true> {
    #[zenoh_macros::unstable]
    fn wait(self) -> <Self as Resolvable>::To {
        let mut sub = AdvancedSubscriber::new(self.with_static_keys())?;
        sub.set_background_impl(true);
        Ok(())
    }
}

#[zenoh_macros::unstable]
impl IntoFuture for AdvancedSubscriberBuilder<'_, '_, '_, Callback<Sample>, true> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    #[zenoh_macros::unstable]
    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

#[zenoh_macros::unstable]
struct Period {
    timer: Timer,
    period: Duration,
}

#[zenoh_macros::unstable]
struct State {
    next_id: usize,
    global_pending_queries: u64,
    sequenced_states: HashMap<EntityGlobalId, SourceState<u32>>,
    timestamped_states: HashMap<ID, SourceState<Timestamp>>,
    session: Session,
    key_expr: KeyExpr<'static>,
    retransmission: bool,
    period: Option<Period>,
    history_depth: usize,
    query_target: QueryTarget,
    query_timeout: Duration,
    callback: Callback<Sample>,
    miss_handlers: HashMap<usize, Callback<Miss>>,
    token: Option<LivelinessToken>,
}

#[zenoh_macros::unstable]
impl State {
    #[zenoh_macros::unstable]
    fn register_miss_callback(&mut self, callback: Callback<Miss>) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.miss_handlers.insert(id, callback);
        id
    }
    #[zenoh_macros::unstable]
    fn unregister_miss_callback(&mut self, id: &usize) {
        self.miss_handlers.remove(id);
    }
}

macro_rules! spawn_periodic_queries {
    ($p:expr,$s:expr,$r:expr) => {{
        if let Some(period) = &$p.period {
            period.timer.add(TimedEvent::periodic(
                period.period,
                PeriodicQuery {
                    source_id: $s,
                    statesref: $r,
                },
            ))
        }
    }};
}

#[zenoh_macros::unstable]
struct SourceState<T> {
    last_delivered: Option<T>,
    pending_queries: u64,
    pending_samples: BTreeMap<T, Sample>,
}

/*
use zenoh_ext::{AdvancedSubscriberBuilderExt, HistoryConfig, RecoveryConfig};

let session = zenoh::open(zenoh::Config::default()).await.unwrap();
let subscriber = session
    .declare_subscriber("key/expression")
    .history(HistoryConfig::default().detect_late_publishers())
    .recovery(RecoveryConfig::default())
    .await
    .unwrap();

let miss_listener = subscriber.sample_miss_listener().await.unwrap();
loop {
    tokio::select! {
        sample = subscriber.recv_async() => {
            if let Ok(sample) = sample {
                // ...
            }
        },
        miss = miss_listener.recv_async() => {
            if let Ok(miss) = miss {
                // ...
            }
        },
    }
}
*/

/// The extension to [`Subscriber`](zenoh::pubsub::Subscriber) that provides advanced functionalities
///
/// The `AdvancedSubscriber` is constructed over a regular [`Subscriber`](zenoh::pubsub::Subscriber)
/// through [`advanced`](crate::AdvancedSubscriberBuilderExt::advanced) method or by using
/// any other method of [`AdvancedSubscriberBuilder`](crate::AdvancedSubscriberBuilder).
///
/// The `AdvancedSubscriber` works with [`AdvancedPublisher`](crate::AdvancedPublisher) to provide additional functionalities such as:
/// * missing samples detection using periodic queries or heartbeat subscription configurable with [`recovery`](crate::AdvancedSubscriberBuilder::recovery) method
/// * recovering missing samples, configured with [`history`](crate::AdvancedSubscriberBuilder::history) method
///   (max age and sample count, late joiner detection and requesting)
/// * liveliness-based subscriber detection with [`subscriber_detection`](crate::AdvancedSubscriberBuilder::subscriber_detection) method
///
/// # Examples
/// ```no_run
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh_ext::{AdvancedSubscriberBuilderExt, HistoryConfig, RecoveryConfig};
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let subscriber = session
///     .declare_subscriber("key/expression")
///     .history(HistoryConfig::default().detect_late_publishers())
///     .recovery(RecoveryConfig::default().heartbeat())
///     .subscriber_detection()
///     .await
///     .unwrap();
/// let miss_listener = subscriber.sample_miss_listener().await.unwrap();
/// loop {
///     tokio::select! {
///         sample = subscriber.recv_async() => {
///             if let Ok(sample) = sample {
///                 // ...
///             }
///         },
///         miss = miss_listener.recv_async() => {
///             if let Ok(miss) = miss {
///                 // ...
///             }
///         },
///     }
/// }
/// # }
/// ```
#[zenoh_macros::unstable]
pub struct AdvancedSubscriber<Receiver> {
    statesref: Arc<Mutex<State>>,
    subscriber: Subscriber<()>,
    receiver: Receiver,
    liveliness_subscriber: Option<Subscriber<()>>,
    heartbeat_subscriber: Option<Subscriber<()>>,
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
fn handle_sample(states: &mut State, sample: Sample) -> bool {
    if let (Some(source_id), Some(source_sn)) = (
        sample.source_info().source_id(),
        sample.source_info().source_sn(),
    ) {
        #[inline]
        fn deliver_and_flush(
            sample: Sample,
            mut source_sn: SourceSn,
            callback: &Callback<Sample>,
            state: &mut SourceState<u32>,
        ) {
            callback.call(sample);
            state.last_delivered = Some(source_sn);
            while let Some(sample) = state.pending_samples.remove(&(source_sn + 1)) {
                callback.call(sample);
                source_sn += 1;
                state.last_delivered = Some(source_sn);
            }
        }

        let entry = states.sequenced_states.entry(*source_id);
        let new = matches!(&entry, Entry::Vacant(_));
        let state = entry.or_insert(SourceState::<u32> {
            last_delivered: None,
            pending_queries: 0,
            pending_samples: BTreeMap::new(),
        });
        if state.last_delivered.is_none() && states.global_pending_queries != 0 {
            // Avoid going through the Map if history_depth == 1
            if states.history_depth == 1 {
                state.last_delivered = Some(source_sn);
                states.callback.call(sample);
            } else {
                state.pending_samples.insert(source_sn, sample);
                if state.pending_samples.len() >= states.history_depth {
                    if let Some((sn, sample)) = state.pending_samples.pop_first() {
                        deliver_and_flush(sample, sn, &states.callback, state);
                    }
                }
            }
        } else if state.last_delivered.is_some() && source_sn != state.last_delivered.unwrap() + 1 {
            if source_sn > state.last_delivered.unwrap() {
                if states.retransmission {
                    state.pending_samples.insert(source_sn, sample);
                } else {
                    tracing::info!(
                        "Sample missed: missed {} samples from {:?}.",
                        source_sn - state.last_delivered.unwrap() - 1,
                        source_id,
                    );
                    for miss_callback in states.miss_handlers.values() {
                        miss_callback.call(Miss {
                            source: *source_id,
                            nb: source_sn - state.last_delivered.unwrap() - 1,
                        });
                    }
                    states.callback.call(sample);
                    state.last_delivered = Some(source_sn);
                }
            }
        } else {
            deliver_and_flush(sample, source_sn, &states.callback, state);
        }
        new
    } else if let Some(timestamp) = sample.timestamp() {
        let entry = states.timestamped_states.entry(*timestamp.get_id());
        let state = entry.or_insert(SourceState::<Timestamp> {
            last_delivered: None,
            pending_queries: 0,
            pending_samples: BTreeMap::new(),
        });
        if state.last_delivered.map(|t| t < *timestamp).unwrap_or(true) {
            if (states.global_pending_queries == 0 && state.pending_queries == 0)
                || states.history_depth == 1
            {
                state.last_delivered = Some(*timestamp);
                states.callback.call(sample);
            } else {
                state.pending_samples.entry(*timestamp).or_insert(sample);
                if state.pending_samples.len() >= states.history_depth {
                    flush_timestamped_source(state, &states.callback);
                }
            }
        }
        false
    } else {
        states.callback.call(sample);
        false
    }
}

#[zenoh_macros::unstable]
fn seq_num_range(start: Option<u32>, end: Option<u32>) -> String {
    match (start, end) {
        (Some(start), Some(end)) => format!("_sn={start}..{end}"),
        (Some(start), None) => format!("_sn={start}.."),
        (None, Some(end)) => format!("_sn=..{end}"),
        (None, None) => "_sn=..".to_string(),
    }
}

#[zenoh_macros::unstable]
#[derive(Clone)]
struct PeriodicQuery {
    source_id: EntityGlobalId,
    statesref: Arc<Mutex<State>>,
}

#[zenoh_macros::unstable]
#[async_trait]
impl Timed for PeriodicQuery {
    async fn run(&mut self) {
        let mut lock = zlock!(self.statesref);
        let states = &mut *lock;
        if let Some(state) = states.sequenced_states.get_mut(&self.source_id) {
            state.pending_queries += 1;
            let query_expr = &states.key_expr
                / KE_ADV_PREFIX
                / KE_STAR
                / &self.source_id.zid().into_keyexpr()
                / &KeyExpr::try_from(self.source_id.eid().to_string()).unwrap()
                / KE_STARSTAR;
            let seq_num_range = seq_num_range(state.last_delivered.map(|s| s + 1), None);

            let session = states.session.clone();
            let key_expr = states.key_expr.clone().into_owned();
            let query_target = states.query_target;
            let query_timeout = states.query_timeout;

            tracing::trace!(
                "AdvancedSubscriber{{key_expr: {}}}: Querying undelivered samples {}?{}",
                states.key_expr,
                query_expr,
                seq_num_range
            );
            drop(lock);

            let handler = SequencedRepliesHandler {
                source_id: self.source_id,
                statesref: self.statesref.clone(),
            };
            let _ = session
                .get(Selector::from((query_expr, seq_num_range)))
                .callback({
                    move |r: Reply| {
                        if let Ok(s) = r.into_result() {
                            if key_expr.intersects(s.key_expr()) {
                                let states = &mut *zlock!(handler.statesref);
                                tracing::trace!(
                                    "AdvancedSubscriber{{key_expr: {}}}: Received reply with Sample{{info:{:?}, ts:{:?}}}",
                                    states.key_expr,
                                    s.source_info(),
                                    s.timestamp()
                                );
                                handle_sample(states, s);
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

#[zenoh_macros::unstable]
impl<Handler> AdvancedSubscriber<Handler> {
    fn new<H>(conf: AdvancedSubscriberBuilder<'_, '_, '_, H>) -> ZResult<Self>
    where
        H: IntoHandler<Sample, Handler = Handler> + Send,
    {
        let (callback, receiver) = conf.handler.into_handler();
        let key_expr = conf.key_expr?;
        let meta = match conf.meta_key_expr {
            Some(meta) => Some(meta?),
            None => None,
        };
        let retransmission = conf.retransmission;
        let query_target = conf.query_target;
        let query_timeout = conf.query_timeout;
        let session = conf.session.clone();
        let statesref = Arc::new(Mutex::new(State {
            next_id: 0,
            sequenced_states: HashMap::new(),
            timestamped_states: HashMap::new(),
            global_pending_queries: if conf.history.is_some() { 1 } else { 0 },
            session,
            period: retransmission.as_ref().and_then(|r| {
                let _rt = ZRuntime::Application.enter();
                r.periodic_queries.map(|p| Period {
                    timer: Timer::new(false),
                    period: p,
                })
            }),
            key_expr: key_expr.clone().into_owned(),
            retransmission: retransmission.is_some(),
            history_depth: conf
                .history
                .as_ref()
                .and_then(|h| h.sample_depth)
                .unwrap_or_default(),
            query_target: conf.query_target,
            query_timeout: conf.query_timeout,
            callback: callback.clone(),
            miss_handlers: HashMap::new(),
            token: None,
        }));

        let sub_callback = {
            let statesref = statesref.clone();
            let session = conf.session.clone();
            let key_expr = key_expr.clone().into_owned();

            move |s: Sample| {
                let mut lock = zlock!(statesref);
                let states = &mut *lock;
                let source_id = s.source_info().source_id().cloned();
                let new = handle_sample(states, s);

                if let Some(source_id) = source_id {
                    if new {
                        spawn_periodic_queries!(states, source_id, statesref.clone());
                    }
                    if let Some(state) = states.sequenced_states.get_mut(&source_id) {
                        if retransmission.is_some()
                            && state.pending_queries == 0
                            && !state.pending_samples.is_empty()
                        {
                            state.pending_queries += 1;
                            let query_expr = &key_expr
                                / KE_ADV_PREFIX
                                / KE_STAR
                                / &source_id.zid().into_keyexpr()
                                / &KeyExpr::try_from(source_id.eid().to_string()).unwrap()
                                / KE_STARSTAR;
                            let seq_num_range =
                                seq_num_range(state.last_delivered.map(|s| s + 1), None);
                            tracing::trace!(
                                "AdvancedSubscriber{{key_expr: {}}}: Querying missing samples {}?{}",
                                states.key_expr,
                                query_expr,
                                seq_num_range
                            );
                            drop(lock);
                            let handler = SequencedRepliesHandler {
                                source_id,
                                statesref: statesref.clone(),
                            };
                            let _ = session
                                .get(Selector::from((query_expr, seq_num_range)))
                                .callback({
                                    let key_expr = key_expr.clone().into_owned();
                                    move |r: Reply| {
                                        if let Ok(s) = r.into_result() {
                                            if key_expr.intersects(s.key_expr()) {
                                                let states = &mut *zlock!(handler.statesref);
                                                tracing::trace!("AdvancedSubscriber{{key_expr: {}}}: Received reply with Sample{{info:{:?}, ts:{:?}}}", states.key_expr, s.source_info(), s.timestamp());
                                                handle_sample(states, s);
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

        tracing::debug!("Create AdvancedSubscriber{{key_expr: {}}}", key_expr,);

        if let Some(historyconf) = conf.history.as_ref() {
            let handler = InitialRepliesHandler {
                statesref: statesref.clone(),
            };
            let mut params = Parameters::empty();
            if let Some(max) = historyconf.sample_depth {
                params.insert("_max", max.to_string());
            }
            if let Some(age) = historyconf.age {
                params.set_time_range(TimeRange {
                    start: TimeBound::Inclusive(TimeExpr::Now { offset_secs: -age }),
                    end: TimeBound::Unbounded,
                });
            }
            tracing::trace!(
                "AdvancedSubscriber{{key_expr: {}}} Querying historical samples {}?{}",
                key_expr,
                &key_expr / KE_ADV_PREFIX / KE_STARSTAR,
                params
            );
            let _ = conf
                .session
                .get(Selector::from((
                    &key_expr / KE_ADV_PREFIX / KE_STARSTAR,
                    params,
                )))
                .callback({
                    let key_expr = key_expr.clone().into_owned();
                    move |r: Reply| {
                        if let Ok(s) = r.into_result() {
                            if key_expr.intersects(s.key_expr()) {
                                let states = &mut *zlock!(handler.statesref);
                                tracing::trace!(
                                    "AdvancedSubscriber{{key_expr: {}}}: Received reply with Sample{{info:{:?}, ts:{:?}}}",
                                    states.key_expr,
                                    s.source_info(),
                                    s.timestamp()
                                );
                                handle_sample(states, s);
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

        let liveliness_subscriber = if let Some(historyconf) = conf.history.as_ref() {
            if historyconf.liveliness {
                let live_callback = {
                    let session = conf.session.clone();
                    let statesref = statesref.clone();
                    let key_expr = key_expr.clone().into_owned();
                    let historyconf = historyconf.clone();
                    move |s: Sample| {
                        if s.kind() == SampleKind::Put {
                            if let Ok(parsed) = ke_liveliness::parse(s.key_expr().as_keyexpr()) {
                                if let Ok(zid) = ZenohId::from_str(parsed.zid().as_str()) {
                                    // TODO : If we already have a state associated to this discovered source
                                    // we should query with the appropriate range to avoid unnecessary retransmissions
                                    if parsed.eid() == KE_UHLC {
                                        let mut lock = zlock!(statesref);
                                        let states = &mut *lock;
                                        tracing::trace!(
                                            "AdvancedSubscriber{{key_expr: {}}}: Detect late joiner publishers with zid={}",
                                            states.key_expr,
                                            parsed.zid().as_str()
                                        );
                                        let entry = states.timestamped_states.entry(ID::from(zid));
                                        let state = entry.or_insert(SourceState::<Timestamp> {
                                            last_delivered: None,
                                            pending_queries: 0,
                                            pending_samples: BTreeMap::new(),
                                        });
                                        state.pending_queries += 1;

                                        let mut params = Parameters::empty();
                                        if let Some(max) = historyconf.sample_depth {
                                            params.insert("_max", max.to_string());
                                        }
                                        if let Some(age) = historyconf.age {
                                            params.set_time_range(TimeRange {
                                                start: TimeBound::Inclusive(TimeExpr::Now {
                                                    offset_secs: -age,
                                                }),
                                                end: TimeBound::Unbounded,
                                            });
                                        }
                                        tracing::trace!(
                                            "AdvancedSubscriber{{key_expr: {}}}: Querying historical samples {}?{}",
                                            states.key_expr,
                                            s.key_expr(),
                                            params
                                        );
                                        drop(lock);

                                        let handler = TimestampedRepliesHandler {
                                            id: ID::from(zid),
                                            statesref: statesref.clone(),
                                            callback: callback.clone(),
                                        };
                                        let _ = session
                                            .get(Selector::from((s.key_expr(), params)))
                                            .callback({
                                                let key_expr = key_expr.clone().into_owned();
                                                move |r: Reply| {
                                                    if let Ok(s) = r.into_result() {
                                                        if key_expr.intersects(s.key_expr()) {
                                                            let states =
                                                                &mut *zlock!(handler.statesref);
                                                            tracing::trace!("AdvancedSubscriber{{key_expr: {}}}: Received reply with Sample{{info:{:?}, ts:{:?}}}", states.key_expr, s.source_info(), s.timestamp());
                                                            handle_sample(states, s);
                                                        }
                                                    }
                                                }
                                            })
                                            .consolidation(ConsolidationMode::None)
                                            .accept_replies(ReplyKeyExpr::Any)
                                            .target(query_target)
                                            .timeout(query_timeout)
                                            .wait();
                                    } else if let Ok(eid) =
                                        EntityId::from_str(parsed.eid().as_str())
                                    {
                                        let source_id = EntityGlobalId::new(zid, eid);
                                        let mut lock = zlock!(statesref);
                                        let states = &mut *lock;
                                        tracing::trace!(
                                            "AdvancedSubscriber{{key_expr: {}}}: Detect late joiner publishers with zid={}",
                                            states.key_expr,
                                            parsed.zid().as_str()
                                        );
                                        let entry = states.sequenced_states.entry(source_id);
                                        let new = matches!(&entry, Entry::Vacant(_));
                                        let state = entry.or_insert(SourceState::<u32> {
                                            last_delivered: None,
                                            pending_queries: 0,
                                            pending_samples: BTreeMap::new(),
                                        });
                                        state.pending_queries += 1;

                                        let mut params = Parameters::empty();
                                        if let Some(max) = historyconf.sample_depth {
                                            params.insert("_max", max.to_string());
                                        }
                                        if let Some(age) = historyconf.age {
                                            params.set_time_range(TimeRange {
                                                start: TimeBound::Inclusive(TimeExpr::Now {
                                                    offset_secs: -age,
                                                }),
                                                end: TimeBound::Unbounded,
                                            });
                                        }
                                        tracing::trace!(
                                            "AdvancedSubscriber{{key_expr: {}}}: Querying historical samples {}?{}",
                                            states.key_expr,
                                            s.key_expr(),
                                            params,
                                        );
                                        drop(lock);

                                        let handler = SequencedRepliesHandler {
                                            source_id,
                                            statesref: statesref.clone(),
                                        };
                                        let _ = session
                                            .get(Selector::from((s.key_expr(), params)))
                                            .callback({
                                                let key_expr = key_expr.clone().into_owned();
                                                move |r: Reply| {
                                                    if let Ok(s) = r.into_result() {
                                                        if key_expr.intersects(s.key_expr()) {
                                                            let states =
                                                                &mut *zlock!(handler.statesref);
                                                            tracing::trace!("AdvancedSubscriber{{key_expr: {}}}: Received reply with Sample{{info:{:?}, ts:{:?}}}", states.key_expr, s.source_info(), s.timestamp());
                                                            handle_sample(states, s);
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
                                            spawn_periodic_queries!(
                                                zlock!(statesref),
                                                source_id,
                                                statesref.clone()
                                            );
                                        }
                                    }
                                } else {
                                    let mut lock = zlock!(statesref);
                                    let states = &mut *lock;
                                    tracing::trace!(
                                        "AdvancedSubscriber{{key_expr: {}}}: Detect late joiner publishers with zid={}",
                                        states.key_expr,
                                        parsed.zid().as_str()
                                    );
                                    states.global_pending_queries += 1;

                                    let mut params = Parameters::empty();
                                    if let Some(max) = historyconf.sample_depth {
                                        params.insert("_max", max.to_string());
                                    }
                                    if let Some(age) = historyconf.age {
                                        params.set_time_range(TimeRange {
                                            start: TimeBound::Inclusive(TimeExpr::Now {
                                                offset_secs: -age,
                                            }),
                                            end: TimeBound::Unbounded,
                                        });
                                    }
                                    tracing::trace!(
                                        "AdvancedSubscriber{{key_expr: {}}}: Querying historical samples {}?{}",
                                        states.key_expr,
                                        s.key_expr(),
                                        params,
                                    );
                                    drop(lock);

                                    let handler = InitialRepliesHandler {
                                        statesref: statesref.clone(),
                                    };
                                    let _ = session
                                        .get(Selector::from((s.key_expr(), params)))
                                        .callback({
                                            let key_expr = key_expr.clone().into_owned();
                                            move |r: Reply| {
                                                if let Ok(s) = r.into_result() {
                                                    if key_expr.intersects(s.key_expr()) {
                                                        let states =
                                                            &mut *zlock!(handler.statesref);
                                                        tracing::trace!("AdvancedSubscriber{{key_expr: {}}}: Received reply with Sample{{info:{:?}, ts:{:?}}}", states.key_expr, s.source_info(), s.timestamp());
                                                        handle_sample(states, s);
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
                                    "AdvancedSubscriber{{}}: Received malformed liveliness token key expression: {}",
                                    s.key_expr()
                                );
                            }
                        }
                    }
                };

                tracing::debug!(
                    "AdvancedSubscriber{{key_expr: {}}}: Detect late joiner publishers on {}",
                    key_expr,
                    &key_expr / KE_ADV_PREFIX / KE_PUB / KE_STARSTAR
                );
                Some(
                    conf.session
                        .liveliness()
                        .declare_subscriber(&key_expr / KE_ADV_PREFIX / KE_PUB / KE_STARSTAR)
                        // .declare_subscriber(keformat!(ke_liveliness_all::formatter(), zid = 0, eid = 0, remaining = key_expr).unwrap())
                        .history(true)
                        .callback(live_callback)
                        .wait()?,
                )
            } else {
                None
            }
        } else {
            None
        };

        let heartbeat_subscriber = if retransmission.is_some_and(|r| r.heartbeat) {
            let ke_heartbeat_sub = &key_expr / KE_ADV_PREFIX / KE_PUB / KE_STARSTAR;
            let statesref = statesref.clone();
            tracing::debug!(
                "AdvancedSubscriber{{key_expr: {}}}: Enable heartbeat subscriber on {}",
                key_expr,
                ke_heartbeat_sub
            );
            let heartbeat_sub = conf
                .session
                .declare_subscriber(ke_heartbeat_sub)
                .callback(move |sample_hb| {
                    if sample_hb.kind() != SampleKind::Put {
                        return;
                    }

                    let heartbeat_keyexpr = sample_hb.key_expr().as_keyexpr();
                    let Ok(parsed_keyexpr) = ke_liveliness::parse(heartbeat_keyexpr) else {
                        return;
                    };
                    let source_id = {
                        let Ok(zid) = ZenohId::from_str(parsed_keyexpr.zid().as_str()) else {
                            return;
                        };
                        let Ok(eid) = EntityId::from_str(parsed_keyexpr.eid().as_str()) else {
                            return;
                        };
                        EntityGlobalId::new(zid, eid)
                    };

                    let Ok(heartbeat_sn) = z_deserialize::<u32>(sample_hb.payload()) else {
                        tracing::debug!(
                            "AdvancedSubscriber{{}}: Skipping invalid heartbeat payload on '{}'",
                            heartbeat_keyexpr
                        );
                        return;
                    };

                    let mut lock = zlock!(statesref);
                    let states = &mut *lock;
                    let entry = states.sequenced_states.entry(source_id);
                    if matches!(&entry, Entry::Vacant(_)) {
                        // NOTE: API does not allow both heartbeat and periodic_queries
                        spawn_periodic_queries!(states, source_id, statesref.clone());
                        if states.global_pending_queries > 0 {
                            tracing::trace!("AdvancedSubscriber{{key_expr: {}}}: Skipping heartbeat on '{}' from publisher that is currently being pulled by global query", states.key_expr, heartbeat_keyexpr);
                            return;
                        }
                    }

                    let state = entry.or_insert(SourceState::<u32> {
                        last_delivered: None,
                        pending_queries: 0,
                        pending_samples: BTreeMap::new(),
                    });

                    // check that it's not an old sn, and that there are no pending queries
                    if (state.last_delivered.is_none()
                        || state.last_delivered.is_some_and(|sn| heartbeat_sn > sn))
                        && state.pending_queries == 0
                    {
                        let seq_num_range = seq_num_range(
                            state.last_delivered.map(|s| s + 1),
                            Some(heartbeat_sn),
                        );

                        let session = states.session.clone();
                        let key_expr = states.key_expr.clone().into_owned();
                        let query_target = states.query_target;
                        let query_timeout = states.query_timeout;
                        state.pending_queries += 1;

                        tracing::trace!("AdvancedSubscriber{{key_expr: {}}}: Querying missing samples {}?{}", states.key_expr, heartbeat_keyexpr, seq_num_range);
                        drop(lock);

                        let handler = SequencedRepliesHandler {
                            source_id,
                            statesref: statesref.clone(),
                        };
                        let _ = session
                            .get(Selector::from((heartbeat_keyexpr, seq_num_range)))
                            .callback({
                                move |r: Reply| {
                                    if let Ok(s) = r.into_result() {
                                        if key_expr.intersects(s.key_expr()) {
                                            let states = &mut *zlock!(handler.statesref);
                                            tracing::trace!("AdvancedSubscriber{{key_expr: {}}}: Received reply with Sample{{info:{:?}, ts:{:?}}}", states.key_expr, s.source_info(), s.timestamp());
                                            handle_sample(states, s);
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
                })
                .allowed_origin(conf.origin)
                .wait()?;
            Some(heartbeat_sub)
        } else {
            None
        };

        if conf.liveliness {
            let suffix = KE_ADV_PREFIX
                / KE_SUB
                / &subscriber.id().zid().into_keyexpr()
                / &KeyExpr::try_from(subscriber.id().eid().to_string()).unwrap();
            let suffix = match meta {
                Some(meta) => suffix / &meta,
                // We need this empty chunk because of a routing matching bug
                _ => suffix / KE_EMPTY,
            };
            tracing::debug!(
                "AdvancedSubscriber{{key_expr: {}}}: Declare liveliness token {}",
                key_expr,
                &key_expr / &suffix,
            );
            let token = conf
                .session
                .liveliness()
                .declare_token(&key_expr / &suffix)
                .wait()?;
            zlock!(statesref).token = Some(token)
        }

        let reliable_subscriber = AdvancedSubscriber {
            statesref,
            subscriber,
            receiver,
            liveliness_subscriber,
            heartbeat_subscriber,
        };

        Ok(reliable_subscriber)
    }

    /// Returns the [`EntityGlobalId`] of this AdvancedSubscriber.
    #[zenoh_macros::unstable]
    pub fn id(&self) -> EntityGlobalId {
        self.subscriber.id()
    }

    /// Returns the [`KeyExpr`] this subscriber subscribes to.
    #[zenoh_macros::unstable]
    pub fn key_expr(&self) -> &KeyExpr<'static> {
        self.subscriber.key_expr()
    }

    /// Returns a reference to this subscriber's handler.
    /// An handler is anything that implements [`zenoh::handlers::IntoHandler`].
    /// The default handler is [`zenoh::handlers::DefaultHandler`].
    #[zenoh_macros::unstable]
    pub fn handler(&self) -> &Handler {
        &self.receiver
    }

    /// Returns a mutable reference to this subscriber's handler.
    /// An handler is anything that implements [`zenoh::handlers::IntoHandler`].
    /// The default handler is [`zenoh::handlers::DefaultHandler`].
    #[zenoh_macros::unstable]
    pub fn handler_mut(&mut self) -> &mut Handler {
        &mut self.receiver
    }

    /// Declares a listener to detect missed samples.
    ///
    /// Missed samples can only be detected from [`AdvancedPublisher`](crate::AdvancedPublisher) that
    /// enable [`sample_miss_detection`](crate::AdvancedPublisherBuilder::sample_miss_detection).
    #[zenoh_macros::unstable]
    pub fn sample_miss_listener(&self) -> SampleMissListenerBuilder<'_, DefaultHandler> {
        SampleMissListenerBuilder {
            statesref: &self.statesref,
            handler: DefaultHandler::default(),
        }
    }

    /// Declares a listener to detect matching publishers.
    ///
    /// Only [`AdvancedPublisher`](crate::AdvancedPublisher) that enable
    /// [`publisher_detection`](crate::AdvancedPublisherBuilder::publisher_detection) can be detected.
    #[zenoh_macros::unstable]
    pub fn detect_publishers(&self) -> LivelinessSubscriberBuilder<'_, '_, DefaultHandler> {
        self.subscriber
            .session()
            .liveliness()
            .declare_subscriber(self.subscriber.key_expr() / KE_ADV_PREFIX / KE_PUB / KE_STARSTAR)
    }

    /// Undeclares this AdvancedSubscriber
    #[inline]
    #[zenoh_macros::unstable]
    pub fn undeclare(self) -> impl Resolve<ZResult<()>> {
        tracing::debug!(
            "AdvancedSubscriber{{key_expr: {}}}: Undeclare",
            self.key_expr()
        );
        self.subscriber.undeclare()
    }

    fn set_background_impl(&mut self, background: bool) {
        self.subscriber.set_background(background);
        if let Some(mut liveliness_sub) = self.liveliness_subscriber.take() {
            liveliness_sub.set_background(background);
        }
        if let Some(mut heartbeat_sub) = self.heartbeat_subscriber.take() {
            heartbeat_sub.set_background(background);
        }
    }

    #[zenoh_macros::internal]
    pub fn set_background(&mut self, background: bool) {
        self.set_background_impl(background)
    }
}

#[zenoh_macros::unstable]
#[inline]
fn flush_sequenced_source(
    state: &mut SourceState<u32>,
    callback: &Callback<Sample>,
    source_id: &EntityGlobalId,
    miss_handlers: &HashMap<usize, Callback<Miss>>,
) {
    if state.pending_queries == 0 && !state.pending_samples.is_empty() {
        let mut pending_samples = BTreeMap::new();
        std::mem::swap(&mut state.pending_samples, &mut pending_samples);
        for (seq_num, sample) in pending_samples {
            match state.last_delivered {
                None => {
                    state.last_delivered = Some(seq_num);
                    callback.call(sample);
                }
                Some(last) if seq_num == last + 1 => {
                    state.last_delivered = Some(seq_num);
                    callback.call(sample);
                }
                Some(last) if seq_num > last + 1 => {
                    tracing::warn!(
                        "Sample missed: missed {} samples from {:?}.",
                        seq_num - last - 1,
                        source_id,
                    );
                    for miss_callback in miss_handlers.values() {
                        miss_callback.call(Miss {
                            source: *source_id,
                            nb: seq_num - last - 1,
                        })
                    }
                    state.last_delivered = Some(seq_num);
                    callback.call(sample);
                }
                _ => {
                    // duplicate
                }
            }
        }
    }
}

#[zenoh_macros::unstable]
#[inline]
fn flush_timestamped_source(state: &mut SourceState<Timestamp>, callback: &Callback<Sample>) {
    if state.pending_queries == 0 && !state.pending_samples.is_empty() {
        let mut pending_samples = BTreeMap::new();
        std::mem::swap(&mut state.pending_samples, &mut pending_samples);
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
}

#[zenoh_macros::unstable]
impl Drop for InitialRepliesHandler {
    fn drop(&mut self) {
        let states = &mut *zlock!(self.statesref);
        states.global_pending_queries = states.global_pending_queries.saturating_sub(1);
        tracing::trace!(
            "AdvancedSubscriber{{key_expr: {}}}: Flush initial replies",
            states.key_expr
        );

        if states.global_pending_queries == 0 {
            for (source_id, state) in states.sequenced_states.iter_mut() {
                flush_sequenced_source(state, &states.callback, source_id, &states.miss_handlers);
                spawn_periodic_queries!(states, *source_id, self.statesref.clone());
            }
            for state in states.timestamped_states.values_mut() {
                flush_timestamped_source(state, &states.callback);
            }
        }
    }
}

#[zenoh_macros::unstable]
#[derive(Clone)]
struct SequencedRepliesHandler {
    source_id: EntityGlobalId,
    statesref: Arc<Mutex<State>>,
}

#[zenoh_macros::unstable]
impl Drop for SequencedRepliesHandler {
    fn drop(&mut self) {
        let states = &mut *zlock!(self.statesref);
        if let Some(state) = states.sequenced_states.get_mut(&self.source_id) {
            state.pending_queries = state.pending_queries.saturating_sub(1);
            if states.global_pending_queries == 0 {
                tracing::trace!(
                    "AdvancedSubscriber{{key_expr: {}}}: Flush sequenced samples",
                    states.key_expr
                );
                flush_sequenced_source(
                    state,
                    &states.callback,
                    &self.source_id,
                    &states.miss_handlers,
                )
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
                tracing::trace!(
                    "AdvancedSubscriber{{key_expr: {}}}: Flush timestamped samples",
                    states.key_expr
                );
                flush_timestamped_source(state, &self.callback);
            }
        }
    }
}

/// A struct that represent missed samples.
#[zenoh_macros::unstable]
#[derive(Debug, Clone)]
pub struct Miss {
    source: EntityGlobalId,
    nb: u32,
}

impl Miss {
    /// The source of missed samples.
    pub fn source(&self) -> EntityGlobalId {
        self.source
    }

    /// The number of missed samples.
    pub fn nb(&self) -> u32 {
        self.nb
    }
}

impl CallbackParameter for Miss {
    type Message<'a> = Self;

    fn from_message(msg: Self::Message<'_>) -> Self {
        msg
    }
}

/// A listener to detect missed samples.
///
/// Missed samples can only be detected from [`AdvancedPublisher`](crate::AdvancedPublisher) that
/// enable [`sample_miss_detection`](crate::AdvancedPublisherBuilder::sample_miss_detection).
#[zenoh_macros::unstable]
pub struct SampleMissListener<Handler> {
    id: usize,
    statesref: Arc<Mutex<State>>,
    handler: Handler,
    undeclare_on_drop: bool,
}

#[zenoh_macros::unstable]
impl<Handler> SampleMissListener<Handler> {
    #[inline]
    pub fn undeclare(self) -> SampleMissHandlerUndeclaration<Handler>
    where
        Handler: Send,
    {
        // self.undeclare_inner(())
        SampleMissHandlerUndeclaration(self)
    }

    fn undeclare_impl(&mut self) -> ZResult<()> {
        // set the flag first to avoid double panic if this function panic
        self.undeclare_on_drop = false;
        zlock!(self.statesref).unregister_miss_callback(&self.id);
        Ok(())
    }

    #[zenoh_macros::internal]
    pub fn set_background(&mut self, background: bool) {
        self.undeclare_on_drop = !background;
    }
}

#[cfg(feature = "unstable")]
impl<Handler> Drop for SampleMissListener<Handler> {
    fn drop(&mut self) {
        if self.undeclare_on_drop {
            if let Err(error) = self.undeclare_impl() {
                tracing::error!(error);
            }
        }
    }
}

// #[zenoh_macros::unstable]
// impl<Handler: Send> UndeclarableSealed<()> for SampleMissHandler<Handler> {
//     type Undeclaration = SampleMissHandlerUndeclaration<Handler>;

//     fn undeclare_inner(self, _: ()) -> Self::Undeclaration {
//         SampleMissHandlerUndeclaration(self)
//     }
// }

#[zenoh_macros::unstable]
impl<Handler> std::ops::Deref for SampleMissListener<Handler> {
    type Target = Handler;

    fn deref(&self) -> &Self::Target {
        &self.handler
    }
}
#[zenoh_macros::unstable]
impl<Handler> std::ops::DerefMut for SampleMissListener<Handler> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.handler
    }
}

/// A [`Resolvable`] returned by [`SampleMissListener::undeclare`]
#[zenoh_macros::unstable]
pub struct SampleMissHandlerUndeclaration<Handler>(SampleMissListener<Handler>);

#[zenoh_macros::unstable]
impl<Handler> Resolvable for SampleMissHandlerUndeclaration<Handler> {
    type To = ZResult<()>;
}

#[zenoh_macros::unstable]
impl<Handler> Wait for SampleMissHandlerUndeclaration<Handler> {
    fn wait(mut self) -> <Self as Resolvable>::To {
        self.0.undeclare_impl()
    }
}

#[zenoh_macros::unstable]
impl<Handler> IntoFuture for SampleMissHandlerUndeclaration<Handler> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

/// A builder for initializing a [`SampleMissListener`].
#[zenoh_macros::unstable]
pub struct SampleMissListenerBuilder<'a, Handler, const BACKGROUND: bool = false> {
    statesref: &'a Arc<Mutex<State>>,
    handler: Handler,
}

#[zenoh_macros::unstable]
impl<'a> SampleMissListenerBuilder<'a, DefaultHandler> {
    /// Receive the sample miss notification with a callback.
    #[inline]
    #[zenoh_macros::unstable]
    pub fn callback<F>(self, callback: F) -> SampleMissListenerBuilder<'a, Callback<Miss>>
    where
        F: Fn(Miss) + Send + Sync + 'static,
    {
        self.with(Callback::from(callback))
    }

    /// Receive the sample miss notification with a mutable callback.
    #[inline]
    #[zenoh_macros::unstable]
    pub fn callback_mut<F>(self, callback: F) -> SampleMissListenerBuilder<'a, Callback<Miss>>
    where
        F: FnMut(Miss) + Send + Sync + 'static,
    {
        self.callback(zenoh::handlers::locked(callback))
    }

    /// Receive the sample miss notification with a [`Handler`](IntoHandler).
    #[inline]
    #[zenoh_macros::unstable]
    pub fn with<Handler>(self, handler: Handler) -> SampleMissListenerBuilder<'a, Handler>
    where
        Handler: IntoHandler<Miss>,
    {
        SampleMissListenerBuilder {
            statesref: self.statesref,
            handler,
        }
    }
}

#[zenoh_macros::unstable]
impl<'a> SampleMissListenerBuilder<'a, Callback<Miss>> {
    /// Make the sample miss notification run in the background until the advanced subscriber is undeclared.
    ///
    /// Background builder doesn't return a `SampleMissHandler` object anymore.
    #[zenoh_macros::unstable]
    pub fn background(self) -> SampleMissListenerBuilder<'a, Callback<Miss>, true> {
        SampleMissListenerBuilder {
            statesref: self.statesref,
            handler: self.handler,
        }
    }
}

#[zenoh_macros::unstable]
impl<Handler> Resolvable for SampleMissListenerBuilder<'_, Handler>
where
    Handler: IntoHandler<Miss> + Send,
    Handler::Handler: Send,
{
    type To = ZResult<SampleMissListener<Handler::Handler>>;
}

#[zenoh_macros::unstable]
impl<Handler> Wait for SampleMissListenerBuilder<'_, Handler>
where
    Handler: IntoHandler<Miss> + Send,
    Handler::Handler: Send,
{
    #[zenoh_macros::unstable]
    fn wait(self) -> <Self as Resolvable>::To {
        let (callback, handler) = self.handler.into_handler();
        let id = zlock!(self.statesref).register_miss_callback(callback);
        Ok(SampleMissListener {
            id,
            statesref: self.statesref.clone(),
            handler,
            undeclare_on_drop: true,
        })
    }
}

#[zenoh_macros::unstable]
impl<Handler> IntoFuture for SampleMissListenerBuilder<'_, Handler>
where
    Handler: IntoHandler<Miss> + Send,
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
impl Resolvable for SampleMissListenerBuilder<'_, Callback<Miss>, true> {
    type To = ZResult<()>;
}

#[zenoh_macros::unstable]
impl Wait for SampleMissListenerBuilder<'_, Callback<Miss>, true> {
    #[zenoh_macros::unstable]
    fn wait(self) -> <Self as Resolvable>::To {
        let (callback, _) = self.handler.into_handler();
        zlock!(self.statesref).register_miss_callback(callback);
        Ok(())
    }
}

#[zenoh_macros::unstable]
impl IntoFuture for SampleMissListenerBuilder<'_, Callback<Miss>, true> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    #[zenoh_macros::unstable]
    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}
