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
    fmt::{self, Debug},
    ops::Not,
    sync::{Arc, Weak},
    time::Duration,
};

use arc_swap::ArcSwap;
use itertools::Itertools;
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
#[cfg(feature = "stats")]
use zenoh_transport::stats::TransportStats;
use zenoh_transport::{multicast::TransportMulticast, Bound};

use super::{super::router::*, interests::PendingCurrentInterest, resource::*, tables::TablesLock};
use crate::net::{
    primitives::{EPrimitives, McastMux, Mux, Primitives},
    routing::{
        dispatcher::{
            interests::{finalize_pending_interests, RemoteInterest},
            queries::{
                finalize_pending_queries, route_send_response, route_send_response_final, Query,
            },
            region::{Region, RegionMap},
        },
        hat::BaseContext,
        interceptor::{
            EgressInterceptor, IngressInterceptor, InterceptorFactory, InterceptorTrait,
            InterceptorsChain,
        },
    },
};

#[derive(Debug)]
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
                if let Some(ctx) = res.face_ctxs.get_mut(&self.face).map(get_mut_unchecked) {
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
    pub(crate) region: Region,
    pub(crate) remote_bound: Bound,
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
    /// Downcasts to `HatFace`.
    pub(crate) hats: RegionMap<Box<dyn Any + Send + Sync>>,
    pub(crate) task_controller: TaskController,
    pub(crate) is_local: bool,
}

pub(crate) struct FaceStateBuilder(FaceState);

impl FaceStateBuilder {
    pub(crate) fn new(
        id: usize,
        zid: ZenohIdProto,
        region: Region,
        remote_bound: Bound,
        primitives: Arc<dyn EPrimitives + Send + Sync>,
        hats: RegionMap<Box<dyn Any + Send + Sync>>,
    ) -> Self {
        FaceStateBuilder(FaceState {
            id,
            zid,
            whatami: WhatAmI::default(),
            region,
            remote_bound,
            primitives,
            local_interests: HashMap::new(),
            remote_key_interests: HashMap::new(),
            pending_current_interests: HashMap::new(),
            local_mappings: IntHashMap::new(),
            remote_mappings: IntHashMap::new(),
            next_qid: 0,
            pending_queries: HashMap::new(),
            mcast_group: None,
            in_interceptors: None,
            hats,
            task_controller: TaskController::default(),
            is_local: false,
            #[cfg(feature = "stats")]
            stats: None,
        })
    }

    pub(crate) fn whatami(mut self, whatami: WhatAmI) -> Self {
        self.0.whatami = whatami;
        self
    }

    pub(crate) fn ingress_interceptors(
        mut self,
        in_interceptors: Arc<ArcSwap<InterceptorsChain>>,
    ) -> Self {
        self.0.in_interceptors = Some(in_interceptors);
        self
    }

    pub(crate) fn multicast_groups(mut self, mcast_group: TransportMulticast) -> Self {
        self.0.mcast_group = Some(mcast_group);
        self
    }

    pub(crate) fn local(mut self, is_local: bool) -> Self {
        self.0.is_local = is_local;
        self
    }

    #[cfg(feature = "stats")]
    pub(crate) fn stats(mut self, stats: Arc<TransportStats>) -> Self {
        self.0.stats = Some(stats);
        self
    }

    pub(crate) fn build(self) -> FaceState {
        self.0
    }
}

impl FaceState {
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
                get_mut_unchecked(get_mut_unchecked(res).face_ctxs.get_mut(&self.id).unwrap())
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
                get_mut_unchecked(get_mut_unchecked(res).face_ctxs.get_mut(&self.id).unwrap())
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
                get_mut_unchecked(get_mut_unchecked(res).face_ctxs.get_mut(&self.id).unwrap())
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.id, self.zid.short(), self.region)
    }
}

impl fmt::Debug for FaceState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FaceState")
            .field("id", &self.id)
            .field("zid", &self.zid)
            .field("bound", &self.region)
            .finish()
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

impl fmt::Debug for Face {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.state, f)
    }
}

