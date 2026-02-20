//
// Copyright (c) 2025 ZettaScale Technology
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
use zenoh_core::polyfill::*;
use zenoh_keyexpr::{
    include::{Includer, DEFAULT_INCLUDER},
    keyexpr,
};
use zenoh_protocol::network::{
    declare::{self, common::ext::WireExprType, queryable::ext::QueryableInfoType, QueryableId},
    Declare, DeclareBody, DeclareQueryable, UndeclareQueryable,
};
use zenoh_sync::get_mut_unchecked;

use super::Hat;
use crate::{
    net::routing::{
        dispatcher::{
            face::FaceState,
            queries::merge_qabl_infos,
            region::RegionMap,
            resource::{NodeId, Resource},
            tables::{QueryTargetQablSet, RoutingExpr, TablesData},
        },
        hat::{
            DispatcherContext, HatBaseTrait, HatQueriesTrait, HatTrait, SendDeclare, Sources,
            UnregisterResult,
        },
        router::{FaceContext, QueryTargetQabl},
        RoutingContext,
    },
    sample::Locality,
};

lazy_static::lazy_static! {
    static ref EMPTY_ROUTE: Arc<QueryTargetQablSet> = Arc::new(Vec::new());
}

impl Hat {
    fn maybe_propagate_queryable(
        &self,
        res: &Arc<Resource>,
        info: &QueryableInfoType,
        dst: &mut Arc<FaceState>,
        send_declare: &mut SendDeclare,
    ) {
        let (should_notify, simple_interests) = self
            .face_hat(dst)
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
            );

        if !should_notify {
            return;
        }

        let face_hat_mut = self.face_hat_mut(dst);
        let (_, qabls_to_notify) = face_hat_mut.local_qabls.insert_simple_resource(
            res.clone(),
            *info,
            || face_hat_mut.next_id.fetch_add(1, Ordering::SeqCst),
            simple_interests,
        );

