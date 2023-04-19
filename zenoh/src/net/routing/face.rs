//
// Copyright (c) 2023 ZettaScale Technology
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
use super::router::*;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;
use zenoh_buffers::ZBuf;
use zenoh_protocol::{
    core::{
        Channel, CongestionControl, ConsolidationMode, QueryTarget, QueryableInfo, SubInfo,
        WhatAmI, WireExpr, ZInt, ZenohId,
    },
    zenoh::{DataInfo, QueryBody, RoutingContext},
};
use zenoh_transport::Primitives;

pub struct FaceState {
    pub(super) id: usize,
    pub(super) zid: ZenohId,
    pub(super) whatami: WhatAmI,
    pub(super) primitives: Arc<dyn Primitives + Send + Sync>,
    pub(super) link_id: usize,
    pub(super) local_mappings: HashMap<ZInt, Arc<Resource>>,
    pub(super) remote_mappings: HashMap<ZInt, Arc<Resource>>,
    pub(super) local_subs: HashSet<Arc<Resource>>,
    pub(super) remote_subs: HashSet<Arc<Resource>>,
    pub(super) local_qabls: HashMap<Arc<Resource>, QueryableInfo>,
    pub(super) remote_qabls: HashSet<Arc<Resource>>,
    pub(super) next_qid: ZInt,
    pub(super) pending_queries: HashMap<ZInt, Arc<Query>>,
}

