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
use async_std::sync::{Arc, channel, Sender, Receiver};
use async_std::task;
use async_trait::async_trait;
use futures::prelude::*;
use std::fmt;
use std::sync::atomic::{AtomicUsize, AtomicU64, Ordering};
use std::collections::HashMap;
use async_std::sync::RwLock;
use log::{error, warn, trace};
use zenoh_protocol:: {
    core::{ rname, ResourceId, ResKey, QueryTarget, QueryConsolidation, queryable },
    io::RBuf,
    proto::Primitives,
};
use zenoh_router::runtime::Runtime;
use zenoh_util::{zerror, zconfigurable};
use zenoh_util::core::{ZResult, ZError, ZErrorKind};
use super::*;
use super::properties::*;

zconfigurable! {
    static ref API_DATA_RECEPTION_CHANNEL_SIZE: usize = 256;
    static ref API_QUERY_RECEPTION_CHANNEL_SIZE: usize = 256;
    static ref API_REPLY_EMISSION_CHANNEL_SIZE: usize = 256;
    static ref API_REPLY_RECEPTION_CHANNEL_SIZE: usize = 256;
}

pub(crate) struct SessionState {
    primitives:         Option<Arc<dyn Primitives + Send + Sync>>, // @TODO replace with MaybeUninit ??
    rid_counter:        AtomicUsize,  // @TODO: manage rollover and uniqueness
    qid_counter:        AtomicU64,
    decl_id_counter:    AtomicUsize,
    local_resources:    HashMap<ResourceId, String>,
    remote_resources:   HashMap<ResourceId, String>,
    publishers:         HashMap<Id, Publisher>,
    subscribers:        HashMap<Id, Subscriber>,
    callback_subscribers: HashMap<Id, CallbackSubscriber>,
    queryables:         HashMap<Id, Queryable>,
    queries:            HashMap<ZInt, Sender<Reply>>,
}

impl SessionState {
    pub(crate) fn new() -> SessionState {
        SessionState  {
            primitives:         None,
            rid_counter:        AtomicUsize::new(1),  // Note: start at 1 because 0 is reserved for NO_RESOURCE
            qid_counter:        AtomicU64::new(0),
            decl_id_counter:    AtomicUsize::new(0),
            local_resources:    HashMap::new(),
            remote_resources:   HashMap::new(),
            publishers:         HashMap::new(),
            subscribers:        HashMap::new(),
            callback_subscribers: HashMap::new(),
            queryables:         HashMap::new(),
            queries:            HashMap::new(),
        }
    }
}

impl SessionState {
    pub fn reskey_to_resname(&self, reskey: &ResKey) -> ZResult<String> {
        use super::ResKey::*;
        match reskey {
            RName(name) => Ok(name.clone()),
            RId(rid) => {
                match self.remote_resources.get(&rid) {
                    Some(name) => Ok(name.clone()),
                    None => {
                        match self.local_resources.get(&rid) {
                            Some(name) => Ok(name.clone()),
                            None => zerror!(ZErrorKind::UnkownResourceId{rid: format!("{}", rid)})
                        }
                    }
                }
            },
            RIdWithSuffix(rid, suffix) => {
                match self.remote_resources.get(&rid) {
                    Some(name) => Ok(name.clone() + suffix),
                    None => {
                        match self.local_resources.get(&rid) {
                            Some(name) => Ok(name.clone() + suffix),
                            None => zerror!(ZErrorKind::UnkownResourceId{rid: format!("{}", rid)})
                        }
                    }
                }
            }
        }
    }

    pub fn localkey_to_resname(&self, reskey: &ResKey) -> ZResult<String> {
        use super::ResKey::*;
        match reskey {
            RName(name) => Ok(name.clone()),
            RId(rid) => {
                match self.local_resources.get(&rid) {
                    Some(name) => Ok(name.clone()),
                    None => zerror!(ZErrorKind::UnkownResourceId{rid: format!("{}", rid)})
                }
            },
            RIdWithSuffix(rid, suffix) => {
                match self.local_resources.get(&rid) {
                    Some(name) => Ok(name.clone() + suffix),
                    None => zerror!(ZErrorKind::UnkownResourceId{rid: format!("{}", rid)})
                }
            }
        }
    }
}

