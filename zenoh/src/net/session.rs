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
    core::{ rname, ResourceId, ResKey },
    io::RBuf,
    proto::{ Primitives, QueryTarget, QueryConsolidation, Reply, queryable},
};
use zenoh_router::runtime::Runtime;
use zenoh_util::{zerror, zconfigurable};
use zenoh_util::core::{ZResult, ZError, ZErrorKind};
use super::*;

zconfigurable! {
    pub static ref API_DATA_RECEPTION_CHANNEL_SIZE: usize = 256;
    pub static ref API_QUERY_RECEPTION_CHANNEL_SIZE: usize = 256;
    pub static ref API_REPLY_EMISSION_CHANNEL_SIZE: usize = 256;
    pub static ref API_REPLY_RECEPTION_CHANNEL_SIZE: usize = 256;
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
    direct_subscribers: HashMap<Id, DirectSubscriber>,
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
            direct_subscribers: HashMap::new(),
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

    // Initialize a Session with an existing Runtime.
    // This operation is used by the plugins to share the same Runtime than the router.
    #[doc(hidden)]
    pub async fn init(runtime: Runtime) -> Session {
        let broker = runtime.read().await.broker.clone();
        let state = Arc::new(RwLock::new(SessionState::new()));
        let session = Session{runtime, state: state.clone()};
        let primitives = Some(broker.new_primitives(Arc::new(session.clone())).await);
        state.write().await.primitives = primitives;
        session
    }


    pub async fn close(&self) -> ZResult<()> {
        // @TODO: implement
        trace!("close()");
        self.runtime.close().await?;

        let primitives = self.state.write().await.primitives.as_ref().unwrap().clone();
        primitives.close().await;

        Ok(())
    }

    pub fn info(&self) -> Properties {
        // @TODO: implement
        trace!("info()");
        let mut info = Properties::new();
        info.insert(ZN_INFO_PEER_KEY, b"tcp/somewhere:7887".to_vec());
        info.insert(ZN_INFO_PID_KEY, vec![1u8, 2, 3]);
        info.insert(ZN_INFO_PEER_PID_KEY, vec![4u8, 5, 6]);
        info
    }

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

    pub async fn undeclare_resource(&self, rid: ResourceId) -> ZResult<()> {
        trace!("undeclare_resource({:?})", rid);
        let mut state = self.state.write().await;
        state.local_resources.remove(&rid);

        let primitives = state.primitives.as_ref().unwrap().clone();
        drop(state);
        primitives.forget_resource(rid).await;

        Ok(())
    }

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

    pub async fn declare_direct_subscriber<DataHandler>(&self, resource: &ResKey, info: &SubInfo, data_handler: DataHandler) -> ZResult<DirectSubscriber>
        where DataHandler: FnMut(/*res_name:*/ &str, /*payload:*/ RBuf, /*data_info:*/ Option<RBuf>) + Send + Sync + 'static
    {
        trace!("declare_direct_subscriber({:?})", resource);
        let mut state = self.state.write().await;
        let id = state.decl_id_counter.fetch_add(1, Ordering::SeqCst);
        let resname = state.localkey_to_resname(resource)?;
        let dhandler = Arc::new(RwLock::new(data_handler));
        let sub = DirectSubscriber{ id, reskey: resource.clone(), resname, session: self.clone(), dhandler };
        state.direct_subscribers.insert(id, sub.clone());

        let primitives = state.primitives.as_ref().unwrap().clone();
        drop(state);
        primitives.subscriber(resource, info).await;

        Ok(sub)
    }

    pub async fn undeclare_subscriber(&self, subscriber: Subscriber) -> ZResult<()>
    {
        trace!("undeclare_subscriber({:?})", subscriber);
        let mut state = self.state.write().await;
        state.subscribers.remove(&subscriber.id);

        // Note: there might be several Subscribers on the same ResKey.
        // Before calling forget_subscriber(reskey), check if this was the last one.
        if !state.direct_subscribers.values().any(|s| s.reskey == subscriber.reskey)
           && !state.subscribers.values().any(|s| s.reskey == subscriber.reskey) {
            let primitives = state.primitives.as_ref().unwrap().clone();
            drop(state);
            primitives.forget_subscriber(&subscriber.reskey).await;
        }
        Ok(())
    }


    pub async fn undeclare_direct_subscriber(&self, subscriber: DirectSubscriber) -> ZResult<()>
    {
        trace!("undeclare_direct_subscriber({:?})", subscriber);
        let mut state = self.state.write().await;
        state.direct_subscribers.remove(&subscriber.id);

        // Note: there might be several Subscribers on the same ResKey.
        // Before calling forget_subscriber(reskey), check if this was the last one.
        if !state.direct_subscribers.values().any(|s| s.reskey == subscriber.reskey)
           && !state.subscribers.values().any(|s| s.reskey == subscriber.reskey) {
            let primitives = state.primitives.as_ref().unwrap().clone();
            drop(state);
            primitives.forget_subscriber(&subscriber.reskey).await;
        }
        Ok(())
    }

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

    pub async fn write(&self, resource: &ResKey, payload: RBuf) -> ZResult<()> {
        trace!("write({:?}, [...])", resource);
        let state = self.state.read().await;
        let primitives = state.primitives.as_ref().unwrap().clone();
        drop(state);
        primitives.data(resource, true, &None, payload).await;
        Ok(())
    }

    pub async fn write_wo(&self, resource: &ResKey, payload: RBuf, encoding: ZInt, kind: ZInt) -> ZResult<()> {
        trace!("write_wo({:?}, [...])", resource);
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
        primitives.data(resource, true, &Some((&infobuf).into()), payload).await;
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
                    // Call matching direct_subscribers
                    for sub in state.direct_subscribers.values() {
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
            sender.send((resname.clone(), payload.clone(), info.clone())).await;
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
            req_sender.send((resname.clone(), predicate.clone(), RepliesSender{ kind, sender: rep_sender.clone() })).await;
        }
        drop(rep_sender); // all senders need to be dropped for the channel to close

        task::spawn( async move { // router is not re-entrant
            while let Some((kind, sample_opt)) = rep_receiver.next().await {
                match sample_opt {
                    Some((resname, payload, info)) => {
                        primitives.reply(qid, Reply::ReplyData {
                            source_kind: kind, 
                            replier_id: pid.clone(),
                            reskey: ResKey::RName(resname), 
                            info,
                            payload,
                        }).await;
                    }
                    None => {
                        primitives.reply(qid, Reply::SourceFinal {
                            source_kind: kind, 
                            replier_id: pid.clone(),
                        }).await;
                    }
                }
            }

            primitives.reply(qid, Reply::ReplyFinal).await;
        });
    }

    async fn reply(&self, qid: ZInt, mut reply: Reply) {
        trace!("recv Reply {:?} {:?}", qid, reply);
        let rep_sender = {
            let state = &mut self.state.write().await;
            let rep_sender = match state.queries.get(&qid) {
                Some(rep_sender) => rep_sender.clone(),
                None => {
                    warn!("Received Reply for unkown Query: {}", qid);
                    return
                }
            };
            match &mut reply {
                Reply::ReplyData {ref mut reskey, ..} => {
                    let resname = match state.reskey_to_resname(&reskey) {
                        Ok(name) => name,
                        Err(e) => {
                            error!("Received Reply for unkown reskey: {}", e);
                            return
                        }
                    };
                    *reskey = ResKey::RName(resname);
                }
                Reply::SourceFinal {..} => {} 
                Reply::ReplyFinal {..} => {state.queries.remove(&qid);}
            };
            rep_sender
        };
        rep_sender.send(reply).await;
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
