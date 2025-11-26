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
#[allow(unused_imports)]
use zenoh_core::compat::*;
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
        interest::InterestId,
    },
};
use zenoh_sync::get_mut_unchecked;

use super::Hat;
use crate::{
    key_expr::KeyExpr,
    net::routing::{
        dispatcher::{
            face::FaceState,
            queries::{get_remote_qabl_info, merge_qabl_infos, update_queryable_info},
            region::RegionMap,
            resource::{Direction, FaceContext, NodeId, Resource, DEFAULT_NODE_ID},
            tables::{QueryTargetQabl, QueryTargetQablSet, RoutingExpr, TablesData},
        },
        hat::{
            peer::{initial_interest, INITIAL_INTEREST_ID},
            BaseContext, HatBaseTrait, HatQueriesTrait, HatTrait, SendDeclare, Sources,
        },
        RoutingContext,
    },
};

impl Hat {
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
    fn maybe_unpropagate_queryable(
        &self,
        face: &mut Arc<FaceState>,
        res: &Arc<Resource>,
        send_declare: &mut SendDeclare,
    ) {
        for update in self
            .face_hat_mut(face)
            .local_qabls
            .remove_simple_resource(res)
        {
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
                                ext_wire_expr: WireExprType::null(),
                            }),
                        },
                        update.resource.expr().to_string(),
                    ),
                ),
            };
        }
    }

    fn get_queryables_matching_resource<'a>(
        &self,
        tables: &'a TablesData,
        face: &Arc<FaceState>,
        res: Option<&'a Arc<Resource>>,
    ) -> Vec<&'a Arc<Resource>> {
        let face_id = face.id;

        tables
            .faces
            .values()
            .filter(move |f| f.id != face_id)
            .flat_map(|f| self.face_hat(f).remote_qabls.values())
            .map(|(qabl, _)| qabl)
            .filter(move |qabl| qabl.ctx.is_some() && res.map(|r| qabl.matches(r)).unwrap_or(true))
            .collect()
    }

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
    fn propagate_simple_queryable_to(
        &self,
        ctx: BaseContext,
        src_face: Option<&Arc<FaceState>>,
        dst_face: &mut Arc<FaceState>,
        res: &Arc<Resource>,
    ) {
        if src_face
            .as_ref()
            .map(|src_face| {
                dst_face.id != src_face.id
                    && (src_face.whatami == WhatAmI::Client || dst_face.whatami == WhatAmI::Client)
            })
            .unwrap_or(true)
        {
            if dst_face.whatami != WhatAmI::Client {
                self.maybe_register_local_queryable(
                    ctx.tables,
                    dst_face,
                    res,
                    Some(INITIAL_INTEREST_ID),
                    ctx.send_declare,
                );
            } else {
                self.maybe_register_local_queryable(
                    ctx.tables,
                    dst_face,
                    res,
                    None,
                    ctx.send_declare,
                );
            }
        }
    }

    fn propagate_simple_queryable(
        &self,
        mut ctx: BaseContext,
        src_face: Option<&Arc<FaceState>>,
        res: &Arc<Resource>,
    ) {
        let faces = self
            .faces(ctx.tables)
            .values()
            .cloned()
            .collect::<Vec<Arc<FaceState>>>();
        for mut dst_face in faces {
            self.propagate_simple_queryable_to(ctx.reborrow(), src_face, &mut dst_face, res);
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
            .insert(id, (res.clone(), *qabl_info));
    }

    fn declare_simple_queryable(
        &self,
        mut ctx: BaseContext,
        id: QueryableId,
        res: &mut Arc<Resource>,
        qabl_info: &QueryableInfoType,
    ) {
        self.register_simple_queryable(ctx.reborrow(), id, res, qabl_info);
        let src_face = ctx.src_face.clone();
        self.propagate_simple_queryable(ctx, Some(&src_face), res);
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
            self.maybe_unpropagate_queryable(face, res, send_declare);
        }
    }

    pub(super) fn undeclare_simple_queryable(&self, mut ctx: BaseContext, res: &mut Arc<Resource>) {
        let remote_qabl_info =
            get_remote_qabl_info(&self.face_hat_mut(ctx.src_face).remote_qabls, res);

        if update_queryable_info(res, ctx.src_face.id, &remote_qabl_info) {
            let mut simple_qabls = self.simple_qabls(res);
            if simple_qabls.is_empty() {
                self.propagate_forget_simple_queryable(ctx.tables, res, ctx.send_declare);
            } else {
                self.propagate_simple_queryable(ctx.reborrow(), None, res);
            }
            if simple_qabls.len() == 1 {
                self.maybe_unpropagate_queryable(&mut simple_qabls[0], res, ctx.send_declare);
            }
        }
    }

    fn forget_simple_queryable(&self, ctx: BaseContext, id: QueryableId) -> Option<Arc<Resource>> {
        if let Some((mut res, _)) = self.face_hat_mut(ctx.src_face).remote_qabls.remove(&id) {
            self.undeclare_simple_queryable(ctx, &mut res);
            Some(res)
        } else {
            None
        }
    }

    pub(super) fn queries_new_face(&self, ctx: BaseContext, other_hats: &RegionMap<&dyn HatTrait>) {
        if ctx.src_face.region.bound().is_south() {
            tracing::trace!(face = %ctx.src_face, "New south-bound peer remote. Not propagating queryables");
            return;
        }

        let qabls = other_hats
            .values()
            .flat_map(|hat| hat.remote_queryables(ctx.tables))
            .fold(HashMap::new(), |mut acc, (res, info)| {
                acc.entry(res.clone())
                    .and_modify(|i| {
                        *i = merge_qabl_infos(*i, info);
                    })
                    .or_insert(info);
                acc
            });

        for (res, info) in qabls {
            // FIXME(regions): We always propagate entities in this codepath; the method name is misleading
            self.maybe_propagate_queryable(&res, &info, ctx.src_face, ctx.send_declare);
        }
    }

    fn maybe_propagate_queryable(
        &self,
        res: &Arc<Resource>,
        info: &QueryableInfoType,
        dst_face: &mut Arc<FaceState>,
        send_declare: &mut SendDeclare,
    ) {
        if self
            .face_hat(dst_face)
            .local_qabls
            .contains_simple_resource(res)
        {
            return;
        };

        // FIXME(regions): it's not because of initial interest that we push subscribers to north-bound peers.
        // Initial interest only exists to track current declarations at startup. This code is misleading.
        // See 20a95fb.
        let initial_interest = dst_face
            .region
            .bound()
            .is_north()
            .then_some(INITIAL_INTEREST_ID);

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

        let face_hat_mut = self.face_hat_mut(dst_face);
        let (_, qabls_to_notify) = face_hat_mut.local_qabls.insert_simple_resource(
            res.clone(),
            *info,
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
}

impl HatQueriesTrait for Hat {
    fn declare_queryable(
        &mut self,
        ctx: BaseContext,
        id: QueryableId,
        res: &mut Arc<Resource>,
        _node_id: NodeId,
        qabl_info: &QueryableInfoType,
    ) {
        // TODO(regions2): clients of this peer are handled as if they were bound to a future broker south hat
        self.declare_simple_queryable(ctx, id, res, qabl_info);
    }

    fn undeclare_queryable(
        &mut self,
        ctx: BaseContext,
        id: QueryableId,
        _res: Option<Arc<Resource>>,
        _node_id: NodeId,
    ) -> Option<Arc<Resource>> {
        self.forget_simple_queryable(ctx, id)
    }

    fn get_queryables(&self, tables: &TablesData) -> Vec<(Arc<Resource>, Sources)> {
        // Compute the list of known queryables (keys)
        let mut qabls = HashMap::new();
        for src_face in self.faces(tables).values() {
            for (qabl, _) in self.face_hat(src_face).remote_qabls.values() {
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
            for face_ctx @ (_, ctx) in self.owned_face_contexts(&mres) {
                if !self.owns(src_face) {
                    if let Some(qabl) = QueryTargetQabl::new(face_ctx, expr, complete, &self.region)
                    {
                        tracing::trace!(dst = %ctx.face, reason = "resource match");
                        route.push(qabl);
                    }
                }
            }
        }

        if src_face.region.bound().is_south() {
            // TODO: BestMatching: What if there is a local compete ?
            for face in self.owned_faces(tables) {
                if face.remote_bound.is_south() {
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
                                node_id: DEFAULT_NODE_ID,
                            },
                            info: None,
                            region: self.region,
                        });
                    }
                } else if initial_interest(face).is_some_and(|i| !i.finalized) {
                    tracing::trace!(dst = %face, reason = "unfinalized initial interest");
                    let wire_expr = expr.get_best_key(face.id);
                    route.push(QueryTargetQabl {
                        dir: Direction {
                            dst_face: face.clone(),
                            wire_expr: wire_expr.to_owned(),
                            node_id: DEFAULT_NODE_ID,
                        },
                        info: None,
                        region: self.region,
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

    #[tracing::instrument(level = "trace", skip_all)]
    fn register_queryable(
        &mut self,
        ctx: BaseContext,
        id: QueryableId,
        mut res: Arc<Resource>,
        _nid: NodeId,
        info: &QueryableInfoType,
    ) {
        debug_assert!(self.owns(ctx.src_face));

        {
            let res = get_mut_unchecked(&mut res);
            match res.face_ctxs.get_mut(&ctx.src_face.id) {
                Some(ctx) => {
                    if ctx.qabl.is_none() {
                        get_mut_unchecked(ctx).qabl = Some(*info);
                    }
                }
                None => {
                    let ctx = res
                        .face_ctxs
                        .entry(ctx.src_face.id)
                        .or_insert_with(|| Arc::new(FaceContext::new(ctx.src_face.clone())));
                    get_mut_unchecked(ctx).qabl = Some(*info);
                }
            }
        }

        self.face_hat_mut(ctx.src_face)
            .remote_qabls
            .insert(id, (res.clone(), *info));
    }

    #[tracing::instrument(level = "trace", skip_all)]
    fn unregister_queryable(
        &mut self,
        ctx: BaseContext,
        id: QueryableId,
        _res: Option<Arc<Resource>>,
        _nid: NodeId,
    ) -> Option<Arc<Resource>> {
        let Some(qabl) = self.face_hat_mut(ctx.src_face).remote_qabls.remove(&id) else {
            tracing::error!(id, "Unknown queryable");
            return None;
        };

        if self
            .face_hat(ctx.src_face)
            .remote_qabls
            .values()
            .contains(&qabl)
        {
            tracing::debug!(id, res = ?qabl.0, info = ?qabl.1, "Duplicated queryable");
            return None;
        };

        let (mut res, _) = qabl;

        if let Some(ctx) = get_mut_unchecked(&mut res)
            .face_ctxs
            .get_mut(&ctx.src_face.id)
        {
            get_mut_unchecked(ctx).qabl = None;
        }

        Some(res)
    }

    #[tracing::instrument(level = "trace", skip_all)]
    fn unregister_face_queryables(&mut self, ctx: BaseContext) -> HashSet<Arc<Resource>> {
        debug_assert!(self.owns(ctx.src_face));

        let fid = ctx.src_face.id;

        self.face_hat_mut(ctx.src_face)
            .remote_qabls
            .drain()
            .map(|(_, (mut res, _))| {
                if let Some(ctx) = get_mut_unchecked(&mut res).face_ctxs.get_mut(&fid) {
                    get_mut_unchecked(ctx).qabl = None;
                }

                res
            })
            .collect()
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region))]
    fn propagate_queryable(
        &mut self,
        ctx: BaseContext,
        res: Arc<Resource>,
        other_info: Option<QueryableInfoType>,
    ) {
        let Some(other_info) = other_info else {
            debug_assert!(self.owns(ctx.src_face));
            return;
        };

        for dst_face in self.owned_faces_mut(ctx.tables) {
            self.maybe_propagate_queryable(&res, &other_info, dst_face, ctx.send_declare);
        }
    }

    #[tracing::instrument(level = "trace", skip_all)]
    fn unpropagate_queryable(&mut self, ctx: BaseContext, res: Arc<Resource>) {
        for mut face in self.owned_faces(ctx.tables).cloned() {
            self.maybe_unpropagate_queryable(&mut face, &res, ctx.send_declare);
        }
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region), ret)]
    fn remote_queryables_of(&self, res: &Resource) -> Option<QueryableInfoType> {
        self.owned_face_contexts(res)
            .filter_map(|(_, ctx)| ctx.qabl)
            .reduce(merge_qabl_infos)
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region), ret)]
    fn remote_queryables(&self, tables: &TablesData) -> HashMap<Arc<Resource>, QueryableInfoType> {
        self.owned_faces(tables)
            .flat_map(|face| self.face_hat(face).remote_qabls.values())
            .cloned()
            .collect()
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region), ret)]
    fn remote_queryables_matching(
        &self,
        tables: &TablesData,
        res: Option<&Resource>,
    ) -> HashMap<Arc<Resource>, QueryableInfoType> {
        self.owned_faces(tables)
            .flat_map(|f| self.face_hat(f).remote_qabls.values())
            .filter(|(qabl, _)| res.is_none_or(|res| res.matches(qabl)))
            .fold(HashMap::new(), |mut acc, (res, info)| {
                acc.entry(res.clone())
                    .and_modify(|i| {
                        *i = merge_qabl_infos(*i, *info);
                    })
                    .or_insert(*info);
                acc
            })
    }
}
