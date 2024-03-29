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
use super::super::router::*;
use super::tables::TablesLock;
use super::token::{
    declare_token, declare_token_interest, undeclare_token, undeclare_token_interest,
};
use super::{resource::*, tables};
use crate::net::primitives::{IngressPrimitives, McastMux, Mux};
use crate::net::routing::interceptor::{InterceptorTrait, InterceptorsChain};
use crate::net::routing::RoutingContext;
use crate::KeyExpr;
use std::any::Any;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use zenoh_protocol::network::declare::{FinalInterest, Interest, InterestId};
use zenoh_protocol::network::{ext, Declare, DeclareBody};
use zenoh_protocol::zenoh::RequestBody;
use zenoh_protocol::{
    core::{ExprId, WhatAmI, ZenohId},
    network::{Mapping, Push, Request, RequestId, Response, ResponseFinal},
};
use zenoh_sync::get_mut_unchecked;
use zenoh_transport::multicast::TransportMulticast;
#[cfg(feature = "stats")]
use zenoh_transport::stats::TransportStats;

pub struct FaceState {
    pub(crate) id: usize,
    pub(crate) zid: ZenohId,
    pub(crate) whatami: WhatAmI,
    #[cfg(feature = "stats")]
    pub(crate) stats: Option<Arc<TransportStats>>,
    pub(crate) primitives: Arc<dyn crate::net::primitives::EgressPrimitives + Send + Sync>,
    pub(crate) local_interests: HashMap<InterestId, (Interest, Option<Arc<Resource>>, bool)>,
    pub(crate) remote_key_interests: HashMap<InterestId, Option<Arc<Resource>>>,
    pub(crate) local_mappings: HashMap<ExprId, Arc<Resource>>,
    pub(crate) remote_mappings: HashMap<ExprId, Arc<Resource>>,
    pub(crate) next_qid: RequestId,
    pub(crate) pending_queries: HashMap<RequestId, Arc<Query>>,
    pub(crate) mcast_group: Option<TransportMulticast>,
    pub(crate) in_interceptors: Option<Arc<InterceptorsChain>>,
    pub(crate) hat: Box<dyn Any + Send + Sync>,
}

impl FaceState {
    #[allow(clippy::too_many_arguments)] // @TODO fix warning
    pub(crate) fn new(
        id: usize,
        zid: ZenohId,
        whatami: WhatAmI,
        #[cfg(feature = "stats")] stats: Option<Arc<TransportStats>>,
        primitives: Arc<dyn crate::net::primitives::EgressPrimitives + Send + Sync>,
        mcast_group: Option<TransportMulticast>,
        in_interceptors: Option<Arc<InterceptorsChain>>,
        hat: Box<dyn Any + Send + Sync>,
    ) -> Arc<FaceState> {
        Arc::new(FaceState {
            id,
            zid,
            whatami,
            #[cfg(feature = "stats")]
            stats,
            primitives,
            local_interests: HashMap::new(),
            remote_key_interests: HashMap::new(),
            local_mappings: HashMap::new(),
            remote_mappings: HashMap::new(),
            next_qid: 0,
            pending_queries: HashMap::new(),
            mcast_group,
            in_interceptors,
            hat,
        })
    }

    #[inline]
    pub(crate) fn get_mapping(
        &self,
        prefixid: &ExprId,
        mapping: Mapping,
    ) -> Option<&std::sync::Arc<Resource>> {
        match mapping {
            Mapping::Sender => self.remote_mappings.get(prefixid),
            Mapping::Receiver => self.local_mappings.get(prefixid),
        }
    }

    #[inline]
    pub(crate) fn get_sent_mapping(
        &self,
        prefixid: &ExprId,
        mapping: Mapping,
    ) -> Option<&std::sync::Arc<Resource>> {
        match mapping {
            Mapping::Sender => self.local_mappings.get(prefixid),
            Mapping::Receiver => self.remote_mappings.get(prefixid),
        }
    }

    pub(crate) fn get_next_local_id(&self) -> ExprId {
        let mut id = 1;
        while self.local_mappings.get(&id).is_some() || self.remote_mappings.get(&id).is_some() {
            id += 1;
        }
        id
    }

