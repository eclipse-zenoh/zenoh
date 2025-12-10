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
        hat::{BaseContext, HatBaseTrait, HatQueriesTrait, HatTrait, SendDeclare, Sources},
        router::{FaceContext, QueryTargetQabl},
        RoutingContext,
    },
    sample::Locality,
};

lazy_static::lazy_static! {
    static ref EMPTY_ROUTE: Arc<QueryTargetQablSet> = Arc::new(Vec::new());
}

impl Hat {
    #[inline]
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

        tracing::trace!(qabl_interests = ?self.face_hat(dst_face).remote_interests, %dst_face);

        let (should_notify, simple_interests) = self
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
            );

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

        tracing::trace!(?res);

        let is_matching_info = |info: &QueryableInfoType| !complete || info.complete;

        let compute_other_matches = || {
            other_hats
                .values()
                .flat_map(|hat| {
                    hat.remote_queryables_matching(tables, Some(&res))
                        .into_values()
                })
                .any(|info| is_matching_info(&info))
        };

        match locality {
            Locality::SessionLocal => self
                .face_hat(src_face)
                .remote_qabls
                .values()
                .any(|(qabl, info)| res.matches(qabl) && is_matching_info(info)),
            Locality::Remote => {
                self.owned_faces(tables)
                    .filter(|f| f.id != src_face.id)
                    .flat_map(|f| self.face_hat(f).remote_qabls.values())
                    .any(|(qabl, info)| res.matches(qabl) && is_matching_info(info))
                    || compute_other_matches()
            }
            Locality::Any => {
                self.owned_faces(tables)
                    .flat_map(|f| self.face_hat(f).remote_qabls.values())
                    .any(|(qabl, info)| res.matches(qabl) && is_matching_info(info))
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

    #[tracing::instrument(level = "trace", skip_all, fields(expr = ?expr, rgn = %self.region))]
    fn compute_query_route(
        &self,
        tables: &TablesData,
        src_face: &FaceState,
        expr: &RoutingExpr,
        _source: NodeId,
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
            for face_ctx @ (_, ctx) in self.owned_face_contexts(&mres) {
                if src_face.id != ctx.face.id {
                    if let Some(qabl) = QueryTargetQabl::new(face_ctx, expr, complete, &self.region)
                    {
                        tracing::trace!(dst = %ctx.face, reason = "resource match");
                        route.push(qabl);
                    }
                }
            }
        }

        route.sort_by_key(|qabl| qabl.info.map_or(u16::MAX, |i| i.distance));
        Arc::new(route)
    }

    #[tracing::instrument(level = "trace", skip_all)]
    fn register_queryable(
        &mut self,
        ctx: BaseContext,
        id: zenoh_protocol::network::declare::SubscriberId,
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

        tracing::trace!(%ctx.src_face, qabls = ?self.face_hat(ctx.src_face).remote_qabls);
    }

    #[tracing::instrument(level = "trace", skip_all)]
    fn unregister_queryable(
        &mut self,
        ctx: BaseContext,
        id: QueryableId,
        _res: Option<Arc<Resource>>,
        _nid: NodeId,
    ) -> Option<Arc<Resource>> {
        let Some((mut res, info)) = self.face_hat_mut(ctx.src_face).remote_qabls.remove(&id) else {
            tracing::error!(id, "Unknown queryable");
            return None;
        };

        if self
            .face_hat(ctx.src_face)
            .remote_qabls
            .values()
            .contains(&(res.clone(), info))
        {
            tracing::debug!(id, ?res, "Duplicated queryable");
            return None;
        };

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
        for dst_face in self
            .owned_faces_mut(ctx.tables)
            .filter(|f| f.id != ctx.src_face.id)
        {
            if let Some(info) = self
                .owned_face_contexts(&res)
                .filter(|(_, face_ctx)| face_ctx.face.id != dst_face.id)
                .flat_map(|(_, face_ctx)| face_ctx.qabl)
                .chain(other_info.into_iter())
                .reduce(merge_qabl_infos)
            {
                self.maybe_propagate_queryable(&res, &info, dst_face, ctx.send_declare);
            }
        }
    }

    #[tracing::instrument(level = "trace", skip_all)]
    fn unpropagate_queryable(&mut self, ctx: BaseContext, res: Arc<Resource>) {
        for mut face in self
            .owned_faces(ctx.tables)
            .filter(|f| f.id != ctx.src_face.id)
            .cloned()
        {
            self.maybe_unpropagate_queryable(&mut face, &res, ctx.send_declare);
        }
    }

    #[tracing::instrument(level = "trace", skip_all)]
    fn unpropagate_last_non_owned_queryable(&mut self, ctx: BaseContext, res: Arc<Resource>) {
        // FIXME(regions): remove this
        debug_assert!(self.remote_queryables_of(&res).is_some());

        if let Ok(face) = self
            .owned_face_contexts(&res)
            .filter_map(|(_, ctx)| ctx.qabl.map(|_| ctx.face.clone()))
            .exactly_one()
            .as_mut()
        {
            self.maybe_unpropagate_queryable(face, &res, ctx.send_declare)
        }
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region), ret)]
    fn remote_queryables_of(&self, res: &Resource) -> Option<QueryableInfoType> {
        self.owned_face_contexts(res)
            .filter_map(|(_, ctx)| ctx.qabl)
            .reduce(merge_qabl_infos)
    }

    #[allow(clippy::incompatible_msrv)]
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
