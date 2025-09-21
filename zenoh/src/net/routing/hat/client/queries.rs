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
    collections::HashMap,
    sync::{atomic::Ordering, Arc},
};

use zenoh_protocol::{
    core::{
        key_expr::include::{Includer, DEFAULT_INCLUDER},
        WhatAmI,
    },
    network::declare::{
        self, common::ext::WireExprType, queryable::ext::QueryableInfoType, Declare, DeclareBody,
        DeclareQueryable, QueryableId, UndeclareQueryable,
    },
};
use zenoh_sync::get_mut_unchecked;

use super::Hat;
use crate::{
    key_expr::KeyExpr,
    net::routing::{
        dispatcher::{
            face::FaceState,
            resource::{FaceContext, NodeId, Resource},
            tables::{QueryTargetQabl, QueryTargetQablSet, RoutingExpr, TablesData},
        },
        hat::{BaseContext, HatQueriesTrait, InterestProfile, SendDeclare, Sources},
        router::{Direction, DEFAULT_NODE_ID},
        RoutingContext,
    },
};

#[inline]
fn merge_qabl_infos(mut this: QueryableInfoType, info: &QueryableInfoType) -> QueryableInfoType {
    this.complete = this.complete || info.complete;
    this.distance = std::cmp::min(this.distance, info.distance);
    this
}