impl FaceState {
    pub(super) fn new(
        id: usize,
        zid: ZenohId,
        whatami: WhatAmI,
        primitives: Arc<dyn Primitives + Send + Sync>,
        link_id: usize,
    ) -> Arc<FaceState> {
        Arc::new(FaceState {
            id,
            zid,
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
        write!(f, "Face{{{}, {}}}", self.id, self.zid)
    }
}

#[derive(Clone)]
pub struct Face {
    pub(crate) tables: Arc<TablesLock>,
    pub(crate) state: Arc<FaceState>,
}

impl Primitives for Face {
    fn decl_resource(&self, expr_id: ZInt, key_expr: &WireExpr) {
        let ctrl_lock = zlock!(self.tables.ctrl_lock);
        register_expr(&self.tables, &mut self.state.clone(), expr_id, key_expr);
        drop(ctrl_lock);
    }

    fn forget_resource(&self, expr_id: ZInt) {
        let ctrl_lock = zlock!(self.tables.ctrl_lock);
        unregister_expr(&self.tables, &mut self.state.clone(), expr_id);
        drop(ctrl_lock);
    }

    fn decl_subscriber(
        &self,
        key_expr: &WireExpr,
        sub_info: &SubInfo,
        routing_context: Option<RoutingContext>,
    ) {
        let ctrl_lock = zlock!(self.tables.ctrl_lock);
        let rtables = zread!(self.tables.tables);
        match (rtables.whatami, self.state.whatami) {
            (WhatAmI::Router, WhatAmI::Router) => {
                if let Some(router) = self.state.get_router(&rtables, routing_context) {
                    declare_router_subscription(
                        &self.tables,
                        rtables,
                        &mut self.state.clone(),
                        key_expr,
                        sub_info,
                        router,
                    );
                }
            }
            (WhatAmI::Router, WhatAmI::Peer)
            | (WhatAmI::Peer, WhatAmI::Router)
            | (WhatAmI::Peer, WhatAmI::Peer) => {
                if rtables.full_net(WhatAmI::Peer) {
                    if let Some(peer) = self.state.get_peer(&rtables, routing_context) {
                        declare_peer_subscription(
                            &self.tables,
                            rtables,
                            &mut self.state.clone(),
                            key_expr,
                            sub_info,
                            peer,
                        );
                    }
                } else {
                    declare_client_subscription(
                        &self.tables,
                        rtables,
                        &mut self.state.clone(),
                        key_expr,
                        sub_info,
                    );
                }
            }
            _ => {
                declare_client_subscription(
                    &self.tables,
                    rtables,
                    &mut self.state.clone(),
                    key_expr,
                    sub_info,
                );
            }
        }
        drop(ctrl_lock);
    }

    fn forget_subscriber(&self, key_expr: &WireExpr, routing_context: Option<RoutingContext>) {
        let ctrl_lock = zlock!(self.tables.ctrl_lock);
        let rtables = zread!(self.tables.tables);
        match (rtables.whatami, self.state.whatami) {
            (WhatAmI::Router, WhatAmI::Router) => {
                if let Some(router) = self.state.get_router(&rtables, routing_context) {
                    forget_router_subscription(
                        &self.tables,
                        rtables,
                        &mut self.state.clone(),
                        key_expr,
                        &router,
                    )
                }
            }
            (WhatAmI::Router, WhatAmI::Peer)
            | (WhatAmI::Peer, WhatAmI::Router)
            | (WhatAmI::Peer, WhatAmI::Peer) => {
                if rtables.full_net(WhatAmI::Peer) {
                    if let Some(peer) = self.state.get_peer(&rtables, routing_context) {
                        forget_peer_subscription(
                            &self.tables,
                            rtables,
                            &mut self.state.clone(),
                            key_expr,
                            &peer,
                        )
                    }
                } else {
                    forget_client_subscription(
                        &self.tables,
                        rtables,
                        &mut self.state.clone(),
                        key_expr,
                    )
                }
            }
            _ => {
                forget_client_subscription(&self.tables, rtables, &mut self.state.clone(), key_expr)
            }
        }
        drop(ctrl_lock);
    }

    fn decl_publisher(&self, _key_expr: &WireExpr, _routing_context: Option<RoutingContext>) {}

    fn forget_publisher(&self, _key_expr: &WireExpr, _routing_context: Option<RoutingContext>) {}

    fn decl_queryable(
        &self,
        key_expr: &WireExpr,
        qabl_info: &QueryableInfo,
        routing_context: Option<RoutingContext>,
    ) {
        let ctrl_lock = zlock!(self.tables.ctrl_lock);
        let rtables = zread!(self.tables.tables);
        match (rtables.whatami, self.state.whatami) {
            (WhatAmI::Router, WhatAmI::Router) => {
                if let Some(router) = self.state.get_router(&rtables, routing_context) {
                    declare_router_queryable(
                        &self.tables,
                        rtables,
                        &mut self.state.clone(),
                        key_expr,
                        qabl_info,
                        router,
                    )
                }
            }
            (WhatAmI::Router, WhatAmI::Peer)
            | (WhatAmI::Peer, WhatAmI::Router)
            | (WhatAmI::Peer, WhatAmI::Peer) => {
                if rtables.full_net(WhatAmI::Peer) {
                    if let Some(peer) = self.state.get_peer(&rtables, routing_context) {
                        declare_peer_queryable(
                            &self.tables,
                            rtables,
                            &mut self.state.clone(),
                            key_expr,
                            qabl_info,
                            peer,
                        )
                    }
                } else {
                    declare_client_queryable(
                        &self.tables,
                        rtables,
                        &mut self.state.clone(),
                        key_expr,
                        qabl_info,
                    )
                }
            }
            _ => declare_client_queryable(
                &self.tables,
                rtables,
                &mut self.state.clone(),
                key_expr,
                qabl_info,
            ),
        }
        drop(ctrl_lock);
    }

    fn forget_queryable(&self, key_expr: &WireExpr, routing_context: Option<RoutingContext>) {
        let ctrl_lock = zlock!(self.tables.ctrl_lock);
        let rtables = zread!(self.tables.tables);
        match (rtables.whatami, self.state.whatami) {
            (WhatAmI::Router, WhatAmI::Router) => {
                if let Some(router) = self.state.get_router(&rtables, routing_context) {
                    forget_router_queryable(
                        &self.tables,
                        rtables,
                        &mut self.state.clone(),
                        key_expr,
                        &router,
                    )
                }
            }
            (WhatAmI::Router, WhatAmI::Peer)
            | (WhatAmI::Peer, WhatAmI::Router)
            | (WhatAmI::Peer, WhatAmI::Peer) => {
                if rtables.full_net(WhatAmI::Peer) {
                    if let Some(peer) = self.state.get_peer(&rtables, routing_context) {
                        forget_peer_queryable(
                            &self.tables,
                            rtables,
                            &mut self.state.clone(),
                            key_expr,
                            &peer,
                        )
                    }
                } else {
                    forget_client_queryable(
                        &self.tables,
                        rtables,
                        &mut self.state.clone(),
                        key_expr,
                    )
                }
            }
            _ => forget_client_queryable(&self.tables, rtables, &mut self.state.clone(), key_expr),
        }
        drop(ctrl_lock);
    }

    fn send_data(
        &self,
        key_expr: &WireExpr,
        payload: ZBuf,
        channel: Channel,
        congestion_control: CongestionControl,
        data_info: Option<DataInfo>,
        routing_context: Option<RoutingContext>,
    ) {
        full_reentrant_route_data(
            &self.tables.tables,
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
        key_expr: &WireExpr,
        parameters: &str,
        qid: ZInt,
        target: QueryTarget,
        consolidation: ConsolidationMode,
        body: Option<QueryBody>,
        routing_context: Option<RoutingContext>,
    ) {
        route_query(
            &self.tables,
            &self.state,
            key_expr,
            parameters,
            qid,
            target,
            consolidation,
            body,
            routing_context,
        );
    }

    fn send_reply_data(
        &self,
        qid: ZInt,
        replier_id: ZenohId,
        key_expr: WireExpr,
        info: Option<DataInfo>,
        payload: ZBuf,
    ) {
        route_send_reply_data(
            &self.tables,
            &mut self.state.clone(),
            qid,
            replier_id,
            key_expr,
            info,
            payload,
        );
    }

    fn send_reply_final(&self, qid: ZInt) {
        route_send_reply_final(&self.tables, &mut self.state.clone(), qid);
    }

    fn send_pull(
        &self,
        is_final: bool,
        key_expr: &WireExpr,
        pull_id: ZInt,
        max_samples: &Option<ZInt>,
    ) {
        pull_data(
            &self.tables.tables,
            &self.state.clone(),
            is_final,
            key_expr,
            pull_id,
            max_samples,
        );
    }

    fn send_close(&self) {
        super::router::close_face(&self.tables, &Arc::downgrade(&self.state));
    }
}

impl fmt::Display for Face {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.state.fmt(f)
    }
}
