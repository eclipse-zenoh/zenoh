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
use zenoh_core::compat::*;
use zenoh_keyexpr::include::{Includer, DEFAULT_INCLUDER};
use zenoh_protocol::network::{
    declare::{self, common::ext::WireExprType, queryable::ext::QueryableInfoType, QueryableId},
    Declare, DeclareBody, DeclareQueryable, UndeclareQueryable,
};
use zenoh_sync::get_mut_unchecked;

use super::Hat;
#[allow(unused_imports)]
use crate::net::routing::OptionExt;
use crate::{
    key_expr::KeyExpr,
    net::routing::{
        dispatcher::{
            face::FaceState,
            queries::merge_qabl_infos,
            resource::{NodeId, Resource},
            tables::{QueryTargetQablSet, RoutingExpr, TablesData},
        },
        hat::{BaseContext, HatBaseTrait, HatQueriesTrait, SendDeclare, Sources},
        router::{FaceContext, QueryTargetQabl},
        RoutingContext,
    },
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
}

impl HatQueriesTrait for Hat {
    fn declare_queryable(
        &mut self,
        _ctx: BaseContext,
        _id: QueryableId,
        _res: &mut Arc<Resource>,
        _node_id: NodeId,
        _qabl_info: &QueryableInfoType,
    ) {
        unimplemented!()
    }

    fn undeclare_queryable(
        &mut self,
        _ctx: BaseContext,
        _id: QueryableId,
        _res: Option<Arc<Resource>>,
        _node_id: NodeId,
    ) -> Option<Arc<Resource>> {
        unimplemented!()
    }

    fn get_queryables(&self, _tables: &TablesData) -> Vec<(Arc<Resource>, Sources)> {
        unimplemented!()
    }

    fn get_queriers(&self, _tables: &TablesData) -> Vec<(Arc<Resource>, Sources)> {
        unimplemented!()
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

    fn get_matching_queryables(
        &self,
        _tables: &TablesData,
        _key_expr: &KeyExpr<'_>,
        _complete: bool,
    ) -> HashMap<usize, Arc<FaceState>> {
        unimplemented!()
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
        debug_assert!(self.owns(&ctx.src_face));

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
        for dst_face in self.owned_faces_mut(ctx.tables) {
            if !self.owns(&ctx.src_face) || ctx.src_face.id != dst_face.id {
                if let Some(info) = self
                    .owned_face_contexts(&res)
                    .filter(|(_, face_ctx)| face_ctx.face.id != dst_face.id)
                    .flat_map(|(_, face_ctx)| face_ctx.qabl)
                    .chain(other_info.into_iter())
                    .reduce(|acc, info| merge_qabl_infos(acc, info))
                {
                    self.maybe_propagate_queryable(&res, &info, dst_face, ctx.send_declare);
                }
            }
        }
    }

    #[tracing::instrument(level = "trace", skip_all)]
    fn unpropagate_queryable(&mut self, ctx: BaseContext, res: Arc<Resource>) {
        for mut face in self.owned_faces(&ctx.tables).cloned() {
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
            .reduce(|acc, info| merge_qabl_infos(acc, info))
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region), ret)]
    fn remote_queryables(&self, tables: &TablesData) -> HashMap<Arc<Resource>, QueryableInfoType> {
        self.remote_queryables_matching(tables, None)
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
