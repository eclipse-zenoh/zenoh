//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
use super::info::*;
use super::queryable::EVAL;
use super::*;
use async_std::sync::Arc;
use async_std::task;
use flume::{bounded, Sender};
use log::{error, trace, warn};
use net::protocol::{
    core::{
        queryable, rname, AtomicZInt, CongestionControl, QueryConsolidation, QueryTarget,
        QueryableInfo, ResKey, ResourceId, ZInt,
    },
    io::ZBuf,
    proto::{DataInfo, Options, RoutingContext},
    session::Primitives,
};
use net::routing::face::Face;
use net::runtime::Runtime;
use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::RwLock;
use std::task::{Context, Poll};
use std::time::Duration;
use uhlc::HLC;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::sync::{zpinbox, Runnable};

zconfigurable! {
    static ref API_DATA_RECEPTION_CHANNEL_SIZE: usize = 256;
    static ref API_QUERY_RECEPTION_CHANNEL_SIZE: usize = 256;
    static ref API_REPLY_EMISSION_CHANNEL_SIZE: usize = 256;
    static ref API_REPLY_RECEPTION_CHANNEL_SIZE: usize = 256;
    static ref API_OPEN_SESSION_DELAY: u64 = 500;
}

pub(crate) struct SessionState {
    primitives: Option<Arc<Face>>, // @TODO replace with MaybeUninit ??
    rid_counter: AtomicUsize,      // @TODO: manage rollover and uniqueness
    qid_counter: AtomicZInt,
    decl_id_counter: AtomicUsize,
    local_resources: HashMap<ResourceId, Resource>,
    remote_resources: HashMap<ResourceId, Resource>,
    publishers: HashMap<Id, Arc<PublisherState>>,
    subscribers: HashMap<Id, Arc<SubscriberState>>,
    local_subscribers: HashMap<Id, Arc<SubscriberState>>,
    queryables: HashMap<Id, Arc<QueryableState>>,
    queries: HashMap<ZInt, QueryState>,
    local_routing: bool,
    join_subscriptions: Vec<String>,
    join_publications: Vec<String>,
}

impl SessionState {
    pub(crate) fn new(
        local_routing: bool,
        join_subscriptions: Vec<String>,
        join_publications: Vec<String>,
    ) -> SessionState {
        SessionState {
            primitives: None,
            rid_counter: AtomicUsize::new(1), // Note: start at 1 because 0 is reserved for NO_RESOURCE
            qid_counter: AtomicZInt::new(0),
            decl_id_counter: AtomicUsize::new(0),
            local_resources: HashMap::new(),
            remote_resources: HashMap::new(),
            publishers: HashMap::new(),
            subscribers: HashMap::new(),
            local_subscribers: HashMap::new(),
            queryables: HashMap::new(),
            queries: HashMap::new(),
            local_routing,
            join_subscriptions,
            join_publications,
        }
    }
}

impl SessionState {
    #[inline]
    fn get_local_res(&self, rid: &ResourceId) -> Option<&Resource> {
        self.local_resources.get(rid)
    }

    #[inline]
    fn get_remote_res(&self, rid: &ResourceId) -> Option<&Resource> {
        match self.remote_resources.get(rid) {
            None => self.local_resources.get(rid),
            res => res,
        }
    }

    #[inline]
    fn get_res(&self, rid: &ResourceId, local: bool) -> Option<&Resource> {
        if local {
            self.get_local_res(rid)
        } else {
            self.get_remote_res(rid)
        }
    }

    #[inline]
    fn localid_to_resname(&self, rid: &ResourceId) -> ZResult<String> {
        match self.local_resources.get(&rid) {
            Some(res) => Ok(res.name.clone()),
            None => zerror!(ZErrorKind::UnkownResourceId {
                rid: format!("{}", rid)
            }),
        }
    }

    #[inline]
    fn rid_to_resname(&self, rid: &ResourceId) -> ZResult<String> {
        match self.remote_resources.get(&rid) {
            Some(res) => Ok(res.name.clone()),
            None => self.localid_to_resname(rid),
        }
    }

    pub fn remotekey_to_resname(&self, reskey: &ResKey) -> ZResult<String> {
        use super::ResKey::*;
        match reskey {
            RName(name) => Ok(name.to_string()),
            RId(rid) => self.rid_to_resname(&rid),
            RIdWithSuffix(rid, suffix) => Ok(self.rid_to_resname(&rid)? + suffix),
        }
    }

    pub fn localkey_to_resname(&self, reskey: &ResKey) -> ZResult<String> {
        use super::ResKey::*;
        match reskey {
            RName(name) => Ok(name.to_string()),
            RId(rid) => self.localid_to_resname(&rid),
            RIdWithSuffix(rid, suffix) => Ok(self.localid_to_resname(&rid)? + suffix),
        }
    }

    pub fn reskey_to_resname(&self, reskey: &ResKey, local: bool) -> ZResult<String> {
        if local {
            self.localkey_to_resname(reskey)
        } else {
            self.remotekey_to_resname(reskey)
        }
    }
}

impl fmt::Debug for SessionState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "SessionState{{ subscribers: {} }}",
            self.subscribers.len()
        )
    }
}

struct Resource {
    pub(crate) name: String,
    pub(crate) subscribers: Vec<Arc<SubscriberState>>,
    pub(crate) local_subscribers: Vec<Arc<SubscriberState>>,
}

impl Resource {
    pub(crate) fn new(name: String) -> Self {
        Resource {
            name,
            subscribers: vec![],
            local_subscribers: vec![],
        }
    }
}

derive_zfuture! {
    /// `PublisherBuilder` is a builder for initializing a [Publisher](Publisher).
    #[derive(Debug, Clone)]
    pub struct PublisherBuilder<'a, 'b> {
        session: &'a Session,
        reskey: ResKey<'b>,
    }
}

impl<'a> Runnable for PublisherBuilder<'a, '_> {
    type Output = ZResult<Publisher<'a>>;

    fn run(&mut self) -> Self::Output {
        trace!("publishing({:?})", self.reskey);
        let mut state = zwrite!(self.session.state);
        let id = state.decl_id_counter.fetch_add(1, Ordering::SeqCst);
        let resname = state.localkey_to_resname(&self.reskey)?;
        let pub_state = Arc::new(PublisherState {
            id,
            reskey: self.reskey.to_owned(),
        });
        let declared_pub = match state
            .join_publications
            .iter()
            .find(|s| rname::include(s, &resname))
        {
            Some(join_pub) => {
                let joined_pub = state.publishers.values().any(|p| {
                    rname::include(join_pub, &state.localkey_to_resname(&p.reskey).unwrap())
                });
                (!joined_pub).then(|| join_pub.clone().into())
            }
            None => {
                let twin_pub = state.publishers.values().any(|p| {
                    state.localkey_to_resname(&p.reskey).unwrap()
                        == state.localkey_to_resname(&pub_state.reskey).unwrap()
                });
                (!twin_pub).then(|| self.reskey.clone())
            }
        };

        state.publishers.insert(id, pub_state.clone());

        if let Some(res) = declared_pub {
            let primitives = state.primitives.as_ref().unwrap().clone();
            drop(state);
            primitives.decl_publisher(&res, None);
        }

        Ok(Publisher {
            session: self.session,
            state: pub_state,
            alive: true,
        })
    }
}