        for update in qabls_to_notify {
            // TODO(regions*): add this everywhere else
            tracing::trace!(%dst);
            let key_expr = Resource::decl_key(&update.resource, dst);
            send_declare(
                &dst.primitives,
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

    #[tracing::instrument(level = "debug", skip(tables, other_hats), ret)]
    pub(crate) fn remote_queryable_matching_status(
        &self,
        tables: &TablesData,
        src_face: &FaceState,
        other_hats: RegionMap<&dyn HatTrait>,
        locality: Locality,
        key_expr: &keyexpr,
        complete: bool,
    ) -> bool {
        debug_assert!(self.owns(src_face));

        let Some(res) = Resource::get_resource(&tables.root_res, key_expr) else {
            tracing::error!(keyexpr = %key_expr, "Unknown matching status resource");
            return false;
        };

        // REVIEW(regions): queryable resource *should* be non-root (?)
        let is_matching_res = |qabl: &Resource| qabl.as_keyexpr().includes(key_expr);
        let is_matching_info = |info: &QueryableInfoType| !complete || info.complete;

        let is_matching = |qabl: &Resource, info: &QueryableInfoType| {
            is_matching_res(qabl) && is_matching_info(info)
        };

        let compute_other_matches = || {
            other_hats
                .values()
                .flat_map(|hat| {
                    hat.remote_queryables_matching(tables, Some(&res))
                        .into_iter()
                })
                .any(|(qabl, info)| is_matching(&qabl, &info))
        };

        match locality {
            Locality::SessionLocal => self
                .face_hat(src_face)
                .remote_qabls
                .values()
                .any(|(qabl, info)| is_matching(qabl, info)),
            Locality::Remote => {
                self.owned_faces(tables)
                    .filter(|f| f.id != src_face.id)
                    .flat_map(|f| self.face_hat(f).remote_qabls.values())
                    .any(|(qabl, info)| is_matching(qabl, info))
                    || compute_other_matches()
            }
            Locality::Any => {
                self.owned_faces(tables)
                    .flat_map(|f| self.face_hat(f).remote_qabls.values())
                    .any(|(qabl, info)| is_matching(qabl, info))
                    || compute_other_matches()
            }
        }
    }
}

impl HatQueriesTrait for Hat {
    #[tracing::instrument(level = "debug", skip(tables), ret)]
    fn sourced_queryables(&self, tables: &TablesData) -> Vec<(Arc<Resource>, Sources)> {
        // Compute the list of known queryables (keys)
        let mut qabls = HashMap::new();
        for face in self.owned_faces(tables) {
            for (qabl, _) in self.face_hat(face).remote_qabls.values() {
                // Insert the key in the list of known queryables
                let srcs = qabls.entry(qabl.clone()).or_insert_with(Sources::empty);
                // Append src_face as a queryable source in the proper list
                srcs.clients.push(face.zid);
            }
        }
        Vec::from_iter(qabls)
    }

    #[tracing::instrument(level = "debug", skip(tables), ret)]
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
                    .clients
                    .push(face.zid);
            }
        }
        result.into_iter().collect()
    }

    #[tracing::instrument(level = "debug", skip(tables, src_face, _node_id), ret)]
    fn compute_query_route(
        &self,
        tables: &TablesData,
        src_face: &FaceState,
        expr: &RoutingExpr,
        _node_id: NodeId,
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
                if src_face.id != ctx.face.id {
                    if let Some(qabl) = QueryTargetQabl::new(ctx, expr, complete, &self.region) {
                        tracing::trace!(dst = %ctx.face, reason = "resource match");
                        route.push(qabl);
                    }
                }
            }
        }

        route.sort_by_key(|qabl| qabl.info.map_or(u16::MAX, |i| i.distance));
        Arc::new(route)
    }

    #[tracing::instrument(level = "debug", skip(ctx, id, _node_id, info), ret)]
    fn register_queryable(
        &mut self,
        ctx: DispatcherContext,
        id: QueryableId,
        mut res: Arc<Resource>,
        _node_id: NodeId,
        info: &QueryableInfoType,
    ) {
        debug_assert!(self.owns(ctx.src_face));

        {
            let res = get_mut_unchecked(&mut res);
            match res.face_ctxs.get_mut(&ctx.src_face.id) {
                Some(ctx) => {
                    // NOTE(regions): this is an update for all queryables on the resource (?)
                    get_mut_unchecked(ctx).qabl = Some(*info);
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

    // FIXME(regions): replicate changes elsewhere
    #[tracing::instrument(level = "debug", skip(ctx, id, _res, _node_id), ret)]
    fn unregister_queryable(
        &mut self,
        ctx: DispatcherContext,
        id: QueryableId,
        _res: Option<Arc<Resource>>,
        _node_id: NodeId,
    ) -> UnregisterResult {
        use UnregisterResult::*;

        debug_assert!(self.owns(ctx.src_face));

        let Some((mut res, info)) = self.face_hat_mut(ctx.src_face).remote_qabls.remove(&id) else {
            tracing::error!("Unknown id");
            return Noop;
        };

        if self
            .face_hat(ctx.src_face)
            .remote_qabls
            .values()
            // REVIEW(regions): use Arc::ptr_eq?
            .any(|(r, i)| r == &res && i == &info)
        {
            tracing::debug!("Duplicate");
            return Noop;
        }

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
            return Noop;
        };

        let Some(old_face_info) = face_ctx.qabl else {
            bug!("Undefined queryable info");
            return Noop;
        };

        if new_face_info == Some(old_face_info) {
            return Noop;
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

    #[tracing::instrument(level = "debug", skip(ctx), ret)]
    fn propagate_queryable(
        &mut self,
        ctx: DispatcherContext,
        res: Arc<Resource>,
        other_info: Option<QueryableInfoType>,
    ) {
        for dst_face in self.owned_faces_mut(ctx.tables) {
            if let Some(info) = self
                .owned_face_contexts(&res)
                .filter(|face_ctx| face_ctx.face.id != dst_face.id)
                .flat_map(|face_ctx| face_ctx.qabl)
                .chain(other_info.into_iter())
                .reduce(merge_qabl_infos)
            {
                self.maybe_propagate_queryable(&res, &info, dst_face, ctx.send_declare);
            }
        }
    }

    #[tracing::instrument(level = "debug", skip(ctx))]
    fn unpropagate_queryable(&mut self, ctx: DispatcherContext, res: Arc<Resource>) {
        for mut face in self
            .owned_faces(ctx.tables)
            .filter(|f| f.id != ctx.src_face.id)
            .cloned()
        {
            self.maybe_unpropagate_queryable(&mut face, &res, ctx.send_declare);
        }
    }

    #[tracing::instrument(level = "debug", skip(ctx), ret)]
    fn unpropagate_last_non_owned_queryable(&mut self, ctx: DispatcherContext, res: Arc<Resource>) {
        debug_assert!(self.remote_queryables_of(&res).is_some());

        if let Ok(face) = self
            .owned_face_contexts(&res)
            .filter_map(|ctx| ctx.qabl.map(|_| ctx.face.clone()))
            .exactly_one()
            .as_mut()
        {
            self.maybe_unpropagate_queryable(face, &res, ctx.send_declare)
        }
    }

    #[tracing::instrument(level = "trace", ret)]
    fn remote_queryables_of(&self, res: &Resource) -> Option<QueryableInfoType> {
        self.owned_face_contexts(res)
            .filter_map(|ctx| ctx.qabl)
            .reduce(merge_qabl_infos)
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
