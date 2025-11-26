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
    sync::{atomic::Ordering, Arc},
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
    network::{
        declare::{
            self, common::ext::WireExprType, queryable::ext::QueryableInfoType, Declare,
            DeclareBody, DeclareQueryable, QueryableId, UndeclareQueryable,
        },
        interest::{InterestId, InterestMode},
    },
};
use zenoh_sync::get_mut_unchecked;

use super::Hat;
use crate::{
    key_expr::KeyExpr,
    net::{
        protocol::network::Network,
        routing::{
            dispatcher::{
                face::{FaceId, FaceState},
                local_resources::LocalResourceInfoTrait,
                queries::{
                    disable_matches_query_routes, get_remote_qabl_info, merge_qabl_infos,
                    update_queryable_info,
                },
                resource::{FaceContext, NodeId, Resource},
                tables::{QueryTargetQabl, QueryTargetQablSet, RoutingExpr, TablesData},
            },
            hat::{BaseContext, HatBaseTrait, HatQueriesTrait, SendDeclare, Sources},
            router::Direction,
            RoutingContext,
        },
    },
};

impl Hat {
    fn local_router_qabl_info(
        &self,
        _tables: &TablesData,
        res: &Arc<Resource>,
    ) -> QueryableInfoType {
        res.face_ctxs
            .values()
            .fold(None, |accu, ctx| {
                if let Some(info) = ctx.qabl.as_ref() {
                    Some(match accu {
                        Some(accu) => merge_qabl_infos(accu, *info),
                        None => *info,
                    })
                } else {
                    accu
                }
            })
            .unwrap_or(QueryableInfoType::DEFAULT)
    }

    fn local_qabl_info(
        &self,
        tables: &TablesData,
        res: &Arc<Resource>,
        face: &Arc<FaceState>,
    ) -> QueryableInfoType {
        let info = if res.ctx.is_some() {
            self.res_hat(res)
                .router_qabls
                .iter()
                .fold(None, |accu, (zid, info)| {
                    if *zid != tables.zid {
                        Some(match accu {
                            Some(accu) => merge_qabl_infos(accu, *info),
                            None => *info,
                        })
                    } else {
                        accu
                    }
                })
        } else {
            None
        };
        res.face_ctxs
            .values()
            .fold(info, |accu, ctx| {
                if (ctx.face.id != face.id && ctx.face.whatami != WhatAmI::Peer)
                    || face.whatami != WhatAmI::Peer
                {
                    if let Some(info) = ctx.qabl.as_ref() {
                        Some(match accu {
                            Some(accu) => merge_qabl_infos(accu, *info),
                            None => *info,
                        })
                    } else {
                        accu
                    }
                } else {
                    accu
                }
            })
            .unwrap_or(QueryableInfoType::DEFAULT)
    }

    #[inline]
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

    #[inline]
    fn maybe_register_local_queryable(
        &self,
        tables: &TablesData,
        dst_face: &mut Arc<FaceState>,
        res: &Arc<Resource>,
        initial_interest: Option<InterestId>,
        send_declare: &mut SendDeclare,
    ) {
        let (should_notify, simple_interests) = match initial_interest {
            Some(interest) => (true, HashSet::from([interest])),
            None => self
                .face_hat(dst_face)
                .remote_interests
                .iter()
                .filter(|(_, i)| i.options.queryables() && i.matches(res))
                .fold(
                    (false, HashSet::new()),
                    |(_, mut simple_interests), (id, i)| {
                        if !i.options.aggregate() {
                            simple_interests.insert(*id);
                        }
                        (true, simple_interests)
                    },
                ),
        };

        if !should_notify {
            return;
        }

        let new_info = self.local_qabl_info(tables, res, dst_face);
        let face_hat_mut = self.face_hat_mut(dst_face);
        let (_, qabls_to_notify) = face_hat_mut.local_qabls.insert_simple_resource(
            res.clone(),
            new_info,
            || face_hat_mut.next_id.fetch_add(1, Ordering::SeqCst),
            simple_interests,
        );

        for update in qabls_to_notify {
            let key_expr = Resource::decl_key(&update.resource, dst_face);
            send_declare(
                &dst_face.primitives,
                RoutingContext::with_expr(
                    Declare {
                        interest_id: None,
                        ext_qos: declare::ext::QoSType::DECLARE,
                        ext_tstamp: None,
                        ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                        body: DeclareBody::DeclareQueryable(DeclareQueryable {
                            id: update.id,
                            wire_expr: key_expr.clone(),
                            ext_info: update.info,
                        }),
                    },
                    update.resource.expr().to_string(),
                ),
            );
        }
    }