impl fmt::Display for Face {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.state, f)
    }
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

        // FIXME(regions): reject sourced interests
    }
}

impl Primitives for Face {
    #[tracing::instrument(level = "trace", skip_all, fields(zid = %self.tables.zid.short(), src = %self, mode = ?msg.mode))]
    fn send_interest(&self, msg: &mut zenoh_protocol::network::Interest) {
        let ctrl_lock = zlock!(self.tables.ctrl_lock);
        if msg.mode != InterestMode::Final {
            let mut declares = vec![];
            self.declare_interest(&self.tables, msg, &mut |p, m| declares.push((p.clone(), m)));
            drop(ctrl_lock);
            for (p, m) in declares {
                m.with_mut(|m| p.send_declare(m));
            }
        } else {
            self.undeclare_interest(&self.tables, msg);
        }
    }

    #[tracing::instrument(level = "trace", skip_all, fields(zid = %self.tables.zid.short(), src = %self))]
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
                self.declare_subscription(
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
                self.undeclare_subscription(
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
                self.declare_queryable(
                    &self.tables,
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
                self.undeclare_queryable(
                    &self.tables,
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
                self.declare_token(
                    &self.tables,
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
                self.undeclare_token(
                    &self.tables,
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
                    let mut wtables = zwrite!(self.tables.tables);
                    let mut declares = vec![];
                    self.declare_final(&mut wtables, id, msg.ext_nodeid.node_id, &mut |p, m| {
                        declares.push((p.clone(), m))
                    });

                    wtables.data.disable_all_routes();

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
    #[tracing::instrument(level = "trace", skip_all, fields(zid = %self.tables.zid.short(), src = %self))]
    fn send_push(&self, msg: &mut Push, reliability: Reliability) {
        route_data(&self.tables, &self.state, msg, reliability);
    }

    #[tracing::instrument(level = "trace", skip_all, fields(zid = %self.tables.zid.short(), src = %self))]
    fn send_request(&self, msg: &mut Request) {
        match msg.payload {
            RequestBody::Query(_) => {
                self.route_query(msg);
            }
        }
    }

    #[tracing::instrument(level = "trace", skip_all, fields(zid = %self.tables.zid.short(), src = %self))]
    fn send_response(&self, msg: &mut Response) {
        route_send_response(&self.tables, &mut self.state.clone(), msg);
    }

    #[tracing::instrument(level = "trace", skip_all, fields(zid = %self.tables.zid.short(), src = %self))]
    fn send_response_final(&self, msg: &mut ResponseFinal) {
        route_send_response_final(&self.tables, &mut self.state.clone(), msg.rid);
    }

    #[tracing::instrument(level = "trace", skip_all, fields(zid = %self.tables.zid.short(), src = %self))]
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
        let mut wtables = zwrite!(self.tables.tables);
        let tables = &mut *wtables;

        let mut ctx = BaseContext {
            tables_lock: &self.tables,
            tables: &mut tables.data,
            src_face: &mut state,
            send_declare: &mut |p, m| declares.push((p.clone(), m)),
        };

        let hats = &mut tables.hats;
        let region = self.state.region;

        let face_subs = hats[region].unregister_face_subscriptions(ctx.reborrow());

        for mut res in face_subs {
            let mut remaining = hats
                .values_mut()
                .filter(|hat| !hat.remote_subscriptions_for(&res).is_empty())
                .collect_vec();
            let remaining = remaining.as_mut_slice();

            if let [] = remaining {
                for hat in hats.values_mut() {
                    hat.unpropagate_subscription(ctx.reborrow(), res.clone());
                }
                Resource::clean(&mut res);
            } else if let [last_owner] = remaining {
                last_owner.unpropagate_last_non_owned_subscription(ctx.reborrow(), res.clone())
            }

            disable_matches_data_routes(&mut res, &self.state.region);
        }

        hats[region].close_face(ctx, &self.tables.clone());

        drop(wtables);
        drop(ctrl_lock);
        for (p, m) in declares {
            m.with_mut(|m| p.send_declare(m));
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
