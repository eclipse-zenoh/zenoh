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
use std::{
    any::Any,
    collections::HashMap,
    fmt,
    ops::Not,
    sync::{Arc, Weak},
    time::Duration,
};

use arc_swap::ArcSwap;
use tokio_util::sync::CancellationToken;
use zenoh_collections::IntHashMap;
use zenoh_protocol::{
    core::{ExprId, Reliability, WhatAmI, ZenohIdProto},
    network::{
        interest::{InterestId, InterestMode, InterestOptions},
        Mapping, Push, Request, RequestId, Response, ResponseFinal,
    },
    zenoh::RequestBody,
};
use zenoh_sync::get_mut_unchecked;
use zenoh_task::TaskController;
use zenoh_transport::multicast::TransportMulticast;
#[cfg(feature = "stats")]
use zenoh_transport::stats::TransportStats;

use super::{
    super::router::*,
    interests::{declare_final, declare_interest, undeclare_interest, PendingCurrentInterest},
    resource::*,
    tables::TablesLock,
};
use crate::net::{
    primitives::{McastMux, Mux, Primitives},
    routing::{
        dispatcher::interests::{finalize_pending_interests, RemoteInterest},
        interceptor::{
            EgressInterceptor, IngressInterceptor, InterceptorFactory, InterceptorTrait,
            InterceptorsChain,
        },
    },
};

pub(crate) struct InterestState {
    face: FaceId,
    pub(crate) options: InterestOptions,
    pub(crate) res: Option<Arc<Resource>>,
    pub(crate) finalized: bool,
}

impl InterestState {
    pub(crate) fn new(
        face: FaceId,
        options: InterestOptions,
        res: Option<Arc<Resource>>,
        finalized: bool,
    ) -> Self {
        let mut interest = Self {
            face,
            options,
            res,
            finalized: false,
        };
        if finalized {
            interest.set_finalized();
        }
        interest
    }

    pub(crate) fn set_finalized(&mut self) {
        self.finalized = true;
        if self.options.subscribers() || self.options.queryables() {
            if let Some(res) = self.res.as_mut().map(get_mut_unchecked) {
                if let Some(ctx) = res.session_ctxs.get_mut(&self.face).map(get_mut_unchecked) {
                    if self.options.subscribers() {
                        ctx.subscriber_interest_finalized = true;
                    }
                    if self.options.queryables() {
                        ctx.queryable_interest_finalized = true;
                    }
                }
            }
        }
    }
}

impl PartialEq<RemoteInterest> for InterestState {
    fn eq(&self, other: &RemoteInterest) -> bool {
        self.options == other.options && self.res == other.res
    }
}

pub(crate) type FaceId = usize;

pub struct FaceState {
    pub(crate) id: FaceId,
    pub(crate) zid: ZenohIdProto,
    pub(crate) whatami: WhatAmI,
    #[cfg(feature = "stats")]
    pub(crate) stats: Option<Arc<TransportStats>>,
    pub(crate) primitives: Arc<dyn crate::net::primitives::EPrimitives + Send + Sync>,
    pub(crate) local_interests: HashMap<InterestId, InterestState>,
    pub(crate) remote_key_interests: HashMap<InterestId, Option<Arc<Resource>>>,
    pub(crate) pending_current_interests: HashMap<InterestId, PendingCurrentInterest>,
    pub(crate) local_mappings: IntHashMap<ExprId, Arc<Resource>>,
    pub(crate) remote_mappings: IntHashMap<ExprId, Arc<Resource>>,
    pub(crate) next_qid: RequestId,
    pub(crate) pending_queries: HashMap<RequestId, (Arc<Query>, CancellationToken)>,
    pub(crate) mcast_group: Option<TransportMulticast>,
    pub(crate) in_interceptors: Option<Arc<ArcSwap<InterceptorsChain>>>,
    pub(crate) hat: Box<dyn Any + Send + Sync>,
    pub(crate) task_controller: TaskController,
    pub(crate) is_local: bool,
}