derive_zfuture! {
    /// `SubscriberBuilder` is a builder for initializing a [Subscriber](Subscriber).
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// let subscriber = session
    ///     .subscribe("/resource/name")
    ///     .best_effort()
    ///     .pull_mode()
    ///     .await
    ///     .unwrap();
    /// # })
    /// ```
    #[derive(Debug, Clone)]
    pub struct SubscriberBuilder<'a, 'b> {
        session: &'a Session,
        reskey: ResKey<'b>,
        info: SubInfo,
        local: bool,
    }
}

impl<'a, 'b> SubscriberBuilder<'a, 'b> {
    /// Make the built Subscruber a CallbackSubscriber.
    #[inline]
    pub fn callback<DataHandler>(self, handler: DataHandler) -> CallbackSubscriberBuilder<'a, 'b>
    where
        DataHandler: FnMut(Sample) + Send + Sync + 'static,
    {
        CallbackSubscriberBuilder {
            session: self.session,
            reskey: self.reskey,
            info: self.info,
            local: self.local,
            handler: Arc::new(RwLock::new(handler)),
        }
    }

    /// Change the subscription reliability.
    #[inline]
    pub fn reliability(mut self, reliability: Reliability) -> Self {
        self.info.reliability = reliability;
        self
    }

    /// Change the subscription reliability to Reliable.
    #[inline]
    pub fn reliable(mut self) -> Self {
        self.info.reliability = Reliability::Reliable;
        self
    }

    /// Change the subscription reliability to BestEffort.
    #[inline]
    pub fn best_effort(mut self) -> Self {
        self.info.reliability = Reliability::BestEffort;
        self
    }

    /// Change the subscription mode.
    #[inline]
    pub fn mode(mut self, mode: SubMode) -> Self {
        self.info.mode = mode;
        self
    }

    /// Change the subscription mode to Push.
    #[inline]
    pub fn push_mode(mut self) -> Self {
        self.info.mode = SubMode::Push;
        self.info.period = None;
        self
    }

    /// Change the subscription mode to Pull.
    #[inline]
    pub fn pull_mode(mut self) -> Self {
        self.info.mode = SubMode::Pull;
        self
    }

    /// Change the subscription period.
    #[inline]
    pub fn period(mut self, period: Option<Period>) -> Self {
        self.info.period = period;
        self
    }

    /// Make the subscription local only.
    #[inline]
    pub fn local(mut self) -> Self {
        self.local = true;
        self
    }
}

impl<'a> Runnable for SubscriberBuilder<'a, '_> {
    type Output = ZResult<Subscriber<'a>>;

    fn run(&mut self) -> Self::Output {
        trace!("subscribe({:?})", self.reskey);
        let (sender, receiver) = bounded(*API_DATA_RECEPTION_CHANNEL_SIZE);

        if self.local {
            self.session
                .register_any_local_subscriber(&self.reskey, SubscriberInvoker::Sender(sender))
                .map(|sub_state| Subscriber {
                    session: self.session,
                    state: sub_state,
                    alive: true,
                    receiver: SampleReceiver::new(receiver),
                })
        } else {
            self.session
                .register_any_subscriber(
                    &self.reskey,
                    SubscriberInvoker::Sender(sender),
                    &self.info,
                )
                .map(|sub_state| Subscriber {
                    session: self.session,
                    state: sub_state,
                    alive: true,
                    receiver: SampleReceiver::new(receiver),
                })
        }
    }
}

derive_zfuture! {
    /// `CallbackSubscriberBuilder` is a builder for initializing a [CallbackSubscriber](CallbackSubscriber).
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// let subscriber = session
    ///     .subscribe("/resource/name")
    ///     .callback(|sample| { println!("Received : {} {}", sample.res_name, sample.value); })
    ///     .best_effort()
    ///     .pull_mode()
    ///     .await
    ///     .unwrap();
    /// # })
    /// ```
    #[derive(Clone)]
    pub struct CallbackSubscriberBuilder<'a, 'b> {
        session: &'a Session,
        reskey: ResKey<'b>,
        info: SubInfo,
        local: bool,
        handler: Arc<RwLock<DataHandler>>,
    }
}

impl fmt::Debug for CallbackSubscriberBuilder<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CallbackSubscriberBuilder")
            .field("session", self.session)
            .field("reskey", &self.reskey)
            .field("info", &self.info)
            .finish()
    }
}

impl<'a, 'b> CallbackSubscriberBuilder<'a, 'b> {
    /// Change the subscription reliability.
    #[inline]
    pub fn reliability(mut self, reliability: Reliability) -> Self {
        self.info.reliability = reliability;
        self
    }

    /// Change the subscription reliability to Reliable.
    #[inline]
    pub fn reliable(mut self) -> Self {
        self.info.reliability = Reliability::Reliable;
        self
    }

    /// Change the subscription reliability to BestEffort.
    #[inline]
    pub fn best_effort(mut self) -> Self {
        self.info.reliability = Reliability::BestEffort;
        self
    }

    /// Change the subscription mode.
    #[inline]
    pub fn mode(mut self, mode: SubMode) -> Self {
        self.info.mode = mode;
        self
    }

    /// Change the subscription mode to Push.
    #[inline]
    pub fn push_mode(mut self) -> Self {
        self.info.mode = SubMode::Push;
        self.info.period = None;
        self
    }

    /// Change the subscription mode to Pull.
    #[inline]
    pub fn pull_mode(mut self) -> Self {
        self.info.mode = SubMode::Pull;
        self
    }

    /// Change the subscription period.
    #[inline]
    pub fn period(mut self, period: Option<Period>) -> Self {
        self.info.period = period;
        self
    }

    /// Make the subscription local onlyu.
    #[inline]
    pub fn local(mut self) -> Self {
        self.local = true;
        self
    }
}

impl<'a> Runnable for CallbackSubscriberBuilder<'a, '_> {
    type Output = ZResult<CallbackSubscriber<'a>>;

    fn run(&mut self) -> Self::Output {
        trace!("declare_callback_subscriber({:?})", self.reskey);

        if self.local {
            self.session
                .register_any_local_subscriber(
                    &self.reskey,
                    SubscriberInvoker::Handler(self.handler.clone()),
                )
                .map(|sub_state| CallbackSubscriber {
                    session: self.session,
                    state: sub_state,
                    alive: true,
                })
        } else {
            self.session
                .register_any_subscriber(
                    &self.reskey,
                    SubscriberInvoker::Handler(self.handler.clone()),
                    &self.info,
                )
                .map(|sub_state| CallbackSubscriber {
                    session: self.session,
                    state: sub_state,
                    alive: true,
                })
        }
    }
}

derive_zfuture! {
    /// `QueryableBuilder` is a builder for initializing a [Queryable](Queryable).
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::*;
    /// use futures::prelude::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// let mut queryable = session
    ///     .register_queryable("/resource/name")
    ///     .kind(queryable::EVAL)
    ///     .await
    ///     .unwrap();
    /// # })
    /// ```
    #[derive(Debug, Clone)]
    pub struct QueryableBuilder<'a, 'b> {
        session: &'a Session,
        reskey: ResKey<'b>,
        kind: ZInt,
        complete: bool,
    }
}

