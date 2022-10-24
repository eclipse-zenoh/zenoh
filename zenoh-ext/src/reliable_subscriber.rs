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
use std::collections::HashMap;
use std::convert::TryInto;
use std::future::Ready;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use zenoh::buffers::reader::{HasReader, Reader};
use zenoh::buffers::ZBuf;
use zenoh::handlers::{locked, DefaultHandler};
use zenoh::prelude::r#async::*;
use zenoh::query::{QueryConsolidation, QueryTarget, Reply, ReplyKeyExpr};
use zenoh::subscriber::{Reliability, Subscriber};
use zenoh::Result as ZResult;
use zenoh_core::{zlock, AsyncResolve, Resolvable, SyncResolve};
use zenoh_protocol::io::ZBufCodec;

/// The builder of ReliableSubscriber, allowing to configure it.
pub struct ReliableSubscriberBuilder<'b, Handler> {
    session: Arc<Session>,
    key_expr: ZResult<KeyExpr<'b>>,
    reliability: Reliability,
    origin: Locality,
    query_selector: Option<ZResult<Selector<'b>>>,
    query_target: QueryTarget,
    query_consolidation: QueryConsolidation,
    query_timeout: Duration,
    handler: Handler,
}

impl<'b> ReliableSubscriberBuilder<'b, DefaultHandler> {
    pub(crate) fn new(
        session: Arc<Session>,
        key_expr: ZResult<KeyExpr<'b>>,
    ) -> ReliableSubscriberBuilder<'b, DefaultHandler> {
        // By default query all matching publication caches and storages
        let query_target = QueryTarget::All;

        // By default no query consolidation, to receive more than 1 sample per-resource
        // (if history of publications is available)
        let query_consolidation = QueryConsolidation::from(zenoh::query::ConsolidationMode::None);

        ReliableSubscriberBuilder {
            session,
            key_expr,
            reliability: Reliability::default(),
            origin: Locality::default(),
            query_selector: None,
            query_target,
            query_consolidation,
            query_timeout: Duration::from_secs(10),
            handler: DefaultHandler,
        }
    }

    /// Add callback to ReliableSubscriber.
    #[inline]
    pub fn callback<Callback>(self, callback: Callback) -> ReliableSubscriberBuilder<'b, Callback>
    where
        Callback: Fn(Sample) + Send + Sync + 'static,
    {
        let ReliableSubscriberBuilder {
            session,
            key_expr,
            reliability,
            origin,
            query_selector,
            query_target,
            query_consolidation,
            query_timeout,
            handler: _,
        } = self;
        ReliableSubscriberBuilder {
            session,
            key_expr,
            reliability,
            origin,
            query_selector,
            query_target,
            query_consolidation,
            query_timeout,
            handler: callback,
        }
    }

    /// Add callback to `ReliableSubscriber`.
    ///
    /// Using this guarantees that your callback will never be called concurrently.
    /// If your callback is also accepted by the [`callback`](ReliableSubscriberBuilder::callback) method, we suggest you use it instead of `callback_mut`
    #[inline]
    pub fn callback_mut<CallbackMut>(
        self,
        callback: CallbackMut,
    ) -> ReliableSubscriberBuilder<'b, impl Fn(Sample) + Send + Sync + 'static>
    where
        CallbackMut: FnMut(Sample) + Send + Sync + 'static,
    {
        self.callback(locked(callback))
    }

    /// Make the built ReliableSubscriber a [`ReliableSubscriber`](ReliableSubscriber).
    #[inline]
    pub fn with<Handler>(self, handler: Handler) -> ReliableSubscriberBuilder<'b, Handler>
    where
        Handler: zenoh::prelude::IntoCallbackReceiverPair<'static, Sample>,
    {
        let ReliableSubscriberBuilder {
            session,
            key_expr,
            reliability,
            origin,
            query_selector,
            query_target,
            query_consolidation,
            query_timeout,
            handler: _,
        } = self;
        ReliableSubscriberBuilder {
            session,
            key_expr,
            reliability,
            origin,
            query_selector,
            query_target,
            query_consolidation,
            query_timeout,
            handler,
        }
    }
}
impl<'b, Handler> ReliableSubscriberBuilder<'b, Handler> {
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
    /// to the ones that have the given [`Locality`](crate::prelude::Locality).
    #[zenoh_core::unstable]
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

    fn with_static_keys(self) -> ReliableSubscriberBuilder<'static, Handler> {
        ReliableSubscriberBuilder {
            session: self.session,
            key_expr: self.key_expr.map(|s| s.into_owned()),
            reliability: self.reliability,
            origin: self.origin,
            query_selector: self.query_selector.map(|s| s.map(|s| s.into_owned())),
            query_target: self.query_target,
            query_consolidation: self.query_consolidation,
            query_timeout: self.query_timeout,
            handler: self.handler,
        }
    }
}

impl<'a, Handler> Resolvable for ReliableSubscriberBuilder<'a, Handler>
where
    Handler: IntoCallbackReceiverPair<'static, Sample>,
    Handler::Receiver: Send,
{
    type To = ZResult<ReliableSubscriber<'a, Handler::Receiver>>;
}

impl<Handler> SyncResolve for ReliableSubscriberBuilder<'_, Handler>
where
    Handler: IntoCallbackReceiverPair<'static, Sample> + Send,
    Handler::Receiver: Send,
{
    fn res_sync(self) -> <Self as Resolvable>::To {
        ReliableSubscriber::new(self.with_static_keys())
    }
}

