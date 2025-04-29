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
    ops::{Deref, Not},
    sync::{Arc, RwLockReadGuard, Weak},
    time::Duration,
};

use ahash::HashMapExt;
use arc_swap::ArcSwap;
use tokio_util::sync::CancellationToken;
use zenoh_config::InterceptorFlow;
use zenoh_protocol::{
    core::{ExprId, Reliability, WhatAmI, WireExpr, ZenohIdProto},
    network::{
        declare,
        interest::{InterestId, InterestMode, InterestOptions},
        response, Declare, DeclareBody, DeclareFinal, Interest, Mapping, NetworkBodyMut,
        NetworkMessageMut, Push, Request, RequestId, Response, ResponseFinal,
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
    tables::{Tables, TablesLock},
};
use crate::net::{
    primitives::{EPrimitives, McastMux, Mux, Primitives},
    routing::{
        dispatcher::interests::finalize_pending_interests,
        interceptor::{
            EgressInterceptor, IngressInterceptor, InterceptorFactory, InterceptorTrait,
            InterceptorsChain,
        },
        RoutingContext,
    },
};

pub(crate) struct InterestState {
    pub(crate) options: InterestOptions,
    pub(crate) res: Option<Arc<Resource>>,
    pub(crate) finalized: bool,
}

pub struct FaceState {
    pub(crate) id: usize,
    pub(crate) zid: ZenohIdProto,
    pub(crate) whatami: WhatAmI,
    #[cfg(feature = "stats")]
    pub(crate) stats: Option<Arc<TransportStats>>,
    pub(crate) primitives: Arc<dyn crate::net::primitives::EPrimitives + Send + Sync>,
    pub(crate) local_interests: HashMap<InterestId, InterestState>,
    pub(crate) remote_key_interests: HashMap<InterestId, Option<Arc<Resource>>>,
    pub(crate) pending_current_interests: HashMap<InterestId, PendingCurrentInterest>,
    pub(crate) local_mappings: ahash::HashMap<ExprId, Arc<Resource>>,
    pub(crate) remote_mappings: ahash::HashMap<ExprId, Arc<Resource>>,
    pub(crate) next_qid: RequestId,
    pub(crate) pending_queries: HashMap<RequestId, (Arc<Query>, CancellationToken)>,
    pub(crate) mcast_group: Option<TransportMulticast>,
    pub(crate) in_interceptors: Option<ArcSwap<InterceptorsChain>>,
    pub(crate) eg_interceptors: Option<ArcSwap<InterceptorsChain>>,
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
        in_interceptors: Option<ArcSwap<InterceptorsChain>>,
        eg_interceptors: Option<ArcSwap<InterceptorsChain>>,
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
            local_mappings: ahash::HashMap::new(),
            remote_mappings: ahash::HashMap::new(),
            next_qid: 0,
            pending_queries: HashMap::new(),
            mcast_group,
            in_interceptors,
            eg_interceptors,
            hat,
            task_controller: TaskController::default(),
            is_local,
        })
    }

    #[inline]
    pub(crate) fn get_mapping(
        &self,
        prefixid: ExprId,
        mapping: Mapping,
    ) -> Option<&std::sync::Arc<Resource>> {
        match mapping {
            Mapping::Sender => self.remote_mappings.get(&prefixid),
            Mapping::Receiver => self.local_mappings.get(&prefixid),
        }
    }

    #[inline]
    pub(crate) fn get_sent_mapping(
        &self,
        prefixid: ExprId,
        mapping: Mapping,
    ) -> Option<&std::sync::Arc<Resource>> {
        match mapping {
            Mapping::Sender => self.local_mappings.get(&prefixid),
            Mapping::Receiver => self.remote_mappings.get(&prefixid),
        }
    }

    pub(crate) fn get_next_local_id(&self) -> ExprId {
        let mut id = 1;
        while self.local_mappings.contains_key(&id) || self.remote_mappings.contains_key(&id) {
            id += 1;
        }
        id
    }

    pub(crate) fn load_interceptors(
        &self,
        flow: InterceptorFlow,
    ) -> Option<arc_swap::Guard<Arc<InterceptorsChain>>> {
        match flow {
            InterceptorFlow::Egress => &self.eg_interceptors,
            InterceptorFlow::Ingress => &self.in_interceptors,
        }
        .as_ref()
        .map(|iceptors| iceptors.load())
        .and_then(|iceptors| iceptors.is_empty().not().then_some(iceptors))
    }

    pub(crate) fn exec_interceptors(
        &self,
        flow: InterceptorFlow,
        iceptor: &InterceptorsChain,
        ctx: &mut RoutingContext<NetworkMessageMut<'_>>,
    ) -> bool {
        // NOTE: the cache should be empty if the wire expr has a suffix, i.e. when the prefix
        // doesn't represent a full keyexpr.
        let prefix = ctx.wire_expr().and_then(|we| {
            if we.has_suffix() {
                None
            } else {
                ctx.prefix.get()
            }
        });
        let cache_guard = prefix
            .as_ref()
            .and_then(|p| p.interceptor_cache(self, iceptor, flow));
        let cache = cache_guard.as_ref().and_then(|c| c.get_ref().as_ref());
        iceptor.intercept(ctx, cache)
    }

    pub(crate) fn update_interceptors_caches(&self, res: &mut Arc<Resource>) {
        if let Some(iceptor) = self.load_interceptors(InterceptorFlow::Ingress) {
            if let Some(expr) = res.keyexpr() {
                let cache = iceptor.compute_keyexpr_cache(expr);
                get_mut_unchecked(
                    get_mut_unchecked(res)
                        .session_ctxs
                        .get_mut(&self.id)
                        .unwrap(),
                )
                .in_interceptor_cache = InterceptorCache::new(cache, iceptor.version);
            }
        }

        if let Some(iceptor) = self.load_interceptors(InterceptorFlow::Egress) {
            if let Some(expr) = res.keyexpr() {
                let cache = iceptor.compute_keyexpr_cache(expr);
                get_mut_unchecked(
                    get_mut_unchecked(res)
                        .session_ctxs
                        .get_mut(&self.id)
                        .unwrap(),
                )
                .e_interceptor_cache = InterceptorCache::new(cache, iceptor.version);
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
            self.in_interceptors
                .as_ref()
                .expect("face in_interceptors should not be None when primitives are Mux")
                .store(Arc::new(ingress));
            self.eg_interceptors
                .as_ref()
                .expect("face eg_interceptors should not be None when primitives are Mux")
                .store(Arc::new(egress));
        } else if let Some(mux) = self.primitives.as_any().downcast_ref::<McastMux>() {
            let interceptor = InterceptorsChain::new(
                factories
                    .iter()
                    .filter_map(|itor| itor.new_transport_multicast(&mux.handler))
                    .collect::<Vec<EgressInterceptor>>(),
                version,
            );
            self.eg_interceptors
                .as_ref()
                .expect("face eg_interceptors should not be None when primitives are DeMux")
                .store(Arc::new(interceptor));
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

    pub(crate) fn reject_interest(&self, interest_id: u32) {
        if let Some(interest) = self.pending_current_interests.get(&interest_id) {
            interest.rejection_token.cancel();
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

    /// Returns a reference to the **ingress** primitives of this [`Face`].
    ///
    /// Use the returned [`Primitives`] to send messages that loop back
    /// into this same side of the [`Face`].
    pub(crate) fn ingress_primitives(&self) -> &dyn Primitives {
        self
    }

    /// Returns a reference to the **egress** primitives of this [`Face`].
    ///
    /// Use the returned [`EPrimitives`] to send messages outward from
    /// this side to the other side of the [`Face`].
    pub(crate) fn egress_primitives(&self) -> &dyn EPrimitives {
        &*self.state.primitives
    }
}

impl Primitives for Face {
    fn send_interest(&self, msg: &mut zenoh_protocol::network::Interest) {
        if let Some(iceptor) = self.state.load_interceptors(InterceptorFlow::Ingress) {
            let Ok(prefix) = msg
                .wire_expr
                .as_ref()
                .map(|we| self.ingress_prefix(we))
                .transpose()
            else {
                return;
            };

            let interest_id = msg.id;
            let msg = NetworkMessageMut {
                body: NetworkBodyMut::Interest(msg),
                reliability: Reliability::Reliable, // Interest is always reliable
            };
            let ctx = &mut match &prefix {
                Some(prefix) => RoutingContext::with_prefix(msg, prefix.prefix.clone()),
                None => RoutingContext::new(msg),
            };

            if !self
                .state
                .exec_interceptors(InterceptorFlow::Ingress, &iceptor, ctx)
            {
                // NOTE: this request was blocked by an ingress interceptor, we need to send
                // DeclareFinal to avoid a timeout error.
                self.egress_primitives()
                    .send_declare(RoutingContext::with_expr(
                        &mut Declare {
                            interest_id: Some(interest_id),
                            ext_qos: declare::ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                            body: DeclareBody::DeclareFinal(DeclareFinal),
                        },
                        prefix.map(|res| res.expr().to_string()).unwrap_or_default(),
                    ));
                return;
            }
        }

        let ctrl_lock = zlock!(self.tables.ctrl_lock);
        if msg.mode != InterestMode::Final {
            let mut declares = vec![];
            declare_interest(
                ctrl_lock.as_ref(),
                &self.tables,
                self,
                msg.id,
                msg.wire_expr.as_ref(),
                msg.mode,
                msg.options,
                &mut |p, m, r| declares.push((p.clone(), m, r)),
            );
            drop(ctrl_lock);
            for (p, mut m, r) in declares {
                p.intercept_declare(&mut m, r.as_ref());
            }
        } else {
            undeclare_interest(
                ctrl_lock.as_ref(),
                &self.tables,
                &mut self.state.clone(),
                msg.id,
            );
        }
    }

    fn send_declare(&self, msg: &mut zenoh_protocol::network::Declare) {
        if let Some(iceptor) = self.state.load_interceptors(InterceptorFlow::Ingress) {
            let Ok(prefix) = msg
                .wire_expr()
                .map(|we| self.ingress_prefix(we))
                .transpose()
            else {
                return;
            };

            let msg = NetworkMessageMut {
                body: NetworkBodyMut::Declare(msg),
                reliability: Reliability::Reliable, // Declare is always reliable
            };

            let ctx = &mut match prefix {
                Some(prefix) => RoutingContext::with_prefix(msg, prefix.clone()),
                None => RoutingContext::new(msg),
            };

            if !self
                .state
                .exec_interceptors(InterceptorFlow::Ingress, &iceptor, ctx)
            {
                return;
            }
        }

        let ctrl_lock = zlock!(self.tables.ctrl_lock);
        match &mut msg.body {
            zenoh_protocol::network::DeclareBody::DeclareKeyExpr(m) => {
                register_expr(&self.tables, self, m.id, &m.wire_expr);
            }
            zenoh_protocol::network::DeclareBody::UndeclareKeyExpr(m) => {
                unregister_expr(&self.tables, &mut self.state.clone(), m.id);
            }
            zenoh_protocol::network::DeclareBody::DeclareSubscriber(m) => {
                let mut declares = vec![];
                declare_subscription(
                    ctrl_lock.as_ref(),
                    &self.tables,
                    self,
                    m.id,
                    &m.wire_expr,
                    &SubscriberInfo,
                    msg.ext_nodeid.node_id,
                    &mut |p, m, r| declares.push((p.clone(), m, r)),
                );
                drop(ctrl_lock);
                for (p, mut m, r) in declares {
                    p.intercept_declare(&mut m, r.as_ref());
                }
            }
            zenoh_protocol::network::DeclareBody::UndeclareSubscriber(m) => {
                let mut declares = vec![];
                undeclare_subscription(
                    ctrl_lock.as_ref(),
                    &self.tables,
                    &mut self.state.clone(),
                    m.id,
                    &m.ext_wire_expr.wire_expr,
                    msg.ext_nodeid.node_id,
                    &mut |p, m, r| declares.push((p.clone(), m, r)),
                );
                drop(ctrl_lock);
                for (p, mut m, r) in declares {
                    p.intercept_declare(&mut m, r.as_ref())
                }
            }
            zenoh_protocol::network::DeclareBody::DeclareQueryable(m) => {
                let mut declares = vec![];
                declare_queryable(
                    ctrl_lock.as_ref(),
                    &self.tables,
                    self,
                    m.id,
                    &m.wire_expr,
                    &m.ext_info,
                    msg.ext_nodeid.node_id,
                    &mut |p, m, r| declares.push((p.clone(), m, r)),
                );
                drop(ctrl_lock);
                for (p, mut m, r) in declares {
                    p.intercept_declare(&mut m, r.as_ref())
                }
            }
            zenoh_protocol::network::DeclareBody::UndeclareQueryable(m) => {
                let mut declares = vec![];
                undeclare_queryable(
                    ctrl_lock.as_ref(),
                    &self.tables,
                    &mut self.state.clone(),
                    m.id,
                    &m.ext_wire_expr.wire_expr,
                    msg.ext_nodeid.node_id,
                    &mut |p, m, r| declares.push((p.clone(), m, r)),
                );
                drop(ctrl_lock);
                for (p, mut m, r) in declares {
                    p.intercept_declare(&mut m, r.as_ref())
                }
            }
            zenoh_protocol::network::DeclareBody::DeclareToken(m) => {
                let mut declares = vec![];
                declare_token(
                    ctrl_lock.as_ref(),
                    &self.tables,
                    self,
                    m.id,
                    &m.wire_expr,
                    msg.ext_nodeid.node_id,
                    msg.interest_id,
                    &mut |p, m, r| declares.push((p.clone(), m, r)),
                );
                drop(ctrl_lock);
                for (p, mut m, r) in declares {
                    p.intercept_declare(&mut m, r.as_ref())
                }
            }
            zenoh_protocol::network::DeclareBody::UndeclareToken(m) => {
                let mut declares = vec![];
                undeclare_token(
                    ctrl_lock.as_ref(),
                    &self.tables,
                    &mut self.state.clone(),
                    m.id,
                    &m.ext_wire_expr,
                    msg.ext_nodeid.node_id,
                    &mut |p, m, r| declares.push((p.clone(), m, r)),
                );
                drop(ctrl_lock);
                for (p, mut m, r) in declares {
                    p.intercept_declare(&mut m, r.as_ref())
                }
            }
            zenoh_protocol::network::DeclareBody::DeclareFinal(_) => {
                if let Some(id) = msg.interest_id {
                    get_mut_unchecked(&mut self.state.clone())
                        .local_interests
                        .entry(id)
                        .and_modify(|interest| interest.finalized = true);

                    let mut wtables = zwrite!(self.tables.tables);
                    let mut declares = vec![];
                    declare_final(
                        ctrl_lock.as_ref(),
                        &mut wtables,
                        &mut self.state.clone(),
                        id,
                        &mut |p, m, r| declares.push((p.clone(), m, r)),
                    );

                    wtables.disable_all_routes();

                    drop(wtables);
                    drop(ctrl_lock);
                    for (p, mut m, r) in declares {
                        p.intercept_declare(&mut m, r.as_ref())
                    }
                }
            }
        }
    }

    #[inline]
    fn send_push(&self, msg: &mut Push, reliability: Reliability) {
        if let Some(iceptor) = self.state.load_interceptors(InterceptorFlow::Ingress) {
            let Ok(prefix) = self.ingress_prefix(&msg.wire_expr) else {
                return;
            };

            let ctx = &mut RoutingContext::with_prefix(
                NetworkMessageMut {
                    body: NetworkBodyMut::Push(msg),
                    reliability,
                },
                prefix.prefix,
            );

            if !self
                .state
                .exec_interceptors(InterceptorFlow::Ingress, &iceptor, ctx)
            {
                return;
            }
        }

        route_data(&self.tables, &self.state, msg, reliability);
    }

    fn send_request(&self, msg: &mut Request) {
        if let Some(iceptor) = self.state.load_interceptors(InterceptorFlow::Ingress) {
            let Ok(prefix) = self.ingress_prefix(&msg.wire_expr) else {
                return;
            };

            let ctx = &mut RoutingContext::with_prefix(
                NetworkMessageMut {
                    body: NetworkBodyMut::Request(msg),
                    reliability: Reliability::Reliable, // NOTE: queries are always reliable
                },
                prefix.prefix,
            );

            if !self
                .state
                .exec_interceptors(InterceptorFlow::Ingress, &iceptor, ctx)
            {
                // NOTE: this request was blocked by an ingress interceptor, we need to send
                // ResponseFinal to avoid a timeout error.
                self.egress_primitives()
                    .send_response_final(&mut ResponseFinal {
                        rid: msg.id,
                        ext_qos: response::ext::QoSType::RESPONSE_FINAL,
                        ext_tstamp: None,
                    });
                return;
            }
        }

        match msg.payload {
            RequestBody::Query(_) => {
                route_query(&self.tables, self, msg);
            }
        }
    }

    fn send_response(&self, msg: &mut Response) {
        if let Some(iceptor) = self.state.load_interceptors(InterceptorFlow::Ingress) {
            let Ok(prefix) = self.ingress_prefix(&msg.wire_expr) else {
                return;
            };

            let ctx = &mut RoutingContext::with_prefix(
                NetworkMessageMut {
                    body: NetworkBodyMut::Response(msg),
                    reliability: Reliability::Reliable, // NOTE: queries are always reliable
                },
                prefix.prefix,
            );

            if !self
                .state
                .exec_interceptors(InterceptorFlow::Ingress, &iceptor, ctx)
            {
                return;
            }
        }

        route_send_response(&self.tables, &mut self.state.clone(), msg);
    }

    fn send_response_final(&self, msg: &mut ResponseFinal) {
        if let Some(iceptor) = self.state.load_interceptors(InterceptorFlow::Ingress) {
            let ctx = &mut RoutingContext::new(NetworkMessageMut {
                body: NetworkBodyMut::ResponseFinal(msg),
                reliability: Reliability::Reliable, // NOTE: queries are always reliable
            });

            // NOTE: ResponseFinal messages have no keyexpr
            if !self
                .state
                .exec_interceptors(InterceptorFlow::Ingress, &iceptor, ctx)
            {
                return;
            }
        }

        route_send_response_final(&self.tables, &mut self.state.clone(), msg.rid);
    }

    fn send_close(&self) {
        tracing::debug!("{} Close", self.state);
        let mut state = self.state.clone();
        state.task_controller.terminate_all(Duration::from_secs(10));
        finalize_pending_queries(&self.tables, &mut state);
        let mut declares = vec![];
        let ctrl_lock = zlock!(self.tables.ctrl_lock);
        finalize_pending_interests(&self.tables, &mut state, &mut |p, m, r| {
            declares.push((p.clone(), m, r))
        });
        ctrl_lock.close_face(
            &self.tables,
            &self.tables.clone(),
            &mut state,
            &mut |p, m, r| declares.push((p.clone(), m, r)),
        );
        drop(ctrl_lock);
        for (p, mut m, r) in declares {
            p.intercept_declare(&mut m, r.as_ref())
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

/// [`Resource`] prefix.
///
/// # Safety
///
/// The [`Tables`] lock is also stored to ensure that the [`Resource`] is not accessed without it.
/// Otherwise we could run into undefined behavior when using [`get_mut_unchecked`].
pub(crate) struct Prefix<'face> {
    _tables: RwLockReadGuard<'face, Tables>,
    pub prefix: Arc<Resource>,
}

impl Deref for Prefix<'_> {
    type Target = Arc<Resource>;

    fn deref(&self) -> &Self::Target {
        &self.prefix
    }
}

impl Face {
    /// Returns the [`Prefix`] associated with the given [`WireExpr`] for transmitted messages.
    pub(crate) fn egress_prefix(&self, wire_expr: &WireExpr) -> Result<Prefix<'_>, ()> {
        let tables = self
            .tables
            .tables
            .read()
            .expect("reading Tables should not fail");

        match tables
            .get_sent_mapping(&self.state, wire_expr.scope, wire_expr.mapping)
            .cloned()
        {
            Some(prefix) => Ok(Prefix {
                _tables: tables,
                prefix,
            }),
            None => {
                tracing::error!(
                    "Got WireExpr with unknown scope {} from {}",
                    wire_expr.scope,
                    self
                );
                Err(())
            }
        }
    }

    /// Returns the [`Prefix`] associated with the given [`WireExpr`] for received messages.
    pub(crate) fn ingress_prefix(&self, wire_expr: &WireExpr) -> Result<Prefix<'_>, ()> {
        let tables = self
            .tables
            .tables
            .read()
            .expect("reading Tables should not fail");

        match tables
            .get_mapping(&self.state, wire_expr.scope, wire_expr.mapping)
            .cloned()
        {
            Some(prefix) => Ok(Prefix {
                _tables: tables,
                prefix,
            }),
            None => {
                tracing::error!(
                    "Got WireExpr with unknown scope {} from {}",
                    wire_expr.scope,
                    self
                );
                Err(())
            }
        }
    }

    pub(crate) fn intercept_interest(&self, msg: &mut Interest, prefix: Option<&Arc<Resource>>) {
        if let Some(iceptor) = self.state.load_interceptors(InterceptorFlow::Egress) {
            let interest_id = msg.id;
            let msg = NetworkMessageMut {
                body: NetworkBodyMut::Interest(msg),
                reliability: Reliability::Reliable, // NOTE: Interest is always reliable
            };

            let ctx = &mut match prefix {
                Some(prefix) => RoutingContext::with_prefix(msg, prefix.clone()),
                None => RoutingContext::new(msg),
            };

            if !self
                .state
                .exec_interceptors(InterceptorFlow::Egress, &iceptor, ctx)
            {
                // NOTE: this request was blocked by an egress interceptor, we need to send
                // DeclareFinal to avoid a timeout error.
                self.state.reject_interest(interest_id);
                return;
            }
        }

        self.egress_primitives()
            .send_interest(RoutingContext::with_expr(
                msg,
                prefix.map(|res| res.expr().to_string()).unwrap_or_default(),
            ));
    }

    pub(crate) fn intercept_declare(&self, msg: &mut Declare, prefix: Option<&Arc<Resource>>) {
        if let Some(iceptor) = self.state.load_interceptors(InterceptorFlow::Egress) {
            let msg = NetworkMessageMut {
                body: NetworkBodyMut::Declare(msg),
                reliability: Reliability::Reliable, // NOTE: Declare is always reliable
            };

            let ctx = &mut match prefix {
                Some(prefix) => RoutingContext::with_prefix(msg, prefix.clone()),
                None => RoutingContext::new(msg),
            };

            if !self
                .state
                .exec_interceptors(InterceptorFlow::Egress, &iceptor, ctx)
            {
                return;
            }
        }

        self.egress_primitives()
            .send_declare(RoutingContext::with_expr(
                msg,
                prefix.map(|res| res.expr().to_string()).unwrap_or_default(),
            ));
    }

    pub(crate) fn intercept_push(&self, msg: &mut Push, reliability: Reliability) {
        if let Some(iceptor) = self.state.load_interceptors(InterceptorFlow::Egress) {
            let Ok(prefix) = self.egress_prefix(&msg.wire_expr) else {
                return;
            };

            let ctx = &mut RoutingContext::with_prefix(
                NetworkMessageMut {
                    body: NetworkBodyMut::Push(msg),
                    reliability,
                },
                prefix.prefix,
            );

            if !self
                .state
                .exec_interceptors(InterceptorFlow::Egress, &iceptor, ctx)
            {
                return;
            }
        }

        self.egress_primitives().send_push(msg, reliability);
    }

    pub(crate) fn intercept_request(&self, msg: &mut Request) {
        if let Some(iceptor) = self.state.load_interceptors(InterceptorFlow::Egress) {
            let Ok(prefix) = self.egress_prefix(&msg.wire_expr) else {
                return;
            };

            let ctx = &mut RoutingContext::with_prefix(
                NetworkMessageMut {
                    body: NetworkBodyMut::Request(msg),
                    reliability: Reliability::Reliable, // NOTE: Request is always reliable
                },
                prefix.prefix,
            );

            if !self
                .state
                .exec_interceptors(InterceptorFlow::Egress, &iceptor, ctx)
            {
                // NOTE: this request was blocked by an egress interceptor, we need to send
                // ResponseFinal to avoid timeout error.
                self.ingress_primitives()
                    .send_response_final(&mut ResponseFinal {
                        rid: msg.id,
                        ext_qos: response::ext::QoSType::RESPONSE_FINAL,
                        ext_tstamp: None,
                    });
                return;
            }
        }

        self.egress_primitives().send_request(msg);
    }

    pub(crate) fn intercept_response(&self, msg: &mut Response) {
        if let Some(iceptor) = self.state.load_interceptors(InterceptorFlow::Egress) {
            let Ok(prefix) = self.egress_prefix(&msg.wire_expr) else {
                return;
            };

            let ctx = &mut RoutingContext::with_prefix(
                NetworkMessageMut {
                    body: NetworkBodyMut::Response(msg),
                    reliability: Reliability::Reliable, // NOTE: Response is always reliable
                },
                prefix.prefix,
            );

            if !self
                .state
                .exec_interceptors(InterceptorFlow::Egress, &iceptor, ctx)
            {
                return;
            }
        }

        self.egress_primitives().send_response(msg);
    }

    pub(crate) fn intercept_response_final(&self, msg: &mut ResponseFinal) {
        if let Some(iceptor) = self.state.load_interceptors(InterceptorFlow::Egress) {
            let ctx = &mut RoutingContext::new(NetworkMessageMut {
                body: NetworkBodyMut::ResponseFinal(msg),
                reliability: Reliability::Reliable, // NOTE: ResponseFinal is always reliable
            });

            // NOTE: ResponseFinal messages have no keyexpr
            if !self
                .state
                .exec_interceptors(InterceptorFlow::Egress, &iceptor, ctx)
            {
                return;
            }
        }

        self.egress_primitives().send_response_final(msg);
    }
}