impl<'a, 'b> QueryableBuilder<'a, 'b> {
    /// Change the queryable kind.
    #[inline]
    pub fn kind(mut self, kind: ZInt) -> Self {
        self.kind = kind;
        self
    }

    /// Change queryable completeness.
    #[inline]
    pub fn complete(mut self, complete: bool) -> Self {
        self.complete = complete;
        self
    }
}

impl<'a> Runnable for QueryableBuilder<'a, '_> {
    type Output = ZResult<Queryable<'a>>;

    fn run(&mut self) -> Self::Output {
        trace!("register_queryable({:?}, {:?})", self.reskey, self.kind);
        let mut state = zwrite!(self.session.state);
        let id = state.decl_id_counter.fetch_add(1, Ordering::SeqCst);
        let (sender, receiver) = bounded(*API_QUERY_RECEPTION_CHANNEL_SIZE);
        let qable_state = Arc::new(QueryableState {
            id,
            reskey: self.reskey.to_owned(),
            kind: self.kind,
            complete: self.complete,
            sender,
        });
        #[cfg(feature = "complete_n")]
        {
            state.queryables.insert(id, qable_state.clone());

            if self.complete {
                let primitives = state.primitives.as_ref().unwrap().clone();
                let complete = Session::complete_twin_qabls(&state, &self.reskey, self.kind);
                drop(state);
                let qabl_info = QueryableInfo {
                    complete,
                    distance: 0,
                };
                primitives.decl_queryable(&self.reskey, self.kind, &qabl_info, None);
            }
        }
        #[cfg(not(feature = "complete_n"))]
        {
            let twin_qabl = Session::twin_qabl(&state, &self.reskey, self.kind);
            let complete_twin_qabl =
                twin_qabl && Session::complete_twin_qabl(&state, &self.reskey, self.kind);

            state.queryables.insert(id, qable_state.clone());

            if !twin_qabl || (!complete_twin_qabl && self.complete) {
                let primitives = state.primitives.as_ref().unwrap().clone();
                let complete = if !complete_twin_qabl && self.complete {
                    1
                } else {
                    0
                };
                drop(state);
                let qabl_info = QueryableInfo {
                    complete,
                    distance: 0,
                };
                primitives.decl_queryable(&self.reskey, self.kind, &qabl_info, None);
            }
        }

        Ok(Queryable {
            session: self.session,
            state: qable_state,
            alive: true,
            receiver: QueryReceiver::new(receiver),
        })
    }
}

derive_zfuture! {
    /// `WriteBuilder` is a builder for initializing a `write`.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// session
    ///     .put("/resource/name", "value")
    ///     .encoding(encoding::TEXT_PLAIN)
    ///     .congestion_control(CongestionControl::Block)
    ///     .await
    ///     .unwrap();
    /// # })
    /// ```
    #[derive(Debug, Clone)]
    pub struct WriteBuilder<'a> {
        session: &'a Session,
        resource: ResKey<'a>,
        value: Option<Value>,
        kind: Option<ZInt>,
        congestion_control: CongestionControl,
    }
}

impl<'a> WriteBuilder<'a> {
    /// Change the congestion_control to apply when routing the data.
    #[inline]
    pub fn congestion_control(mut self, congestion_control: CongestionControl) -> WriteBuilder<'a> {
        self.congestion_control = congestion_control;
        self
    }

    /// Change the kind of the written data.
    #[inline]
    pub fn kind(mut self, kind: SampleKind) -> Self {
        self.kind = Some(kind as ZInt);
        self
    }

    /// Change the encoding of the written data.
    #[inline]
    pub fn encoding(mut self, encoding: ZInt) -> Self {
        if let Some(mut payload) = self.value.as_mut() {
            payload.encoding = encoding;
        }
        self
    }
}

impl Runnable for WriteBuilder<'_> {
    type Output = ZResult<()>;

    fn run(&mut self) -> Self::Output {
        trace!("write({:?}, [...])", self.resource);
        let state = zread!(self.session.state);
        let primitives = state.primitives.as_ref().unwrap().clone();
        let local_routing = state.local_routing;
        drop(state);

        let value = self.value.take().unwrap();
        let mut info = net::protocol::proto::DataInfo::new();
        info.kind = match self.kind {
            Some(data_kind::DEFAULT) => None,
            kind => kind,
        };
        info.encoding = if value.encoding != encoding::DEFAULT {
            Some(value.encoding)
        } else {
            None
        };
        info.timestamp = self.session.runtime.new_timestamp();
        let data_info = if info.has_options() { Some(info) } else { None };

        primitives.send_data(
            &self.resource,
            value.payload.clone(),
            Reliability::Reliable, // @TODO: need to check subscriptions to determine the right reliability value
            self.congestion_control,
            data_info.clone(),
            None,
        );
        if local_routing {
            self.session
                .handle_data(true, &self.resource, data_info, value.payload);
        }
        Ok(())
    }
}

derive_zfuture! {
    /// `QueryBuilder` is a builder for initializing a `query`.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::*;
    /// use futures::prelude::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// let mut replies = session
    ///     .get("/resource/name?value>1")
    ///     .target(QueryTarget{ kind: queryable::ALL_KINDS, target: Target::All })
    ///     .consolidation(QueryConsolidation::none())
    ///     .await
    ///     .unwrap();
    /// # })
    /// ```
    #[derive(Debug, Clone)]
    pub struct QueryBuilder<'a> {
        session: &'a Session,
        selector: KeySelector<'a>,
        target: Option<QueryTarget>,
        consolidation: Option<QueryConsolidation>,
    }
}

impl<'a> QueryBuilder<'a> {
    /// Change the target of the query.
    #[inline]
    pub fn target(mut self, target: QueryTarget) -> Self {
        self.target = Some(target);
        self
    }

    /// Change the consolidation mode of the query.
    #[inline]
    pub fn consolidation(mut self, consolidation: QueryConsolidation) -> Self {
        self.consolidation = Some(consolidation);
        self
    }
}

impl Runnable for QueryBuilder<'_> {
    type Output = ZResult<ReplyReceiver>;

    fn run(&mut self) -> Self::Output {
        trace!(
            "get({}, {:?}, {:?})",
            self.selector,
            self.target,
            self.consolidation
        );
        let mut state = zwrite!(self.session.state);
        let target = self.target.take().unwrap();
        let consolidation = self.consolidation.take().unwrap();
        let qid = state.qid_counter.fetch_add(1, Ordering::SeqCst);
        let (rep_sender, rep_receiver) = bounded(*API_REPLY_RECEPTION_CHANNEL_SIZE);
        state.queries.insert(
            qid,
            QueryState {
                nb_final: 2,
                reception_mode: consolidation.reception,
                replies: if consolidation.reception != ConsolidationMode::None {
                    Some(HashMap::new())
                } else {
                    None
                },
                rep_sender,
            },
        );

        let primitives = state.primitives.as_ref().unwrap().clone();
        let local_routing = state.local_routing;
        drop(state);
        primitives.send_query(
            &self.selector.key,
            self.selector.predicate,
            qid,
            target.clone(),
            consolidation.clone(),
            None,
        );
        if local_routing {
            self.session.handle_query(
                true,
                &self.selector.key,
                self.selector.predicate,
                qid,
                target,
                consolidation,
            );
        }

        Ok(ReplyReceiver::new(rep_receiver))
    }
}

