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
use async_std::sync::{Arc, Weak};
use async_std::task::JoinHandle;
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, RwLock};
use uhlc::HLC;
use zenoh_util::sync::get_mut_unchecked;

use super::protocol::core::{whatami, PeerId, WhatAmI, ZInt};
use super::protocol::link::Link;
use super::protocol::proto::{ZenohBody, ZenohMessage};
use super::protocol::session::{DeMux, Mux, Primitives, Session};

use zenoh_util::core::ZResult;
use zenoh_util::zconfigurable;

use super::face::{Face, FaceState};
use super::network::{shared_nodes, Network};
pub use super::pubsub::*;
pub use super::queries::*;
pub use super::resource::*;
use super::runtime::orchestrator::SessionOrchestrator;

zconfigurable! {
    static ref LINK_CLOSURE_DELAY: u64 = 200;
    static ref TREES_COMPUTATION_DELAY: u64 = 100;
}

pub struct Tables {
    pub(crate) pid: PeerId,
    pub(crate) whatami: whatami::Type,
    face_counter: usize,
    #[allow(dead_code)]
    pub(crate) hlc: Option<HLC>,
    pub(crate) root_res: Arc<Resource>,
    pub(crate) faces: HashMap<usize, Arc<FaceState>>,
    pub(crate) pull_caches_lock: Mutex<()>,
    pub(crate) router_subs: HashSet<Arc<Resource>>,
    pub(crate) peer_subs: HashSet<Arc<Resource>>,
    pub(crate) router_qabls: HashSet<Arc<Resource>>,
    pub(crate) peer_qabls: HashSet<Arc<Resource>>,
    pub(crate) routers_net: Option<Network>,
    pub(crate) peers_net: Option<Network>,
    pub(crate) shared_nodes: Vec<PeerId>,
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
            pull_caches_lock: Mutex::new(()),
            router_subs: HashSet::new(),
            peer_subs: HashSet::new(),
            router_qabls: HashSet::new(),
            peer_qabls: HashSet::new(),
            routers_net: None,
            peers_net: None,
            shared_nodes: vec![],
            routers_trees_task: None,
            peers_trees_task: None,
        }
    }

    #[doc(hidden)]
    pub fn _get_root(&self) -> &Arc<Resource> {
        &self.root_res
    }

    pub fn print(&self) -> String {
        Resource::print_tree(&self.root_res)
    }

    #[inline]
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

    fn open_net_face(
        &mut self,
        pid: PeerId,
        whatami: WhatAmI,
        primitives: Arc<dyn Primitives + Send + Sync>,
        link_id: usize,
    ) -> Weak<FaceState> {
        let fid = self.face_counter;
        self.face_counter += 1;
        let mut newface = self
            .faces
            .entry(fid)
            .or_insert_with(|| FaceState::new(fid, pid, whatami, primitives.clone(), link_id))
            .clone();
        log::debug!("New {}", newface);

        if whatami == whatami::CLIENT {
            pubsub_new_client_face(self, &mut newface);
            queries_new_client_face(self, &mut newface);
        }
        Arc::downgrade(&newface)
    }

    pub fn open_face(
        &mut self,
        pid: PeerId,
        whatami: WhatAmI,
        primitives: Arc<dyn Primitives + Send + Sync>,
    ) -> Weak<FaceState> {
        self.open_net_face(pid, whatami, primitives, 0)
    }

    pub fn close_face(&mut self, face: &Weak<FaceState>) {
        match face.upgrade() {
            Some(mut face) => {
                log::debug!("Close {}", face);
                finalize_pending_queries(self, &mut face);

                let mut face_clone = face.clone();
                let face = get_mut_unchecked(&mut face);
                for mut res in face.remote_mappings.values_mut() {
                    get_mut_unchecked(res).session_ctxs.remove(&face.id);
                    Resource::clean(&mut res);
                }
                face.remote_mappings.clear();
                for mut res in face.local_mappings.values_mut() {
                    get_mut_unchecked(res).session_ctxs.remove(&face.id);
                    Resource::clean(&mut res);
                }
                face.local_mappings.clear();
                while let Some(mut res) = face.remote_subs.pop() {
                    get_mut_unchecked(&mut res).session_ctxs.remove(&face.id);
                    undeclare_client_subscription(self, &mut face_clone, &mut res);
                    Resource::clean(&mut res);
                }
                while let Some(mut res) = face.remote_qabls.pop() {
                    get_mut_unchecked(&mut res).session_ctxs.remove(&face.id);
                    undeclare_client_queryable(self, &mut face_clone, &mut res);
                    Resource::clean(&mut res);
                }
                self.faces.remove(&face.id);
            }
            None => log::error!("Face already closed!"),
        }
    }

    fn compute_routes(&mut self, res: &mut Arc<Resource>) {
        compute_data_routes(self, res);
        compute_query_routes(self, res);
    }

    pub(crate) fn compute_matches_routes(&mut self, res: &mut Arc<Resource>) {
        if res.context.is_some() {
            self.compute_routes(res);

            let resclone = res.clone();
            for match_ in &mut get_mut_unchecked(res).context_mut().matches {
                let match_ = &mut match_.upgrade().unwrap();
                if !Arc::ptr_eq(match_, &resclone) && match_.context.is_some() {
                    self.compute_routes(match_);
                }
            }
        }
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
                let mut tables = zwrite!(tables_ref);
                let new_childs = match net_type {
                    whatami::ROUTER => tables.routers_net.as_mut().unwrap().compute_trees(),
                    _ => tables.peers_net.as_mut().unwrap().compute_trees(),
                };
                pubsub_tree_change(&mut tables, &new_childs, net_type);
                queries_tree_change(&mut tables, &new_childs, net_type);
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
        let mut tables = zwrite!(self.tables);
        tables.peers_net = Some(Network::new(
            "[Peers network]".to_string(),
            tables.pid.clone(),
            orchestrator.clone(),
            peers_autoconnect,
        ));
        if orchestrator.whatami == whatami::ROUTER {
            tables.routers_net = Some(Network::new(
                "[Routers network]".to_string(),
                tables.pid.clone(),
                orchestrator,
                peers_autoconnect,
            ));
            tables.shared_nodes = shared_nodes(
                tables.routers_net.as_ref().unwrap(),
                tables.peers_net.as_ref().unwrap(),
            );
        }
    }

    pub async fn new_primitives(&self, primitives: Arc<dyn Primitives + Send + Sync>) -> Arc<Face> {
        Arc::new(Face {
            tables: self.tables.clone(),
            state: {
                let mut tables = zwrite!(self.tables);
                let pid = tables.pid.clone();
                tables
                    .open_face(pid, whatami::CLIENT, primitives)
                    .upgrade()
                    .unwrap()
            },
        })
    }

    pub fn new_session(&self, session: Session) -> ZResult<Arc<LinkStateInterceptor>> {
        let mut tables = zwrite!(self.tables);
        let whatami = session.get_whatami()?;

        let link_id = match (self.whatami, whatami) {
            (whatami::ROUTER, whatami::ROUTER) => tables
                .routers_net
                .as_mut()
                .unwrap()
                .add_link(session.clone()),
            (whatami::ROUTER, whatami::PEER)
            | (whatami::PEER, whatami::ROUTER)
            | (whatami::PEER, whatami::PEER) => {
                tables.peers_net.as_mut().unwrap().add_link(session.clone())
            }
            _ => 0,
        };

        if tables.whatami == whatami::ROUTER {
            tables.shared_nodes = shared_nodes(
                tables.routers_net.as_ref().unwrap(),
                tables.peers_net.as_ref().unwrap(),
            );
        }

        let handler = Arc::new(LinkStateInterceptor::new(
            session.clone(),
            self.tables.clone(),
            DeMux::new(Face {
                tables: self.tables.clone(),
                state: tables
                    .open_net_face(
                        session.get_pid().unwrap(),
                        whatami,
                        Arc::new(Mux::new(session)),
                        link_id,
                    )
                    .upgrade()
                    .unwrap(),
            }),
        ));

        match (self.whatami, whatami) {
            (whatami::ROUTER, whatami::ROUTER) => {
                tables.schedule_compute_trees(self.tables.clone(), whatami::ROUTER);
            }
            (whatami::ROUTER, whatami::PEER)
            | (whatami::PEER, whatami::ROUTER)
            | (whatami::PEER, whatami::PEER) => {
                tables.schedule_compute_trees(self.tables.clone(), whatami::PEER);
            }
            _ => (),
        }
        Ok(handler)
    }
}