    #[inline]
    fn maybe_unregister_local_queryable(
        &self,
        _tables: &TablesData,
        face: &mut Arc<FaceState>,
        res: &Arc<Resource>,
        send_ext_wire_expr: bool,
        send_declare: &mut SendDeclare,
    ) {
        for update in self
            .face_hat_mut(face)
            .local_qabls
            .remove_simple_resource(res)
        {
            let ext_wire_expr = if send_ext_wire_expr {
                WireExprType {
                    wire_expr: Resource::get_best_key(&update.resource, "", face.id),
                }
            } else {
                WireExprType::null()
            };
            match update.update {
                Some(new_qabl_info) => {
                    let key_expr = Resource::decl_key(&update.resource, face);
                    send_declare(
                        &face.primitives,
                        RoutingContext::with_expr(
                            Declare {
                                interest_id: None,
                                ext_qos: declare::ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                                body: DeclareBody::DeclareQueryable(DeclareQueryable {
                                    id: update.id,
                                    wire_expr: key_expr.clone(),
                                    ext_info: new_qabl_info,
                                }),
                            },
                            update.resource.expr().to_string(),
                        ),
                    );
                }
                None => send_declare(
                    &face.primitives,
                    RoutingContext::with_expr(
                        Declare {
                            interest_id: None,
                            ext_qos: declare::ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                            body: DeclareBody::UndeclareQueryable(UndeclareQueryable {
                                id: update.id,
                                ext_wire_expr,
                            }),
                        },
                        update.resource.expr().to_string(),
                    ),
                ),
            };
        }
    }