/// A zenoh-net session.
///
pub struct Session {
    pub(crate) runtime: Runtime,
    pub(crate) state: Arc<RwLock<SessionState>>,
    pub(crate) alive: bool,
}

impl Session {
    pub(crate) fn clone(&self) -> Self {
        Session {
            runtime: self.runtime.clone(),
            state: self.state.clone(),
            alive: false,
        }
    }

    pub(super) fn new(config: ConfigProperties) -> impl ZFuture<Output = ZResult<Session>> {
        zpinbox(async {
            let local_routing = config
                .get_or(&ZN_LOCAL_ROUTING_KEY, ZN_LOCAL_ROUTING_DEFAULT)
                .to_lowercase()
                == ZN_TRUE;
            let join_subscriptions = match config.get(&ZN_JOIN_SUBSCRIPTIONS_KEY) {
                Some(s) => s.split(',').map(|s| s.to_string()).collect(),
                None => vec![],
            };
            let join_publications = match config.get(&ZN_JOIN_PUBLICATIONS_KEY) {
                Some(s) => s.split(',').map(|s| s.to_string()).collect(),
                None => vec![],
            };
            match Runtime::new(0, config.0.into(), None).await {
                Ok(runtime) => {
                    let session = Self::init(
                        runtime,
                        local_routing,
                        join_subscriptions,
                        join_publications,
                    )
                    .await;
                    // Workaround for the declare_and_shoot problem
                    task::sleep(Duration::from_millis(*API_OPEN_SESSION_DELAY)).await;
                    Ok(session)
                }
                Err(err) => Err(err),
            }
        })
    }

    /// Returns the identifier for this session.
    pub fn id(&self) -> impl ZFuture<Output = String> {
        zready(self.runtime.get_pid_str())
    }

    pub fn hlc(&self) -> Option<&HLC> {
        self.runtime.hlc.as_ref().map(Arc::as_ref)
    }

    /// Initialize a Session with an existing Runtime.
    /// This operation is used by the plugins to share the same Runtime than the router.
    #[doc(hidden)]
    pub fn init(
        runtime: Runtime,
        local_routing: bool,
        join_subscriptions: Vec<String>,
        join_publications: Vec<String>,
    ) -> impl ZFuture<Output = Session> {
        let router = runtime.router.clone();
        let state = Arc::new(RwLock::new(SessionState::new(
            local_routing,
            join_subscriptions,
            join_publications,
        )));
        let session = Session {
            runtime,
            state: state.clone(),
            alive: true,
        };
        let primitives = Some(router.new_primitives(Arc::new(session.clone())));
        zwrite!(state).primitives = primitives;
        zready(session)
    }

    fn close_alive(self) -> impl ZFuture<Output = ZResult<()>> {
        zpinbox(async move {
            trace!("close()");
            self.runtime.close().await?;

            let primitives = zwrite!(self.state).primitives.as_ref().unwrap().clone();
            primitives.send_close();

            Ok(())
        })
    }

    /// Close the zenoh-net [Session](Session).
    ///
    /// Sessions are automatically closed when dropped, but you may want to use this function to handle errors or
    /// close the Session asynchronously.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// session.close().await.unwrap();
    /// # })
    /// ```
    pub fn close(mut self) -> impl ZFuture<Output = ZResult<()>> {
        self.alive = false;
        self.close_alive()
    }

    /// Get informations about the zenoh-net [Session](Session).
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// let info = session.info();
    /// # })
    /// ```
    pub fn info(&self) -> impl ZFuture<Output = InfoProperties> {
        trace!("info()");
        let sessions = self.runtime.manager().get_sessions();
        let peer_pids = sessions
            .iter()
            .filter(|s| {
                s.get_whatami()
                    .ok()
                    .map(|what| what & whatami::PEER != 0)
                    .or(Some(false))
                    .unwrap()
            })
            .filter_map(|s| {
                s.get_pid()
                    .ok()
                    .map(|pid| hex::encode_upper(pid.as_slice()))
            })
            .collect::<Vec<String>>();
        let mut router_pids = vec![];
        if self.runtime.whatami & whatami::ROUTER != 0 {
            router_pids.push(hex::encode_upper(self.runtime.pid.as_slice()));
        }
        router_pids.extend(
            sessions
                .iter()
                .filter(|s| {
                    s.get_whatami()
                        .ok()
                        .map(|what| what & whatami::ROUTER != 0)
                        .or(Some(false))
                        .unwrap()
                })
                .filter_map(|s| {
                    s.get_pid()
                        .ok()
                        .map(|pid| hex::encode_upper(pid.as_slice()))
                })
                .collect::<Vec<String>>(),
        );

        let mut info = InfoProperties::default();
        info.insert(ZN_INFO_PEER_PID_KEY, peer_pids.join(","));
        info.insert(ZN_INFO_ROUTER_PID_KEY, router_pids.join(","));
        info.insert(
            ZN_INFO_PID_KEY,
            hex::encode_upper(self.runtime.pid.as_slice()),
        );
        zready(info)
    }