impl fmt::Debug for SessionState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "SessionState{{ subscribers: {} }}",
            self.subscribers.len())
    }
}

/// A zenoh-net session.
#[derive(Clone)]
pub struct Session {
    runtime: Runtime,
    state: Arc<RwLock<SessionState>>,
}

impl Session {

    pub(super) async fn new(config: Config, _ps: Option<Properties>) -> Session {

        let runtime = match Runtime::new(0, config).await {
            Ok(runtime) => runtime,
            _ => std::process::exit(-1),
        };

        let session = Self::init(runtime).await;

        // Workaround for the declare_and_shoot problem
        task::sleep(std::time::Duration::from_millis(200)).await;

        session
    }

    /// Initialize a Session with an existing Runtime.
    /// This operation is used by the plugins to share the same Runtime than the router.
    #[doc(hidden)]
    pub async fn init(runtime: Runtime) -> Session {
        let broker = runtime.read().await.broker.clone();
        let state = Arc::new(RwLock::new(SessionState::new()));
        let session = Session{runtime, state: state.clone()};
        let primitives = Some(broker.new_primitives(Arc::new(session.clone())).await);
        state.write().await.primitives = primitives;
        session
    }

    /// Close the zenoh-net session.
    /// 
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::net::*;
    ///
    /// let session = open(Config::peer(), None).await.unwrap();
    /// session.close();
    /// # })
    /// ```
    pub async fn close(&self) -> ZResult<()> {
        // @TODO: implement
        trace!("close()");
        self.runtime.close().await?;

        let primitives = self.state.write().await.primitives.as_ref().unwrap().clone();
        primitives.close().await;

        Ok(())
    }

