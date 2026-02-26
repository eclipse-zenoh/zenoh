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
use crate::net::routing::{
    dispatcher::{
        face::FaceState,
        queries::merge_qabl_infos,
        region::RegionMap,
        resource::{FaceContext, NodeId, Resource},
        tables::{QueryTargetQabl, QueryTargetQablSet, RoutingExpr, TablesData},
    },
    hat::{DispatcherContext, HatBaseTrait, HatQueriesTrait, HatTrait, Sources, UnregisterResult},
    router::{Direction, DEFAULT_NODE_ID},
    RoutingContext,
};

impl Hat {
    pub(super) fn queries_new_face(
        &self,
        ctx: DispatcherContext,
        other_hats: &RegionMap<&dyn HatTrait>,
    ) {
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
        let mut qabls = HashMap::new();
        for face in self.owned_faces(tables) {
            for (qabl, _) in self.face_hat(face).remote_qabls.values() {
                let srcs = qabls.entry(qabl.clone()).or_insert_with(Sources::empty);
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

    #[tracing::instrument(level = "debug", skip(tables, src_face, _node_id), ret)]
    fn compute_query_route(
        &self,
        tables: &TablesData,
        src_face: &FaceState,
        expr: &RoutingExpr,
        _node_id: NodeId,
    ) -> Arc<QueryTargetQablSet> {
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

    #[tracing::instrument(level = "debug", skip(ctx, id, _res, _node_id), ret)]
    fn unregister_queryable(
        &mut self,
        ctx: DispatcherContext,
        id: QueryableId,
        _res: Option<Arc<Resource>>,
        _node_id: NodeId,
    ) -> UnregisterResult {
        use UnregisterResult::*;

        let Some(qabl) = self.face_hat_mut(ctx.src_face).remote_qabls.remove(&id) else {
            tracing::error!("Unknown id");
            return Noop;
        };

        if self
            .face_hat(ctx.src_face)
            .remote_qabls
            .values()
            .contains(&qabl)
        {
            tracing::debug!("Duplicate");
            return Noop;
        };

        let (mut res, _) = qabl;

        if let Some(ctx) = get_mut_unchecked(&mut res)
            .face_ctxs
            .get_mut(&ctx.src_face.id)
        {
            get_mut_unchecked(ctx).qabl = None;
        }

        LastUnregistered { res }
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

        // TODO(regions*): this didn't check if the info is different before.
        // I need to make sure that all similar code paths are updated like this one is.
        let id = match self.face_hat(&dst_face).local_qabls.get(&res) {
            Some((_, old_info)) if old_info == &info => return,
            Some((id, _)) => *id,
            None => self
                .face_hat(&dst_face)
                .next_id
                .fetch_add(1, Ordering::SeqCst),
        };

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

    #[tracing::instrument(level = "debug", skip(ctx), ret)]
    fn unpropagate_queryable(&mut self, ctx: DispatcherContext, res: Arc<Resource>) {
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
