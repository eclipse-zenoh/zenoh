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
use super::protocol::core::{
    whatami, Channel, CongestionControl, PeerId, QueryConsolidation, QueryTarget, ResKey, SubInfo,
    WhatAmI, ZInt,
};
use super::protocol::io::ZBuf;
use super::protocol::proto::{DataInfo, RoutingContext};
use super::protocol::session::Primitives;
use super::router::*;
use async_std::sync::Arc;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::RwLock;

pub struct FaceState {
    pub(super) id: usize,
    pub(super) pid: PeerId,
    pub(super) whatami: WhatAmI,
    pub(super) primitives: Arc<dyn Primitives + Send + Sync>,
    pub(super) link_id: usize,
    pub(super) local_mappings: HashMap<ZInt, Arc<Resource>>,
    pub(super) remote_mappings: HashMap<ZInt, Arc<Resource>>,
    pub(super) local_subs: HashSet<Arc<Resource>>,
    pub(super) remote_subs: HashSet<Arc<Resource>>,
    pub(super) local_qabls: HashMap<Arc<Resource>, ZInt>,
    pub(super) remote_qabls: HashSet<Arc<Resource>>,
    pub(super) next_qid: ZInt,
    pub(super) pending_queries: HashMap<ZInt, Arc<Query>>,
}

impl FaceState {
    pub(super) fn new(
        id: usize,
        pid: PeerId,
        whatami: WhatAmI,
        primitives: Arc<dyn Primitives + Send + Sync>,
        link_id: usize,
    ) -> Arc<FaceState> {
        Arc::new(FaceState {
            id,
            pid,
            whatami,
            primitives,
            link_id,
            local_mappings: HashMap::new(),
            remote_mappings: HashMap::new(),
            local_subs: HashSet::new(),
            remote_subs: HashSet::new(),
            local_qabls: HashMap::new(),
            remote_qabls: HashSet::new(),
            next_qid: 0,
            pending_queries: HashMap::new(),
        })
    }

    #[inline]
    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub(super) fn get_mapping(&self, prefixid: &ZInt) -> Option<&std::sync::Arc<Resource>> {
        match self.remote_mappings.get(prefixid) {
            Some(prefix) => Some(prefix),
            None => self.local_mappings.get(prefixid),
        }
    }

    pub(super) fn get_next_local_id(&self) -> ZInt {
        let mut id = 1;
        while self.local_mappings.get(&id).is_some() || self.remote_mappings.get(&id).is_some() {
            id += 1;
        }
        id
    }
}

impl fmt::Display for FaceState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Face{{{}, {}}}", self.id, self.pid)
    }
}

#[derive(Clone)]
pub struct Face {
    pub(crate) tables: Arc<RwLock<Tables>>,
    pub(crate) state: Arc<FaceState>,
}

impl Primitives for Face {
    fn decl_resource(&self, rid: ZInt, reskey: &ResKey) {
        let (prefixid, suffix) = reskey.into();
        let mut tables = zwrite!(self.tables);
        declare_resource(&mut tables, &mut self.state.clone(), rid, prefixid, suffix);
    }

    fn forget_resource(&self, rid: ZInt) {
        let mut tables = zwrite!(self.tables);
        undeclare_resource(&mut tables, &mut self.state.clone(), rid);
    }

    fn decl_subscriber(
        &self,
        reskey: &ResKey,
        sub_info: &SubInfo,
        routing_context: Option<RoutingContext>,
    ) {
        let (prefixid, suffix) = reskey.into();
        let mut tables = zwrite!(self.tables);
        match (tables.whatami, self.state.whatami) {
            (whatami::ROUTER, whatami::ROUTER) => match routing_context {
                Some(routing_context) => {
                    let router = match tables
                        .routers_net
                        .as_ref()
                        .unwrap()
                        .get_link(self.state.link_id)
                        .get_pid(&routing_context.tree_id)
                    {
                        Some(router) => router.clone(),
                        None => {
                            log::error!(
                                "Received router subscription with unknown routing context id {}",
                                routing_context.tree_id
                            );
                            return;
                        }
                    };

                    declare_router_subscription(
                        &mut tables,
                        &mut self.state.clone(),
                        prefixid,
                        suffix,
                        sub_info,
                        router,
                    )
                }

                None => {
                    log::error!("Received router subscription with no routing context");
                }
            },
            (whatami::ROUTER, whatami::PEER)
            | (whatami::PEER, whatami::ROUTER)
            | (whatami::PEER, whatami::PEER) => match routing_context {
                Some(routing_context) => {
                    let peer = match tables
                        .peers_net
                        .as_ref()
                        .unwrap()
                        .get_link(self.state.link_id)
                        .get_pid(&routing_context.tree_id)
                    {
                        Some(peer) => peer.clone(),
                        None => {
                            log::error!(
                                "Received peer subscription with unknown routing context id {}",
                                routing_context.tree_id
                            );
                            return;
                        }
                    };

                    declare_peer_subscription(
                        &mut tables,
                        &mut self.state.clone(),
                        prefixid,
                        suffix,
                        sub_info,
                        peer,
                    )
                }

                None => {
                    log::error!("Received peer subscription with no routing context");
                }
            },
            _ => declare_client_subscription(
                &mut tables,
                &mut self.state.clone(),
                prefixid,
                suffix,
                sub_info,
            ),
        }
    }

