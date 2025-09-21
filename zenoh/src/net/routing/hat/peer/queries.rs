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
    net::routing::{
        dispatcher::{
            face::FaceState,
            resource::{FaceContext, NodeId, Resource},
            tables::{QueryTargetQabl, QueryTargetQablSet, RoutingExpr, TablesData},
        },
        hat::{
            peer::initial_interest, BaseContext, CurrentFutureTrait, HatQueriesTrait,
            InterestProfile, SendDeclare, Sources,
        },
        router::Direction,
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

    #[inline]
    fn propagate_simple_queryable_to(
        &self,
        ctx: BaseContext,
        src_face: Option<&Arc<FaceState>>,
        dst_face: &mut Arc<FaceState>,
        res: &Arc<Resource>,
        profile: InterestProfile,
    ) {
        let info = self.local_qabl_info(ctx.tables, res, dst_face);
        let current = self.face_hat(dst_face).local_qabls.get(res);
        if src_face
            .as_ref()
            .map(|src_face| dst_face.id != src_face.id)
            .unwrap_or(true)
            && (current.is_none() || current.unwrap().1 != info)
            && ((dst_face.whatami != WhatAmI::Client && profile.is_push())
                || self
                    .face_hat(dst_face)
                    .remote_interests
                    .values()
                    .any(|i| i.options.queryables() && i.matches(res)))
            && src_face
                .as_ref()
                .map(|src_face| {
                    src_face.whatami == WhatAmI::Client || dst_face.whatami == WhatAmI::Client
                })
                .unwrap_or(true)
        {
            let id = current.map(|c| c.0).unwrap_or(
                self.face_hat(dst_face)
                    .next_id
                    .fetch_add(1, Ordering::SeqCst),
            );
            self.face_hat_mut(dst_face)
                .local_qabls
                .insert(res.clone(), (id, info));
            let key_expr =
                Resource::decl_key(res, dst_face, super::push_declaration_profile(dst_face));
            (ctx.send_declare)(
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

    fn propagate_simple_queryable(
        &self,
        mut ctx: BaseContext,
        src_face: Option<&Arc<FaceState>>,
        res: &Arc<Resource>,
        profile: InterestProfile,
    ) {
        let faces = self
            .faces(ctx.tables)
            .values()
            .cloned()
            .collect::<Vec<Arc<FaceState>>>();
        for mut dst_face in faces {
            self.propagate_simple_queryable_to(
                ctx.reborrow(),
                src_face,
                &mut dst_face,
                res,
                profile,
            );
        }
    }

    fn register_simple_queryable(
        &self,
        ctx: BaseContext,
        id: QueryableId,
        res: &mut Arc<Resource>,
        qabl_info: &QueryableInfoType,
    ) {
        // Register queryable
        {
            let res = get_mut_unchecked(res);
            get_mut_unchecked(
                res.face_ctxs
                    .entry(ctx.src_face.id)
                    .or_insert_with(|| Arc::new(FaceContext::new(ctx.src_face.clone()))),
            )
            .qabl = Some(*qabl_info);
        }
        self.face_hat_mut(ctx.src_face)
            .remote_qabls
            .insert(id, res.clone());
    }

    fn declare_simple_queryable(
        &self,
        mut ctx: BaseContext,
        id: QueryableId,
        res: &mut Arc<Resource>,
        qabl_info: &QueryableInfoType,
        profile: InterestProfile,
    ) {
        self.register_simple_queryable(ctx.reborrow(), id, res, qabl_info);
        let src_face = ctx.src_face.clone();
        self.propagate_simple_queryable(ctx, Some(&src_face), res, profile);
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
    fn remote_simple_qabls(&self, res: &Arc<Resource>, face: &Arc<FaceState>) -> bool {
        res.face_ctxs
            .values()
            .any(|ctx| ctx.face.id != face.id && ctx.qabl.is_some())
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
            for res in self
                .face_hat(face)
                .local_qabls
                .keys()
                .cloned()
                .collect::<Vec<Arc<Resource>>>()
            {
                if !res.context().matches.iter().any(|m| {
                    m.upgrade()
                        .is_some_and(|m| m.ctx.is_some() && self.remote_simple_qabls(&m, face))
                }) {
                    if let Some((id, _)) = self.face_hat_mut(face).local_qabls.remove(&res) {
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
        }
    }

    pub(super) fn undeclare_simple_queryable(
        &self,
        mut ctx: BaseContext,
        res: &mut Arc<Resource>,
        profile: InterestProfile,
    ) {
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
                self.propagate_simple_queryable(ctx.reborrow(), None, res, profile);
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
                for res in self
                    .face_hat(face)
                    .local_qabls
                    .keys()
                    .cloned()
                    .collect::<Vec<Arc<Resource>>>()
                {
                    if !res.context().matches.iter().any(|m| {
                        m.upgrade().is_some_and(|m| {
                            m.ctx.is_some() && (self.remote_simple_qabls(&m, face))
                        })
                    }) {
                        if let Some((id, _)) = self.face_hat_mut(face).local_qabls.remove(&res) {
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
        }
    }

    fn forget_simple_queryable(
        &self,
        ctx: BaseContext,
        id: QueryableId,
        profile: InterestProfile,
    ) -> Option<Arc<Resource>> {
        if let Some(mut res) = self.face_hat_mut(ctx.src_face).remote_qabls.remove(&id) {
            self.undeclare_simple_queryable(ctx, &mut res, profile);
            Some(res)
        } else {
            None
        }
    }

    pub(super) fn queries_new_face(&self, mut ctx: BaseContext, profile: InterestProfile) {
        if ctx.src_face.whatami != WhatAmI::Client {
            for face in self
                .faces(ctx.tables)
                .values()
                .cloned()
                .collect::<Vec<Arc<FaceState>>>()
            {
                for qabl in self.face_hat(&face).remote_qabls.values() {
                    let dst_face = &mut ctx.src_face.clone();
                    self.propagate_simple_queryable_to(
                        ctx.reborrow(),
                        Some(&face.clone()), // src
                        dst_face,            // dst
                        qabl,
                        profile,
                    );
                }
            }
        }
    }

    #[inline]
    fn make_qabl_id(
        &self,
        res: &Arc<Resource>,
        face: &mut Arc<FaceState>,
        mode: InterestMode,
        info: QueryableInfoType,
    ) -> u32 {
        if mode.future() {
            if let Some((id, _)) = self.face_hat(face).local_qabls.get(res) {
                *id
            } else {
                let id = self.face_hat(face).next_id.fetch_add(1, Ordering::SeqCst);
                self.face_hat_mut(face)
                    .local_qabls
                    .insert(res.clone(), (id, info));
                id
            }
        } else {
            0
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn declare_qabl_interest(
        &self,
        tables: &mut TablesData,
        face: &mut Arc<FaceState>,
        id: InterestId,
        res: Option<&mut Arc<Resource>>,
        mode: InterestMode,
        aggregate: bool,
        send_declare: &mut SendDeclare,
    ) {
        if mode.current() && face.whatami == WhatAmI::Client {
            let interest_id = Some(id);
            if let Some(res) = res.as_ref() {
                if aggregate {
                    if self.faces(tables).values().any(|src_face| {
                        src_face.id != face.id
                            && self
                                .face_hat(src_face)
                                .remote_qabls
                                .values()
                                .any(|qabl| qabl.ctx.is_some() && qabl.matches(res))
                    }) {
                        let info = self.local_qabl_info(tables, res, face);
                        let id = self.make_qabl_id(res, face, mode, info);
                        let wire_expr =
                            Resource::decl_key(res, face, super::push_declaration_profile(face));
                        send_declare(
                            &face.primitives,
                            RoutingContext::with_expr(
                                Declare {
                                    interest_id,
                                    ext_qos: declare::ext::QoSType::DECLARE,
                                    ext_tstamp: None,
                                    ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                                    body: DeclareBody::DeclareQueryable(DeclareQueryable {
                                        id,
                                        wire_expr,
                                        ext_info: info,
                                    }),
                                },
                                res.expr().to_string(),
                            ),
                        );
                    }
                } else {
                    for src_face in self.faces(tables).values().cloned().collect::<Vec<_>>() {
                        if src_face.id != face.id {
                            for qabl in self.face_hat(&src_face).remote_qabls.values() {
                                if qabl.ctx.is_some() && qabl.matches(res) {
                                    let info = self.local_qabl_info(tables, qabl, face);
                                    let id = self.make_qabl_id(qabl, face, mode, info);
                                    let key_expr = Resource::decl_key(
                                        qabl,
                                        face,
                                        super::push_declaration_profile(face),
                                    );
                                    send_declare(
                                        &face.primitives,
                                        RoutingContext::with_expr(
                                            Declare {
                                                interest_id,
                                                ext_qos: declare::ext::QoSType::DECLARE,
                                                ext_tstamp: None,
                                                ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                                                body: DeclareBody::DeclareQueryable(
                                                    DeclareQueryable {
                                                        id,
                                                        wire_expr: key_expr,
                                                        ext_info: info,
                                                    },
                                                ),
                                            },
                                            qabl.expr().to_string(),
                                        ),
                                    );
                                }
                            }
                        }
                    }
                }
            } else {
                for src_face in self
                    .faces(tables)
                    .values()
                    .filter(|f| f.id != face.id)
                    .cloned()
                    .collect::<Vec<Arc<FaceState>>>()
                {
                    for qabl in self.face_hat(&src_face).remote_qabls.values() {
                        if qabl.ctx.is_some() {
                            let info = self.local_qabl_info(tables, qabl, face);
                            let id = self.make_qabl_id(qabl, face, mode, info);
                            let key_expr = Resource::decl_key(
                                qabl,
                                face,
                                super::push_declaration_profile(face),
                            );
                            send_declare(
                                &face.primitives,
                                RoutingContext::with_expr(
                                    Declare {
                                        interest_id,
                                        ext_qos: declare::ext::QoSType::DECLARE,
                                        ext_tstamp: None,
                                        ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                                        body: DeclareBody::DeclareQueryable(DeclareQueryable {
                                            id,
                                            wire_expr: key_expr,
                                            ext_info: info,
                                        }),
                                    },
                                    qabl.expr().to_string(),
                                ),
                            );
                        }
                    }
                }
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
        _node_id: NodeId,
        qabl_info: &QueryableInfoType,
        profile: InterestProfile,
    ) {
        // TODO(regions2): clients of this peer are handled as if they were bound to a future broker south hat
        self.declare_simple_queryable(ctx, id, res, qabl_info, profile);
    }

    fn undeclare_queryable(
        &mut self,
        ctx: BaseContext,
        id: QueryableId,
        _res: Option<Arc<Resource>>,
        _node_id: NodeId,
        profile: InterestProfile,
    ) -> Option<Arc<Resource>> {
        self.forget_simple_queryable(ctx, id, profile)
    }

    fn get_queryables(&self, tables: &TablesData) -> Vec<(Arc<Resource>, Sources)> {
        // Compute the list of known queryables (keys)
        let mut qabls = HashMap::new();
        for src_face in self.faces(tables).values() {
            for qabl in self.face_hat(src_face).remote_qabls.values() {
                // Insert the key in the list of known queryables
                let srcs = qabls.entry(qabl.clone()).or_insert_with(Sources::empty);
                // Append src_face as a queryable source in the proper list
                let whatami = if src_face.is_local {
                    tables.hats.north().whatami // REVIEW(fuzzypixelz)
                } else {
                    src_face.whatami
                };
                match whatami {
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

    #[tracing::instrument(level = "trace", skip_all, fields(expr = ?expr))]
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
            for face_ctx @ (_, ctx) in self.owned_face_contexts(&mres) {
                // REVIEW(regions): not sure
                if src_face.bound.is_north() ^ ctx.face.bound.is_north()
                    || src_face.whatami == WhatAmI::Client
                    || ctx.face.whatami == WhatAmI::Client
                {
                    if let Some(qabl) = QueryTargetQabl::new(face_ctx, expr, complete, &self.bound)
                    {
                        tracing::trace!(dst = %ctx.face, reason = "resource match");
                        route.push(qabl);
                    }
                }
            }
        }

        if !src_face.bound.is_north() {
            // TODO: BestMatching: What if there is a local compete ?
            for face in self.faces(tables).values() {
                if self.face_hat(face).is_gateway {
                    let has_interest_finalized = expr
                        .resource()
                        .and_then(|res| res.face_ctxs.get(&face.id))
                        .is_some_and(|ctx| ctx.queryable_interest_finalized);
                    if !has_interest_finalized {
                        tracing::trace!(dst = %face, reason = "unfinalized queryable interest");
                        let wire_expr = expr.get_best_key(face.id);
                        route.push(QueryTargetQabl {
                            dir: Direction {
                                dst_face: face.clone(),
                                wire_expr: wire_expr.to_owned(),
                                node_id: NodeId::default(),
                            },
                            info: None,
                            bound: self.bound,
                        });
                    }
                } else if face.whatami == WhatAmI::Peer
                    && face.bound.is_north() // REVIEW(regions): not sure
                    && initial_interest(face).is_some_and(|i| !i.finalized)
                {
                    tracing::trace!(dst = %face, reason = "unfinalized initial interest");
                    let wire_expr = expr.get_best_key(face.id);
                    route.push(QueryTargetQabl {
                        dir: Direction {
                            dst_face: face.clone(),
                            wire_expr: wire_expr.to_owned(),
                            node_id: NodeId::default(),
                        },
                        info: None,
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
            for (fid, ctx) in &mres.face_ctxs {
                if match complete {
                    true => ctx.qabl.is_some_and(|q| q.complete),
                    false => ctx.qabl.is_some(),
                } {
                    matching_queryables
                        .entry(*fid)
                        .or_insert_with(|| ctx.face.clone());
                }
            }
        }
        matching_queryables
    }
}
