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
use async_std::sync::{Arc, RwLock, Weak};
use async_std::task::{sleep, JoinHandle};
use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use uhlc::HLC;

use zenoh_protocol::core::{whatami, PeerId, WhatAmI, ZInt};
use zenoh_protocol::proto::{ZenohBody, ZenohMessage};
use zenoh_protocol::session::{
    DeMux, Mux, Primitives, Session, SessionEventHandler, SessionHandler,
};

use zenoh_util::core::ZResult;
use zenoh_util::zconfigurable;

use crate::routing::face::{Face, FaceState};
use crate::routing::network::Network;
pub use crate::routing::pubsub::*;
pub use crate::routing::queries::*;
pub use crate::routing::resource::*;
use crate::runtime::orchestrator::SessionOrchestrator;

zconfigurable! {
    static ref LINK_CLOSURE_DELAY: u64 = 200;
    static ref TREES_COMPUTATION_DELAY: u64 = 100;
}

pub struct Tables {
    pub(crate) pid: PeerId,
    pub(crate) whatami: whatami::Type,
    face_counter: usize,
    pub(crate) hlc: Option<HLC>,
    pub(crate) root_res: Arc<Resource>,
    pub(crate) faces: HashMap<usize, Arc<FaceState>>,
    pub(crate) router_subs: HashSet<Arc<Resource>>,
    pub(crate) peer_subs: HashSet<Arc<Resource>>,
    pub(crate) router_qabls: HashSet<Arc<Resource>>,
    pub(crate) peer_qabls: HashSet<Arc<Resource>>,
    pub(crate) routers_net: Option<Network>,
    pub(crate) peers_net: Option<Network>,
    pub(crate) routers_trees_task: Option<JoinHandle<()>>,
    pub(crate) peers_trees_task: Option<JoinHandle<()>>,
}

impl Tables {
    pub fn new(pid: PeerId, whatami: whatami::Type, hlc: Option<HLC>) -> Self {
        Tables {
            pid,
            whatami,
            face_counter: 0,
            hlc,
            root_res: Resource::root(),
            faces: HashMap::new(),
            router_subs: HashSet::new(),
            peer_subs: HashSet::new(),
            router_qabls: HashSet::new(),
            peer_qabls: HashSet::new(),
            routers_net: None,
            peers_net: None,
            routers_trees_task: None,
            peers_trees_task: None,
        }
    }

    #[doc(hidden)]
    pub fn _get_root(&self) -> &Arc<Resource> {
        &self.root_res
    }

    // pub(crate) unsafe fn split_mut(self) -> (Self, &mut Self) {
    //     let r = &mut self as *mut Self;
    //     (self, &mut *r)
    // }