    /// Associate a numerical Id with the given resource key.
    ///
    /// This numerical Id will be used on the network to save bandwidth and
    /// ease the retrieval of the concerned resource in the routing tables.
    ///
    /// # Arguments
    ///
    /// * `resource` - The resource key to map to a numerical Id
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// let rid = session.register_resource("/resource/name").await.unwrap();
    /// # })
    /// ```
    pub fn register_resource<'a, IntoResKey>(
        &self,
        resource: IntoResKey,
    ) -> impl ZFuture<Output = ZResult<ResourceId>>
    where
        IntoResKey: Into<ResKey<'a>>,
    {
        let resource = resource.into();
        trace!("register_resource({:?})", resource);
        let mut state = zwrite!(self.state);

        zready(state.localkey_to_resname(&resource).map(|resname| {
            match state
                .local_resources
                .iter()
                .find(|(_rid, res)| res.name == resname)
            {
                Some((rid, _res)) => *rid,
                None => {
                    let rid = state.rid_counter.fetch_add(1, Ordering::SeqCst) as ZInt;
                    let mut res = Resource::new(resname.clone());
                    for sub in state.subscribers.values() {
                        if rname::matches(&resname, &sub.resname) {
                            res.subscribers.push(sub.clone());
                        }
                    }

                    state.local_resources.insert(rid, res);

                    let primitives = state.primitives.as_ref().unwrap().clone();
                    drop(state);
                    primitives.decl_resource(rid, &resource);

                    rid
                }
            }
        }))
    }

    /// Undeclare the *numerical Id/resource key* association previously declared
    /// with [register_resource](Session::register_resource).
    ///
    /// # Arguments
    ///
    /// * `rid` - The numerical Id to unmap
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// let rid = session.register_resource("/resource/name").await.unwrap();
    /// session.unregister_resource(rid).await;
    /// # })
    /// ```
    pub fn unregister_resource(&self, rid: ResourceId) -> impl ZFuture<Output = ZResult<()>> {
        trace!("unregister_resource({:?})", rid);
        let mut state = zwrite!(self.state);
        state.local_resources.remove(&rid);

        let primitives = state.primitives.as_ref().unwrap().clone();
        drop(state);
        primitives.forget_resource(rid);

        zready(Ok(()))
    }

    /// Declare a [Publisher](Publisher) for the given resource key.
    ///
    /// Written resources that match the given key will only be sent on the network
    /// if matching subscribers exist in the system.
    ///
    /// # Arguments
    ///
    /// * `resource` - The resource key to publish
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// let publisher = session.publishing("/resource/name").await.unwrap();
    /// session.put("/resource/name", "value").await.unwrap();
    /// # })
    /// ```
    pub fn publishing<'a, 'b, IntoResKey>(&'a self, reskey: IntoResKey) -> PublisherBuilder<'a, 'b>
    where
        IntoResKey: Into<ResKey<'b>>,
    {
        PublisherBuilder {
            session: self,
            reskey: reskey.into(),
        }
    }

    pub(crate) fn unpublishing(&self, pid: usize) -> impl ZFuture<Output = ZResult<()>> {
        let mut state = zwrite!(self.state);
        zready(if let Some(pub_state) = state.publishers.remove(&pid) {
            trace!("unpublishing({:?})", pub_state);
            // Note: there might be several Publishers on the same ResKey.
            // Before calling forget_publisher(reskey), check if this was the last one.
            state.localkey_to_resname(&pub_state.reskey).map(|resname| {
                match state
                    .join_publications
                    .iter()
                    .find(|s| rname::include(s, &resname))
                {
                    Some(join_pub) => {
                        let joined_pub = state.publishers.values().any(|p| {
                            rname::include(join_pub, &state.localkey_to_resname(&p.reskey).unwrap())
                        });
                        if !joined_pub {
                            let primitives = state.primitives.as_ref().unwrap().clone();
                            let reskey = join_pub.clone().into();
                            drop(state);
                            primitives.forget_publisher(&reskey, None);
                        }
                    }
                    None => {
                        let twin_pub = state.publishers.values().any(|p| {
                            state.localkey_to_resname(&p.reskey).unwrap()
                                == state.localkey_to_resname(&pub_state.reskey).unwrap()
                        });
                        if !twin_pub {
                            let primitives = state.primitives.as_ref().unwrap().clone();
                            drop(state);
                            primitives.forget_publisher(&pub_state.reskey, None);
                        }
                    }
                };
            })
        } else {
            zerror!(ZErrorKind::Other {
                descr: "Unable to find publisher".into()
            })
        })
    }

    fn register_any_subscriber(
        &self,
        reskey: &ResKey,
        invoker: SubscriberInvoker,
        info: &SubInfo,
    ) -> ZResult<Arc<SubscriberState>> {
        let mut state = zwrite!(self.state);
        let id = state.decl_id_counter.fetch_add(1, Ordering::SeqCst);
        let resname = state.localkey_to_resname(reskey)?;
        let sub_state = Arc::new(SubscriberState {
            id,
            reskey: reskey.to_owned(),
            resname,
            invoker,
        });
        let declared_sub = match state
            .join_subscriptions
            .iter()
            .find(|s| rname::include(s, &sub_state.resname))
        {
            Some(join_sub) => {
                let joined_sub = state.subscribers.values().any(|s| {
                    rname::include(join_sub, &state.localkey_to_resname(&s.reskey).unwrap())
                });
                (!joined_sub).then(|| join_sub.clone().into())
            }
            None => {
                let twin_sub = state.subscribers.values().any(|s| {
                    state.localkey_to_resname(&s.reskey).unwrap()
                        == state.localkey_to_resname(&sub_state.reskey).unwrap()
                });
                (!twin_sub).then(|| sub_state.reskey.clone())
            }
        };

        state.subscribers.insert(sub_state.id, sub_state.clone());
        for res in state.local_resources.values_mut() {
            if rname::matches(&sub_state.resname, &res.name) {
                res.subscribers.push(sub_state.clone());
            }
        }
        for res in state.remote_resources.values_mut() {
            if rname::matches(&sub_state.resname, &res.name) {
                res.subscribers.push(sub_state.clone());
            }
        }

        if let Some(reskey) = declared_sub {
            let primitives = state.primitives.as_ref().unwrap().clone();
            drop(state);

            // If reskey is a pure RName, remap it to optimal Rid or RidWithSuffix
            let reskey = match reskey {
                ResKey::RName(name) => match name.find('*') {
                    Some(pos) => {
                        let id = self.register_resource(&name[..pos]).wait()?;
                        ResKey::RIdWithSuffix(id, name[pos..].to_string().into())
                    }
                    None => {
                        let id = self.register_resource(&ResKey::RName(name)).wait()?;
                        ResKey::RId(id)
                    }
                },
                reskey => reskey,
            };

            primitives.decl_subscriber(&reskey, info, None);
        }

        Ok(sub_state)
    }

    fn register_any_local_subscriber(
        &self,
        reskey: &ResKey,
        invoker: SubscriberInvoker,
    ) -> ZResult<Arc<SubscriberState>> {
        let mut state = zwrite!(self.state);
        let id = state.decl_id_counter.fetch_add(1, Ordering::SeqCst);
        let resname = state.localkey_to_resname(reskey)?;
        let sub_state = Arc::new(SubscriberState {
            id,
            reskey: reskey.to_owned(),
            resname,
            invoker,
        });
        state
            .local_subscribers
            .insert(sub_state.id, sub_state.clone());
        for res in state.local_resources.values_mut() {
            if rname::matches(&sub_state.resname, &res.name) {
                res.local_subscribers.push(sub_state.clone());
            }
        }
        for res in state.remote_resources.values_mut() {
            if rname::matches(&sub_state.resname, &res.name) {
                res.local_subscribers.push(sub_state.clone());
            }
        }

        Ok(sub_state)
    }

    /// Declare a [Subscriber](Subscriber) for the given resource key.
    ///
    /// # Arguments
    ///
    /// * `resource` - The resource key to subscribe
    ///
    /// # Examples
    /// ```no_run
    /// # async_std::task::block_on(async {
    /// use zenoh::*;
    /// use futures::prelude::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// let mut subscriber = session.subscribe("/resource/name").await.unwrap();
    /// while let Some(sample) = subscriber.receiver().next().await {
    ///     println!("Received : {:?}", sample);
    /// }
    /// # })
    /// ```
    pub fn subscribe<'a, 'b, IntoResKey>(&'a self, reskey: IntoResKey) -> SubscriberBuilder<'a, 'b>
    where
        IntoResKey: Into<ResKey<'b>>,
    {
        SubscriberBuilder {
            session: self,
            reskey: reskey.into(),
            info: SubInfo::default(),
            local: false,
        }
    }

    // /// Declare a [CallbackSubscriber](CallbackSubscriber) for the given resource key.
    // ///
    // /// # Arguments
    // ///
    // /// * `resource` - The resource key to subscribe
    // /// * `data_handler` - The callback that will be called on each data reception
    // ///
    // /// # Examples
    // /// ```
    // /// # async_std::task::block_on(async {
    // /// use zenoh::*;
    // ///
    // /// let session = open(config::peer()).await.unwrap();
    // /// let subscriber = session.declare_callback_subscriber("/resource/name",
    // ///     |sample| { println!("Received : {} {}", sample.res_name, sample.payload); }
    // /// ).await.unwrap();
    // /// # })
    // /// ```
    // pub fn declare_callback_subscriber<'a, 'b, DataHandler>(
    //     &'a self,
    //     reskey: &'b ResKey,
    //     data_handler: DataHandler,
    // ) -> CallbackSubscriberBuilder<'a, 'b>
    // where
    //     DataHandler: FnMut(Sample) + Send + Sync + 'static,
    // {
    //     CallbackSubscriberBuilder {
    //         session: self,
    //         reskey,
    //         info: SubInfo::default(),
    //         local: false,
    //         handler: Arc::new(RwLock::new(data_handler)),
    //     }
    // }

    pub(crate) fn unsubscribe(&self, sid: usize) -> impl ZFuture<Output = ZResult<()>> {
        let mut state = zwrite!(self.state);
        zready(if let Some(sub_state) = state.subscribers.remove(&sid) {
            trace!("unsubscribe({:?})", sub_state);
            for res in state.local_resources.values_mut() {
                res.subscribers.retain(|sub| sub.id != sub_state.id);
            }
            for res in state.remote_resources.values_mut() {
                res.subscribers.retain(|sub| sub.id != sub_state.id);
            }

            // Note: there might be several Subscribers on the same ResKey.
            // Before calling forget_subscriber(reskey), check if this was the last one.
            state.localkey_to_resname(&sub_state.reskey).map(|resname| {
                match state
                    .join_subscriptions
                    .iter()
                    .find(|s| rname::include(s, &resname))
                {
                    Some(join_sub) => {
                        let joined_sub = state.subscribers.values().any(|s| {
                            rname::include(join_sub, &state.localkey_to_resname(&s.reskey).unwrap())
                        });
                        if !joined_sub {
                            let primitives = state.primitives.as_ref().unwrap().clone();
                            let reskey = join_sub.clone().into();
                            drop(state);
                            primitives.forget_subscriber(&reskey, None);
                        }
                    }
                    None => {
                        let twin_sub = state.subscribers.values().any(|s| {
                            state.localkey_to_resname(&s.reskey).unwrap()
                                == state.localkey_to_resname(&sub_state.reskey).unwrap()
                        });
                        if !twin_sub {
                            let primitives = state.primitives.as_ref().unwrap().clone();
                            drop(state);
                            primitives.forget_subscriber(&sub_state.reskey, None);
                        }
                    }
                };
            })
        } else if let Some(sub_state) = state.local_subscribers.remove(&sid) {
            trace!("unsubscribe({:?})", sub_state);
            for res in state.local_resources.values_mut() {
                res.local_subscribers.retain(|sub| sub.id != sub_state.id);
            }
            for res in state.remote_resources.values_mut() {
                res.local_subscribers.retain(|sub| sub.id != sub_state.id);
            }
            Ok(())
        } else {
            zerror!(ZErrorKind::Other {
                descr: "Unable to find subscriber".into()
            })
        })
    }

    fn twin_qabl(state: &SessionState, key: &ResKey, kind: ZInt) -> bool {
        state.queryables.values().any(|q| {
            q.kind == kind
                && state.localkey_to_resname(&q.reskey).unwrap()
                    == state.localkey_to_resname(key).unwrap()
        })
    }

    #[cfg(not(feature = "complete_n"))]
    fn complete_twin_qabl(state: &SessionState, key: &ResKey, kind: ZInt) -> bool {
        state.queryables.values().any(|q| {
            q.complete
                && q.kind == kind
                && state.localkey_to_resname(&q.reskey).unwrap()
                    == state.localkey_to_resname(key).unwrap()
        })
    }

    #[cfg(feature = "complete_n")]
    fn complete_twin_qabls(state: &SessionState, key: &ResKey, kind: ZInt) -> ZInt {
        state
            .queryables
            .values()
            .filter(|q| {
                q.complete
                    && q.kind == kind
                    && state.localkey_to_resname(&q.reskey).unwrap()
                        == state.localkey_to_resname(key).unwrap()
            })
            .count() as ZInt
    }

    /// Declare a [Queryable](Queryable) for the given resource key.
    ///
    /// # Arguments
    ///
    /// * `resource` - The resource key the [Queryable](Queryable) will reply to
    ///
    /// # Examples
    /// ```no_run
    /// # async_std::task::block_on(async {
    /// use zenoh::*;
    /// use futures::prelude::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// let mut queryable = session.register_queryable("/resource/name").await.unwrap();
    /// while let Some(query) = queryable.receiver().next().await {
    ///     query.reply_async(Sample::new(
    ///         "/resource/name".to_string(),
    ///         "value",
    ///     )).await;
    /// }
    /// # })
    /// ```
    pub fn register_queryable<'a, 'b, IntoResKey>(
        &'a self,
        reskey: IntoResKey,
    ) -> QueryableBuilder<'a, 'b>
    where
        IntoResKey: Into<ResKey<'b>>,
    {
        QueryableBuilder {
            session: self,
            reskey: reskey.into(),
            kind: EVAL,
            complete: true,
        }
    }

    pub(crate) fn unregister_queryable(&self, qid: usize) -> impl ZFuture<Output = ZResult<()>> {
        let mut state = zwrite!(self.state);
        zready(if let Some(qable_state) = state.queryables.remove(&qid) {
            trace!("unregister_queryable({:?})", qable_state);
            if Session::twin_qabl(&state, &qable_state.reskey, qable_state.kind) {
                // There still exist Queryables on the same ResKey.
                if qable_state.complete {
                    #[cfg(feature = "complete_n")]
                    {
                        let complete = Session::complete_twin_qabls(
                            &state,
                            &qable_state.reskey,
                            qable_state.kind,
                        );
                        let primitives = state.primitives.as_ref().unwrap();
                        let qabl_info = QueryableInfo {
                            complete,
                            distance: 0,
                        };
                        primitives.decl_queryable(
                            &qable_state.reskey,
                            qable_state.kind,
                            &qabl_info,
                            None,
                        );
                    }
                    #[cfg(not(feature = "complete_n"))]
                    {
                        if !Session::complete_twin_qabl(
                            &state,
                            &qable_state.reskey,
                            qable_state.kind,
                        ) {
                            let primitives = state.primitives.as_ref().unwrap();
                            let qabl_info = QueryableInfo {
                                complete: 0,
                                distance: 0,
                            };
                            primitives.decl_queryable(
                                &qable_state.reskey,
                                qable_state.kind,
                                &qabl_info,
                                None,
                            );
                        }
                    }
                }
            } else {
                // There are no more Queryables on the same ResKey.
                let primitives = state.primitives.as_ref().unwrap();
                primitives.forget_queryable(&qable_state.reskey, qable_state.kind, None);
            }
            Ok(())
        } else {
            zerror!(ZErrorKind::Other {
                descr: "Unable to find queryable".into()
            })
        })
    }

    /// Write data.
    ///
    /// # Arguments
    ///
    /// * `resource` - The resource key to write
    /// * `payload` - The value to write
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// session.put("/resource/name", "value")
    ///        .encoding(encoding::TEXT_PLAIN).await.unwrap();
    /// # })
    /// ```
    #[inline]
    pub fn put<'a, IntoResKey, IntoValue>(
        &'a self,
        resource: IntoResKey,
        value: IntoValue,
    ) -> WriteBuilder<'a>
    where
        IntoResKey: Into<ResKey<'a>>,
        IntoValue: Into<Value>,
    {
        WriteBuilder {
            session: self,
            resource: resource.into(),
            value: Some(value.into()),
            kind: None,
            congestion_control: CongestionControl::default(),
        }
    }

    /// Delete data.
    ///
    /// # Arguments
    ///
    /// * `resource` - The resource key to write
    /// * `payload` - The value to write
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// session.delete("/resource/name");
    /// # })
    /// ```
    #[inline]
    pub fn delete<'a, IntoResKey>(&'a self, resource: IntoResKey) -> WriteBuilder<'a>
    where
        IntoResKey: Into<ResKey<'a>>,
    {
        WriteBuilder {
            session: self,
            resource: resource.into(),
            value: Some(Value::empty()),
            kind: Some(data_kind::DELETE),
            congestion_control: CongestionControl::default(),
        }
    }

    #[inline]
    fn invoke_subscriber(
        invoker: &SubscriberInvoker,
        res_name: String,
        payload: ZBuf,
        data_info: Option<DataInfo>,
    ) {
        match invoker {
            SubscriberInvoker::Handler(handler) => {
                let handler = &mut *zwrite!(handler);
                handler(Sample::with_info(res_name, payload, data_info));
            }
            SubscriberInvoker::Sender(sender) => {
                if let Err(e) = sender.send(Sample::with_info(res_name, payload, data_info)) {
                    error!("SubscriberInvoker error: {}", e);
                }
            }
        }
    }

    fn handle_data(&self, local: bool, reskey: &ResKey, info: Option<DataInfo>, payload: ZBuf) {
        let state = zread!(self.state);
        if let ResKey::RId(rid) = reskey {
            match state.get_res(rid, local) {
                Some(res) => {
                    if !local && res.subscribers.len() == 1 {
                        let sub = res.subscribers.get(0).unwrap();
                        Session::invoke_subscriber(&sub.invoker, res.name.clone(), payload, info);
                    } else {
                        for sub in &res.subscribers {
                            Session::invoke_subscriber(
                                &sub.invoker,
                                res.name.clone(),
                                payload.clone(),
                                info.clone(),
                            );
                        }
                        if local {
                            for sub in &res.local_subscribers {
                                Session::invoke_subscriber(
                                    &sub.invoker,
                                    res.name.clone(),
                                    payload.clone(),
                                    info.clone(),
                                );
                            }
                        }
                    }
                }
                None => {
                    error!("Received Data for unkown rid: {}", rid);
                }
            }
        } else {
            match state.reskey_to_resname(reskey, local) {
                Ok(resname) => {
                    for sub in state.subscribers.values() {
                        if rname::matches(&sub.resname, &resname) {
                            Session::invoke_subscriber(
                                &sub.invoker,
                                resname.clone(),
                                payload.clone(),
                                info.clone(),
                            );
                        }
                    }
                    if local {
                        for sub in state.local_subscribers.values() {
                            if rname::matches(&sub.resname, &resname) {
                                Session::invoke_subscriber(
                                    &sub.invoker,
                                    resname.clone(),
                                    payload.clone(),
                                    info.clone(),
                                );
                            }
                        }
                    }
                }
                Err(err) => {
                    error!("Received Data for unkown reskey: {}", err);
                }
            }
        }
    }

    pub(crate) fn pull(&self, reskey: &ResKey) -> impl ZFuture<Output = ZResult<()>> {
        trace!("pull({:?})", reskey);
        let state = zread!(self.state);
        let primitives = state.primitives.as_ref().unwrap().clone();
        drop(state);
        primitives.send_pull(true, reskey, 0, &None);
        zready(Ok(()))
    }

    /// Query data from the matching queryables in the system.
    ///
    /// # Arguments
    ///
    /// * `resource` - The resource key to query
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::*;
    /// use futures::prelude::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// let mut replies = session.get("/resource/name").await.unwrap();
    /// while let Some(reply) = replies.next().await {
    ///     println!(">> Received {:?}", reply.data);
    /// }
    /// # })
    /// ```
    pub fn get<'a, IntoKeySelector>(&'a self, selector: IntoKeySelector) -> QueryBuilder<'a>
    where
        IntoKeySelector: Into<KeySelector<'a>>,
    {
        QueryBuilder {
            session: self,
            selector: selector.into(),
            target: Some(QueryTarget::default()),
            consolidation: Some(QueryConsolidation::default()),
        }
    }

    fn handle_query(
        &self,
        local: bool,
        reskey: &ResKey,
        predicate: &str,
        qid: ZInt,
        target: QueryTarget,
        _consolidation: QueryConsolidation,
    ) {
        let (primitives, resname, kinds_and_senders) = {
            let state = zread!(self.state);
            match state.reskey_to_resname(reskey, local) {
                Ok(resname) => {
                    let kinds_and_senders = state
                        .queryables
                        .values()
                        .filter(
                            |queryable| match state.localkey_to_resname(&queryable.reskey) {
                                Ok(qablname) => {
                                    rname::matches(&qablname, &resname)
                                        && ((queryable.kind == queryable::ALL_KINDS
                                            || target.kind == queryable::ALL_KINDS)
                                            || (queryable.kind & target.kind != 0))
                                }
                                Err(err) => {
                                    error!(
                                        "{}. Internal error (queryable reskey to resname failed).",
                                        err
                                    );
                                    false
                                }
                            },
                        )
                        .map(|qable| (qable.kind, qable.sender.clone()))
                        .collect::<Vec<(ZInt, Sender<Query>)>>();
                    (
                        state.primitives.as_ref().unwrap().clone(),
                        resname,
                        kinds_and_senders,
                    )
                }
                Err(err) => {
                    error!("Received Query for unkown reskey: {}", err);
                    return;
                }
            }
        };

        let predicate = predicate.to_string();
        let (rep_sender, rep_receiver) = bounded(*API_REPLY_EMISSION_CHANNEL_SIZE);

        let pid = self.runtime.pid.clone(); // @TODO build/use prebuilt specific pid

        for (kind, req_sender) in kinds_and_senders {
            let _ = req_sender.send(Query {
                res_name: resname.clone(),
                predicate: predicate.clone(),
                replies_sender: RepliesSender {
                    kind,
                    sender: rep_sender.clone(),
                },
            });
        }
        drop(rep_sender); // all senders need to be dropped for the channel to close

        // router is not re-entrant

        if local {
            let this = self.clone();
            task::spawn(async move {
                while let Some((replier_kind, sample)) = rep_receiver.stream().next().await {
                    let (res_name, payload, data_info) = sample.split();
                    this.send_reply_data(
                        qid,
                        replier_kind,
                        pid.clone(),
                        res_name.into(),
                        Some(data_info),
                        payload,
                    );
                }
                this.send_reply_final(qid);
            });
        } else {
            task::spawn(async move {
                while let Some((replier_kind, sample)) = rep_receiver.stream().next().await {
                    let (res_name, payload, data_info) = sample.split();
                    primitives.send_reply_data(
                        qid,
                        replier_kind,
                        pid.clone(),
                        res_name.into(),
                        Some(data_info),
                        payload,
                    );
                }
                primitives.send_reply_final(qid);
            });
        }
    }

    pub fn reskey_to_resname(&self, reskey: &ResKey) -> ZResult<String> {
        let state = zread!(self.state);
        state.remotekey_to_resname(reskey)
    }
}

