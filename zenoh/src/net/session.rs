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
use super::routing::face::Face;
use super::*;
use async_std::sync::Arc;
use async_std::task;
use flume::{bounded, Sender};
use log::{error, trace, warn};
use protocol::{
    core::{
        queryable, rname, AtomicZInt, CongestionControl, QueryConsolidation, QueryTarget, ResKey,
        ResourceId, ZInt,
    },
    io::ZBuf,
    proto::RoutingContext,
    session::Primitives,
};
use runtime::Runtime;
use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::RwLock;
use std::time::Duration;
use uhlc::HLC;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::{zconfigurable, zerror, zpending, zresolved};

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
            RName(name) => Ok(name.clone()),
            RId(rid) => self.rid_to_resname(&rid),
            RIdWithSuffix(rid, suffix) => Ok(self.rid_to_resname(&rid)? + suffix),
        }
    }

    pub fn localkey_to_resname(&self, reskey: &ResKey) -> ZResult<String> {
        use super::ResKey::*;
        match reskey {
            RName(name) => Ok(name.clone()),
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

    pub(super) fn new(config: ConfigProperties) -> ZPendingFuture<ZResult<Session>> {
        zpending!(async {
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
    pub fn id(&self) -> ZResolvedFuture<String> {
        zresolved!(self.runtime.get_pid_str())
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
    ) -> ZResolvedFuture<Session> {
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
        zresolved!(session)
    }

    fn close_alive(self) -> ZPendingFuture<ZResult<()>> {
        zpending!(async move {
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
    /// use zenoh::net::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// session.close().await.unwrap();
    /// # })
    /// ```
    pub fn close(mut self) -> ZPendingFuture<ZResult<()>> {
        self.alive = false;
        self.close_alive()
    }

    /// Get informations about the zenoh-net [Session](Session).
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::net::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// let info = session.info();
    /// # })
    /// ```
    pub fn info(&self) -> ZResolvedFuture<InfoProperties> {
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
        zresolved!(info)
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
    /// use zenoh::net::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// let rid = session.declare_resource(&"/resource/name".into()).await.unwrap();
    /// # })
    /// ```
    pub fn declare_resource(&self, resource: &ResKey) -> ZResolvedFuture<ZResult<ResourceId>> {
        trace!("declare_resource({:?})", resource);
        let mut state = zwrite!(self.state);

        zresolved!(state.localkey_to_resname(resource).map(|resname| {
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
                    primitives.decl_resource(rid, resource);

                    rid
                }
            }
        }))
    }

    /// Undeclare the *numerical Id/resource key* association previously declared
    /// with [declare_resource](Session::declare_resource).
    ///
    /// # Arguments
    ///
    /// * `rid` - The numerical Id to unmap
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::net::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// let rid = session.declare_resource(&"/resource/name".into()).await.unwrap();
    /// session.undeclare_resource(rid).await;
    /// # })
    /// ```
    pub fn undeclare_resource(&self, rid: ResourceId) -> ZResolvedFuture<ZResult<()>> {
        trace!("undeclare_resource({:?})", rid);
        let mut state = zwrite!(self.state);
        state.local_resources.remove(&rid);

        let primitives = state.primitives.as_ref().unwrap().clone();
        drop(state);
        primitives.forget_resource(rid);

        zresolved!(Ok(()))
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
    /// use zenoh::net::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// let publisher = session.declare_publisher(&"/resource/name".into()).await.unwrap();
    /// session.write(&"/resource/name".into(), "value".as_bytes().into()).await.unwrap();
    /// # })
    /// ```
    pub fn declare_publisher(&self, resource: &ResKey) -> ZResolvedFuture<ZResult<Publisher<'_>>> {
        trace!("declare_publisher({:?})", resource);
        let mut state = zwrite!(self.state);
        let id = state.decl_id_counter.fetch_add(1, Ordering::SeqCst);
        zresolved!(state.localkey_to_resname(resource).map(|resname| {
            let pub_state = Arc::new(PublisherState {
                id,
                reskey: resource.clone(),
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
                    (!twin_pub).then(|| resource.clone())
                }
            };

            state.publishers.insert(id, pub_state.clone());

            if let Some(res) = declared_pub {
                let primitives = state.primitives.as_ref().unwrap().clone();
                drop(state);
                primitives.decl_publisher(&res, None);
            }

            Publisher {
                session: self,
                state: pub_state,
                alive: true,
            }
        }))
    }

    pub(crate) fn undeclare_publisher(&self, pid: usize) -> ZResolvedFuture<ZResult<()>> {
        let mut state = zwrite!(self.state);
        zresolved!(if let Some(pub_state) = state.publishers.remove(&pid) {
            trace!("undeclare_publisher({:?})", pub_state);
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

    fn declare_any_subscriber(
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
            reskey: reskey.clone(),
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
                        let id = self.declare_resource(&name[..pos].into()).wait()?;
                        ResKey::RIdWithSuffix(id, name[pos..].into())
                    }
                    None => {
                        let id = self.declare_resource(&name.into()).wait()?;
                        ResKey::RId(id)
                    }
                },
                reskey => reskey,
            };

            primitives.decl_subscriber(&reskey, info, None);
        }

        Ok(sub_state)
    }

    /// Declare a [Subscriber](Subscriber) for the given resource key.
    ///
    /// # Arguments
    ///
    /// * `resource` - The resource key to subscribe
    /// * `info` - The [SubInfo](SubInfo) to configure the subscription
    ///
    /// # Examples
    /// ```no_run
    /// # async_std::task::block_on(async {
    /// use zenoh::net::*;
    /// use futures::prelude::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// let sub_info = SubInfo {
    ///     reliability: Reliability::Reliable,
    ///     mode: SubMode::Push,
    ///     period: None
    /// };
    /// let mut subscriber = session.declare_subscriber(&"/resource/name".into(), &sub_info).await.unwrap();
    /// while let Some(sample) = subscriber.receiver().next().await {
    ///     println!("Received : {:?}", sample);
    /// }
    /// # })
    /// ```
    pub fn declare_subscriber(
        &self,
        reskey: &ResKey,
        info: &SubInfo,
    ) -> ZResolvedFuture<ZResult<Subscriber<'_>>> {
        trace!("declare_subscriber({:?})", reskey);
        let (sender, receiver) = bounded(*API_DATA_RECEPTION_CHANNEL_SIZE);

        zresolved!(self
            .declare_any_subscriber(reskey, SubscriberInvoker::Sender(sender), info)
            .map(|sub_state| Subscriber {
                session: self,
                state: sub_state,
                alive: true,
                receiver: SampleReceiver::new(receiver),
            }))
    }

    /// Declare a [CallbackSubscriber](CallbackSubscriber) for the given resource key.
    ///
    /// # Arguments
    ///
    /// * `resource` - The resource key to subscribe
    /// * `info` - The [SubInfo](SubInfo) to configure the subscription
    /// * `data_handler` - The callback that will be called on each data reception
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::net::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// let sub_info = SubInfo {
    ///     reliability: Reliability::Reliable,
    ///     mode: SubMode::Push,
    ///     period: None
    /// };
    /// let subscriber = session.declare_callback_subscriber(&"/resource/name".into(), &sub_info,
    ///     |sample| { println!("Received : {} {}", sample.res_name, sample.payload); }
    /// ).await.unwrap();
    /// # })
    /// ```
    pub fn declare_callback_subscriber<DataHandler>(
        &self,
        reskey: &ResKey,
        info: &SubInfo,
        data_handler: DataHandler,
    ) -> ZResolvedFuture<ZResult<CallbackSubscriber<'_>>>
    where
        DataHandler: FnMut(Sample) + Send + Sync + 'static,
    {
        trace!("declare_callback_subscriber({:?})", reskey);
        let dhandler = Arc::new(RwLock::new(data_handler));
        zresolved!(self
            .declare_any_subscriber(reskey, SubscriberInvoker::Handler(dhandler), info)
            .map(|sub_state| CallbackSubscriber {
                session: self,
                state: sub_state,
                alive: true,
            }))
    }

    /// This is an experimental API.
    pub fn declare_local_subscriber(
        &self,
        reskey: &ResKey,
    ) -> ZResolvedFuture<ZResult<Subscriber<'_>>> {
        trace!("declare_subscriber({:?})", reskey);
        let (sender, receiver) = bounded(*API_DATA_RECEPTION_CHANNEL_SIZE);
        let mut state = zwrite!(self.state);
        let id = state.decl_id_counter.fetch_add(1, Ordering::SeqCst);
        zresolved!(state
            .localkey_to_resname(reskey)
            .map(|resname| {
                let sub_state = Arc::new(SubscriberState {
                    id,
                    reskey: reskey.clone(),
                    resname,
                    invoker: SubscriberInvoker::Sender(sender),
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
                sub_state
            })
            .map(|sub_state| Subscriber {
                session: self,
                state: sub_state,
                alive: true,
                receiver: SampleReceiver::new(receiver),
            }))
    }

    pub(crate) fn undeclare_subscriber(&self, sid: usize) -> ZResolvedFuture<ZResult<()>> {
        let mut state = zwrite!(self.state);
        zresolved!(if let Some(sub_state) = state.subscribers.remove(&sid) {
            trace!("undeclare_subscriber({:?})", sub_state);
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
            trace!("undeclare_subscriber({:?})", sub_state);
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

    fn compute_local_queryable_kind(state: &mut SessionState, key: &ResKey) -> Option<ZInt> {
        let res_name = state.localkey_to_resname(key).unwrap();
        state.queryables.values().fold(None, |accu, q| {
            if state.localkey_to_resname(&q.reskey).unwrap() == res_name {
                Some(accu.unwrap_or(0) | q.kind)
            } else {
                accu
            }
        })
    }

    /// Declare a [Queryable](Queryable) for the given resource key.
    ///
    /// # Arguments
    ///
    /// * `resource` - The resource key the [Queryable](Queryable) will reply to
    /// * `kind` - The kind of [Queryable](Queryable)
    ///
    /// # Examples
    /// ```no_run
    /// # async_std::task::block_on(async {
    /// use zenoh::net::*;
    /// use zenoh::net::queryable::EVAL;
    /// use futures::prelude::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// let mut queryable = session.declare_queryable(&"/resource/name".into(), EVAL).await.unwrap();
    /// while let Some(query) = queryable.receiver().next().await {
    ///     query.reply_async(Sample{
    ///         res_name: "/resource/name".to_string(),
    ///         payload: "value".as_bytes().into(),
    ///         data_info: None,
    ///     }).await;
    /// }
    /// # })
    /// ```
    pub fn declare_queryable(
        &self,
        resource: &ResKey,
        kind: ZInt,
    ) -> ZResolvedFuture<ZResult<Queryable<'_>>> {
        trace!("declare_queryable({:?}, {:?})", resource, kind);
        let mut state = zwrite!(self.state);
        let id = state.decl_id_counter.fetch_add(1, Ordering::SeqCst);
        let (sender, receiver) = bounded(*API_QUERY_RECEPTION_CHANNEL_SIZE);
        let qable_state = Arc::new(QueryableState {
            id,
            reskey: resource.clone(),
            kind,
            sender,
        });
        let computed_kind = Session::compute_local_queryable_kind(&mut state, &qable_state.reskey);

        state.queryables.insert(id, qable_state.clone());

        let send_kind = match computed_kind {
            Some(computed_kind) => {
                if computed_kind != computed_kind | kind {
                    Some(computed_kind | kind)
                } else {
                    None
                }
            }
            None => Some(kind),
        };

        if let Some(send_kind) = send_kind {
            let primitives = state.primitives.as_ref().unwrap().clone();
            drop(state);
            primitives.decl_queryable(resource, send_kind, None);
        }

        zresolved!(Ok(Queryable {
            session: self,
            state: qable_state,
            alive: true,
            receiver: QueryReceiver::new(receiver),
        }))
    }

    pub(crate) fn undeclare_queryable(&self, qid: usize) -> ZResolvedFuture<ZResult<()>> {
        let mut state = zwrite!(self.state);
        zresolved!(if let Some(qable_state) = state.queryables.remove(&qid) {
            trace!("undeclare_queryable({:?})", qable_state);
            let computed_kind =
                Session::compute_local_queryable_kind(&mut state, &qable_state.reskey);
            if let Some(computed_kind) = computed_kind {
                if computed_kind != computed_kind | qable_state.kind {
                    // There still exist Queryables on the same ResKey and the merge kind changed
                    let primitives = state.primitives.as_ref().unwrap();
                    primitives.decl_queryable(&qable_state.reskey, computed_kind, None);
                }
            } else {
                // There are no more Queryables on the same ResKey.
                let primitives = state.primitives.as_ref().unwrap();
                primitives.forget_queryable(&qable_state.reskey, None);
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
    /// use zenoh::net::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// session.write(&"/resource/name".into(), "value".as_bytes().into()).await.unwrap();
    /// # })
    /// ```
    pub fn write(&self, resource: &ResKey, payload: ZBuf) -> ZResolvedFuture<ZResult<()>> {
        trace!("write({:?}, [...])", resource);
        let state = zread!(self.state);
        let primitives = state.primitives.as_ref().unwrap().clone();
        let local_routing = state.local_routing;
        drop(state);

        // if we can create a local timestamp, send it into a DataInfo
        let data_info = self.runtime.new_timestamp().map(|ts| {
            let mut data_info = DataInfo::new();
            data_info.timestamp = Some(ts);
            data_info
        });

        primitives.send_data(
            resource,
            payload.clone(),
            Reliability::Reliable, // @TODO: need to check subscriptions to determine the right reliability value
            CongestionControl::default(), // Default congestion control when writing data
            data_info.clone(),
            None,
        );
        if local_routing {
            self.handle_data(true, resource, data_info, payload);
        }
        zresolved!(Ok(()))
    }

    /// Write data with options.
    ///
    /// # Arguments
    ///
    /// * `resource` - The resource key to write
    /// * `payload` - The value to write
    /// * `encoding` - The encoding of the value
    /// * `kind` - The kind of value
    /// * `congestion_control` - The value for the congestion control
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::net::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// session.write_ext(&"/resource/name".into(), "value".as_bytes().into(), encoding::TEXT_PLAIN, data_kind::PUT, CongestionControl::Drop).await.unwrap();
    /// # })
    /// ```
    pub fn write_ext(
        &self,
        resource: &ResKey,
        payload: ZBuf,
        encoding: ZInt,
        kind: ZInt,
        congestion_control: CongestionControl,
    ) -> ZResolvedFuture<ZResult<()>> {
        trace!("write_ext({:?}, [...])", resource);
        let state = zread!(self.state);
        let primitives = state.primitives.as_ref().unwrap().clone();
        let local_routing = state.local_routing;
        drop(state);

        let mut info = protocol::proto::DataInfo::new();
        info.kind = Some(kind);
        info.encoding = Some(encoding);
        info.timestamp = self.runtime.new_timestamp();
        let data_info = Some(info);

        primitives.send_data(
            resource,
            payload.clone(),
            Reliability::Reliable, // TODO: need to check subscriptions to determine the right reliability value
            congestion_control,
            data_info.clone(),
            None,
        );
        if local_routing {
            self.handle_data(true, resource, data_info, payload);
        }
        zresolved!(Ok(()))
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
                handler(Sample {
                    res_name,
                    payload,
                    data_info,
                });
            }
            SubscriberInvoker::Sender(sender) => {
                if let Err(e) = sender.send(Sample {
                    res_name,
                    payload,
                    data_info,
                }) {
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

    pub(crate) fn pull(&self, reskey: &ResKey) -> ZResolvedFuture<ZResult<()>> {
        trace!("pull({:?})", reskey);
        let state = zread!(self.state);
        let primitives = state.primitives.as_ref().unwrap().clone();
        drop(state);
        primitives.send_pull(true, reskey, 0, &None);
        zresolved!(Ok(()))
    }

    /// Query data from the matching queryables in the system.
    ///
    /// # Arguments
    ///
    /// * `resource` - The resource key to query
    /// * `predicate` - An indication to matching queryables about the queried data
    /// * `target` - The kind of queryables that should be target of this query
    /// * `consolidation` - The kind of consolidation that should be applied on replies
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::net::*;
    /// use futures::prelude::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// let mut replies = session.query(
    ///     &"/resource/name".into(),
    ///     "predicate",
    ///     QueryTarget::default(),
    ///     QueryConsolidation::default()
    /// ).await.unwrap();
    /// while let Some(reply) = replies.next().await {
    ///     println!(">> Received {:?}", reply.data);
    /// }
    /// # })
    /// ```
    pub fn query(
        &self,
        resource: &ResKey,
        predicate: &str,
        target: QueryTarget,
        consolidation: QueryConsolidation,
    ) -> ZResolvedFuture<ZResult<ReplyReceiver>> {
        trace!(
            "query({:?}, {:?}, {:?}, {:?})",
            resource,
            predicate,
            target,
            consolidation
        );
        let mut state = zwrite!(self.state);
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
            resource,
            predicate,
            qid,
            target.clone(),
            consolidation.clone(),
            None,
        );
        if local_routing {
            self.handle_query(true, resource, predicate, qid, target, consolidation);
        }

        zresolved!(Ok(ReplyReceiver::new(rep_receiver)))
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
                while let Some((kind, sample)) = rep_receiver.stream().next().await {
                    this.send_reply_data(
                        qid,
                        kind,
                        pid.clone(),
                        ResKey::RName(sample.res_name),
                        sample.data_info,
                        sample.payload,
                    );
                }
                this.send_reply_final(qid);
            });
        } else {
            task::spawn(async move {
                while let Some((kind, sample)) = rep_receiver.stream().next().await {
                    primitives.send_reply_data(
                        qid,
                        kind,
                        pid.clone(),
                        ResKey::RName(sample.res_name),
                        sample.data_info,
                        sample.payload,
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
        _routing_context: Option<RoutingContext>,
    ) {
        trace!("recv Decl Queryable {:?}", _reskey);
    }

    fn forget_queryable(&self, _reskey: &ResKey, _routing_context: Option<RoutingContext>) {
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
                    data: Sample {
                        res_name,
                        payload,
                        data_info,
                    },
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
                                if new_reply.data.data_info > reply.data.data_info {
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
                                if new_reply.data.data_info > reply.data.data_info {
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
