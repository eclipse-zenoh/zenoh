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
use async_std::sync::Arc;
use async_std::task;
use async_trait::async_trait;
use rand::prelude::*;
use std::fmt;
use std::sync::atomic::{AtomicUsize, AtomicU64, AtomicBool, Ordering};
use std::collections::HashMap;
use spin::RwLock;
use log::{error, warn, info, trace};
use zenoh_protocol:: {
    core::{ rname, PeerId, ResourceId, ResKey },
    io::RBuf,
    proto::{ Primitives, QueryTarget, QueryConsolidation, Reply, whatami, queryable},
    session::{SessionManager, SessionManagerConfig},
};
use zenoh_router::routing::broker::Broker;
use zenoh_util::zerror;
use zenoh_util::core::{ZResult, ZError, ZErrorKind};
use super::*;


// rename to avoid conflicts
type TxSession = zenoh_protocol::session::Session;

#[derive(Clone)]
pub struct Session {
    session_manager: SessionManager,
    tx_session: Option<Arc<TxSession>>,
    broker: Arc<Broker>,
    inner: Arc<RwLock<InnerSession>>,
}

impl Session {

    pub(super) async fn new(locator: &str, _ps: Option<Properties>) -> Session {
        let broker = Arc::new(Broker::new());
        
        let mut pid = vec![0, 0, 0, 0];
        rand::thread_rng().fill_bytes(&mut pid);
        let peerid = PeerId{id: pid};

        let config = SessionManagerConfig {
            version: 0,
            whatami: whatami::CLIENT,
            id: peerid.clone(),
            handler: broker.clone()
        };
        let session_manager = SessionManager::new(config, None);

        // @TODO: scout if locator = "". For now, replace by "tcp/127.0.0.1:7447"
        let locator = if locator.is_empty() { "tcp/127.0.0.1:7447" } else { &locator };

        let mut tx_session: Option<Arc<TxSession>> = None;

        // @TODO: manage a tcp.port property (and tcp.interface?)
        // try to open TCP port 7447
        if let Err(_err) = session_manager.add_locator(&"tcp/127.0.0.1:7447".parse().unwrap()).await {
            // if failed, try to connect to peer on locator
            info!("Unable to open listening TCP port on 127.0.0.1:7447. Try connection to {}", locator);
            let attachment = None;
            match session_manager.open_session(&locator.parse().unwrap(), &attachment).await {
                Ok(s) => tx_session = Some(Arc::new(s)),
                Err(err) => {
                    error!("Unable to connect to {}! {:?}", locator, err);
                    std::process::exit(-1);
                }
            }
        } else {
            info!("Listening on TCP: 127.0.0.1:7447.");
        }

        let inner = Arc::new(RwLock::new(
            InnerSession::new(peerid)
        ));
        let inner2 = inner.clone();
        let session = Session{ session_manager, tx_session, broker, inner };

        let prim = session.broker.new_primitives(Arc::new(session.clone())).await;
        inner2.write().primitives = Some(prim);

        // Workaround for the declare_and_shoot problem
        task::sleep(std::time::Duration::from_millis(200)).await;

        session
    }

    pub async fn close(&self) -> ZResult<()> {
        // @TODO: implement
        trace!("close()");
        let inner = self.inner.read();
        if let Some(tx_session) = &self.tx_session {
            return tx_session.close().await
        }
        // @TODO: session_manager.del_locator()

        let primitives = inner.primitives.as_ref().unwrap().clone();
        drop(inner);
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
        let mut inner = self.inner.write();
        let rid = inner.rid_counter.fetch_add(1, Ordering::SeqCst) as ZInt;
        let rname = inner.localkey_to_resname(resource)?;
        inner.local_resources.insert(rid, rname);

        let primitives = inner.primitives.as_ref().unwrap().clone();
        drop(inner);
        primitives.resource(rid, resource).await;

        Ok(rid)
    }

    pub async fn undeclare_resource(&self, rid: ResourceId) -> ZResult<()> {
        trace!("undeclare_resource({:?})", rid);
        let mut inner = self.inner.write();
        inner.local_resources.remove(&rid);

        let primitives = inner.primitives.as_ref().unwrap().clone();
        drop(inner);
        primitives.forget_resource(rid).await;

        Ok(())
    }