    /// Get informations about the zenoh-net session.
    /// 
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::net::*;
    ///
    /// let session = open(Config::peer(), None).await.unwrap();
    /// let info = session.info();
    /// # })
    /// ```
    pub async fn info(&self) -> Properties {
        // @TODO: implement
        trace!("info()");
        let mut info = Properties::new();
        let runtime = self.runtime.read().await;
        info.push((ZN_INFO_PID_KEY, runtime.pid.id.clone()));
        for session in runtime.orchestrator.manager.get_sessions().await {
            if let Ok(what) = session.get_whatami() {
                if what & whatami::PEER != 0 {
                    if let Ok(peer) = session.get_pid() {
                        info.push((ZN_INFO_PEER_PID_KEY, peer.id));
                    }
                }
            }
        }
        if runtime.orchestrator.whatami & whatami::BROKER != 0 {
            info.push((ZN_INFO_ROUTER_PID_KEY, runtime.pid.id.clone()));
        }
        for session in runtime.orchestrator.manager.get_sessions().await {
            if let Ok(what) = session.get_whatami() {
                if what & whatami::BROKER != 0 {
                    if let Ok(peer) = session.get_pid() {
                        info.push((ZN_INFO_ROUTER_PID_KEY, peer.id));
                    }
                }
            }
        }
        info
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
    /// let session = open(Config::peer(), None).await.unwrap();
    /// let rid = session.declare_resource(&"/resource/name".into()).await.unwrap();
    /// # })
    /// ```
    pub async fn declare_resource(&self, resource: &ResKey) -> ZResult<ResourceId> {
        trace!("declare_resource({:?})", resource);
        let mut state = self.state.write().await;
        let rid = state.rid_counter.fetch_add(1, Ordering::SeqCst) as ZInt;
        let rname = state.localkey_to_resname(resource)?;
        state.local_resources.insert(rid, rname);

        let primitives = state.primitives.as_ref().unwrap().clone();
        drop(state);
        primitives.resource(rid, resource).await;

        Ok(rid)
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
    /// let session = open(Config::peer(), None).await.unwrap();
    /// let rid = session.declare_resource(&"/resource/name".into()).await.unwrap();
    /// session.undeclare_resource(rid).await;
    /// # })
    /// ```
    pub async fn undeclare_resource(&self, rid: ResourceId) -> ZResult<()> {
        trace!("undeclare_resource({:?})", rid);
        let mut state = self.state.write().await;
        state.local_resources.remove(&rid);

        let primitives = state.primitives.as_ref().unwrap().clone();
        drop(state);
        primitives.forget_resource(rid).await;

        Ok(())
    }

    /// Declare a [Publisher](Publisher) for the given resource key.
    /// 
    /// Resources written with the given key will only be sent on the network 
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
    /// let session = open(Config::peer(), None).await.unwrap();
    /// let publisher = session.declare_publisher(&"/resource/name".into()).await.unwrap();
    /// # })
    /// ```
    pub async fn declare_publisher(&self, resource: &ResKey) -> ZResult<Publisher> {
        trace!("declare_publisher({:?})", resource);
        let mut state = self.state.write().await;

        let id = state.decl_id_counter.fetch_add(1, Ordering::SeqCst);
        let publ = Publisher{ id, reskey: resource.clone() };
        state.publishers.insert(id, publ.clone());

        let primitives = state.primitives.as_ref().unwrap().clone();
        drop(state);
        primitives.publisher(resource).await;

        Ok(publ)
    }

    /// Undeclare a [Publisher](Publisher) previously declared with [declare_publisher](Session::declare_publisher).
    /// 
    /// # Arguments
    ///
    /// * `resource` - The [Publisher](Publisher) to undeclare
    /// 
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::net::*;
    ///
    /// let session = open(Config::peer(), None).await.unwrap();
    /// let publisher = session.declare_publisher(&"/resource/name".into()).await.unwrap();
    /// session.undeclare_publisher(publisher).await;
    /// # })
    /// ```
    pub async fn undeclare_publisher(&self, publisher: Publisher) -> ZResult<()> {
        trace!("undeclare_publisher({:?})", publisher);
        let mut state = self.state.write().await;
        state.publishers.remove(&publisher.id);

        // Note: there might be several Publishers on the same ResKey.
        // Before calling forget_publisher(reskey), check if this was the last one.
        if !state.publishers.values().any(|p| p.reskey == publisher.reskey) {
            let primitives = state.primitives.as_ref().unwrap().clone();
            drop(state);
            primitives.forget_publisher(&publisher.reskey).await;
        }
        Ok(())
    }

    /// Declare a [Subscriber](Subscriber) for the given resource key.
    /// 
    /// The returned [Subscriber](Subscriber) implements the Stream trait.
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
    /// let session = open(Config::peer(), None).await.unwrap();
    /// let sub_info = SubInfo {
    ///     reliability: Reliability::Reliable,
    ///     mode: SubMode::Push,
    ///     period: None
    /// };
    /// let mut subscriber = session.declare_subscriber(&"/resource/name".into(), &sub_info).await.unwrap();
    /// while let Some(sample) = subscriber.next().await {
    ///     println!("Received : {:?}", sample);
    /// }
    /// # })
    /// ```
    pub async fn declare_subscriber(&self, resource: &ResKey, info: &SubInfo) -> ZResult<Subscriber>
    {
        trace!("declare_subscriber({:?})", resource);
        let mut state = self.state.write().await;
        let id = state.decl_id_counter.fetch_add(1, Ordering::SeqCst);
        let resname = state.localkey_to_resname(resource)?;
        let (sender, receiver) = channel(*API_DATA_RECEPTION_CHANNEL_SIZE);
        let sub = Subscriber{ id, reskey: resource.clone(), resname, session: self.clone(), sender, receiver };
        state.subscribers.insert(id, sub.clone());

        let primitives = state.primitives.as_ref().unwrap().clone();
        drop(state);
        primitives.subscriber(resource, info).await;

        Ok(sub)
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
    /// let session = open(Config::peer(), None).await.unwrap();
    /// let sub_info = SubInfo {
    ///     reliability: Reliability::Reliable,
    ///     mode: SubMode::Push,
    ///     period: None
    /// };
    /// let subscriber = session.declare_callback_subscriber(&"/resource/name".into(), &sub_info, 
    ///     |res_name, payload, _info| { println!("Received : {} {}", res_name, payload); }
    /// ).await.unwrap();
    /// # })
    /// ```
    pub async fn declare_callback_subscriber<DataHandler>(&self, resource: &ResKey, info: &SubInfo, data_handler: DataHandler) -> ZResult<CallbackSubscriber>
        where DataHandler: FnMut(/*res_name:*/ &str, /*payload:*/ RBuf, /*data_info:*/ Option<RBuf>) + Send + Sync + 'static
    {
        trace!("declare_callback_subscriber({:?})", resource);
        let mut state = self.state.write().await;
        let id = state.decl_id_counter.fetch_add(1, Ordering::SeqCst);
        let resname = state.localkey_to_resname(resource)?;
        let dhandler = Arc::new(RwLock::new(data_handler));
        let sub = CallbackSubscriber{ id, reskey: resource.clone(), resname, session: self.clone(), dhandler };
        state.callback_subscribers.insert(id, sub.clone());

        let primitives = state.primitives.as_ref().unwrap().clone();
        drop(state);
        primitives.subscriber(resource, info).await;

        Ok(sub)
    }

    /// Undeclare a [Subscriber](Subscriber) previously declared with [declare_subscriber](Session::declare_subscriber).
    /// 
    /// # Arguments
    ///
    /// * `subscriber` - The [Subscriber](Subscriber) to undeclare
    /// 
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::net::*;
    ///
    /// let session = open(Config::peer(), None).await.unwrap();
    /// # let sub_info = SubInfo {
    /// #     reliability: Reliability::Reliable,
    /// #     mode: SubMode::Push,
    /// #     period: None
    /// # };
    /// let subscriber = session.declare_subscriber(&"/resource/name".into(), &sub_info).await.unwrap();
    /// session.undeclare_subscriber(subscriber).await;
    /// # })
    /// ```
    pub async fn undeclare_subscriber(&self, subscriber: Subscriber) -> ZResult<()>
    {
        trace!("undeclare_subscriber({:?})", subscriber);
        let mut state = self.state.write().await;
        state.subscribers.remove(&subscriber.id);

        // Note: there might be several Subscribers on the same ResKey.
        // Before calling forget_subscriber(reskey), check if this was the last one.
        if !state.callback_subscribers.values().any(|s| s.reskey == subscriber.reskey)
           && !state.subscribers.values().any(|s| s.reskey == subscriber.reskey) {
            let primitives = state.primitives.as_ref().unwrap().clone();
            drop(state);
            primitives.forget_subscriber(&subscriber.reskey).await;
        }
        Ok(())
    }
    
    /// Undeclare a [CallbackSubscriber](CallbackSubscriber) previously declared with [declare_callback_subscriber](Session::declare_callback_subscriber).
    /// 
    /// # Arguments
    ///
    /// * `subscriber` - The [CallbackSubscriber](CallbackSubscriber) to undeclare
    /// 
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::net::*;
    ///
    /// let session = open(Config::peer(), None).await.unwrap();
    /// # let sub_info = SubInfo {
    /// #     reliability: Reliability::Reliable,
    /// #     mode: SubMode::Push,
    /// #     period: None
    /// # };
    /// # fn data_handler(res_name: &str, payload: RBuf, _info: Option<RBuf>) { println!("Received : {} {}", res_name, payload); };
    /// let subscriber = session.declare_callback_subscriber(&"/resource/name".into(), &sub_info, data_handler).await.unwrap();
    /// session.undeclare_callback_subscriber(subscriber).await;
    /// # })
    /// ```
    pub async fn undeclare_callback_subscriber(&self, subscriber: CallbackSubscriber) -> ZResult<()>
    {
        trace!("undeclare_callback_subscriber({:?})", subscriber);
        let mut state = self.state.write().await;
        state.callback_subscribers.remove(&subscriber.id);

        // Note: there might be several Subscribers on the same ResKey.
        // Before calling forget_subscriber(reskey), check if this was the last one.
        if !state.callback_subscribers.values().any(|s| s.reskey == subscriber.reskey)
           && !state.subscribers.values().any(|s| s.reskey == subscriber.reskey) {
            let primitives = state.primitives.as_ref().unwrap().clone();
            drop(state);
            primitives.forget_subscriber(&subscriber.reskey).await;
        }
        Ok(())
    }

    /// Declare a [Queryable](Queryable) for the given resource key.
    /// 
    /// The returned [Queryable](Queryable) implements the Stream trait.
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
    /// let session = open(Config::peer(), None).await.unwrap();
    /// let mut queryable = session.declare_queryable(&"/resource/name".into(), EVAL).await.unwrap();
    /// while let Some(query) = queryable.next().await { 
    ///     query.replies_sender.send(Sample{
    ///         res_name: "/resource/name".to_string(),
    ///         payload: "value".as_bytes().into(),
    ///         data_info: None,
    ///     }).await;
    /// }
    /// # })
    /// ```
    pub async fn declare_queryable(&self, resource: &ResKey, kind: ZInt) -> ZResult<Queryable>
    {
        trace!("declare_queryable({:?}, {:?})", resource, kind);
        let mut state = self.state.write().await;
        let id = state.decl_id_counter.fetch_add(1, Ordering::SeqCst);
        let (req_sender, req_receiver) = channel(*API_QUERY_RECEPTION_CHANNEL_SIZE);
        let qable = Queryable{ id, reskey: resource.clone(), kind, req_sender, req_receiver };
        state.queryables.insert(id, qable.clone());

        let primitives = state.primitives.as_ref().unwrap().clone();
        drop(state);
        primitives.queryable(resource).await;

        Ok(qable)
    }

    /// Undeclare a [Queryable](Queryable) previously declared with [declare_queryable](Session::declare_queryable).
    /// 
    /// # Arguments
    ///
    /// * `queryable` - The [Queryable](Queryable) to undeclare
    /// 
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::net::*;
    /// use zenoh::net::queryable::EVAL;
    ///
    /// let session = open(Config::peer(), None).await.unwrap();
    /// let queryable = session.declare_queryable(&"/resource/name".into(), EVAL).await.unwrap();
    /// session.undeclare_queryable(queryable).await;
    /// # })
    /// ```
    pub async fn undeclare_queryable(&self, queryable: Queryable) -> ZResult<()> {
        trace!("undeclare_queryable({:?})", queryable);
        let mut state = self.state.write().await;
        state.queryables.remove(&queryable.id);

        // Note: there might be several Queryables on the same ResKey.
        // Before calling forget_eval(reskey), check if this was the last one.
        if !state.queryables.values().any(|e| e.reskey == queryable.reskey) {
            let primitives = state.primitives.as_ref().unwrap();
            primitives.forget_queryable(&queryable.reskey).await;
        }
        Ok(())
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
    /// let session = open(Config::peer(), None).await.unwrap();
    /// session.write(&"/resource/name".into(), "value".as_bytes().into()).await.unwrap();
    /// # })
    /// ```
    pub async fn write(&self, resource: &ResKey, payload: RBuf) -> ZResult<()> {
        trace!("write({:?}, [...])", resource);
        let state = self.state.read().await;
        let primitives = state.primitives.as_ref().unwrap().clone();
        drop(state);
        primitives.data(resource, true, &None, payload).await;
        Ok(())
    }

    /// Write data with options.
    /// 
    /// # Arguments
    ///
    /// * `resource` - The resource key to write
    /// * `payload` - The value to write
    /// * `encoding` - The encoding of the value
    /// * `kind` - The kind of value
    /// 
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::net::*;
    ///
    /// let session = open(Config::peer(), None).await.unwrap();
    /// session.write_ext(&"/resource/name".into(), "value".as_bytes().into(), encoding::TEXT_PLAIN, data_kind::PUT).await.unwrap();
    /// # })
    /// ```
    pub async fn write_ext(&self, resource: &ResKey, payload: RBuf, encoding: ZInt, kind: ZInt) -> ZResult<()> {
        trace!("write_ext({:?}, [...])", resource);
        let state = self.state.read().await;
        let primitives = state.primitives.as_ref().unwrap().clone();
        drop(state);
        let info = zenoh_protocol::proto::DataInfo{
            source_id: None,
            source_sn: None,
            fist_broker_id: None,
            fist_broker_sn: None,
            timestamp: None,
            kind: Some(kind),
            encoding: Some(encoding),
        };
        let mut infobuf = zenoh_protocol::io::WBuf::new(64, false);
        infobuf.write_datainfo(&info);
        primitives.data(resource, true, &Some(infobuf.into()), payload).await;
        Ok(())
    }

    pub(crate) async fn pull(&self, reskey: &ResKey) -> ZResult<()> {
        trace!("pull({:?})", reskey);
        let state = self.state.read().await;
        let primitives = state.primitives.as_ref().unwrap().clone();
        drop(state);
        primitives.pull(true, reskey, 0, &None).await;
        Ok(())
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
    /// #![feature(async_closure)]
    /// # async_std::task::block_on(async {
    /// use zenoh::net::*;
    /// use futures::prelude::*;
    ///
    /// let session = open(Config::peer(), None).await.unwrap();
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
    pub async fn query(&self,
        resource:        &ResKey,
        predicate:       &str,
        target:          QueryTarget,
        consolidation:   QueryConsolidation
    ) -> ZResult<Receiver<Reply>>
    {
        trace!("query({:?}, {:?}, {:?}, {:?})", resource, predicate, target, consolidation);
        let mut state = self.state.write().await;
        let qid = state.qid_counter.fetch_add(1, Ordering::SeqCst);
        let (rep_sender, rep_receiver) = channel(*API_REPLY_RECEPTION_CHANNEL_SIZE);
        state.queries.insert(qid, rep_sender);

        let primitives = state.primitives.as_ref().unwrap().clone();
        drop(state);
        primitives.query(resource, predicate, qid, target, consolidation).await;

        Ok(rep_receiver)
    }
}

#[async_trait]
impl Primitives for Session {

    async fn resource(&self, rid: ZInt, reskey: &ResKey) {
        trace!("recv Resource {} {:?}", rid, reskey);
        let state = &mut self.state.write().await;
        match state.reskey_to_resname(reskey) {
            Ok(name) => {state.remote_resources.insert(rid, name);}
            Err(_) => error!("Received Resource for unkown reskey: {}", reskey)
        }
    }

    async fn forget_resource(&self, _rid: ZInt) {
        trace!("recv Forget Resource {}", _rid);
    }

    async fn publisher(&self, _reskey: &ResKey) {
        trace!("recv Publisher {:?}", _reskey);
    }

    async fn forget_publisher(&self, _reskey: &ResKey) {
        trace!("recv Forget Publisher {:?}", _reskey);
    }

    async fn subscriber(&self, _reskey: &ResKey, _sub_info: &SubInfo) {
        trace!("recv Subscriber {:?} , {:?}", _reskey, _sub_info);
    }

    async fn forget_subscriber(&self, _reskey: &ResKey) {
        trace!("recv Forget Subscriber {:?}", _reskey);
    }

    async fn queryable(&self, _reskey: &ResKey) {
        trace!("recv Queryable {:?}", _reskey);
    }

    async fn forget_queryable(&self, _reskey: &ResKey) {
        trace!("recv Forget Queryable {:?}", _reskey);
    }

    async fn data(&self, reskey: &ResKey, _reliable: bool, info: &Option<RBuf>, payload: RBuf) {
        trace!("recv Data {:?} {:?} {:?} {:?}", reskey, _reliable, info, payload);
        let (resname, senders) = {
            let state = self.state.read().await;
            match state.reskey_to_resname(reskey) {
                Ok(resname) => {
                    // Call matching callback_subscribers
                    for sub in state.callback_subscribers.values() {
                        if rname::intersect(&sub.resname, &resname) {
                            let handler = &mut *sub.dhandler.write().await;
                            handler(&resname, payload.clone(), info.clone());
                        }
                    }
                    // Collect matching subscribers
                    let subs = state.subscribers.values().filter(|sub|rname::intersect(&sub.resname, &resname))
                        .map(|sub|sub.sender.clone()).collect::<Vec<Sender<Sample>>>();
                    (resname, subs)
                },
                Err(err) => {
                    error!("Received Data for unkown reskey: {}", err);
                    return
                }
            }
        };
        for sender in senders {
            sender.send( Sample {
                res_name: resname.clone(),
                payload: payload.clone(),
                data_info: info.clone(),
            }).await;
        }
    }

    async fn query(&self, reskey: &ResKey, predicate: &str, qid: ZInt, target: QueryTarget, _consolidation: QueryConsolidation) {
        trace!("recv Query {:?} {:?} {:?} {:?}", reskey, predicate, target, _consolidation);
        let (primitives, resname, kinds_and_senders) = {
            let state = self.state.read().await;
            match state.reskey_to_resname(reskey) {
                Ok(resname) => {
                    let kinds_and_senders = state.queryables.values().filter(|queryable| {
                        match state.reskey_to_resname(&queryable.reskey) {
                            Ok(qablname) => {
                                rname::intersect(&qablname, &resname) 
                                && ((queryable.kind == queryable::ALL_KINDS || target.kind  == queryable::ALL_KINDS) 
                                    || (queryable.kind & target.kind != 0))
                            },
                            Err(err) => {error!("{}. Internal error (queryable reskey to resname failed).", err); false}
                        }
                    }).map(|qable|(qable.kind, qable.req_sender.clone())).collect::<Vec<(ZInt, Sender<Query>)>>();
                    (state.primitives.as_ref().unwrap().clone(), resname, kinds_and_senders)
                }
                Err(err) => {
                    error!("Received Query for unkown reskey: {}", err);
                    return
                }
            }
        };

        let predicate = predicate.to_string();
        let (rep_sender, mut rep_receiver) = channel(*API_REPLY_EMISSION_CHANNEL_SIZE);
        let pid = self.runtime.read().await.pid.clone(); // @TODO build/use prebuilt specific pid
            
        for (kind, req_sender) in kinds_and_senders {
            req_sender.send( Query {
                res_name: resname.clone(),
                predicate: predicate.clone(),
                replies_sender: RepliesSender{ kind, sender: rep_sender.clone() }
            }).await;
        }
        drop(rep_sender); // all senders need to be dropped for the channel to close

        task::spawn( async move { // router is not re-entrant
            while let Some((kind, sample)) = rep_receiver.next().await {
                primitives.reply_data(qid, kind, pid.clone(), ResKey::RName(sample.res_name), sample.data_info, sample.payload).await;
            }
            primitives.reply_final(qid).await;
        });
    }

    async fn reply_data(&self, qid: ZInt, source_kind: ZInt, replier_id: PeerId, reskey: ResKey, data_info: Option<RBuf>, payload: RBuf) {
        trace!("recv ReplyData {:?} {:?} {:?} {:?} {:?} {:?}", qid, source_kind, replier_id, reskey, data_info, payload);
        let (rep_sender, reply) = {
            let state = &mut self.state.write().await;
            let rep_sender = match state.queries.get(&qid) {
                Some(rep_sender) => rep_sender.clone(),
                None => {
                    warn!("Received Reply for unkown Query: {}", qid);
                    return
                }
            };
            let res_name = match state.reskey_to_resname(&reskey) {
                Ok(name) => name,
                Err(e) => {
                    error!("Received Reply for unkown reskey: {}", e);
                    return
                }
            };
            (rep_sender, Reply { data: Sample{res_name, payload, data_info}, source_kind, replier_id })
        };
        rep_sender.send(reply).await;
    }


    async fn reply_final(&self, qid: ZInt) {
        trace!("recv ReplyFinal {:?}", qid);
        self.state.write().await.queries.remove(&qid);
    }

    async fn pull(&self, _is_final: bool, _reskey: &ResKey, _pull_id: ZInt, _max_samples: &Option<ZInt>) {
        trace!("recv Pull {:?} {:?} {:?} {:?}", _is_final, _reskey, _pull_id, _max_samples);
    }

    async fn close(&self) {
        trace!("recv Close");
    }
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Session{{...}}")
    }
}