impl Primitives for Session {
    fn decl_resource(&self, rid: ZInt, reskey: &ResKey) {
        trace!("recv Decl Resource {} {:?}", rid, reskey);
        let state = &mut zwrite!(self.state);
        match state.remotekey_to_resname(reskey) {
            Ok(resname) => {
                let mut res = Resource::new(resname.clone());
                for sub in state.subscribers.values() {
                    if rname::matches(&resname, &sub.resname) {
                        res.subscribers.push(sub.clone());
                    }
                }

                state.remote_resources.insert(rid, res);
            }
            Err(_) => error!("Received Resource for unkown reskey: {}", reskey),
        }
    }

    fn forget_resource(&self, _rid: ZInt) {
        trace!("recv Forget Resource {}", _rid);
    }

    fn decl_publisher(&self, _reskey: &ResKey, _routing_context: Option<RoutingContext>) {
        trace!("recv Decl Publisher {:?}", _reskey);
    }

    fn forget_publisher(&self, _reskey: &ResKey, _routing_context: Option<RoutingContext>) {
        trace!("recv Forget Publisher {:?}", _reskey);
    }

    fn decl_subscriber(
        &self,
        _reskey: &ResKey,
        _sub_info: &SubInfo,
        _routing_context: Option<RoutingContext>,
    ) {
        trace!("recv Decl Subscriber {:?} , {:?}", _reskey, _sub_info);
    }