impl<Handler> AsyncResolve for ReliableSubscriberBuilder<'_, Handler>
where
    Handler: IntoCallbackReceiverPair<'static, Sample> + Send,
    Handler::Receiver: Send,
{
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}
struct InnerState {
    last_seq_num: Option<ZInt>,
    pending_queries: u64,
    pending_samples: HashMap<ZInt, Sample>,
}

pub struct ReliableSubscriber<'a, Receiver> {
    _subscriber: Subscriber<'a, ()>,
    receiver: Receiver,
}
impl<Receiver> std::ops::Deref for ReliableSubscriber<'_, Receiver> {
    type Target = Receiver;
    fn deref(&self) -> &Self::Target {
        &self.receiver
    }
}
impl<Receiver> std::ops::DerefMut for ReliableSubscriber<'_, Receiver> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.receiver
    }
}

fn handle_sample(
    states: &mut HashMap<ZenohId, InnerState>,
    sample: Sample,
    callback: &Arc<dyn Fn(Sample) + Send + Sync>,
) -> ZenohId {
    let mut buf = sample.value.payload.reader();
    let id = buf.read_zid().unwrap(); //TODO
    let seq_num = buf.read_zint().unwrap(); //TODO
    let mut payload = ZBuf::default();
    buf.read_into_zbuf(&mut payload, buf.remaining());
    let value = Value::new(payload).encoding(sample.encoding.clone());
    let s = Sample::new(sample.key_expr, value);
    let state = states.entry(id).or_insert(InnerState {
        last_seq_num: None,
        pending_queries: 0,
        pending_samples: HashMap::new(),
    });
    if state.last_seq_num.is_some() && seq_num != state.last_seq_num.unwrap() + 1 {
        if seq_num > state.last_seq_num.unwrap() {
            state.pending_samples.insert(seq_num, s);
        }
    } else {
        callback(s);
        let mut last_seq_num = seq_num;
        state.last_seq_num = Some(last_seq_num);
        while let Some(s) = state.pending_samples.remove(&(last_seq_num + 1)) {
            callback(s);
            last_seq_num += 1;
            state.last_seq_num = Some(last_seq_num);
        }
    }
    id
}

fn seq_num_range(start: Option<ZInt>, end: Option<ZInt>) -> String {
    match (start, end) {
        (Some(start), Some(end)) => format!("_sn={}..{}", start, end),
        (Some(start), None) => format!("_sn={}..", start),
        (None, Some(end)) => format!("_sn=..{}", end),
        (None, None) => "_sn=..".to_string(),
    }
}

impl<'a, Receiver> ReliableSubscriber<'a, Receiver> {
    fn new<Handler>(conf: ReliableSubscriberBuilder<'a, Handler>) -> ZResult<Self>
    where
        Handler: IntoCallbackReceiverPair<'static, Sample, Receiver = Receiver> + Send,
    {
        let statesref = Arc::new(Mutex::new(HashMap::new()));
        let (callback, receiver) = conf.handler.into_cb_receiver_pair();
        let key_expr = conf.key_expr?;
        let query_timeout = conf.query_timeout;

        let sub_callback = {
            let session = conf.session.clone();
            let callback = callback.clone();
            let key_expr = key_expr.clone().into_owned();

            move |s: Sample| {
                let mut states = zlock!(statesref);
                let id = handle_sample(&mut states, s, &callback);
                if let Some(state) = states.get_mut(&id) {
                    if state.pending_queries == 0 && !state.pending_samples.is_empty() {
                        state.pending_queries += 1;
                        let key_expr = (&id.into_keyexpr()) / &key_expr;
                        let seq_num_range =
                            seq_num_range(Some(state.last_seq_num.unwrap() + 1), None);
                        drop(states);
                        let handler = RepliesHandler {
                            id,
                            statesref: statesref.clone(),
                            callback: callback.clone(),
                        };
                        let _ = session
                            .get(Selector::from(key_expr).with_parameters(&seq_num_range))
                            .callback({
                                move |r: Reply| {
                                    if let Ok(s) = r.sample {
                                        let states = &mut zlock!(handler.statesref);
                                        handle_sample(states, s, &handler.callback);
                                    }
                                }
                            })
                            .consolidation(ConsolidationMode::None)
                            .accept_replies(ReplyKeyExpr::Any)
                            .timeout(query_timeout)
                            .res_sync();
                    }
                }
            }
        };

        let subscriber = conf
            .session
            .declare_subscriber(&key_expr)
            .callback(sub_callback)
            .reliability(conf.reliability)
            .allowed_origin(conf.origin)
            .res_sync()?;

        let reliable_subscriber = ReliableSubscriber {
            _subscriber: subscriber,
            receiver,
        };

        Ok(reliable_subscriber)
    }

    /// Close this ReliableSubscriber
    #[inline]
    pub fn close(self) -> impl Resolve<ZResult<()>> + 'a {
        self._subscriber.undeclare()
    }
}

#[derive(Clone)]
struct RepliesHandler {
    id: ZenohId,
    statesref: Arc<Mutex<HashMap<ZenohId, InnerState>>>,
    callback: Arc<dyn Fn(Sample) + Send + Sync>,
}

impl Drop for RepliesHandler {
    fn drop(&mut self) {
        let mut states = zlock!(self.statesref);
        if let Some(state) = states.get_mut(&self.id) {
            state.pending_queries -= 1;
            if !state.pending_samples.is_empty() {
                log::error!("Sample missed: unable to retrieve some missing samples.");
                let mut pending_samples = state
                    .pending_samples
                    .drain()
                    .collect::<Vec<(ZInt, Sample)>>();
                pending_samples.sort_by_key(|(k, _s)| *k);
                for (seq_num, sample) in pending_samples {
                    state.last_seq_num = Some(seq_num);
                    (self.callback)(sample);
                }
            }
        }
    }
}