pub struct LinkStateInterceptor {
    pub(crate) session: Session,
    pub(crate) tables: Arc<RwLock<Tables>>,
    pub(crate) demux: DeMux,
}

impl LinkStateInterceptor {
    fn new(session: Session, tables: Arc<RwLock<Tables>>, demux: DeMux) -> Self {
        LinkStateInterceptor {
            session,
            tables,
            demux,
        }
    }

    pub(crate) fn handle_message(&self, msg: ZenohMessage) -> ZResult<()> {
        match msg.body {
            ZenohBody::LinkStateList(list) => {
                let pid = self.session.get_pid().unwrap();
                let mut tables = zwrite!(self.tables);
                let whatami = self.session.get_whatami()?;
                match (tables.whatami, whatami) {
                    (whatami::ROUTER, whatami::ROUTER) => {
                        for (_, removed_node) in tables
                            .routers_net
                            .as_mut()
                            .unwrap()
                            .link_states(list.link_states, pid)
                        {
                            pubsub_remove_node(&mut tables, &removed_node.pid, whatami::ROUTER);
                            queries_remove_node(&mut tables, &removed_node.pid, whatami::ROUTER);
                        }

                        tables.shared_nodes = shared_nodes(
                            tables.routers_net.as_ref().unwrap(),
                            tables.peers_net.as_ref().unwrap(),
                        );

                        tables.schedule_compute_trees(self.tables.clone(), whatami::ROUTER);
                    }
                    (whatami::ROUTER, whatami::PEER)
                    | (whatami::PEER, whatami::ROUTER)
                    | (whatami::PEER, whatami::PEER) => {
                        for (_, removed_node) in tables
                            .peers_net
                            .as_mut()
                            .unwrap()
                            .link_states(list.link_states, pid)
                        {
                            pubsub_remove_node(&mut tables, &removed_node.pid, whatami::PEER);
                            queries_remove_node(&mut tables, &removed_node.pid, whatami::PEER);
                        }

                        if tables.whatami == whatami::ROUTER {
                            tables.shared_nodes = shared_nodes(
                                tables.routers_net.as_ref().unwrap(),
                                tables.peers_net.as_ref().unwrap(),
                            );
                        }

                        tables.schedule_compute_trees(self.tables.clone(), whatami::PEER);
                    }
                    _ => (),
                };

                Ok(())
            }
            _ => self.demux.handle_message(msg),
        }
    }

