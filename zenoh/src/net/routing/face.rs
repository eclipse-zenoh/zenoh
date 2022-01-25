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
    Channel, CongestionControl, KeyExpr, QueryConsolidation, QueryTarget, QueryableInfo, SubInfo,
    WhatAmI, ZInt, ZenohId,
};
use super::protocol::io::ZBuf;
use super::protocol::message::{DataInfo, RoutingContext};
use super::router::*;
use super::transport::Primitives;
use async_std::sync::Arc;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::RwLock;

pub struct FaceState {
    pub(super) id: usize,
    pub(super) pid: ZenohId,
    pub(super) whatami: WhatAmI,
    pub(super) primitives: Arc<dyn Primitives + Send + Sync>,
    pub(super) link_id: usize,
    pub(super) local_mappings: HashMap<ZInt, Arc<Resource>>,
    pub(super) remote_mappings: HashMap<ZInt, Arc<Resource>>,
    pub(super) local_subs: HashSet<Arc<Resource>>,
    pub(super) remote_subs: HashSet<Arc<Resource>>,
    pub(super) local_qabls: HashMap<(Arc<Resource>, ZInt), QueryableInfo>,
    pub(super) remote_qabls: HashSet<(Arc<Resource>, ZInt)>,
    pub(super) next_qid: ZInt,
    pub(super) pending_queries: HashMap<ZInt, Arc<Query>>,
}