    fn forget_subscriber(&self, _reskey: &ResKey, _routing_context: Option<RoutingContext>) {
        trace!("recv Forget Subscriber {:?}", _reskey);
    }

    fn decl_queryable(
        &self,
        _reskey: &ResKey,
        _kind: ZInt,
        _qabl_info: &QueryableInfo,
        _routing_context: Option<RoutingContext>,
    ) {
        trace!("recv Decl Queryable {:?}", _reskey);
    }

    fn forget_queryable(
        &self,
        _reskey: &ResKey,
        _kind: ZInt,
        _routing_context: Option<RoutingContext>,
    ) {
        trace!("recv Forget Queryable {:?}", _reskey);
    }

    fn send_data(
        &self,
        reskey: &ResKey,
        payload: ZBuf,
        reliability: Reliability,
        congestion_control: CongestionControl,
        info: Option<DataInfo>,
        _routing_context: Option<RoutingContext>,
    ) {
        trace!(
            "recv Data {:?} {:?} {:?} {:?} {:?}",
            reskey,
            payload,
            reliability,
            congestion_control,
            info,
        );
        self.handle_data(false, reskey, info, payload)
    }

    fn send_query(
        &self,
        reskey: &ResKey,
        predicate: &str,
        qid: ZInt,
        target: QueryTarget,
        consolidation: QueryConsolidation,
        _routing_context: Option<RoutingContext>,
    ) {
        trace!(
            "recv Query {:?} {:?} {:?} {:?}",
            reskey,
            predicate,
            target,
            consolidation
        );
        self.handle_query(false, reskey, predicate, qid, target, consolidation)
    }