impl FaceState {
    #[allow(clippy::too_many_arguments)] // @TODO fix warning
    pub(crate) fn new(
        id: usize,
        zid: ZenohIdProto,
        whatami: WhatAmI,
        #[cfg(feature = "stats")] stats: Option<Arc<TransportStats>>,
        primitives: Arc<dyn crate::net::primitives::EPrimitives + Send + Sync>,
        mcast_group: Option<TransportMulticast>,
        in_interceptors: Option<Arc<ArcSwap<InterceptorsChain>>>,
        hat: Box<dyn Any + Send + Sync>,
        is_local: bool,
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
            pending_current_interests: HashMap::new(),
            local_mappings: IntHashMap::new(),
            remote_mappings: IntHashMap::new(),
            next_qid: 0,
            pending_queries: HashMap::new(),
            mcast_group,
            in_interceptors,
            hat,
            task_controller: TaskController::default(),
            is_local,
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
        while self.local_mappings.contains_key(&id) || self.remote_mappings.contains_key(&id) {
            id += 1;
        }
        id
    }

    pub(crate) fn update_interceptors_caches(&self, res: &mut Arc<Resource>) {
        if let Some(interceptor) = self
            .in_interceptors
            .as_ref()
            .map(|itor| itor.load())
            .and_then(|is| is.is_empty().not().then_some(is))
        {
            if let Some(expr) = res.keyexpr() {
                let cache = interceptor.compute_keyexpr_cache(expr);
                get_mut_unchecked(
                    get_mut_unchecked(res)
                        .session_ctxs
                        .get_mut(&self.id)
                        .unwrap(),
                )
                .in_interceptor_cache = InterceptorCache::new(cache, interceptor.version);
            }
        }

        if let Some(interceptor) = self
            .primitives
            .as_any()
            .downcast_ref::<Mux>()
            .map(|mux| mux.interceptor.load())
            .and_then(|is| is.is_empty().not().then_some(is))
        {
            if let Some(expr) = res.keyexpr() {
                let cache = interceptor.compute_keyexpr_cache(expr);
                get_mut_unchecked(
                    get_mut_unchecked(res)
                        .session_ctxs
                        .get_mut(&self.id)
                        .unwrap(),
                )
                .e_interceptor_cache = InterceptorCache::new(cache, interceptor.version);
            }
        }

        if let Some(interceptor) = self
            .primitives
            .as_any()
            .downcast_ref::<McastMux>()
            .map(|mux| mux.interceptor.load())
            .and_then(|is| is.is_empty().not().then_some(is))
        {
            if let Some(expr) = res.keyexpr() {
                let cache = interceptor.compute_keyexpr_cache(expr);
                get_mut_unchecked(
                    get_mut_unchecked(res)
                        .session_ctxs
                        .get_mut(&self.id)
                        .unwrap(),
                )
                .e_interceptor_cache = InterceptorCache::new(cache, interceptor.version);
            }
        }
    }

    pub(crate) fn set_interceptors_from_factories(
        &self,
        factories: &[InterceptorFactory],
        version: usize,
    ) {
        if let Some(mux) = self.primitives.as_any().downcast_ref::<Mux>() {
            let (ingress, egress): (Vec<_>, Vec<_>) = factories
                .iter()
                .map(|itor| itor.new_transport_unicast(&mux.handler))
                .unzip();
            let (ingress, egress) = (
                InterceptorsChain::new(ingress.into_iter().flatten().collect::<Vec<_>>(), version),
                InterceptorsChain::new(egress.into_iter().flatten().collect::<Vec<_>>(), version),
            );
            mux.interceptor.store(egress.into());
            self.in_interceptors
                .as_ref()
                .expect("face in_interceptors should not be None when primitives are Mux")
                .store(ingress.into());
        } else if let Some(mux) = self.primitives.as_any().downcast_ref::<McastMux>() {
            let interceptor = InterceptorsChain::new(
                factories
                    .iter()
                    .filter_map(|itor| itor.new_transport_multicast(&mux.handler))
                    .collect::<Vec<EgressInterceptor>>(),
                version,
            );
            mux.interceptor.store(Arc::new(interceptor));
            debug_assert!(self.in_interceptors.is_none());
        } else if let Some(transport) = &self.mcast_group {
            let interceptor = InterceptorsChain::new(
                factories
                    .iter()
                    .filter_map(|itor| itor.new_peer_multicast(transport))
                    .collect::<Vec<IngressInterceptor>>(),
                version,
            );
            self.in_interceptors
                .as_ref()
                .expect("face in_interceptors should not be None when mcast_group is set")
                .store(interceptor.into());
        }
    }
}