    pub(crate) fn new_link(&self, _link: Link) {}

    pub(crate) fn del_link(&self, _link: Link) {}

    pub(crate) fn closing(&self) {
        self.demux.closing();
        let tables_ref = self.tables.clone();
        let pid = self.session.get_pid().unwrap();
        let whatami = self.session.get_whatami();
        async_std::task::spawn(async move {
            async_std::task::sleep(std::time::Duration::from_millis(*LINK_CLOSURE_DELAY)).await;
            let mut tables = zwrite!(tables_ref);
            match whatami {
                Ok(whatami) => match (tables.whatami, whatami) {
                    (whatami::ROUTER, whatami::ROUTER) => {
                        for (_, removed_node) in
                            tables.routers_net.as_mut().unwrap().remove_link(&pid)
                        {
                            pubsub_remove_node(&mut tables, &removed_node.pid, whatami::ROUTER);
                            queries_remove_node(&mut tables, &removed_node.pid, whatami::ROUTER);
                        }

                        tables.shared_nodes = shared_nodes(
                            tables.routers_net.as_ref().unwrap(),
                            tables.peers_net.as_ref().unwrap(),
                        );

                        tables.schedule_compute_trees(tables_ref.clone(), whatami::ROUTER);
                    }
                    (whatami::ROUTER, whatami::PEER)
                    | (whatami::PEER, whatami::ROUTER)
                    | (whatami::PEER, whatami::PEER) => {
                        for (_, removed_node) in
                            tables.peers_net.as_mut().unwrap().remove_link(&pid)
                        {
                            pubsub_remove_node(&mut tables, &removed_node.pid, whatami::PEER);
                            queries_remove_node(&mut tables, &removed_node.pid, whatami::PEER);
                        }

                        if tables.whatami == whatami::ROUTER {
                            tables.shared_nodes = shared_nodes(
                                tables.routers_net.as_ref().unwrap(),
                                tables.peers_net.as_ref().unwrap(),
                            );
                        }

                        tables.schedule_compute_trees(tables_ref.clone(), whatami::PEER);
                    }
                    _ => (),
                },
                Err(_) => log::error!("Unable to get whatami closing session!"),
            };
        });
    }

    pub(crate) fn closed(&self) {}
}
