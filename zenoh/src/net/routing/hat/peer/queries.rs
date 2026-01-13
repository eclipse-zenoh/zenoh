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

#[allow(unused_imports)]
use zenoh_core::polyfill::*;
use zenoh_protocol::{
    core::key_expr::include::{Includer, DEFAULT_INCLUDER},
    network::declare::{
        self, common::ext::WireExprType, queryable::ext::QueryableInfoType, Declare, DeclareBody,
        DeclareQueryable, QueryableId, UndeclareQueryable,
    },
};
use zenoh_sync::get_mut_unchecked;

use super::Hat;
use crate::net::routing::{
    dispatcher::{
        face::FaceState,
        queries::merge_qabl_infos,
        region::RegionMap,
        resource::{Direction, FaceContext, NodeId, Resource, DEFAULT_NODE_ID},
        tables::{QueryTargetQabl, QueryTargetQablSet, RoutingExpr, TablesData},
    },
    hat::{
        peer::{initial_interest, INITIAL_INTEREST_ID},
        BaseContext, HatBaseTrait, HatQueriesTrait, HatTrait, SendDeclare, Sources,
        UnregisterResult,
    },
    RoutingContext,
};

impl Hat {
    pub(super) fn queries_new_face(&self, ctx: BaseContext, other_hats: &RegionMap<&dyn HatTrait>) {
        if ctx.src_face.region.bound().is_south() {
            tracing::trace!(face = %ctx.src_face, "New south-bound peer remote; not propagating queryables");
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
}

impl HatQueriesTrait for Hat {
    #[tracing::instrument(level = "trace", skip(tables), ret)]
    fn sourced_queryables(&self, tables: &TablesData) -> Vec<(Arc<Resource>, Sources)> {
        // Compute the list of known queryables (keys)
        let mut qabls = HashMap::new();
        for face in self.owned_faces(tables) {
            for (qabl, _) in self.face_hat(face).remote_qabls.values() {
                // Insert the key in the list of known queryables
                let srcs = qabls.entry(qabl.clone()).or_insert_with(Sources::empty);
                // Append src_face as a queryables source in the proper list
                srcs.peers.push(face.zid);
            }
        }
        Vec::from_iter(qabls)
    }

    #[tracing::instrument(level = "trace", skip(tables), ret)]
    fn sourced_queriers(&self, tables: &TablesData) -> Vec<(Arc<Resource>, Sources)> {
        let mut result = HashMap::new();
        for face in self.owned_faces(tables) {
            for res in self
                .face_hat(face)
                .remote_interests
                .values()
                .filter_map(|i| {
                    if i.options.queryables() {
                        i.res.as_ref()
                    } else {
                        None
                    }
                })
            {
                result
                    .entry(res.clone())
                    .or_insert_with(Sources::default)
                    .peers
                    .push(face.zid);
            }
        }
        result.into_iter().collect()
    }

    #[tracing::instrument(level = "debug", skip(tables, _nid), fields(%src_face) ret)]
    fn compute_query_route(
        &self,
        tables: &TablesData,
        src_face: &FaceState,
        expr: &RoutingExpr,
        _nid: NodeId,
    ) -> Arc<QueryTargetQablSet> {
        lazy_static::lazy_static! {
            static ref EMPTY_ROUTE: Arc<QueryTargetQablSet> = Arc::new(Vec::new());
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
            for ctx in self.owned_face_contexts(&mres) {
                if !self.owns(src_face) {
                    if let Some(qabl) = QueryTargetQabl::new(ctx, expr, complete, &self.region) {
                        tracing::trace!(dst = %ctx.face, reason = "resource match");
                        route.push(qabl);
                    }
                }
            }
        }

        if src_face.region.bound().is_south() {
            // TODO: BestMatching: What if there is a local compete ?
            if let Some(face) = self.owned_faces(tables).find(|f| f.remote_bound.is_south()) {
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
            }

            for face in self.owned_faces(tables).filter(|f| {
                f.remote_bound.is_north() && initial_interest(f).is_some_and(|i| !i.finalized)
            }) {
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

        route.sort_by_key(|qabl| qabl.info.map_or(u16::MAX, |i| i.distance));
        Arc::new(route)
    }

    #[tracing::instrument(level = "debug", skip(ctx, _nid))]
    fn register_queryable(
        &mut self,
        ctx: BaseContext,
        id: QueryableId,
        mut res: Arc<Resource>,
        _nid: NodeId,
        info: &QueryableInfoType,
    ) {
        debug_assert!(self.owns(ctx.src_face));

        self.face_hat_mut(ctx.src_face)
            .remote_qabls
            .entry(id)
            .and_modify(|(_, old_info)| *old_info = *info)
            .or_insert_with(|| (res.clone(), *info));

        let new_face_info = self
            .face_hat(ctx.src_face)
            .remote_qabls
            .values()
            .filter_map(|(r, i)| (r == &res).then_some(*i))
            .reduce(merge_qabl_infos);

        let res = get_mut_unchecked(&mut res);
        match res.face_ctxs.get_mut(&ctx.src_face.id) {
            Some(ctx) => {
                get_mut_unchecked(ctx).qabl = new_face_info;
            }
            None => {
                let ctx = res
                    .face_ctxs
                    .entry(ctx.src_face.id)
                    .or_insert_with(|| Arc::new(FaceContext::new(ctx.src_face.clone())));
                get_mut_unchecked(ctx).qabl = new_face_info;
            }
        }
    }

    #[tracing::instrument(level = "debug", skip(ctx, _nid), ret)]
    fn unregister_queryable(
        &mut self,
        ctx: BaseContext,
        id: QueryableId,
        _res: Option<Arc<Resource>>,
        _nid: NodeId,
    ) -> UnregisterResult {
        debug_assert!(self.owns(ctx.src_face));

        let Some((mut res, info)) = self.face_hat_mut(ctx.src_face).remote_qabls.remove(&id) else {
            tracing::error!(id, "Unknown id");
            return UnregisterResult::Noop;
        };

        if self
            .face_hat(ctx.src_face)
            .remote_qabls
            .values()
           // REVIEW(regions): use Arc::ptr_eq?
            .any(|(r, i)| r == &res && i == &info)
        {
            tracing::debug!("Duplicated queryable");
            return UnregisterResult::Noop;
        };

        let new_face_info = self
            .face_hat(ctx.src_face)
            .remote_qabls
            .values()
            .filter_map(|(r, i)| (r == &res).then_some(*i))
            .reduce(merge_qabl_infos);

        let Some(face_ctx) = get_mut_unchecked(&mut res)
            .face_ctxs
            .get_mut(&ctx.src_face.id)
        else {
            bug!("Undefined face context");
            return UnregisterResult::Noop;
        };

        let Some(old_face_info) = face_ctx.qabl else {
            bug!("Undefined queryable info");
            return UnregisterResult::Noop;
        };

        if new_face_info == Some(old_face_info) {
            return UnregisterResult::Noop;
        } else {
            get_mut_unchecked(face_ctx).qabl = new_face_info;
        }

        let other_info = self
            .owned_face_contexts(&res)
            .filter(|face_ctx| face_ctx.face.id != ctx.src_face.id)
            .filter_map(|face_ctx| face_ctx.qabl)
            .reduce(merge_qabl_infos);

        let old_info = other_info
            .map(|i| merge_qabl_infos(i, old_face_info))
            .unwrap_or(old_face_info);

        let new_info = other_info
            .into_iter()
            .chain(new_face_info.into_iter())
            .reduce(merge_qabl_infos);

        match new_info {
            Some(new_info) => {
                if new_info == old_info {
                    UnregisterResult::Noop
                } else {
                    UnregisterResult::InfoUpdate { res }
                }
            }
            None => UnregisterResult::LastUnregistered { res },
        }
    }

    #[tracing::instrument(level = "debug", skip(ctx), ret)]
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

    #[tracing::instrument(level = "trace", skip(ctx), ret)]
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

    #[tracing::instrument(level = "trace", skip(ctx))]
    fn unpropagate_queryable(&mut self, ctx: BaseContext, res: Arc<Resource>) {
        if self.owns(ctx.src_face) {
            return;
        }

        for mut face in self.owned_faces(ctx.tables).cloned() {
            self.maybe_unpropagate_queryable(&mut face, &res, ctx.send_declare);
        }
    }

    #[tracing::instrument(level = "trace", ret)]
    fn remote_queryables_of(&self, res: &Resource) -> Option<QueryableInfoType> {
        self.owned_face_contexts(res)
            .filter_map(|ctx| ctx.qabl)
            .reduce(merge_qabl_infos)
    }

    #[tracing::instrument(level = "trace", ret)]
    fn remote_queryables(&self, tables: &TablesData) -> HashMap<Arc<Resource>, QueryableInfoType> {
        self.owned_faces(tables)
            .flat_map(|face| self.face_hat(face).remote_qabls.values())
            .cloned()
            .collect()
    }

    #[allow(clippy::incompatible_msrv)]
    #[tracing::instrument(level = "trace", skip(tables), ret)]
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