    pub async fn print(&self) -> String {
        Resource::print_tree(&self.root_res)
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub(crate) fn get_mapping<'a>(
        &'a self,
        face: &'a FaceState,
        rid: &ZInt,
    ) -> Option<&'a Arc<Resource>> {
        match rid {
            0 => Some(&self.root_res),
            rid => face.get_mapping(rid),
        }
    }

    #[inline]
    pub(crate) fn get_net(&self, net_type: whatami::Type) -> Option<&Network> {
        match net_type {
            whatami::ROUTER => self.routers_net.as_ref(),
            whatami::PEER => self.peers_net.as_ref(),
            _ => None,
        }
    }

    #[inline]
    pub(crate) fn get_face(&self, pid: &PeerId) -> Option<&Arc<FaceState>> {
        self.faces.values().find(|face| face.pid == *pid)
    }

    pub async fn open_face(
        &mut self,
        pid: PeerId,
        whatami: WhatAmI,
        primitives: Arc<dyn Primitives + Send + Sync>,
    ) -> Weak<FaceState> {
        unsafe {
            let fid = self.face_counter;
            log::debug!("New face {}", fid);
            self.face_counter += 1;
            let mut newface = self
                .faces
                .entry(fid)
                .or_insert_with(|| FaceState::new(fid, pid, whatami, primitives.clone()))
                .clone();

            if whatami == whatami::CLIENT {
                pubsub_new_client_face(self, &mut newface).await;
                queries_new_client_face(self, &mut newface).await;
            }
            Arc::downgrade(&newface)
        }
    }

    pub async fn close_face(&mut self, face: &Weak<FaceState>) {
        match face.upgrade() {
            Some(mut face) => unsafe {
                log::debug!("Close face {}", face.id);
                finalize_pending_queries(self, &mut face).await;

                let mut face_clone = face.clone();
                let face = Arc::get_mut_unchecked(&mut face);
                for mut res in face.remote_mappings.values_mut() {
                    Arc::get_mut_unchecked(res).contexts.remove(&face.id);
                    Resource::clean(&mut res);
                }
                face.remote_mappings.clear();
                for mut res in face.local_mappings.values_mut() {
                    Arc::get_mut_unchecked(res).contexts.remove(&face.id);
                    Resource::clean(&mut res);
                }
                face.local_mappings.clear();
                while let Some(mut res) = face.remote_subs.pop() {
                    Arc::get_mut_unchecked(&mut res).contexts.remove(&face.id);
                    undeclare_client_subscription(self, &mut face_clone, &mut res).await;
                    Resource::clean(&mut res);
                }
                while let Some(mut res) = face.remote_qabls.pop() {
                    Arc::get_mut_unchecked(&mut res).contexts.remove(&face.id);
                    Resource::clean(&mut res);
                }
                self.faces.remove(&face.id);
            },
            None => log::error!("Face already closed!"),
        }
    }

    pub(crate) unsafe fn compute_matches_routes(&mut self, res: &mut Arc<Resource>) {
        compute_matches_data_routes(self, res);
    }

    pub(crate) fn schedule_compute_trees(
        &mut self,
        tables_ref: Arc<RwLock<Tables>>,
        net_type: whatami::Type,
    ) {
        if (net_type == whatami::ROUTER && self.routers_trees_task.is_none())
            || (net_type == whatami::PEER && self.peers_trees_task.is_none())
        {
            let task = Some(async_std::task::spawn(async move {
                async_std::task::sleep(std::time::Duration::from_millis(*TREES_COMPUTATION_DELAY))
                    .await;
                let mut tables = tables_ref.write().await;
                let new_childs = match net_type {
                    whatami::ROUTER => tables.routers_net.as_mut().unwrap().compute_trees().await,
                    _ => tables.peers_net.as_mut().unwrap().compute_trees().await,
                };
                pubsub_tree_change(&mut tables, &new_childs, net_type).await;
                queries_tree_change(&mut tables, &new_childs, net_type).await;
                match net_type {
                    whatami::ROUTER => tables.routers_trees_task = None,
                    _ => tables.peers_trees_task = None,
                };
            }));
            match net_type {
                whatami::ROUTER => self.routers_trees_task = task,
                _ => self.peers_trees_task = task,
            };
        }
    }
}

pub struct Router {
    whatami: whatami::Type,
    pub tables: Arc<RwLock<Tables>>,
}

impl Router {
    pub fn new(pid: PeerId, whatami: whatami::Type, hlc: Option<HLC>) -> Self {
        Router {
            whatami,
            tables: Arc::new(RwLock::new(Tables::new(pid, whatami, hlc))),
        }
    }

    pub async fn init_link_state(
        &mut self,
        orchestrator: SessionOrchestrator,
        peers_autoconnect: bool,
    ) {
        let mut tables = self.tables.write().await;
        if orchestrator.whatami == whatami::ROUTER {
            tables.routers_net = Some(
                Network::new(
                    "[Routers network]".to_string(),
                    tables.pid.clone(),
                    orchestrator.clone(),
                    peers_autoconnect,
                )
                .await,
            );
        }
        tables.peers_net = Some(
            Network::new(
                "[Peers network]".to_string(),
                tables.pid.clone(),
                orchestrator,
                peers_autoconnect,
            )
            .await,
        );
    }

    pub async fn new_primitives(
        &self,
        primitives: Arc<dyn Primitives + Send + Sync>,
    ) -> Arc<dyn Primitives + Send + Sync> {
        Arc::new(Face {
            tables: self.tables.clone(),
            state: {
                let mut tables = self.tables.write().await;
                let pid = tables.pid.clone();
                tables
                    .open_face(pid, whatami::CLIENT, primitives)
                    .await
                    .upgrade()
                    .unwrap()
            },
        })
    }
}

