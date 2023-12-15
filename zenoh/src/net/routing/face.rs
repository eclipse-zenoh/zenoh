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
use zenoh_protocol::zenoh::RequestBody;
use zenoh_protocol::{
    core::{ExprId, WhatAmI, ZenohId},
    network::{
        declare::queryable::ext::QueryableInfo, Mapping, Push, Request, RequestId, Response,
        ResponseFinal,
    },
};
#[cfg(feature = "stats")]
use zenoh_transport::stats::TransportStats;
use zenoh_transport::{multicast::TransportMulticast, primitives::Primitives};

pub struct FaceState {
    pub(super) id: usize,
    pub(super) zid: ZenohId,
    pub(super) whatami: WhatAmI,
    pub(super) local: bool,
    #[cfg(feature = "stats")]
    pub(super) stats: Option<Arc<TransportStats>>,
    pub(super) primitives: Arc<dyn Primitives + Send + Sync>,
    pub(super) link_id: usize,
    pub(super) local_mappings: HashMap<ExprId, Arc<Resource>>,
    pub(super) remote_mappings: HashMap<ExprId, Arc<Resource>>,
    pub(super) local_subs: HashSet<Arc<Resource>>,
    pub(super) remote_subs: HashSet<Arc<Resource>>,
    pub(super) local_qabls: HashMap<Arc<Resource>, QueryableInfo>,
    pub(super) remote_qabls: HashSet<Arc<Resource>>,
    pub(super) next_qid: RequestId,
    pub(super) pending_queries: HashMap<RequestId, Arc<Query>>,
    pub(super) mcast_group: Option<TransportMulticast>,
}

