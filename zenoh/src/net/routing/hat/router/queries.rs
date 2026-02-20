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
    borrow::Cow,
    collections::{HashMap, HashSet},
    sync::Arc,
};

use itertools::Itertools;
use petgraph::graph::NodeIndex;
#[allow(unused_imports)]
use zenoh_core::polyfill::*;
use zenoh_protocol::{
    core::{
        key_expr::include::{Includer, DEFAULT_INCLUDER},
        WhatAmI, ZenohIdProto,
    },
    network::declare::{
        self, common::ext::WireExprType, queryable::ext::QueryableInfoType, Declare, DeclareBody,
        DeclareQueryable, QueryableId, UndeclareQueryable,
    },
};

use super::Hat;
use crate::net::{
    protocol::network::Network,
    routing::{
        dispatcher::{
            face::FaceState,
            queries::merge_qabl_infos,
            resource::{NodeId, Resource},
            tables::{QueryTargetQabl, QueryTargetQablSet, RoutingExpr, TablesData},
        },
        hat::{DispatcherContext, HatBaseTrait, HatQueriesTrait, Sources, UnregisterResult},
        router::Direction,
        RoutingContext,
    },
};

impl Hat {
    pub(super) fn queries_tree_change(
        &self,
        tables: &mut TablesData,
        new_children: &[Vec<NodeIndex>],
    ) {
        let net = &self.routers_net.as_ref().unwrap();

        // propagate qabls to new children
        for (tree_sid, tree_children) in new_children.iter().enumerate() {
            if !tree_children.is_empty() {
                let tree_idx = NodeIndex::new(tree_sid);
                if net.graph.contains_node(tree_idx) {
                    let tree_id = net.graph[tree_idx].zid;

                    let qabls_res = &self.router_qabls;

                    for res in qabls_res {
                        let qabls = &self.res_hat(res).router_qabls;
                        if let Some(qabl_info) = qabls.get(&tree_id) {
                            self.send_sourced_queryable_to_net_children(
                                tables,
                                net,
                                tree_children,
                                res,
                                qabl_info,
                                None,
                                tree_sid as NodeId,
                            );
                        }
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn send_sourced_queryable_to_net_children(
        &self,
        tables: &TablesData,
        net: &Network,
        children: &[NodeIndex],
        res: &Arc<Resource>,
        qabl_info: &QueryableInfoType,
        src_face: Option<&mut Arc<FaceState>>,
        routing_context: NodeId,
    ) {
        for child in children {
            if net.graph.contains_node(*child) {
                match self.face(tables, &net.graph[*child].zid).cloned() {
                    Some(mut someface) => {
                        if src_face
                            .as_ref()
                            .map(|src_face| someface.id != src_face.id)
                            .unwrap_or(true)
                        {
                            let key_expr = Resource::decl_key(res, &mut someface);

                            someface.primitives.send_declare(RoutingContext::with_expr(
                                &mut Declare {
                                    interest_id: None,
                                    ext_qos: declare::ext::QoSType::DECLARE,
                                    ext_tstamp: None,
                                    ext_nodeid: declare::ext::NodeIdType {
                                        node_id: routing_context,
                                    },
                                    body: DeclareBody::DeclareQueryable(DeclareQueryable {
                                        id: QueryableId::default(), // Sourced queryables do not use ids
                                        wire_expr: key_expr,
                                        ext_info: *qabl_info,
                                    }),
                                },
                                res.expr().to_string(),
                            ));
                        }
                    }
                    None => {
                        tracing::trace!("Unable to find face for zid {}", net.graph[*child].zid)
                    }
                }
            }
        }
    }

    // FIXME(regions): use a different word for "propagate"
    fn propagate_sourced_queryable(
        &self,
        tables: &TablesData,
        res: &Arc<Resource>,
        qabl_info: &QueryableInfoType,
        src_face: Option<&mut Arc<FaceState>>,
        source: &ZenohIdProto,
    ) {
        let net = self.routers_net.as_ref().unwrap();
        match net.get_idx(source) {
            Some(tree_sid) => {
                if net.trees.len() > tree_sid.index() {
                    self.send_sourced_queryable_to_net_children(
                        tables,
                        net,
                        &net.trees[tree_sid.index()].children,
                        res,
                        qabl_info,
                        src_face,
                        tree_sid.index() as NodeId,
                    );
                } else {
                    tracing::trace!(
                        "Propagating qabl {}: tree for node {} sid:{} not yet ready",
                        res.expr(),
                        tree_sid.index(),
                        source
                    );
                }
            }
            None => tracing::error!(
                "Error propagating qabl {}: cannot get index of {}!",
                res.expr(),
                source
            ),
        }
    }

    #[inline]
    fn send_forget_sourced_queryable_to_net_children(
        &self,
        tables: &TablesData,
        net: &Network,
        children: &[NodeIndex],
        res: &Arc<Resource>,
        src_face: Option<&Arc<FaceState>>,
        routing_context: NodeId,
    ) {
        for child in children {
            if net.graph.contains_node(*child) {
                match self.face(tables, &net.graph[*child].zid).cloned() {
                    Some(mut someface) => {
                        if src_face
                            .map(|src_face| someface.id != src_face.id)
                            .unwrap_or(true)
                        {
                            let wire_expr = Resource::decl_key(res, &mut someface);

                            someface.primitives.send_declare(RoutingContext::with_expr(
                                &mut Declare {
                                    interest_id: None,
                                    ext_qos: declare::ext::QoSType::DECLARE,
                                    ext_tstamp: None,
                                    ext_nodeid: declare::ext::NodeIdType {
                                        node_id: routing_context,
                                    },
                                    body: DeclareBody::UndeclareQueryable(UndeclareQueryable {
                                        id: QueryableId::default(), // Sourced queryables do not use ids
                                        ext_wire_expr: WireExprType { wire_expr },
                                    }),
                                },
                                res.expr().to_string(),
                            ));
                        }
                    }
                    None => {
                        tracing::trace!("Unable to find face for zid {}", net.graph[*child].zid)
                    }
                }
            }
        }
    }

    fn propagate_forget_sourced_queryable(
        &self,
        tables: &TablesData,
        res: &Arc<Resource>,
        src_face: Option<&Arc<FaceState>>,
        source: &ZenohIdProto,
    ) {
        let net = self.routers_net.as_ref().unwrap();
        match net.get_idx(source) {
            Some(tree_sid) => {
                if net.trees.len() > tree_sid.index() {
                    self.send_forget_sourced_queryable_to_net_children(
                        tables,
                        net,
                        &net.trees[tree_sid.index()].children,
                        res,
                        src_face,
                        tree_sid.index() as NodeId,
                    );
                } else {
                    tracing::trace!(
                        "Propagating forget qabl {}: tree for node {} sid:{} not yet ready",
                        res.expr(),
                        tree_sid.index(),
                        source
                    );
                }
            }
            None => tracing::error!(
                "Error propagating forget qabl {}: cannot get index of {}!",
                res.expr(),
                source
            ),
        }
    }

    pub(super) fn unregister_node_queryables(
        &mut self,
        zid: &ZenohIdProto,
    ) -> HashSet<Arc<Resource>> {
        let removed_routers = self
            .net_mut()
            .remove_link(zid)
            .into_iter()
            .map(|(_, zid)| zid)
            .collect::<HashSet<_>>();

        let mut resources = HashSet::new();

        for mut res in self.router_qabls.iter().cloned().collect_vec() {
            self.res_hat_mut(&mut res)
                .router_qabls
                .retain(|router, _| !removed_routers.contains(router));

            if self.res_hat(&res).router_qabls.is_empty() {
                self.router_qabls.retain(|r| !Arc::ptr_eq(r, &res));
                resources.insert(res);
            }
        }

        resources
    }
}

impl HatQueriesTrait for Hat {
    #[tracing::instrument(level = "debug", skip(_tables), ret)]
    fn sourced_queryables(&self, _tables: &TablesData) -> Vec<(Arc<Resource>, Sources)> {
        // Compute the list of known queryables (keys)
        self.router_qabls
            .iter()
            .map(|sub| {
                (
                    sub.clone(),
                    Sources {
                        routers: Vec::from_iter(self.res_hat(sub).router_qabls.keys().cloned()),
                        peers: Vec::default(),
                        clients: Vec::default(),
                    },
                )
            })
            .collect()
    }

    #[tracing::instrument(level = "debug", skip(_tables), ret)]
    fn sourced_queriers(&self, _tables: &TablesData) -> Vec<(Arc<Resource>, Sources)> {
        Vec::default()
    }

    #[tracing::instrument(level = "debug", skip(tables, src_face), ret)]
    fn compute_query_route(
        &self,
        tables: &TablesData,
        src_face: &FaceState,
        expr: &RoutingExpr,
        node_id: NodeId,
    ) -> Arc<QueryTargetQablSet> {
        lazy_static::lazy_static! {
            static ref EMPTY_ROUTE: Arc<QueryTargetQablSet> = Arc::new(Vec::new());
        }

        #[allow(clippy::too_many_arguments)]
        fn insert_target_for_qabls(
            this: &Hat,
            route: &mut QueryTargetQablSet,
            expr: &RoutingExpr,
            tables: &TablesData,
            net: &Network,
            source: NodeId,
            qabls: &HashMap<ZenohIdProto, QueryableInfoType>,
            complete: bool,
        ) {
            if net.trees.len() > source as usize {
                for (qabl, qabl_info) in qabls {
                    if let Some(qabl_idx) = net.get_idx(qabl) {
                        if net.trees[source as usize].directions.len() > qabl_idx.index() {
                            if let Some(direction) =
                                net.trees[source as usize].directions[qabl_idx.index()]
                            {
                                if net.graph.contains_node(direction) {
                                    if let Some(face) = this.face(tables, &net.graph[direction].zid)
                                    {
                                        if net.distances.len() > qabl_idx.index() {
                                            let wire_expr = expr.get_best_key(face.id);
                                            route.push(QueryTargetQabl {
                                                dir: Direction {
                                                    dst_face: face.clone(),
                                                    wire_expr: wire_expr.to_owned(),
                                                    node_id: source,
                                                },
                                                info: Some(QueryableInfoType {
                                                    complete: complete && qabl_info.complete,
                                                    distance: net.distances[qabl_idx.index()]
                                                        as u16,
                                                }),
                                                region: this.region,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                tracing::trace!("Tree for node sid:{} not yet ready", source);
            }
        }

        let mut route = QueryTargetQablSet::new();
        let Some(key_expr) = expr.key_expr() else {
            return EMPTY_ROUTE.clone();
        };

        let matches = expr
            .resource()
            .as_ref()
            .and_then(|res| res.ctx.as_ref())
            .map(|ctx| Cow::from(&ctx.matches))
            .unwrap_or_else(|| Cow::from(Resource::get_matches(tables, key_expr)));

        for mres in matches.iter() {
            let mres = mres.upgrade().unwrap();
            let complete = DEFAULT_INCLUDER.includes(mres.expr().as_bytes(), key_expr.as_bytes());
            let net = self.routers_net.as_ref().unwrap();
            // FIXME(regions): remove this
            let router_source = match src_face.whatami {
                WhatAmI::Router => node_id,
                _ => net.idx.index() as NodeId,
            };
            insert_target_for_qabls(
                self,
                &mut route,
                expr,
                tables,
                net,
                router_source,
                &self.res_hat(&mres).router_qabls,
                complete,
            );
        }

        route.sort_by_key(|qabl| qabl.info.map_or(u16::MAX, |i| i.distance));
        Arc::new(route)
    }

    #[tracing::instrument(level = "debug", skip(ctx, _id, info), ret)]
    fn register_queryable(
        &mut self,
        ctx: DispatcherContext,
        _id: QueryableId,
        mut res: Arc<Resource>,
        node_id: NodeId,
        info: &QueryableInfoType,
    ) {
        debug_assert!(self.owns(ctx.src_face));

        let Some(router) = self.get_router(ctx.src_face, node_id) else {
            tracing::error!(%node_id, "Queryable from unknown router");
            return;
        };

        debug_assert_ne!(router, ctx.tables.zid);

        self.res_hat_mut(&mut res)
            .router_qabls
            .insert(router, *info);
        self.router_qabls.insert(res.clone());

        self.propagate_sourced_queryable(ctx.tables, &res, info, Some(ctx.src_face), &router);
    }

    #[tracing::instrument(level = "debug", skip(ctx, _id), ret)]
    fn unregister_queryable(
        &mut self,
        ctx: DispatcherContext,
        _id: QueryableId,
        res: Option<Arc<Resource>>,
        node_id: NodeId,
    ) -> UnregisterResult {
        use UnregisterResult::*;

        debug_assert!(self.owns(ctx.src_face));

        let Some(router) = self.get_router(ctx.src_face, node_id) else {
            tracing::error!(%node_id, "Queryable from unknown router");
            return Noop;
        };

        debug_assert_ne!(router, ctx.tables.zid);

        let Some(mut res) = res else {
            tracing::error!("Queryable undeclaration in router region with no resource");
            return Noop;
        };

        let Some(old_info) = self.remote_queryables_of(&res) else {
            tracing::error!("Queryable undeclaration in router region with no info");
            return Noop;
        };

        self.res_hat_mut(&mut res).router_qabls.remove(&router);

        let new_info = self.remote_queryables_of(&res);

        if new_info.is_none() {
            self.router_qabls.retain(|r| !Arc::ptr_eq(r, &res));
        }

        self.propagate_forget_sourced_queryable(ctx.tables, &res, Some(ctx.src_face), &router);

        match new_info {
            Some(new_info) => {
                if new_info == old_info {
                    Noop
                } else {
                    InfoUpdate { res }
                }
            }
            None => LastUnregistered { res },
        }
    }

    #[tracing::instrument(level = "debug", skip(ctx), ret)]
    fn unregister_face_queryables(&mut self, ctx: DispatcherContext) -> HashSet<Arc<Resource>> {
        debug_assert!(self.owns(ctx.src_face));

        self.unregister_node_queryables(&ctx.src_face.zid)
    }

    #[allow(clippy::incompatible_msrv)]
    #[tracing::instrument(level = "debug", skip(ctx), ret)]
    fn propagate_queryable(
        &mut self,
        ctx: DispatcherContext,
        mut res: Arc<Resource>,
        other_info: Option<QueryableInfoType>,
    ) {
        let Some(other_info) = other_info else {
            // NOTE(regions): see Hat::register_queryable
            debug_assert!(self.owns(ctx.src_face));
            return;
        };

        if self
            .res_hat(&res)
            .router_qabls
            .get(&ctx.tables.zid)
            .is_none_or(|info| info != &other_info)
        {
            self.res_hat_mut(&mut res)
                .router_qabls
                .insert(ctx.tables.zid, other_info);
            self.router_qabls.insert(res.clone());

            self.propagate_sourced_queryable(ctx.tables, &res, &other_info, None, &ctx.tables.zid);
        }
    }

    #[tracing::instrument(level = "debug", skip(ctx), ret)]
    fn unpropagate_queryable(&mut self, ctx: DispatcherContext, mut res: Arc<Resource>) {
        if self.owns(ctx.src_face) {
            // NOTE(regions): see Hat::unregister_queryable
            return;
        }

        let was_propagated = self
            .res_hat(&res)
            .router_qabls
            .contains_key(&ctx.tables.zid);

        debug_assert!(was_propagated);

        if was_propagated {
            self.res_hat_mut(&mut res)
                .router_qabls
                .remove(&ctx.tables.zid);
            if self.res_hat(&res).router_qabls.is_empty() {
                self.router_qabls.retain(|r| !Arc::ptr_eq(r, &res));
            }

            self.propagate_forget_sourced_queryable(ctx.tables, &res, None, &ctx.tables.zid);
        }
    }

    #[tracing::instrument(level = "trace", ret)]
    fn remote_queryables_of(&self, res: &Resource) -> Option<QueryableInfoType> {
        // FIXME(regions): use TablesData::zid?
        let net = self.net();
        let this_router = &net.graph[net.idx].zid;

        self.res_hat(res)
            .router_qabls
            .iter()
            .filter_map(|(router, info)| (router != this_router).then_some(*info))
            .reduce(merge_qabl_infos)
    }

    #[allow(clippy::incompatible_msrv)]
    #[tracing::instrument(level = "trace", skip(tables), ret)]
    fn remote_queryables_matching(
        &self,
        tables: &TablesData,
        res: Option<&Resource>,
    ) -> HashMap<Arc<Resource>, QueryableInfoType> {
        self.router_qabls
            .iter()
            .filter(|qabl| res.is_none_or(|res| res.matches(qabl)))
            .flat_map(|qabl| {
                std::iter::repeat(qabl).zip(
                    self.res_hat(qabl)
                        .router_qabls
                        .iter()
                        .filter_map(|(router, info)| (router != &tables.zid).then_some(*info)),
                )
            })
            .fold(HashMap::new(), |mut acc, (res, info)| {
                acc.entry(res.clone())
                    .and_modify(|i| {
                        *i = merge_qabl_infos(*i, info);
                    })
                    .or_insert(info);
                acc
            })
    }
}