    fn forget_subscriber(&self, reskey: &ResKey, routing_context: Option<RoutingContext>) {
        let (prefixid, suffix) = reskey.into();
        let mut tables = zwrite!(self.tables);
        match (tables.whatami, self.state.whatami) {
            (whatami::ROUTER, whatami::ROUTER) => match routing_context {
                Some(routing_context) => {
                    let router = match tables
                        .routers_net
                        .as_ref()
                        .unwrap()
                        .get_link(self.state.link_id)
                        .get_pid(&routing_context.tree_id)
                    {
                        Some(router) => router.clone(),
                        None => {
                            log::error!(
                                "Received router forget subscription with unknown routing context id {}",
                                routing_context.tree_id
                            );
                            return;
                        }
                    };

                    forget_router_subscription(
                        &mut tables,
                        &mut self.state.clone(),
                        prefixid,
                        suffix,
                        &router,
                    )
                }

                None => {
                    log::error!("Received router forget subscription with no routing context");
                }
            },
            (whatami::ROUTER, whatami::PEER)
            | (whatami::PEER, whatami::ROUTER)
            | (whatami::PEER, whatami::PEER) => match routing_context {
                Some(routing_context) => {
                    let peer = match tables
                        .peers_net
                        .as_ref()
                        .unwrap()
                        .get_link(self.state.link_id)
                        .get_pid(&routing_context.tree_id)
                    {
                        Some(peer) => peer.clone(),
                        None => {
                            log::error!(
                                "Received peer forget subscription with unknown routing context id {}",
                                routing_context.tree_id
                            );
                            return;
                        }
                    };

                    forget_peer_subscription(
                        &mut tables,
                        &mut self.state.clone(),
                        prefixid,
                        suffix,
                        &peer,
                    )
                }

                None => {
                    log::error!("Received peer forget subscription with no routing context");
                }
            },
            _ => forget_client_subscription(&mut tables, &mut self.state.clone(), prefixid, suffix),
        }
    }

    fn decl_publisher(&self, _reskey: &ResKey, _routing_context: Option<RoutingContext>) {}

    fn forget_publisher(&self, _reskey: &ResKey, _routing_context: Option<RoutingContext>) {}

    fn decl_queryable(&self, reskey: &ResKey, kind: ZInt, routing_context: Option<RoutingContext>) {
        let (prefixid, suffix) = reskey.into();
        let mut tables = zwrite!(self.tables);
        match (tables.whatami, self.state.whatami) {
            (whatami::ROUTER, whatami::ROUTER) => match routing_context {
                Some(routing_context) => {
                    let router = match tables
                        .routers_net
                        .as_ref()
                        .unwrap()
                        .get_link(self.state.link_id)
                        .get_pid(&routing_context.tree_id)
                    {
                        Some(router) => router.clone(),
                        None => {
                            log::error!(
                                "Received router queryable with unknown routing context id {}",
                                routing_context.tree_id
                            );
                            return;
                        }
                    };

                    declare_router_queryable(
                        &mut tables,
                        &mut self.state.clone(),
                        prefixid,
                        suffix,
                        kind,
                        router,
                    )
                }

                None => {
                    log::error!("Received router queryable with no routing context");
                }
            },
            (whatami::ROUTER, whatami::PEER)
            | (whatami::PEER, whatami::ROUTER)
            | (whatami::PEER, whatami::PEER) => match routing_context {
                Some(routing_context) => {
                    let peer = match tables
                        .peers_net
                        .as_ref()
                        .unwrap()
                        .get_link(self.state.link_id)
                        .get_pid(&routing_context.tree_id)
                    {
                        Some(peer) => peer.clone(),
                        None => {
                            log::error!(
                                "Received peer queryable with unknown routing context id {}",
                                routing_context.tree_id
                            );
                            return;
                        }
                    };

                    declare_peer_queryable(
                        &mut tables,
                        &mut self.state.clone(),
                        prefixid,
                        suffix,
                        kind,
                        peer,
                    )
                }

                None => {
                    log::error!("Received peer queryable with no routing context");
                }
            },
            _ => declare_client_queryable(
                &mut tables,
                &mut self.state.clone(),
                prefixid,
                suffix,
                kind,
            ),
        }
    }