    pub async fn declare_publisher(&self, resource: &ResKey) -> ZResult<Publisher> {
        trace!("declare_publisher({:?})", resource);
        let mut inner = self.inner.write();

        let id = inner.decl_id_counter.fetch_add(1, Ordering::SeqCst);
        let publ = Publisher{ id, reskey: resource.clone() };
        inner.publishers.insert(id, publ.clone());

        let primitives = inner.primitives.as_ref().unwrap().clone();
        drop(inner);
        primitives.publisher(resource).await;

        Ok(publ)
    }

    pub async fn undeclare_publisher(&self, publisher: Publisher) -> ZResult<()> {
        trace!("undeclare_publisher({:?})", publisher);
        let mut inner = self.inner.write();
        inner.publishers.remove(&publisher.id);

        // Note: there might be several Publishers on the same ResKey.
        // Before calling forget_publisher(reskey), check if this was the last one.
        if !inner.publishers.values().any(|p| p.reskey == publisher.reskey) {
            let primitives = inner.primitives.as_ref().unwrap().clone();
            drop(inner);
            primitives.forget_publisher(&publisher.reskey).await;
        }
        Ok(())
    }

    pub async fn declare_subscriber<DataHandler>(&self, resource: &ResKey, info: &SubInfo, data_handler: DataHandler) -> ZResult<Subscriber>
        where DataHandler: FnMut(/*res_name:*/ &str, /*payload:*/ RBuf, /*data_info:*/ Option<RBuf>) + Send + Sync + 'static
    {
        trace!("declare_subscriber({:?})", resource);
        let mut inner = self.inner.write();
        let id = inner.decl_id_counter.fetch_add(1, Ordering::SeqCst);
        let resname = inner.localkey_to_resname(resource)?;
        let dhandler = Arc::new(RwLock::new(data_handler));
        let sub = Subscriber{ id, reskey: resource.clone(), resname, dhandler, session: self.inner.clone() };
        inner.subscribers.insert(id, sub.clone());

        let primitives = inner.primitives.as_ref().unwrap().clone();
        drop(inner);
        primitives.subscriber(resource, info).await;

        Ok(sub)
    }

    pub async fn undeclare_subscriber(&self, subscriber: Subscriber) -> ZResult<()>
    {
        trace!("undeclare_subscriber({:?})", subscriber);
        let mut inner = self.inner.write();
        inner.subscribers.remove(&subscriber.id);

        // Note: there might be several Subscribers on the same ResKey.
        // Before calling forget_subscriber(reskey), check if this was the last one.
        if !inner.subscribers.values().any(|s| s.reskey == subscriber.reskey) {
            let primitives = inner.primitives.as_ref().unwrap().clone();
            drop(inner);
            primitives.forget_subscriber(&subscriber.reskey).await;
        }
        Ok(())
    }

    pub async fn declare_queryable<QueryHandler>(&self, resource: &ResKey, kind: ZInt, query_handler: QueryHandler) -> ZResult<Queryable>
        where QueryHandler: FnMut(/*res_name:*/ &str, /*predicate:*/ &str, /*replies_sender:*/ &RepliesSender, /*query_handle:*/ QueryHandle) + Send + Sync + 'static
    {
        trace!("declare_queryable({:?}, {:?})", resource, kind);
        let mut inner = self.inner.write();
        let id = inner.decl_id_counter.fetch_add(1, Ordering::SeqCst);
        let qhandler = Arc::new(RwLock::new(query_handler));
        let qable = Queryable{ id, reskey: resource.clone(), kind, qhandler };
        inner.queryables.insert(id, qable.clone());

        let primitives = inner.primitives.as_ref().unwrap().clone();
        drop(inner);
        primitives.queryable(resource).await;

        Ok(qable)

    }