impl Hat {
    fn local_qabl_info(
        &self,
        _tables: &TablesData,
        res: &Arc<Resource>,
        face: &Arc<FaceState>,
    ) -> QueryableInfoType {
        res.face_ctxs
            .values()
            .fold(None, |accu, ctx| {
                if ctx.face.id != face.id {
                    if let Some(info) = ctx.qabl.as_ref() {
                        Some(match accu {
                            Some(accu) => merge_qabl_infos(accu, info),
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

    fn propagate_simple_queryable(
        &self,
        tables: &mut TablesData,
        res: &Arc<Resource>,
        src_face: Option<&mut Arc<FaceState>>,
        send_declare: &mut SendDeclare,
    ) {
        let faces = self.faces(tables).values().cloned();
        for mut dst_face in faces {
            let info = self.local_qabl_info(tables, res, &dst_face);
            let current = self.face_hat(&dst_face).local_qabls.get(res);
            if src_face
                .as_ref()
                .map(|src_face| dst_face.id != src_face.id)
                .unwrap_or(true)
                && (current.is_none() || current.unwrap().1 != info)
                && src_face
                    .as_ref()
                    .map(|src_face| {
                        src_face.whatami == WhatAmI::Client || dst_face.whatami == WhatAmI::Client
                    })
                    .unwrap_or(true)
            {
                let id = current.map(|c| c.0).unwrap_or(
                    self.face_hat(&dst_face)
                        .next_id
                        .fetch_add(1, Ordering::SeqCst),
                );
                self.face_hat_mut(&mut dst_face)
                    .local_qabls
                    .insert(res.clone(), (id, info));
                let key_expr = Resource::decl_key(res, &mut dst_face, true);
                send_declare(
                    &dst_face.primitives,
                    RoutingContext::with_expr(
                        Declare {
                            interest_id: None,
                            ext_qos: declare::ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                            body: DeclareBody::DeclareQueryable(DeclareQueryable {
                                id,
                                wire_expr: key_expr,
                                ext_info: info,
                            }),
                        },
                        res.expr().to_string(),
                    ),
                );
            }
        }
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
        self.face_hat_mut(face).remote_qabls.insert(id, res.clone());
    }

    fn declare_simple_queryable(
        &self,
        tables: &mut TablesData,
        face: &mut Arc<FaceState>,
        id: QueryableId,
        res: &mut Arc<Resource>,
        qabl_info: &QueryableInfoType,
        send_declare: &mut SendDeclare,
    ) {
        self.register_simple_queryable(tables, face, id, res, qabl_info);
        self.propagate_simple_queryable(tables, res, Some(face), send_declare);
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

    fn propagate_forget_simple_queryable(
        &self,
        tables: &mut TablesData,
        res: &mut Arc<Resource>,
        send_declare: &mut SendDeclare,
    ) {
        for face in self.faces_mut(tables).values_mut() {
            if let Some((id, _)) = self.face_hat_mut(face).local_qabls.remove(res) {
                send_declare(
                    &face.primitives,
                    RoutingContext::with_expr(
                        Declare {
                            interest_id: None,
                            ext_qos: declare::ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                            body: DeclareBody::UndeclareQueryable(UndeclareQueryable {
                                id,
                                ext_wire_expr: WireExprType::null(),
                            }),
                        },
                        res.expr().to_string(),
                    ),
                );
            }
        }
    }

    pub(super) fn undeclare_simple_queryable(&self, ctx: BaseContext, res: &mut Arc<Resource>) {
        if !self
            .face_hat_mut(ctx.src_face)
            .remote_qabls
            .values()
            .any(|s| *s == *res)
        {
            if let Some(ctx) = get_mut_unchecked(res).face_ctxs.get_mut(&ctx.src_face.id) {
                get_mut_unchecked(ctx).qabl = None;
            }

            let mut simple_qabls = self.simple_qabls(res);
            if simple_qabls.is_empty() {
                self.propagate_forget_simple_queryable(ctx.tables, res, ctx.send_declare);
            } else {
                self.propagate_simple_queryable(ctx.tables, res, None, ctx.send_declare);
            }
            if simple_qabls.len() == 1 {
                let face = &mut simple_qabls[0];
                if let Some((id, _)) = self.face_hat_mut(face).local_qabls.remove(res) {
                    (ctx.send_declare)(
                        &face.primitives,
                        RoutingContext::with_expr(
                            Declare {
                                interest_id: None,
                                ext_qos: declare::ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                                body: DeclareBody::UndeclareQueryable(UndeclareQueryable {
                                    id,
                                    ext_wire_expr: WireExprType::null(),
                                }),
                            },
                            res.expr().to_string(),
                        ),
                    );
                }
            }
        }
    }

    fn forget_simple_queryable(&self, ctx: BaseContext, id: QueryableId) -> Option<Arc<Resource>> {
        if let Some(mut res) = self.face_hat_mut(ctx.src_face).remote_qabls.remove(&id) {
            self.undeclare_simple_queryable(ctx, &mut res);
            Some(res)
        } else {
            None
        }
    }

    pub(super) fn queries_new_face(
        &self,
        tables: &mut TablesData,
        _face: &mut Arc<FaceState>,
        send_declare: &mut SendDeclare,
    ) {
        for face in self
            .faces(tables)
            .values()
            .cloned()
            .collect::<Vec<Arc<FaceState>>>()
        {
            for qabl in self.face_hat(&face).remote_qabls.values() {
                self.propagate_simple_queryable(
                    tables,
                    qabl,
                    Some(&mut face.clone()),
                    send_declare,
                );
            }
        }
    }
}

lazy_static::lazy_static! {
    static ref EMPTY_ROUTE: Arc<QueryTargetQablSet> = Arc::new(Vec::new());
}

impl HatQueriesTrait for Hat {
    fn declare_queryable(
        &mut self,
        ctx: BaseContext,
        id: QueryableId,
        res: &mut Arc<Resource>,
        _node_id: NodeId,
        qabl_info: &QueryableInfoType,
        _profile: InterestProfile,
    ) {
        // FIXME(regions): InterestProfile is ignored
        self.declare_simple_queryable(
            ctx.tables,
            ctx.src_face,
            id,
            res,
            qabl_info,
            ctx.send_declare,
        );
    }

    fn undeclare_queryable(
        &mut self,
        ctx: BaseContext,
        id: QueryableId,
        _res: Option<Arc<Resource>>,
        _node_id: NodeId,
        _profile: InterestProfile,
    ) -> Option<Arc<Resource>> {
        self.forget_simple_queryable(ctx, id)
    }

    fn get_queryables(&self, tables: &TablesData) -> Vec<(Arc<Resource>, Sources)> {
        // Compute the list of known queryables (keys)
        let mut qabls = HashMap::new();
        for src_face in self.faces(tables).values() {
            for qabl in self.face_hat(src_face).remote_qabls.values() {
                // Insert the key in the list of known queryables
                let srcs = qabls.entry(qabl.clone()).or_insert_with(Sources::empty);
                // Append src_face as a queryable source in the proper list
                match src_face.whatami {
                    WhatAmI::Router => srcs.routers.push(src_face.zid),
                    WhatAmI::Peer => srcs.peers.push(src_face.zid),
                    WhatAmI::Client => srcs.clients.push(src_face.zid),
                }
            }
        }
        Vec::from_iter(qabls)
    }

    fn get_queriers(&self, tables: &TablesData) -> Vec<(Arc<Resource>, Sources)> {
        let mut result = HashMap::new();
        for face in self.faces(tables).values() {
            for interest in self.face_hat(face).remote_interests.values() {
                if interest.options.queryables() {
                    if let Some(res) = interest.res.as_ref() {
                        let sources = result.entry(res.clone()).or_insert_with(Sources::default);
                        match face.whatami {
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

    fn compute_query_route(
        &self,
        tables: &TablesData,
        face: &FaceState,
        expr: &RoutingExpr,
        source: NodeId,
    ) -> Arc<QueryTargetQablSet> {
        let mut route = QueryTargetQablSet::new();
        let Some(key_expr) = expr.key_expr() else {
            return EMPTY_ROUTE.clone();
        };
        let source_type = face.whatami;
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
            for face_ctx @ (_, ctx) in self.owned_face_contexts(&mres) {
                // REVIEW(regions): not sure
                if !face.bound.is_north() ^ !ctx.face.bound.is_north() {
                    if let Some(qabl) = QueryTargetQabl::new(face_ctx, expr, complete, &self.bound)
                    {
                        tracing::trace!(dst = %ctx.face, reason = "resource match");
                        route.push(qabl);
                    }
                }
            }
        }

        if !face.bound.is_north() {
            // REVIEW(regions): there should only be one such face?
            for face in self.faces(tables).values().filter(|f| f.bound.is_north()) {
                let has_interest_finalized = expr
                    .resource()
                    .and_then(|res| res.face_ctxs.get(&face.id))
                    .is_some_and(|ctx| ctx.queryable_interest_finalized);
                if !has_interest_finalized {
                    tracing::trace!(dst = %face, reason = "unfinalized queryable interest");
                    let wire_expr = expr.get_best_key(face.id);
                    route.push(QueryTargetQabl {
                        info: None,
                        dir: Direction {
                            dst_face: face.clone(),
                            wire_expr: wire_expr.to_owned(),
                            node_id: DEFAULT_NODE_ID,
                            dst_node_id: DEFAULT_NODE_ID,
                        },
                        bound: self.bound,
                    });
                }
            }
        }

        route.sort_by_key(|qabl| qabl.info.map_or(u16::MAX, |i| i.distance));
        Arc::new(route)
    }

    fn get_matching_queryables(
        &self,
        tables: &TablesData,
        key_expr: &KeyExpr<'_>,
        complete: bool,
    ) -> HashMap<usize, Arc<FaceState>> {
        let mut matching_queryables = HashMap::new();
        if key_expr.ends_with('/') {
            return matching_queryables;
        }
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
            for (sid, ctx) in &mres.face_ctxs {
                if match complete {
                    true => ctx.qabl.is_some_and(|q| q.complete),
                    false => ctx.qabl.is_some(),
                } {
                    matching_queryables
                        .entry(*sid)
                        .or_insert_with(|| ctx.face.clone());
                }
            }
        }
        matching_queryables
    }
}
