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
use zenoh_core::polyfill::*;
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
            queries::merge_qabl_infos,
            region::RegionMap,
            resource::{FaceContext, NodeId, Resource},
            tables::{QueryTargetQabl, QueryTargetQablSet, RoutingExpr, TablesData},
        },
        hat::{BaseContext, HatBaseTrait, HatQueriesTrait, HatTrait, Sources},
        router::{Direction, DEFAULT_NODE_ID},
        RoutingContext,
    },
};

impl Hat {
    pub(super) fn queries_new_face(&self, ctx: BaseContext, other_hats: &RegionMap<&dyn HatTrait>) {
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
            if self.face_hat(ctx.src_face).local_qabls.contains_key(&res) {
                continue;
            }

            let id = self
                .face_hat(ctx.src_face)
                .next_id
                .fetch_add(1, Ordering::SeqCst);
            self.face_hat_mut(ctx.src_face)
                .local_qabls
                .insert(res.clone(), (id, info));
            let key_expr = Resource::decl_key(&res, ctx.src_face);
            (ctx.send_declare)(
                &ctx.src_face.primitives,
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

lazy_static::lazy_static! {
    static ref EMPTY_ROUTE: Arc<QueryTargetQablSet> = Arc::new(Vec::new());
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
                // Append face as a queryable source in the proper list
                match face.whatami {
                    WhatAmI::Router => srcs.routers.push(face.zid),
                    WhatAmI::Peer => srcs.peers.push(face.zid),
                    WhatAmI::Client => srcs.clients.push(face.zid),
                }
            }
        }
        Vec::from_iter(qabls)
    }

    #[tracing::instrument(level = "debug", skip(_tables), ret)]
    fn sourced_queriers(&self, _tables: &TablesData) -> Vec<(Arc<Resource>, Sources)> {
        Vec::default()
    }

    #[tracing::instrument(level = "trace", skip_all, fields(expr = ?expr, rgn = %self.region))]
    fn compute_query_route(
        &self,
        tables: &TablesData,
        src_face: &FaceState,
        expr: &RoutingExpr,
        source: NodeId,
    ) -> Arc<QueryTargetQablSet> {
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
            // REVIEW(regions): there should only be one such face?
            for face in self
                .owned_faces(tables)
                .filter(|f| f.region.bound().is_north())
            {
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
                        },
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
        let Some(info) = other_info else {
            debug_assert!(self.owns(ctx.src_face));
            return;
        };

        let Some(mut dst_face) = self.owned_faces(ctx.tables).next().cloned() else {
            tracing::debug!(
                "Client region is empty; Will not propagate queryable declaration upstream"
            );
            return;
        };

        if self.face_hat(&dst_face).local_qabls.contains_key(&res) {
            return;
        }

        let id = self
            .face_hat(&dst_face)
            .next_id
            .fetch_add(1, Ordering::SeqCst);
        self.face_hat_mut(&mut dst_face)
            .local_qabls
            .insert(res.clone(), (id, info));
        let key_expr = Resource::decl_key(&res, &mut dst_face);
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

    #[tracing::instrument(level = "trace", skip_all)]
    fn unpropagate_queryable(&mut self, ctx: BaseContext, res: Arc<Resource>) {
        let Some(mut dst_face) = self.owned_faces(ctx.tables).next().cloned() else {
            tracing::debug!(
                "Client region is empty; Will not propagate queryable undeclaration upstream"
            );
            return;
        };

        if let Some((id, _)) = self.face_hat_mut(&mut dst_face).local_qabls.remove(&res) {
            (ctx.send_declare)(
                &dst_face.primitives,
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

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region), ret)]
    fn remote_queryables_of(&self, res: &Resource) -> Option<QueryableInfoType> {
        self.owned_face_contexts(res)
            .filter_map(|(_, ctx)| ctx.qabl)
            .reduce(merge_qabl_infos)
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