    fn send_reply_data(
        &self,
        qid: ZInt,
        replier_kind: ZInt,
        replier_id: PeerId,
        reskey: ResKey,
        data_info: Option<DataInfo>,
        payload: ZBuf,
    ) {
        trace!(
            "recv ReplyData {:?} {:?} {:?} {:?} {:?} {:?}",
            qid,
            replier_kind,
            replier_id,
            reskey,
            data_info,
            payload
        );
        let state = &mut zwrite!(self.state);
        let res_name = match state.remotekey_to_resname(&reskey) {
            Ok(name) => name,
            Err(e) => {
                error!("Received ReplyData for unkown reskey: {}", e);
                return;
            }
        };
        match state.queries.get_mut(&qid) {
            Some(query) => {
                let new_reply = Reply {
                    data: Sample::with_info(res_name, payload, data_info),
                    replier_kind,
                    replier_id,
                };
                match query.reception_mode {
                    ConsolidationMode::None => {
                        let _ = query.rep_sender.send(new_reply);
                    }
                    ConsolidationMode::Lazy => {
                        match query
                            .replies
                            .as_ref()
                            .unwrap()
                            .get(&new_reply.data.res_name)
                        {
                            Some(reply) => {
                                if new_reply.data.timestamp > reply.data.timestamp {
                                    query
                                        .replies
                                        .as_mut()
                                        .unwrap()
                                        .insert(new_reply.data.res_name.clone(), new_reply.clone());
                                    let _ = query.rep_sender.send(new_reply);
                                }
                            }
                            None => {
                                query
                                    .replies
                                    .as_mut()
                                    .unwrap()
                                    .insert(new_reply.data.res_name.clone(), new_reply.clone());
                                let _ = query.rep_sender.send(new_reply);
                            }
                        }
                    }
                    ConsolidationMode::Full => {
                        match query
                            .replies
                            .as_ref()
                            .unwrap()
                            .get(&new_reply.data.res_name)
                        {
                            Some(reply) => {
                                if new_reply.data.timestamp > reply.data.timestamp {
                                    query
                                        .replies
                                        .as_mut()
                                        .unwrap()
                                        .insert(new_reply.data.res_name.clone(), new_reply.clone());
                                }
                            }
                            None => {
                                query
                                    .replies
                                    .as_mut()
                                    .unwrap()
                                    .insert(new_reply.data.res_name.clone(), new_reply.clone());
                            }
                        };
                    }
                }
            }
            None => {
                warn!("Received ReplyData for unkown Query: {}", qid);
            }
        }
    }

    fn send_reply_final(&self, qid: ZInt) {
        trace!("recv ReplyFinal {:?}", qid);
        let mut state = zwrite!(self.state);
        match state.queries.get_mut(&qid) {
            Some(mut query) => {
                query.nb_final -= 1;
                if query.nb_final == 0 {
                    let query = state.queries.remove(&qid).unwrap();
                    if query.reception_mode == ConsolidationMode::Full {
                        for (_, reply) in query.replies.unwrap().into_iter() {
                            let _ = query.rep_sender.send(reply);
                        }
                    }
                }
            }
            None => {
                warn!("Received ReplyFinal for unkown Query: {}", qid);
            }
        }
    }

    fn send_pull(
        &self,
        _is_final: bool,
        _reskey: &ResKey,
        _pull_id: ZInt,
        _max_samples: &Option<ZInt>,
    ) {
        trace!(
            "recv Pull {:?} {:?} {:?} {:?}",
            _is_final,
            _reskey,
            _pull_id,
            _max_samples
        );
    }

    fn send_close(&self) {
        trace!("recv Close");
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if self.alive {
            let _ = self.clone().close_alive().wait();
        }
    }
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Session{{...}}")
    }
}