    fn propagate_simple_queryable(
        &self,
        tables: &mut TablesData,
        res: &Arc<Resource>,
        src_face: Option<&mut Arc<FaceState>>,
        send_declare: &mut SendDeclare,
    ) {
        let faces = tables.faces.values().cloned();
        for mut dst_face in faces {
            if src_face
                .as_ref()
                .map(|src_face| dst_face.id != src_face.id)
                .unwrap_or(true)
                && dst_face.whatami != WhatAmI::Router
                && src_face
                    .as_ref()
                    .map(|src_face| {
                        src_face.whatami != WhatAmI::Peer || dst_face.whatami != WhatAmI::Peer
                    })
                    .unwrap_or(true)
            {
                self.maybe_register_local_queryable(tables, &mut dst_face, res, None, send_declare);
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

    #[allow(clippy::too_many_arguments)]
    fn register_router_queryable(
        &mut self,
        tables: &mut TablesData,
        mut face: Option<&mut Arc<FaceState>>,
        res: &mut Arc<Resource>,
        qabl_info: &QueryableInfoType,
        router: ZenohIdProto,
        send_declare: &mut SendDeclare,
    ) {
        let current_info = self.res_hat(res).router_qabls.get(&router);
        if current_info.is_none() || current_info.unwrap() != qabl_info {
            // Register router queryable
            {
                self.res_hat_mut(res)
                    .router_qabls
                    .insert(router, *qabl_info);
                self.router_qabls.insert(res.clone());
            }

            // Propagate queryable to routers
            self.propagate_sourced_queryable(tables, res, qabl_info, face.as_deref_mut(), &router);
        }

        // Propagate queryable to clients
        self.propagate_simple_queryable(tables, res, face, send_declare);
    }

    #[allow(clippy::too_many_arguments)]
    fn declare_router_queryable(
        &mut self,
        tables: &mut TablesData,
        face: &mut Arc<FaceState>,
        res: &mut Arc<Resource>,
        qabl_info: &QueryableInfoType,
        router: ZenohIdProto,
        send_declare: &mut SendDeclare,
    ) {
        self.register_router_queryable(tables, Some(face), res, qabl_info, router, send_declare);
    }

    fn register_simple_queryable(
        &self,
        _tables: &mut TablesData,
        face: &mut Arc<FaceState>,
        id: QueryableId,
        res: &mut Arc<Resource>,
        qabl_info: &QueryableInfoType,
    ) {
        // Register queryable
        {
            let res = get_mut_unchecked(res);
            get_mut_unchecked(
                res.face_ctxs
                    .entry(face.id)
                    .or_insert_with(|| Arc::new(FaceContext::new(face.clone()))),
            )
            .qabl = Some(*qabl_info);
        }
        self.face_hat_mut(face)
            .remote_qabls
            .insert(id, (res.clone(), *qabl_info));
    }

    #[allow(clippy::too_many_arguments)]
    fn declare_simple_queryable(
        &mut self,
        tables: &mut TablesData,
        face: &mut Arc<FaceState>,
        id: QueryableId,
        res: &mut Arc<Resource>,
        qabl_info: &QueryableInfoType,
        send_declare: &mut SendDeclare,
    ) {
        self.register_simple_queryable(tables, face, id, res, qabl_info);
        let local_details = self.local_router_qabl_info(tables, res);
        let zid = tables.zid;
        self.register_router_queryable(tables, Some(face), res, &local_details, zid, send_declare);
    }

    #[inline]
    fn remote_router_qabls(&self, tables: &TablesData, res: &Arc<Resource>) -> bool {
        res.ctx.is_some()
            && self
                .res_hat(res)
                .router_qabls
                .keys()
                .any(|router| router != &tables.zid)
    }

    #[inline]
    fn simple_qabls(&self, res: &Arc<Resource>) -> Vec<Arc<FaceState>> {
        res.face_ctxs
            .values()
            .filter_map(|ctx| {
                if ctx.qabl.is_some() {
                    Some(ctx.face.clone())
                } else {
                    None
                }
            })
            .collect()
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

    fn propagate_forget_simple_queryable(
        &self,
        tables: &mut TablesData,
        res: &mut Arc<Resource>,
        send_declare: &mut SendDeclare,
    ) {
        for mut face in self.faces(tables).values().cloned() {
            self.maybe_unregister_local_queryable(tables, &mut face, res, false, send_declare);
        }
    }

    fn propagate_forget_simple_queryable_to_peers(
        &self,
        tables: &mut TablesData,
        res: &mut Arc<Resource>,
        send_declare: &mut SendDeclare,
    ) {
        if self.res_hat(res).router_qabls.len() == 1
            && self.res_hat(res).router_qabls.contains_key(&tables.zid)
        {
            for mut face in tables
                .faces
                .values()
                .cloned()
                .collect::<Vec<Arc<FaceState>>>()
            {
                if face.whatami == WhatAmI::Peer
                    && self
                        .face_hat(&face)
                        .local_qabls
                        .contains_simple_resource(res)
                    && !res.face_ctxs.values().any(|s| {
                        face.zid != s.face.zid
                            && s.qabl.is_some()
                            && s.face.whatami == WhatAmI::Client
                    })
                {
                    self.maybe_unregister_local_queryable(
                        tables,
                        &mut face,
                        res,
                        false,
                        send_declare,
                    );
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

    fn unregister_router_queryable(
        &mut self,
        tables: &mut TablesData,
        res: &mut Arc<Resource>,
        router: &ZenohIdProto,
        send_declare: &mut SendDeclare,
    ) {
        self.res_hat_mut(res).router_qabls.remove(router);

        if self.res_hat(res).router_qabls.is_empty() {
            self.router_qabls.retain(|qabl| !Arc::ptr_eq(qabl, res));

            self.propagate_forget_simple_queryable(tables, res, send_declare);
        }

        self.propagate_forget_simple_queryable_to_peers(tables, res, send_declare);
    }

    fn undeclare_router_queryable(
        &mut self,
        tables: &mut TablesData,
        face: Option<&Arc<FaceState>>,
        res: &mut Arc<Resource>,
        router: &ZenohIdProto,
        send_declare: &mut SendDeclare,
    ) {
        if self.res_hat(res).router_qabls.contains_key(router) {
            self.unregister_router_queryable(tables, res, router, send_declare);
            self.propagate_forget_sourced_queryable(tables, res, face, router);
        }
    }

    fn forget_router_queryable(
        &mut self,
        tables: &mut TablesData,
        face: &mut Arc<FaceState>,
        res: &mut Arc<Resource>,
        router: &ZenohIdProto,
        send_declare: &mut SendDeclare,
    ) {
        self.undeclare_router_queryable(tables, Some(face), res, router, send_declare);
    }

    pub(super) fn undeclare_simple_queryable(&mut self, ctx: BaseContext, res: &mut Arc<Resource>) {
        let remote_qabl_info =
            get_remote_qabl_info(&self.face_hat_mut(ctx.src_face).remote_qabls, res);

        if update_queryable_info(res, ctx.src_face.id, &remote_qabl_info) {
            let mut simple_qabls = self.simple_qabls(res);
            let router_qabls = self.remote_router_qabls(ctx.tables, res);

            if simple_qabls.is_empty() {
                self.undeclare_router_queryable(
                    ctx.tables,
                    None,
                    res,
                    &ctx.tables.zid.clone(),
                    ctx.send_declare,
                );
            } else {
                let local_info = self.local_router_qabl_info(ctx.tables, res);
                self.register_router_queryable(
                    ctx.tables,
                    None,
                    res,
                    &local_info,
                    ctx.tables.zid,
                    ctx.send_declare,
                );
                self.propagate_forget_simple_queryable_to_peers(ctx.tables, res, ctx.send_declare);
            }

            if simple_qabls.len() == 1 && !router_qabls {
                let face = &mut simple_qabls[0];
                self.maybe_unregister_local_queryable(
                    ctx.tables,
                    face,
                    res,
                    false,
                    ctx.send_declare,
                );
            }
        }
    }

    fn forget_simple_queryable(
        &mut self,
        ctx: BaseContext,
        id: QueryableId,
    ) -> Option<Arc<Resource>> {
        if let Some((mut res, _)) = self.face_hat_mut(ctx.src_face).remote_qabls.remove(&id) {
            self.undeclare_simple_queryable(ctx, &mut res);
            Some(res)
        } else {
            None
        }
    }

    pub(super) fn queries_remove_node(
        &mut self,
        tables: &mut TablesData,
        node: &ZenohIdProto,
        send_declare: &mut SendDeclare,
    ) {
        let mut qabls = vec![];
        for res in self.router_qabls.iter() {
            for qabl in self.res_hat(res).router_qabls.keys() {
                if qabl == node {
                    qabls.push(res.clone());
                }
            }
        }
        for mut res in qabls {
            self.unregister_router_queryable(tables, &mut res, node, send_declare);

            disable_matches_query_routes(&mut res, &self.region);
            Resource::clean(&mut res);
        }
    }

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

    #[inline]
    #[allow(clippy::too_many_arguments)]
    fn insert_target_for_qabls(
        &self,
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
                                if let Some(face) = self.face(tables, &net.graph[direction].zid) {
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
                                                distance: net.distances[qabl_idx.index()] as u16,
                                            }),
                                            region: self.region,
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

    fn get_queryables_matching_resource(
        &self,
        tables: &TablesData,
        face: &Arc<FaceState>,
        res: Option<&Arc<Resource>>,
    ) -> Vec<Arc<Resource>> {
        let face_id = face.id;
        let face_what_am_i = face.whatami;

        self.router_qabls
            .iter()
            .filter(move |qabl| {
                qabl.ctx.is_some()
                    && res.as_ref().map(|r| qabl.matches(r)).unwrap_or(true)
                    && (self
                        .res_hat(qabl)
                        .router_qabls
                        .keys()
                        .any(|r| *r != tables.zid)
                        || qabl.face_ctxs.values().any(|s| {
                            s.face.id != face_id
                                && s.qabl.is_some()
                                && (s.face.whatami == WhatAmI::Client
                                    || face_what_am_i == WhatAmI::Client)
                        }))
            })
            .cloned()
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn declare_qabl_interest(
        &self,
        tables: &mut TablesData,
        face: &mut Arc<FaceState>,
        interest_id: InterestId,
        res: Option<&mut Arc<Resource>>,
        mode: InterestMode,
        aggregate: bool,
        _source: NodeId,
        send_declare: &mut SendDeclare,
    ) {
        let res = res.map(|r| r.clone());
        let matching_qabls = self.get_queryables_matching_resource(tables, face, res.as_ref());
        let matching_qabls = matching_qabls.iter();

        if aggregate && (mode.is_current() || mode.is_future()) {
            if let Some(aggregated_res) = &res {
                let (resource_id, qabl_info) = if mode.is_future() {
                    for qabl in matching_qabls {
                        let qabl_info = self.local_qabl_info(tables, qabl, face);
                        let face_hat_mut = self.face_hat_mut(face);
                        face_hat_mut.local_qabls.insert_simple_resource(
                            qabl.clone(),
                            qabl_info,
                            || face_hat_mut.next_id.fetch_add(1, Ordering::SeqCst),
                            HashSet::new(),
                        );
                    }
                    let face_hat_mut = self.face_hat_mut(face);
                    face_hat_mut.local_qabls.insert_aggregated_resource(
                        aggregated_res.clone(),
                        || face_hat_mut.next_id.fetch_add(1, Ordering::SeqCst),
                        HashSet::from_iter([interest_id]),
                    )
                } else {
                    (
                        0,
                        QueryableInfoType::aggregate_many(
                            aggregated_res,
                            matching_qabls.map(|q| (q, self.local_qabl_info(tables, q, face))),
                        ),
                    )
                };
                if let Some(ext_info) = mode.is_current().then_some(qabl_info).flatten() {
                    // send declare only if there is at least one resource matching the aggregate
                    let wire_expr = Resource::decl_key(aggregated_res, face);
                    send_declare(
                        &face.primitives,
                        RoutingContext::with_expr(
                            Declare {
                                interest_id: Some(interest_id),
                                ext_qos: declare::ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                                body: DeclareBody::DeclareQueryable(DeclareQueryable {
                                    id: resource_id,
                                    wire_expr,
                                    ext_info,
                                }),
                            },
                            aggregated_res.expr().to_string(),
                        ),
                    );
                }
            }
        } else if !aggregate && mode.is_current() {
            for qabl in matching_qabls {
                let qabl_info = self.local_qabl_info(tables, qabl, face);
                let resource_id = if mode.is_future() {
                    let face_hat_mut = self.face_hat_mut(face);
                    face_hat_mut
                        .local_qabls
                        .insert_simple_resource(
                            qabl.clone(),
                            qabl_info,
                            || face_hat_mut.next_id.fetch_add(1, Ordering::SeqCst),
                            HashSet::from([interest_id]),
                        )
                        .0
                } else {
                    0
                };
                let wire_expr = Resource::decl_key(qabl, face);
                send_declare(
                    &face.primitives,
                    RoutingContext::with_expr(
                        Declare {
                            interest_id: Some(interest_id),
                            ext_qos: declare::ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                            body: DeclareBody::DeclareQueryable(DeclareQueryable {
                                id: resource_id,
                                wire_expr,
                                ext_info: qabl_info,
                            }),
                        },
                        qabl.expr().to_string(),
                    ),
                );
            }
        }
    }
}

impl HatQueriesTrait for Hat {
    fn declare_queryable(
        &mut self,
        ctx: BaseContext,
        id: QueryableId,
        res: &mut Arc<Resource>,
        node_id: NodeId,

        qabl_info: &QueryableInfoType,
    ) {
        let router = if self.owns_router(ctx.src_face) {
            let Some(router) = self.get_router(ctx.src_face, node_id) else {
                tracing::error!(%node_id, "Queryable from unknown router");
                return;
            };

            router
        } else {
            ctx.tables.zid
        };

        match ctx.src_face.whatami {
            WhatAmI::Router => {
                self.declare_router_queryable(
                    ctx.tables,
                    ctx.src_face,
                    res,
                    qabl_info,
                    router,
                    ctx.send_declare,
                );
            }
            WhatAmI::Peer | WhatAmI::Client => {
                // TODO(regions2): clients and peers of this
                // router are handled as if they were bound to future broker/peer-to-peer south hats resp.
                self.declare_simple_queryable(
                    ctx.tables,
                    ctx.src_face,
                    id,
                    res,
                    qabl_info,
                    ctx.send_declare,
                );
            }
        }
    }

    fn undeclare_queryable(
        &mut self,
        ctx: BaseContext,
        id: QueryableId,
        res: Option<Arc<Resource>>,
        node_id: NodeId,
    ) -> Option<Arc<Resource>> {
        let router = if self.owns_router(ctx.src_face) {
            let Some(router) = self.get_router(ctx.src_face, node_id) else {
                tracing::error!(%node_id, "Queryable from unknown router");
                return None;
            };

            router
        } else {
            ctx.tables.zid
        };

        let mut res = res?;

        match ctx.src_face.whatami {
            WhatAmI::Router => {
                self.forget_router_queryable(
                    ctx.tables,
                    ctx.src_face,
                    &mut res,
                    &router,
                    ctx.send_declare,
                );
                Some(res)
            }
            WhatAmI::Peer | WhatAmI::Client => self.forget_simple_queryable(ctx, id),
        }
    }

    fn get_queryables(&self, _tables: &TablesData) -> Vec<(Arc<Resource>, Sources)> {
        // Compute the list of known queryables (keys)
        self.router_qabls
            .iter()
            .map(|s| {
                // Compute the list of routers, peers and clients that are known
                // sources of those queryables
                let routers = Vec::from_iter(self.res_hat(s).router_qabls.keys().cloned());
                let mut peers = vec![];
                let mut clients = vec![];
                for ctx in s
                    .face_ctxs
                    .values()
                    .filter(|ctx| ctx.qabl.is_some() && !ctx.face.is_local)
                {
                    match ctx.face.whatami {
                        WhatAmI::Router => (),
                        WhatAmI::Peer => peers.push(ctx.face.zid),
                        WhatAmI::Client => clients.push(ctx.face.zid),
                    }
                }
                (
                    s.clone(),
                    Sources {
                        routers,
                        peers,
                        clients,
                    },
                )
            })
            .collect()
    }

    fn get_queriers(&self, tables: &TablesData) -> Vec<(Arc<Resource>, Sources)> {
        let mut result = HashMap::new();
        for face in self.faces(tables).values() {
            for interest in self.face_hat(face).remote_interests.values() {
                if interest.options.queryables() {
                    if let Some(res) = interest.res.as_ref() {
                        let sources = result.entry(res.clone()).or_insert_with(Sources::default);
                        let whatami = if face.is_local {
                            tables.hats.north().whatami // REVIEW(fuzzypixelz)
                        } else {
                            face.whatami
                        };
                        match whatami {
                            WhatAmI::Router => sources.routers.push(face.zid),
                            WhatAmI::Peer => sources.peers.push(face.zid),
                            WhatAmI::Client => sources.clients.push(face.zid),
                        }
                    }
                }
            }
        }
        result.into_iter().collect()
    }

    #[tracing::instrument(level = "trace", skip_all, fields(expr = ?expr, rgn = %self.region))]
    fn compute_query_route(
        &self,
        tables: &TablesData,
        src_face: &FaceState,
        expr: &RoutingExpr,
        source: NodeId,
    ) -> Arc<QueryTargetQablSet> {
        lazy_static::lazy_static! {
            static ref EMPTY_ROUTE: Arc<QueryTargetQablSet> = Arc::new(Vec::new());
        }

        let mut route = QueryTargetQablSet::new();
        let Some(key_expr) = expr.key_expr() else {
            return EMPTY_ROUTE.clone();
        };
        let source_type = src_face.whatami;
        tracing::trace!(
            "compute_query_route({}, {:?}, {:?})",
            key_expr,
            source,
            source_type
        );
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
            let router_source = match source_type {
                WhatAmI::Router => source,
                _ => net.idx.index() as NodeId,
            };
            self.insert_target_for_qabls(
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

    fn get_matching_queryables(
        &self,
        tables: &TablesData,
        key_expr: &KeyExpr<'_>,
        complete: bool,
    ) -> HashMap<FaceId, Arc<FaceState>> {
        let mut matching_queryables = HashMap::new();

        tracing::trace!(
            "get_matching_queryables({}; complete: {})",
            key_expr,
            complete
        );
        let res = Resource::get_resource(&tables.root_res, key_expr);
        let matches = res
            .as_ref()
            .and_then(|res| res.ctx.as_ref())
            .map(|ctx| Cow::from(&ctx.matches))
            .unwrap_or_else(|| Cow::from(Resource::get_matches(tables, key_expr)));

        for mres in matches.iter() {
            let mres = mres.upgrade().unwrap();
            if complete && !KeyExpr::keyexpr_include(mres.expr(), key_expr) {
                continue;
            }

            let net = self.routers_net.as_ref().unwrap();
            self.insert_faces_for_qbls(
                &mut matching_queryables,
                tables,
                net,
                &self.res_hat(&mres).router_qabls,
                complete,
            );

            for (sid, ctx) in &mres.face_ctxs {
                if match complete {
                    true => ctx.qabl.is_some_and(|q| q.complete),
                    false => ctx.qabl.is_some(),
                } && ctx.face.whatami != WhatAmI::Router
                {
                    matching_queryables
                        .entry(*sid)
                        .or_insert_with(|| ctx.face.clone());
                }
            }
        }
        matching_queryables
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region))]
    fn register_queryable(
        &mut self,
        ctx: BaseContext,
        _id: QueryableId,
        mut res: Arc<Resource>,
        nid: NodeId,
        info: &QueryableInfoType,
    ) {
        debug_assert!(self.owns(ctx.src_face));

        let Some(router) = self.get_router(ctx.src_face, nid) else {
            tracing::error!(%nid, "Queryable from unknown router");
            return;
        };

        debug_assert_ne!(router, ctx.tables.zid);

        self.res_hat_mut(&mut res)
            .router_qabls
            .insert(router, *info);
        self.router_qabls.insert(res.clone());

        self.propagate_sourced_queryable(ctx.tables, &res, info, Some(ctx.src_face), &router);
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region), ret)]
    fn unregister_queryable(
        &mut self,
        ctx: BaseContext,
        _id: QueryableId,
        res: Option<Arc<Resource>>,
        nid: NodeId,
    ) -> Option<Arc<Resource>> {
        debug_assert!(self.owns(ctx.src_face));

        let Some(router) = self.get_router(ctx.src_face, nid) else {
            tracing::error!(%nid, "Queryable from unknown router");
            return None;
        };

        debug_assert_ne!(router, ctx.tables.zid);

        let Some(mut res) = res else {
            tracing::error!("Queryable undeclaration in router region with no resource");
            return None;
        };

        self.res_hat_mut(&mut res).router_qabls.remove(&router);

        if self.res_hat(&res).router_qabls.is_empty() {
            self.router_qabls.retain(|r| !Arc::ptr_eq(r, &res));
        }

        self.propagate_forget_sourced_queryable(ctx.tables, &res, Some(ctx.src_face), &router);

        Some(res)
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region), ret)]
    fn unregister_face_queryables(&mut self, ctx: BaseContext) -> HashSet<Arc<Resource>> {
        let removed_routers = self
            .net_mut()
            .remove_link(&ctx.src_face.zid)
            .into_iter()
            .map(|(_, node)| node.zid)
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

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region))]
    fn propagate_queryable(
        &mut self,
        ctx: BaseContext,
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

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region))]
    fn unpropagate_queryable(&mut self, ctx: BaseContext, res: Arc<Resource>) {
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
            self.propagate_forget_sourced_queryable(ctx.tables, &res, None, &ctx.tables.zid);
        }
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region), ret)]
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

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region), ret)]
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

impl Hat {
    #[inline]
    fn insert_faces_for_qbls(
        &self,
        route: &mut HashMap<usize, Arc<FaceState>>,
        tables: &TablesData,
        net: &Network,
        qbls: &HashMap<ZenohIdProto, QueryableInfoType>,
        complete: bool,
    ) {
        let source = net.idx.index();
        if net.trees.len() > source {
            for qbl in qbls {
                if complete && !qbl.1.complete {
                    continue;
                }
                if let Some(qbl_idx) = net.get_idx(qbl.0) {
                    if net.trees[source].directions.len() > qbl_idx.index() {
                        if let Some(direction) = net.trees[source].directions[qbl_idx.index()] {
                            if net.graph.contains_node(direction) {
                                if let Some(face) = self.face(tables, &net.graph[direction].zid) {
                                    route.entry(face.id).or_insert_with(|| face.clone());
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
}