impl FaceState {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        id: usize,
        zid: ZenohId,
        whatami: WhatAmI,
        local: bool,
        #[cfg(feature = "stats")] stats: Option<Arc<TransportStats>>,
        primitives: Arc<dyn Primitives + Send + Sync>,
        link_id: usize,
        mcast_group: Option<TransportMulticast>,
    ) -> Arc<FaceState> {
        Arc::new(FaceState {
            id,
            zid,
            whatami,
            local,
            #[cfg(feature = "stats")]
            stats,
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
            mcast_group,
        })
    }

    #[inline]
    pub fn is_local(&self) -> bool {
        self.local
    }

    #[inline]
    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub(super) fn get_mapping(
        &self,
        prefixid: &ExprId,
        mapping: Mapping,
    ) -> Option<&std::sync::Arc<Resource>> {
        match mapping {
            Mapping::Sender => self.remote_mappings.get(prefixid),
            Mapping::Receiver => self.local_mappings.get(prefixid),
        }
    }

    pub(super) fn get_next_local_id(&self) -> ExprId {
        let mut id = 1;
        while self.local_mappings.get(&id).is_some() || self.remote_mappings.get(&id).is_some() {
            id += 1;
        }
        id
    }

    pub(super) fn get_router(&self, tables: &Tables, nodeid: &u64) -> Option<ZenohId> {
        match tables.routers_net.as_ref().unwrap().get_link(self.link_id) {
            Some(link) => match link.get_zid(nodeid) {
                Some(router) => Some(*router),
                None => {
                    log::error!(
                        "Received router declaration with unknown routing context id {}",
                        nodeid
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

    pub(super) fn get_peer(&self, tables: &Tables, nodeid: &u64) -> Option<ZenohId> {
        match tables.peers_net.as_ref().unwrap().get_link(self.link_id) {
            Some(link) => match link.get_zid(nodeid) {
                Some(router) => Some(*router),
                None => {
                    log::error!(
                        "Received peer declaration with unknown routing context id {}",
                        nodeid
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
    fn send_declare(&self, msg: zenoh_protocol::network::Declare) {
        let ctrl_lock = zlock!(self.tables.ctrl_lock);
        match msg.body {
            zenoh_protocol::network::DeclareBody::DeclareKeyExpr(m) => {
                register_expr(&self.tables, &mut self.state.clone(), m.id, &m.wire_expr);
            }
            zenoh_protocol::network::DeclareBody::UndeclareKeyExpr(m) => {
                unregister_expr(&self.tables, &mut self.state.clone(), m.id);
            }
            zenoh_protocol::network::DeclareBody::DeclareSubscriber(m) => {
                let rtables = zread!(self.tables.tables);
                match (rtables.whatami, self.state.whatami) {
                    (WhatAmI::Router, WhatAmI::Router) => {
                        if let Some(router) = self
                            .state
                            .get_router(&rtables, &(msg.ext_nodeid.node_id as u64))
                        {
                            declare_router_subscription(
                                &self.tables,
                                rtables,
                                &mut self.state.clone(),
                                &m.wire_expr,
                                &m.ext_info,
                                router,
                            )
                        }
                    }
                    (WhatAmI::Router, WhatAmI::Peer)
                    | (WhatAmI::Peer, WhatAmI::Router)
                    | (WhatAmI::Peer, WhatAmI::Peer) => {
                        if rtables.full_net(WhatAmI::Peer) {
                            if let Some(peer) = self
                                .state
                                .get_peer(&rtables, &(msg.ext_nodeid.node_id as u64))
                            {
                                declare_peer_subscription(
                                    &self.tables,
                                    rtables,
                                    &mut self.state.clone(),
                                    &m.wire_expr,
                                    &m.ext_info,
                                    peer,
                                )
                            }
                        } else {
                            declare_client_subscription(
                                &self.tables,
                                rtables,
                                &mut self.state.clone(),
                                &m.wire_expr,
                                &m.ext_info,
                            )
                        }
                    }
                    _ => declare_client_subscription(
                        &self.tables,
                        rtables,
                        &mut self.state.clone(),
                        &m.wire_expr,
                        &m.ext_info,
                    ),
                }
            }
            zenoh_protocol::network::DeclareBody::UndeclareSubscriber(m) => {
                let rtables = zread!(self.tables.tables);
                match (rtables.whatami, self.state.whatami) {
                    (WhatAmI::Router, WhatAmI::Router) => {
                        if let Some(router) = self
                            .state
                            .get_router(&rtables, &(msg.ext_nodeid.node_id as u64))
                        {
                            forget_router_subscription(
                                &self.tables,
                                rtables,
                                &mut self.state.clone(),
                                &m.ext_wire_expr.wire_expr,
                                &router,
                            )
                        }
                    }
                    (WhatAmI::Router, WhatAmI::Peer)
                    | (WhatAmI::Peer, WhatAmI::Router)
                    | (WhatAmI::Peer, WhatAmI::Peer) => {
                        if rtables.full_net(WhatAmI::Peer) {
                            if let Some(peer) = self
                                .state
                                .get_peer(&rtables, &(msg.ext_nodeid.node_id as u64))
                            {
                                forget_peer_subscription(
                                    &self.tables,
                                    rtables,
                                    &mut self.state.clone(),
                                    &m.ext_wire_expr.wire_expr,
                                    &peer,
                                )
                            }
                        } else {
                            forget_client_subscription(
                                &self.tables,
                                rtables,
                                &mut self.state.clone(),
                                &m.ext_wire_expr.wire_expr,
                            )
                        }
                    }
                    _ => forget_client_subscription(
                        &self.tables,
                        rtables,
                        &mut self.state.clone(),
                        &m.ext_wire_expr.wire_expr,
                    ),
                }
            }
            zenoh_protocol::network::DeclareBody::DeclareQueryable(m) => {
                let rtables = zread!(self.tables.tables);
                match (rtables.whatami, self.state.whatami) {
                    (WhatAmI::Router, WhatAmI::Router) => {
                        if let Some(router) = self
                            .state
                            .get_router(&rtables, &(msg.ext_nodeid.node_id as u64))
                        {
                            declare_router_queryable(
                                &self.tables,
                                rtables,
                                &mut self.state.clone(),
                                &m.wire_expr,
                                &m.ext_info,
                                router,
                            )
                        }
                    }
                    (WhatAmI::Router, WhatAmI::Peer)
                    | (WhatAmI::Peer, WhatAmI::Router)
                    | (WhatAmI::Peer, WhatAmI::Peer) => {
                        if rtables.full_net(WhatAmI::Peer) {
                            if let Some(peer) = self
                                .state
                                .get_peer(&rtables, &(msg.ext_nodeid.node_id as u64))
                            {
                                declare_peer_queryable(
                                    &self.tables,
                                    rtables,
                                    &mut self.state.clone(),
                                    &m.wire_expr,
                                    &m.ext_info,
                                    peer,
                                )
                            }
                        } else {
                            declare_client_queryable(
                                &self.tables,
                                rtables,
                                &mut self.state.clone(),
                                &m.wire_expr,
                                &m.ext_info,
                            )
                        }
                    }
                    _ => declare_client_queryable(
                        &self.tables,
                        rtables,
                        &mut self.state.clone(),
                        &m.wire_expr,
                        &m.ext_info,
                    ),
                }
            }
            zenoh_protocol::network::DeclareBody::UndeclareQueryable(m) => {
                let rtables = zread!(self.tables.tables);
                match (rtables.whatami, self.state.whatami) {
                    (WhatAmI::Router, WhatAmI::Router) => {
                        if let Some(router) = self
                            .state
                            .get_router(&rtables, &(msg.ext_nodeid.node_id as u64))
                        {
                            forget_router_queryable(
                                &self.tables,
                                rtables,
                                &mut self.state.clone(),
                                &m.ext_wire_expr.wire_expr,
                                &router,
                            )
                        }
                    }
                    (WhatAmI::Router, WhatAmI::Peer)
                    | (WhatAmI::Peer, WhatAmI::Router)
                    | (WhatAmI::Peer, WhatAmI::Peer) => {
                        if rtables.full_net(WhatAmI::Peer) {
                            if let Some(peer) = self
                                .state
                                .get_peer(&rtables, &(msg.ext_nodeid.node_id as u64))
                            {
                                forget_peer_queryable(
                                    &self.tables,
                                    rtables,
                                    &mut self.state.clone(),
                                    &m.ext_wire_expr.wire_expr,
                                    &peer,
                                )
                            }
                        } else {
                            forget_client_queryable(
                                &self.tables,
                                rtables,
                                &mut self.state.clone(),
                                &m.ext_wire_expr.wire_expr,
                            )
                        }
                    }
                    _ => forget_client_queryable(
                        &self.tables,
                        rtables,
                        &mut self.state.clone(),
                        &m.ext_wire_expr.wire_expr,
                    ),
                }
            }
            zenoh_protocol::network::DeclareBody::DeclareToken(_m) => todo!(),
            zenoh_protocol::network::DeclareBody::UndeclareToken(_m) => todo!(),
            zenoh_protocol::network::DeclareBody::DeclareInterest(_m) => todo!(),
            zenoh_protocol::network::DeclareBody::FinalInterest(_m) => todo!(),
            zenoh_protocol::network::DeclareBody::UndeclareInterest(_m) => todo!(),
        }
        drop(ctrl_lock);
    }

    fn send_push(&self, msg: Push) {
        full_reentrant_route_data(
            &self.tables.tables,
            &self.state,
            &msg.wire_expr,
            msg.ext_qos,
            msg.payload,
            msg.ext_nodeid.node_id as u64,
        );
    }

    fn send_request(&self, msg: Request) {
        match msg.payload {
            RequestBody::Query(_) => {
                route_query(
                    &self.tables,
                    &self.state,
                    &msg.wire_expr,
                    // parameters,
                    msg.id,
                    msg.ext_target,
                    // consolidation,
                    msg.payload,
                    msg.ext_nodeid.node_id as u64,
                );
            }
            RequestBody::Pull(_) => {
                pull_data(&self.tables.tables, &self.state.clone(), msg.wire_expr);
            }
            _ => {
                log::error!("Unsupported request");
            }
        }
    }

    fn send_response(&self, msg: Response) {
        route_send_response(
            &self.tables,
            &mut self.state.clone(),
            msg.rid,
            msg.ext_respid,
            msg.wire_expr,
            msg.payload,
        );
    }

    fn send_response_final(&self, msg: ResponseFinal) {
        route_send_response_final(&self.tables, &mut self.state.clone(), msg.rid);
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