    pub async fn undeclare_queryable(&self, queryable: Queryable) -> ZResult<()> {
        trace!("undeclare_queryable({:?})", queryable);
        let mut inner = self.inner.write();
        inner.queryables.remove(&queryable.id);

        // Note: there might be several Queryables on the same ResKey.
        // Before calling forget_eval(reskey), check if this was the last one.
        if !inner.queryables.values().any(|e| e.reskey == queryable.reskey) {
            let primitives = inner.primitives.as_ref().unwrap();
            primitives.forget_queryable(&queryable.reskey).await;
        }
        Ok(())
    }

    pub async fn write(&self, resource: &ResKey, payload: RBuf) -> ZResult<()> {
        trace!("write({:?}, [...])", resource);
        let inner = self.inner.read();
        let primitives = inner.primitives.as_ref().unwrap().clone();
        drop(inner);
        primitives.data(resource, true, &None, payload).await;
        Ok(())
    }

    pub async fn query<RepliesHandler>(&self,
        resource:        &ResKey,
        predicate:       &str,
        replies_handler: RepliesHandler,
        target:          QueryTarget,
        consolidation:   QueryConsolidation
    ) -> ZResult<()>
        where RepliesHandler: FnMut(&Reply) + Send + Sync + 'static
    {
        trace!("query({:?}, {:?}, {:?}, {:?})", resource, predicate, target, consolidation);
        let mut inner = self.inner.write();
        let qid = inner.qid_counter.fetch_add(1, Ordering::SeqCst);
        inner.queries.insert(qid, Arc::new(RwLock::new(replies_handler)));

        let primitives = inner.primitives.as_ref().unwrap().clone();
        drop(inner);
        primitives.query(resource, predicate, qid, target, consolidation).await;

        Ok(())
    }
}

#[async_trait]
impl Primitives for Session {