    pub(crate) fn update_interceptors_caches(&self, res: &mut Arc<Resource>) {
        if let Ok(expr) = KeyExpr::try_from(res.expr()) {
            if let Some(interceptor) = self.in_interceptors.as_ref() {
                let cache = interceptor.compute_keyexpr_cache(&expr);
                get_mut_unchecked(
                    get_mut_unchecked(res)
                        .session_ctxs
                        .get_mut(&self.id)
                        .unwrap(),
                )
                .in_interceptor_cache = cache;
            }
            if let Some(mux) = self.primitives.as_any().downcast_ref::<Mux>() {
                let cache = mux.interceptor.compute_keyexpr_cache(&expr);
                get_mut_unchecked(
                    get_mut_unchecked(res)
                        .session_ctxs
                        .get_mut(&self.id)
                        .unwrap(),
                )
                .e_interceptor_cache = cache;
            }
            if let Some(mux) = self.primitives.as_any().downcast_ref::<McastMux>() {
                let cache = mux.interceptor.compute_keyexpr_cache(&expr);
                get_mut_unchecked(
                    get_mut_unchecked(res)
                        .session_ctxs
                        .get_mut(&self.id)
                        .unwrap(),
                )
                .e_interceptor_cache = cache;
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

impl IngressPrimitives for Face {
    fn ingress_declare(&self, msg: zenoh_protocol::network::Declare) {
        let ctrl_lock = zlock!(self.tables.ctrl_lock);
        match msg.body {
            zenoh_protocol::network::DeclareBody::DeclareKeyExpr(m) => {
                register_expr(&self.tables, &mut self.state.clone(), m.id, &m.wire_expr);
            }
            zenoh_protocol::network::DeclareBody::UndeclareKeyExpr(m) => {
                unregister_expr(&self.tables, &mut self.state.clone(), m.id);
            }
            zenoh_protocol::network::DeclareBody::DeclareSubscriber(m) => {
                declare_subscription(
                    ctrl_lock.as_ref(),
                    &self.tables,
                    &mut self.state.clone(),
                    m.id,
                    &m.wire_expr,
                    &m.ext_info,
                    msg.ext_nodeid.node_id,
                );
            }
            zenoh_protocol::network::DeclareBody::UndeclareSubscriber(m) => {
                undeclare_subscription(
                    ctrl_lock.as_ref(),
                    &self.tables,
                    &mut self.state.clone(),
                    m.id,
                    &m.ext_wire_expr.wire_expr,
                    msg.ext_nodeid.node_id,
                );
            }
            zenoh_protocol::network::DeclareBody::DeclareQueryable(m) => {
                declare_queryable(
                    ctrl_lock.as_ref(),
                    &self.tables,
                    &mut self.state.clone(),
                    m.id,
                    &m.wire_expr,
                    &m.ext_info,
                    msg.ext_nodeid.node_id,
                );
            }
            zenoh_protocol::network::DeclareBody::UndeclareQueryable(m) => {
                undeclare_queryable(
                    ctrl_lock.as_ref(),
                    &self.tables,
                    &mut self.state.clone(),
                    m.id,
                    &m.ext_wire_expr.wire_expr,
                    msg.ext_nodeid.node_id,
                );
            }
            zenoh_protocol::network::DeclareBody::DeclareToken(m) => {
                declare_token(
                    ctrl_lock.as_ref(),
                    &self.tables,
                    &mut self.state.clone(),
                    m.id,
                    &m.wire_expr,
                    msg.ext_nodeid.node_id,
                );
            }
            zenoh_protocol::network::DeclareBody::UndeclareToken(m) => {
                undeclare_token(
                    ctrl_lock.as_ref(),
                    &self.tables,
                    &mut self.state.clone(),
                    m.id,
                    &m.ext_wire_expr,
                    msg.ext_nodeid.node_id,
                );
            }
            zenoh_protocol::network::DeclareBody::DeclareInterest(m) => {
                if m.interest.keyexprs() && m.interest.future() {
                    register_expr_interest(
                        &self.tables,
                        &mut self.state.clone(),
                        m.id,
                        m.wire_expr.as_ref(),
                    );
                }
                if m.interest.subscribers() {
                    declare_sub_interest(
                        ctrl_lock.as_ref(),
                        &self.tables,
                        &mut self.state.clone(),
                        m.id,
                        m.wire_expr.as_ref(),
                        m.interest.current(),
                        m.interest.future(),
                        m.interest.aggregate(),
                    );
                }
                if m.interest.tokens() {
                    declare_token_interest(
                        ctrl_lock.as_ref(),
                        &self.tables,
                        &mut self.state.clone(),
                        m.id,
                        m.wire_expr.as_ref(),
                        m.interest.current(),
                        m.interest.future(),
                        m.interest.aggregate(),
                    );
                }
                if m.interest.queryables() {
                    declare_qabl_interest(
                        ctrl_lock.as_ref(),
                        &self.tables,
                        &mut self.state.clone(),
                        m.id,
                        m.wire_expr.as_ref(),
                        m.interest.current(),
                        m.interest.future(),
                        m.interest.aggregate(),
                    );
                }
                if m.interest.current() {
                    self.state
                        .primitives
                        .egress_declare(RoutingContext::new_out(
                            Declare {
                                ext_qos: ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType::DEFAULT,
                                body: DeclareBody::FinalInterest(FinalInterest { id: m.id }),
                            },
                            self.clone(),
                        ));
                }
            }
            zenoh_protocol::network::DeclareBody::FinalInterest(m) => {
                get_mut_unchecked(&mut self.state.clone())
                    .local_interests
                    .entry(m.id)
                    .and_modify(|interest| interest.2 = true);
            }
            zenoh_protocol::network::DeclareBody::UndeclareInterest(m) => {
                unregister_expr_interest(&self.tables, &mut self.state.clone(), m.id);
                undeclare_sub_interest(
                    ctrl_lock.as_ref(),
                    &self.tables,
                    &mut self.state.clone(),
                    m.id,
                );
                undeclare_qabl_interest(
                    ctrl_lock.as_ref(),
                    &self.tables,
                    &mut self.state.clone(),
                    m.id,
                );
                undeclare_token_interest(
                    ctrl_lock.as_ref(),
                    &self.tables,
                    &mut self.state.clone(),
                    m.id,
                );
            }
        }
        drop(ctrl_lock);
    }

    #[inline]
    fn ingress_push(&self, msg: Push) {
        full_reentrant_route_data(
            &self.tables,
            &self.state,
            &msg.wire_expr,
            msg.ext_qos,
            msg.payload,
            msg.ext_nodeid.node_id,
        );
    }

    fn ingress_request(&self, msg: Request) {
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
                    msg.ext_nodeid.node_id,
                );
            }
        }
    }

    fn ingress_response(&self, msg: Response) {
        route_send_response(
            &self.tables,
            &mut self.state.clone(),
            msg.rid,
            msg.ext_respid,
            msg.wire_expr,
            msg.payload,
        );
    }

    fn ingress_response_final(&self, msg: ResponseFinal) {
        route_send_response_final(&self.tables, &mut self.state.clone(), msg.rid);
    }

    fn ingress_close(&self) {
        tables::close_face(&self.tables, &Arc::downgrade(&self.state));
    }
}

impl fmt::Display for Face {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.state.fmt(f)
    }
}