impl fmt::Display for FaceState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Face{{{}, {}}}", self.id, self.zid)
    }
}

#[derive(Clone, Debug)]
pub struct WeakFace {
    pub(crate) tables: Weak<TablesLock>,
    pub(crate) state: Weak<FaceState>,
}

impl WeakFace {
    pub fn upgrade(&self) -> Option<Face> {
        Some(Face {
            tables: self.tables.upgrade()?,
            state: self.state.upgrade()?,
        })
    }
}

#[derive(Clone)]
pub struct Face {
    pub(crate) tables: Arc<TablesLock>,
    pub(crate) state: Arc<FaceState>,
}

impl Face {
    pub fn downgrade(&self) -> WeakFace {
        WeakFace {
            tables: Arc::downgrade(&self.tables),
            state: Arc::downgrade(&self.state),
        }
    }

    pub(crate) fn reject_interest(&self, interest_id: u32) {
        if let Some(interest) = self.state.pending_current_interests.get(&interest_id) {
            interest.rejection_token.cancel();
        }
    }
}

impl Primitives for Face {
    fn send_interest(&self, msg: &mut zenoh_protocol::network::Interest) {
        let ctrl_lock = zlock!(self.tables.ctrl_lock);
        if msg.mode != InterestMode::Final {
            let mut declares = vec![];
            declare_interest(
                self.tables.hat_code.as_ref(),
                &self.tables,
                &mut self.state.clone(),
                msg.id,
                msg.wire_expr.as_ref(),
                msg.mode,
                msg.options,
                &mut |p, m| declares.push((p.clone(), m)),
            );
            drop(ctrl_lock);
            for (p, m) in declares {
                m.with_mut(|m| p.send_declare(m));
            }
        } else {
            undeclare_interest(
                self.tables.hat_code.as_ref(),
                &self.tables,
                &mut self.state.clone(),
                msg.id,
            );
        }
    }