#[async_trait]
impl SessionHandler for Router {
    async fn new_session(
        &self,
        session: Session,
    ) -> ZResult<Arc<dyn SessionEventHandler + Send + Sync>> {
        let mut tables = self.tables.write().await;
        let whatami = session.get_whatami()?;
        if whatami != whatami::CLIENT && tables.peers_net.is_some() {
            let handler = Arc::new(LinkStateInterceptor::new(
                session.clone(),
                self.tables.clone(),
                DeMux::new(Face {
                    tables: self.tables.clone(),
                    state: tables
                        .open_face(
                            session.get_pid().unwrap(),
                            whatami,
                            Arc::new(Mux::new(Arc::new(session.clone()))),
                        )
                        .await
                        .upgrade()
                        .unwrap(),
                }),
            ));

            match (self.whatami, whatami) {
                (whatami::ROUTER, whatami::ROUTER) => {
                    let net = tables.routers_net.as_mut().unwrap();
                    net.add_link(session.clone()).await;
                    tables.schedule_compute_trees(self.tables.clone(), whatami::ROUTER);
                }
                _ => {
                    let net = tables.peers_net.as_mut().unwrap();
                    net.add_link(session.clone()).await;
                    tables.schedule_compute_trees(self.tables.clone(), whatami::PEER);
                }
            };

            Ok(handler)
        } else {
            Ok(Arc::new(DeMux::new(Face {
                tables: self.tables.clone(),
                state: tables
                    .open_face(
                        session.get_pid().unwrap(),
                        whatami,
                        Arc::new(Mux::new(Arc::new(session.clone()))),
                    )
                    .await
                    .upgrade()
                    .unwrap(),
            })))
        }
    }
}

pub struct LinkStateInterceptor {
    session: Session,
    tables: Arc<RwLock<Tables>>,
    demux: DeMux<Face>,
}

impl LinkStateInterceptor {
    fn new(session: Session, tables: Arc<RwLock<Tables>>, demux: DeMux<Face>) -> Self {
        LinkStateInterceptor {
            session,
            tables,
            demux,
        }
    }
}

#[async_trait]
impl SessionEventHandler for LinkStateInterceptor {
    async fn handle_message(&self, msg: ZenohMessage) -> ZResult<()> {
        match msg.body {
            ZenohBody::LinkStateList(list) => {
                let pid = self.session.get_pid().unwrap();
                let mut tables = self.tables.write().await;
                // let tables2 = &mut *(&tables as *mut Tables);
                let whatami = self.session.get_whatami()?;
                match (tables.whatami, whatami) {
                    (whatami::ROUTER, whatami::ROUTER) => {
                        for (_, removed_node) in tables
                            .routers_net
                            .as_mut()
                            .unwrap()
                            .link_states(list.link_states, pid)
                            .await
                        {
                            pubsub_remove_node(&mut tables, &removed_node.pid, whatami::ROUTER)
                                .await;
                            queries_remove_node(&mut tables, &removed_node.pid, whatami::ROUTER)
                                .await;
                        }
                        tables.schedule_compute_trees(self.tables.clone(), whatami::ROUTER);
                    }
                    _ => {
                        for (_, removed_node) in tables
                            .peers_net
                            .as_mut()
                            .unwrap()
                            .link_states(list.link_states, pid)
                            .await
                        {
                            pubsub_remove_node(&mut tables, &removed_node.pid, whatami::PEER).await;
                            queries_remove_node(&mut tables, &removed_node.pid, whatami::PEER)
                                .await;
                        }
                        tables.schedule_compute_trees(self.tables.clone(), whatami::PEER);
                    }
                };

                Ok(())
            }
            _ => self.demux.handle_message(msg).await,
        }
    }

    async fn new_link(&self, _link: zenoh_protocol::link::Link) {}

    async fn del_link(&self, _link: zenoh_protocol::link::Link) {}

    async fn closing(&self) {
        self.demux.closing().await;
        sleep(Duration::from_millis(*LINK_CLOSURE_DELAY)).await;
        let mut tables = self.tables.write().await;
        match self.session.get_whatami() {
            Ok(whatami) => match (tables.whatami, whatami) {
                (whatami::ROUTER, whatami::ROUTER) => {
                    for (_, removed_node) in tables
                        .routers_net
                        .as_mut()
                        .unwrap()
                        .remove_link(&self.session)
                        .await
                    {
                        pubsub_remove_node(&mut tables, &removed_node.pid, whatami::ROUTER).await;
                        queries_remove_node(&mut tables, &removed_node.pid, whatami::ROUTER).await;
                    }
                    tables.schedule_compute_trees(self.tables.clone(), whatami::ROUTER);
                }
                _ => {
                    for (_, removed_node) in tables
                        .peers_net
                        .as_mut()
                        .unwrap()
                        .remove_link(&self.session)
                        .await
                    {
                        pubsub_remove_node(&mut tables, &removed_node.pid, whatami::PEER).await;
                        queries_remove_node(&mut tables, &removed_node.pid, whatami::PEER).await;
                    }
                    tables.schedule_compute_trees(self.tables.clone(), whatami::PEER);
                }
            },
            Err(_) => log::error!("Unable to get whatami closing session!"),
        };
    }

    async fn closed(&self) {}
}