impl FaceState {
    pub(super) fn new(
        id: usize,
        pid: ZenohId,
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

    pub(super) fn get_router(
        &self,
        tables: &Tables,
        routing_context: Option<RoutingContext>,
    ) -> Option<ZenohId> {
        match routing_context {
            Some(routing_context) => {
                match tables.routers_net.as_ref().unwrap().get_link(self.link_id) {
                    Some(link) => match link.get_zid(&routing_context.tree_id) {
                        Some(router) => Some(*router),
                        None => {
                            log::error!(
                                "Received router declaration with unknown routing context id {}",
                                routing_context.tree_id
                            );
                            None
                        }
                    },
                    None => {
                        log::error!(
                            "Could not find corresponding link in routers network for {}",
                            self
                        );
                        None
                    }
                }
            }
            None => {
                log::error!("Received router declaration with no routing context");
                None
            }
        }
    }

    pub(super) fn get_peer(
        &self,
        tables: &Tables,
        routing_context: Option<RoutingContext>,
    ) -> Option<ZenohId> {
        match routing_context {
            Some(routing_context) => {
                match tables.peers_net.as_ref().unwrap().get_link(self.link_id) {
                    Some(link) => match link.get_zid(&routing_context.tree_id) {
                        Some(router) => Some(*router),
                        None => {
                            log::error!(
                                "Received peer declaration with unknown routing context id {}",
                                routing_context.tree_id
                            );
                            None
                        }
                    },
                    None => {
                        log::error!(
                            "Could not find corresponding link in peers network for {}",
                            self
                        );
                        None
                    }
                }
            }
            None => {
                log::error!("Received peer declaration with no routing context");
                None
            }
        }
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
    fn decl_resource(&self, expr_id: ZInt, key_expr: &KeyExpr) {
        let mut tables = zwrite!(self.tables);
        register_expr(&mut tables, &mut self.state.clone(), expr_id, key_expr);
    }

    fn forget_resource(&self, expr_id: ZInt) {
        let mut tables = zwrite!(self.tables);
        unregister_expr(&mut tables, &mut self.state.clone(), expr_id);
    }

    fn decl_subscriber(
        &self,
        key_expr: &KeyExpr,
        sub_info: &SubInfo,
        routing_context: Option<RoutingContext>,
    ) {
        let mut tables = zwrite!(self.tables);
        match (tables.whatami, self.state.whatami) {
            (WhatAmI::Router, WhatAmI::Router) => {
                if let Some(router) = self.state.get_router(&tables, routing_context) {
                    declare_router_subscription(
                        &mut tables,
                        &mut self.state.clone(),
                        key_expr,
                        sub_info,
                        router,
                    )
                }
            }
            (WhatAmI::Router, WhatAmI::Peer)
            | (WhatAmI::Peer, WhatAmI::Router)
            | (WhatAmI::Peer, WhatAmI::Peer) => {
                if let Some(peer) = self.state.get_peer(&tables, routing_context) {
                    declare_peer_subscription(
                        &mut tables,
                        &mut self.state.clone(),
                        key_expr,
                        sub_info,
                        peer,
                    )
                }
            }
            _ => declare_client_subscription(
                &mut tables,
                &mut self.state.clone(),
                key_expr,
                sub_info,
            ),
        }
    }

    fn forget_subscriber(&self, key_expr: &KeyExpr, routing_context: Option<RoutingContext>) {
        let mut tables = zwrite!(self.tables);
        match (tables.whatami, self.state.whatami) {
            (WhatAmI::Router, WhatAmI::Router) => {
                if let Some(router) = self.state.get_router(&tables, routing_context) {
                    forget_router_subscription(
                        &mut tables,
                        &mut self.state.clone(),
                        key_expr,
                        &router,
                    )
                }
            }
            (WhatAmI::Router, WhatAmI::Peer)
            | (WhatAmI::Peer, WhatAmI::Router)
            | (WhatAmI::Peer, WhatAmI::Peer) => {
                if let Some(peer) = self.state.get_peer(&tables, routing_context) {
                    forget_peer_subscription(&mut tables, &mut self.state.clone(), key_expr, &peer)
                }
            }
            _ => forget_client_subscription(&mut tables, &mut self.state.clone(), key_expr),
        }
    }

    fn decl_publisher(&self, _key_expr: &KeyExpr, _routing_context: Option<RoutingContext>) {}

    fn forget_publisher(&self, _key_expr: &KeyExpr, _routing_context: Option<RoutingContext>) {}

    fn decl_queryable(
        &self,
        key_expr: &KeyExpr,
        kind: ZInt,
        qabl_info: &QueryableInfo,
        routing_context: Option<RoutingContext>,
    ) {
        let mut tables = zwrite!(self.tables);
        match (tables.whatami, self.state.whatami) {
            (WhatAmI::Router, WhatAmI::Router) => {
                if let Some(router) = self.state.get_router(&tables, routing_context) {
                    declare_router_queryable(
                        &mut tables,
                        &mut self.state.clone(),
                        key_expr,
                        kind,
                        qabl_info,
                        router,
                    )
                }
            }
            (WhatAmI::Router, WhatAmI::Peer)
            | (WhatAmI::Peer, WhatAmI::Router)
            | (WhatAmI::Peer, WhatAmI::Peer) => {
                if let Some(peer) = self.state.get_peer(&tables, routing_context) {
                    declare_peer_queryable(
                        &mut tables,
                        &mut self.state.clone(),
                        key_expr,
                        kind,
                        qabl_info,
                        peer,
                    )
                }
            }
            _ => declare_client_queryable(
                &mut tables,
                &mut self.state.clone(),
                key_expr,
                kind,
                qabl_info,
            ),
        }
    }

    fn forget_queryable(
        &self,
        key_expr: &KeyExpr,
        kind: ZInt,
        routing_context: Option<RoutingContext>,
    ) {
        let mut tables = zwrite!(self.tables);
        match (tables.whatami, self.state.whatami) {
            (WhatAmI::Router, WhatAmI::Router) => {
                if let Some(router) = self.state.get_router(&tables, routing_context) {
                    forget_router_queryable(
                        &mut tables,
                        &mut self.state.clone(),
                        key_expr,
                        kind,
                        &router,
                    )
                }
            }
            (WhatAmI::Router, WhatAmI::Peer)
            | (WhatAmI::Peer, WhatAmI::Router)
            | (WhatAmI::Peer, WhatAmI::Peer) => {
                if let Some(peer) = self.state.get_peer(&tables, routing_context) {
                    forget_peer_queryable(
                        &mut tables,
                        &mut self.state.clone(),
                        key_expr,
                        kind,
                        &peer,
                    )
                }
            }
            _ => forget_client_queryable(&mut tables, &mut self.state.clone(), key_expr, kind),
        }
    }

    fn send_data(
        &self,
        key_expr: &KeyExpr,
        payload: ZBuf,
        channel: Channel,
        congestion_control: CongestionControl,
        data_info: Option<DataInfo>,
        routing_context: Option<RoutingContext>,
    ) {
        full_reentrant_route_data(
            &self.tables,
            &self.state,
            key_expr,
            channel,
            congestion_control,
            data_info,
            payload,
            routing_context,
        );
    }

    fn send_query(
        &self,
        key_expr: &KeyExpr,
        value_selector: &str,
        qid: ZInt,
        target: QueryTarget,
        consolidation: QueryConsolidation,
        routing_context: Option<RoutingContext>,
    ) {
        route_query(
            &self.tables,
            &self.state,
            key_expr,
            value_selector,
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
        replier_id: ZenohId,
        key_expr: KeyExpr,
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
            key_expr,
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
        key_expr: &KeyExpr,
        pull_id: ZInt,
        max_samples: &Option<ZInt>,
    ) {
        let mut tables = zwrite!(self.tables);
        pull_data(
            &mut tables,
            &self.state.clone(),
            is_final,
            key_expr,
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