    fn send_declare(&self, msg: &mut zenoh_protocol::network::Declare) {
        let ctrl_lock = zlock!(self.tables.ctrl_lock);
        match &mut msg.body {
            zenoh_protocol::network::DeclareBody::DeclareKeyExpr(m) => {
                register_expr(&self.tables, &mut self.state.clone(), m.id, &m.wire_expr);
            }
            zenoh_protocol::network::DeclareBody::UndeclareKeyExpr(m) => {
                unregister_expr(&self.tables, &mut self.state.clone(), m.id);
            }
            zenoh_protocol::network::DeclareBody::DeclareSubscriber(m) => {
                let mut declares = vec![];
                declare_subscription(
                    self.tables.hat_code.as_ref(),
                    &self.tables,
                    &mut self.state.clone(),
                    m.id,
                    &m.wire_expr,
                    &SubscriberInfo,
                    msg.ext_nodeid.node_id,
                    &mut |p, m| declares.push((p.clone(), m)),
                );
                drop(ctrl_lock);
                for (p, m) in declares {
                    m.with_mut(|m| p.send_declare(m));
                }
            }
            zenoh_protocol::network::DeclareBody::UndeclareSubscriber(m) => {
                let mut declares = vec![];
                undeclare_subscription(
                    self.tables.hat_code.as_ref(),
                    &self.tables,
                    &mut self.state.clone(),
                    m.id,
                    &m.ext_wire_expr.wire_expr,
                    msg.ext_nodeid.node_id,
                    &mut |p, m| declares.push((p.clone(), m)),
                );
                drop(ctrl_lock);
                for (p, m) in declares {
                    m.with_mut(|m| p.send_declare(m));
                }
            }
            zenoh_protocol::network::DeclareBody::DeclareQueryable(m) => {
                let mut declares = vec![];
                declare_queryable(
                    self.tables.hat_code.as_ref(),
                    &self.tables,
                    &mut self.state.clone(),
                    m.id,
                    &m.wire_expr,
                    &m.ext_info,
                    msg.ext_nodeid.node_id,
                    &mut |p, m| declares.push((p.clone(), m)),
                );
                drop(ctrl_lock);
                for (p, m) in declares {
                    m.with_mut(|m| p.send_declare(m));
                }
            }
            zenoh_protocol::network::DeclareBody::UndeclareQueryable(m) => {
                let mut declares = vec![];
                undeclare_queryable(
                    self.tables.hat_code.as_ref(),
                    &self.tables,
                    &mut self.state.clone(),
                    m.id,
                    &m.ext_wire_expr.wire_expr,
                    msg.ext_nodeid.node_id,
                    &mut |p, m| declares.push((p.clone(), m)),
                );
                drop(ctrl_lock);
                for (p, m) in declares {
                    m.with_mut(|m| p.send_declare(m));
                }
            }
            zenoh_protocol::network::DeclareBody::DeclareToken(m) => {
                let mut declares = vec![];
                declare_token(
                    self.tables.hat_code.as_ref(),
                    &self.tables,
                    &mut self.state.clone(),
                    m.id,
                    &m.wire_expr,
                    msg.ext_nodeid.node_id,
                    msg.interest_id,
                    &mut |p, m| declares.push((p.clone(), m)),
                );
                drop(ctrl_lock);
                for (p, m) in declares {
                    m.with_mut(|m| p.send_declare(m));
                }
            }
            zenoh_protocol::network::DeclareBody::UndeclareToken(m) => {
                let mut declares = vec![];
                undeclare_token(
                    self.tables.hat_code.as_ref(),
                    &self.tables,
                    &mut self.state.clone(),
                    m.id,
                    &m.ext_wire_expr,
                    msg.ext_nodeid.node_id,
                    &mut |p, m| declares.push((p.clone(), m)),
                );
                drop(ctrl_lock);
                for (p, m) in declares {
                    m.with_mut(|m| p.send_declare(m));
                }
            }
            zenoh_protocol::network::DeclareBody::DeclareFinal(_) => {
                if let Some(id) = msg.interest_id {
                    get_mut_unchecked(&mut self.state.clone())
                        .local_interests
                        .entry(id)
                        .and_modify(|interest| interest.set_finalized());

                    let mut wtables = zwrite!(self.tables.tables);
                    let mut declares = vec![];
                    declare_final(
                        self.tables.hat_code.as_ref(),
                        &mut wtables,
                        &mut self.state.clone(),
                        id,
                        &mut |p, m| declares.push((p.clone(), m)),
                    );

                    wtables.disable_all_routes();

                    drop(wtables);
                    drop(ctrl_lock);
                    for (p, m) in declares {
                        m.with_mut(|m| p.send_declare(m));
                    }
                }
            }
        }
    }

    #[inline]
    fn send_push(&self, msg: &mut Push, reliability: Reliability) {
        route_data(&self.tables, &self.state, msg, reliability);
    }

    fn send_request(&self, msg: &mut Request) {
        match msg.payload {
            RequestBody::Query(_) => {
                route_query(&self.tables, &self.state, msg);
            }
        }
    }

    fn send_response(&self, msg: &mut Response) {
        route_send_response(&self.tables, &mut self.state.clone(), msg);
    }

    fn send_response_final(&self, msg: &mut ResponseFinal) {
        route_send_response_final(&self.tables, &mut self.state.clone(), msg.rid);
    }

    fn send_close(&self) {
        tracing::debug!("{} Close", self.state);
        let mut state = self.state.clone();
        state.task_controller.terminate_all(Duration::from_secs(10));
        finalize_pending_queries(&self.tables, &mut state);
        let mut declares = vec![];
        let ctrl_lock = zlock!(self.tables.ctrl_lock);
        finalize_pending_interests(&self.tables, &mut state, &mut |p, m| {
            declares.push((p.clone(), m))
        });
        self.tables.hat_code.close_face(
            &self.tables,
            &self.tables.clone(),
            &mut state,
            &mut |p, m| declares.push((p.clone(), m)),
        );
        drop(ctrl_lock);
        for (p, m) in declares {
            m.with_mut(|m| p.send_declare(m));
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl fmt::Display for Face {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.state.fmt(f)
    }
}