    fn forget_queryable(&self, reskey: &ResKey, routing_context: Option<RoutingContext>) {
        let (prefixid, suffix) = reskey.into();
        let mut tables = zwrite!(self.tables);
        match (tables.whatami, self.state.whatami) {
            (whatami::ROUTER, whatami::ROUTER) => match routing_context {
                Some(routing_context) => {
                    let router = match tables
                        .routers_net
                        .as_ref()
                        .unwrap()
                        .get_link(self.state.link_id)
                        .get_pid(&routing_context.tree_id)
                    {
                        Some(router) => router.clone(),
                        None => {
                            log::error!(
                                "Received router forget queryable with unknown routing context id {}",
                                routing_context.tree_id
                            );
                            return;
                        }
                    };

                    forget_router_queryable(
                        &mut tables,
                        &mut self.state.clone(),
                        prefixid,
                        suffix,
                        &router,
                    )
                }

                None => {
                    log::error!("Received router forget queryable with no routing context");
                }
            },
            (whatami::ROUTER, whatami::PEER)
            | (whatami::PEER, whatami::ROUTER)
            | (whatami::PEER, whatami::PEER) => match routing_context {
                Some(routing_context) => {
                    let peer = match tables
                        .peers_net
                        .as_ref()
                        .unwrap()
                        .get_link(self.state.link_id)
                        .get_pid(&routing_context.tree_id)
                    {
                        Some(peer) => peer.clone(),
                        None => {
                            log::error!(
                                "Received peer forget queryable with unknown routing context id {}",
                                routing_context.tree_id
                            );
                            return;
                        }
                    };

                    forget_peer_queryable(
                        &mut tables,
                        &mut self.state.clone(),
                        prefixid,
                        suffix,
                        &peer,
                    )
                }

                None => {
                    log::error!("Received peer forget queryable with no routing context");
                }
            },
            _ => forget_client_queryable(&mut tables, &mut self.state.clone(), prefixid, suffix),
        }
    }

    fn send_data(
        &self,
        reskey: &ResKey,
        payload: ZBuf,
        channel: Channel,
        congestion_control: CongestionControl,
        data_info: Option<DataInfo>,
        routing_context: Option<RoutingContext>,
    ) {
        let (prefixid, suffix) = reskey.into();
        full_reentrant_route_data(
            &self.tables,
            &self.state,
            prefixid,
            suffix,
            channel,
            congestion_control,
            data_info,
            payload,
            routing_context,
        );
    }

    fn send_query(
        &self,
        reskey: &ResKey,
        predicate: &str,
        qid: ZInt,
        target: QueryTarget,
        consolidation: QueryConsolidation,
        routing_context: Option<RoutingContext>,
    ) {
        let (prefixid, suffix) = reskey.into();
        let mut tables = zwrite!(self.tables);
        route_query(
            &mut tables,
            &self.state,
            prefixid,
            suffix,
            predicate,
            qid,
            target,
            consolidation,
            routing_context,
        );
    }

    fn send_reply_data(
        &self,
        qid: ZInt,
        replier_kind: ZInt,
        replier_id: PeerId,
        reskey: ResKey,
        info: Option<DataInfo>,
        payload: ZBuf,
    ) {
        let mut tables = zwrite!(self.tables);
        route_send_reply_data(
            &mut tables,
            &mut self.state.clone(),
            qid,
            replier_kind,
            replier_id,
            reskey,
            info,
            payload,
        );
    }

    fn send_reply_final(&self, qid: ZInt) {
        let mut tables = zwrite!(self.tables);
        route_send_reply_final(&mut tables, &mut self.state.clone(), qid);
    }

    fn send_pull(
        &self,
        is_final: bool,
        reskey: &ResKey,
        pull_id: ZInt,
        max_samples: &Option<ZInt>,
    ) {
        let (prefixid, suffix) = reskey.into();
        let mut tables = zwrite!(self.tables);
        pull_data(
            &mut tables,
            &self.state.clone(),
            is_final,
            prefixid,
            suffix,
            pull_id,
            max_samples,
        );
    }

    fn send_close(&self) {
        zwrite!(self.tables).close_face(&Arc::downgrade(&self.state));
    }
}

impl fmt::Display for Face {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.state.fmt(f)
    }
}