    async fn resource(&self, rid: ZInt, reskey: &ResKey) {
        trace!("recv Resource {} {:?}", rid, reskey);
        let inner = &mut self.inner.write();
        match inner.reskey_to_resname(reskey) {
            Ok(name) => {inner.remote_resources.insert(rid, name);}
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
        let inner = self.inner.read();
        match inner.reskey_to_resname(reskey) {
            Ok(resname) => {
                // Call matching subscribers
                for sub in inner.subscribers.values() {
                    if rname::intersect(&sub.resname, &resname) {
                        let handler = &mut *sub.dhandler.write();
                        handler(&resname, payload.clone(), info.clone());
                    }
                }
            },
            Err(err) => error!("Received Data for unkown reskey: {}", err)
        }
    }

    async fn query(&self, reskey: &ResKey, predicate: &str, qid: ZInt, target: QueryTarget, _consolidation: QueryConsolidation) {
        trace!("recv Query {:?} {:?} {:?} {:?}", reskey, predicate, target, _consolidation);
        let inner = self.inner.read();
        match inner.reskey_to_resname(reskey) {
            Ok(resname) => {
                let queryables = inner.queryables.values().filter(|queryable| {
                    match inner.reskey_to_resname(&queryable.reskey) {
                        Ok(qablname) => {
                            rname::intersect(&qablname, &resname) 
                            && ((queryable.kind == queryable::ALL_KINDS || target.kind  == queryable::ALL_KINDS) 
                                || (queryable.kind & target.kind != 0))
                        },
                        Err(err) => {error!("{}. Internal error (queryable reskey to resname failed).", err); false}
                    }
                });

                let nb_qhandlers = Arc::new(AtomicUsize::new(queryables.size_hint().1.unwrap()));
                let sent_final = Arc::new(AtomicBool::new(false));
                for queryable in queryables {
                    let handler = &mut *queryable.qhandler.write();

                    fn replies_sender(query_handle: QueryHandle, replies: Vec<(String, RBuf, Option<RBuf>)>) {
                        async_std::task::spawn(
                            async move {
                                for (reskey, payload, info) in replies {
                                    query_handle.primitives.reply(query_handle.qid, &Reply::ReplyData {
                                        source_kind: query_handle.kind, 
                                        replier_id: query_handle.pid.clone(), 
                                        reskey: ResKey::RName(reskey.to_string()), 
                                        info,
                                        payload,
                                    }).await;
                                }
                                query_handle.primitives.reply(query_handle.qid, &Reply::SourceFinal {
                                    source_kind: query_handle.kind, 
                                    replier_id: query_handle.pid.clone(),
                                }).await;

                                query_handle.nb_qhandlers.fetch_sub(1, Ordering::Relaxed);
                                if query_handle.nb_qhandlers.load(Ordering::Relaxed) == 0 && !query_handle.sent_final.swap(true, Ordering::Relaxed) {
                                    query_handle.primitives.reply(query_handle.qid, &Reply::ReplyFinal).await;
                                }
                            }
                        );
                    }
                    let qhandle = QueryHandle {
                        pid: inner.pid.clone(), // @TODO build/use prebuilt specific pid
                        kind: queryable.kind,
                        primitives: inner.primitives.clone().unwrap(),
                        qid,
                        nb_qhandlers: nb_qhandlers.clone(),
                        sent_final: sent_final.clone(),
                    };
                    handler(&resname, predicate, &replies_sender, qhandle);

                }
            },
            Err(err) => error!("Received Query for unkown reskey: {}", err)
        }
    }

    async fn reply(&self, qid: ZInt, reply: &Reply) {
        trace!("recv Reply {:?} {:?}", qid, reply);
        let inner = &mut self.inner.write();
        let rhandler = &mut * match inner.queries.get(&qid) {
            Some(arc) => arc.write(),
            None => {
                warn!("Received Reply for unkown Query: {}", qid);
                return
            }
        };
        match reply {
            Reply::ReplyData {source_kind, replier_id, reskey, info, payload} => {
                let resname = match inner.reskey_to_resname(&reskey) {
                    Ok(name) => name,
                    Err(e) => {
                        error!("Received Reply for unkown reskey: {}", e);
                        return
                    }
                };
                rhandler(&Reply::ReplyData {
                    source_kind: *source_kind, 
                    replier_id: replier_id.clone(), 
                    reskey: ResKey::RName(resname), 
                    info: info.clone(), 
                    payload: payload.clone()} ); // @TODO find something more efficient than cloning everything
            }
            Reply::SourceFinal {..} => {rhandler(reply);} 
            Reply::ReplyFinal {..} => {rhandler(reply);} // @TODO remove query
        }
    }

    async fn pull(&self, _is_final: bool, _reskey: &ResKey, _pull_id: ZInt, _max_samples: &Option<ZInt>) {
        trace!("recv Pull {:?} {:?} {:?} {:?}", _is_final, _reskey, _pull_id, _max_samples);
    }

    async fn close(&self) {
        trace!("recv Close");
    }
}



pub(crate) struct InnerSession {
    pid:              PeerId,
    primitives:       Option<Arc<dyn Primitives + Send + Sync>>, // @TODO replace with MaybeUninit ??
    rid_counter:      AtomicUsize,  // @TODO: manage rollover and uniqueness
    qid_counter:      AtomicU64,
    decl_id_counter:  AtomicUsize,
    local_resources:  HashMap<ResourceId, String>,
    remote_resources: HashMap<ResourceId, String>,
    publishers:       HashMap<Id, Publisher>,
    subscribers:      HashMap<Id, Subscriber>,
    queryables:       HashMap<Id, Queryable>,
    queries:          HashMap<ZInt, Arc<RwLock<RepliesHandler>>>,
}

impl InnerSession {
    pub(crate) fn new(pid: PeerId) -> InnerSession {
        InnerSession  { 
            pid, 
            primitives:       None,
            rid_counter:      AtomicUsize::new(1),  // Note: start at 1 because 0 is reserved for NO_RESOURCE
            qid_counter:      AtomicU64::new(0),
            decl_id_counter:  AtomicUsize::new(0),
            local_resources:  HashMap::new(),
            remote_resources: HashMap::new(),
            publishers:       HashMap::new(),
            subscribers:      HashMap::new(),
            queryables:       HashMap::new(),
            queries:          HashMap::new(),
        }
    }
}

impl InnerSession {
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

impl fmt::Debug for InnerSession {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "InnerSession{{ subscribers: {} }}",
            self.subscribers.len())
    }
}

