use crate::net::routing::PREFIX_LIVELINESS;

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
use super::super::hat::network::Network;
use super::face::FaceState;
use super::resource::{QueryRoute, QueryRoutes, QueryTargetQabl, QueryTargetQablSet, Resource};
use super::tables::{RoutingExpr, Tables, TablesLock};
use async_trait::async_trait;
use ordered_float::OrderedFloat;
use petgraph::graph::NodeIndex;
use std::borrow::Cow;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::sync::{Arc, Weak};
use zenoh_buffers::ZBuf;
use zenoh_protocol::{
    core::{
        key_expr::{
            include::{Includer, DEFAULT_INCLUDER},
            OwnedKeyExpr,
        },
        Encoding, WhatAmI, WireExpr, ZenohId,
    },
    network::{
        declare::{ext, queryable::ext::QueryableInfo},
        request::{ext::TargetType, Request, RequestId},
        response::{self, ext::ResponderIdType, Response, ResponseFinal},
    },
    zenoh::{reply::ext::ConsolidationType, Reply, RequestBody, ResponseBody},
};
use zenoh_sync::get_mut_unchecked;
use zenoh_util::Timed;

pub(crate) struct Query {
    src_face: Arc<FaceState>,
    src_qid: RequestId,
}

#[inline]
#[allow(clippy::too_many_arguments)]
fn insert_target_for_qabls(
    route: &mut QueryTargetQablSet,
    expr: &mut RoutingExpr,
    tables: &Tables,
    net: &Network,
    source: usize,
    qabls: &HashMap<ZenohId, QueryableInfo>,
    complete: bool,
) {
    if net.trees.len() > source {
        for (qabl, qabl_info) in qabls {
            if let Some(qabl_idx) = net.get_idx(qabl) {
                if net.trees[source].directions.len() > qabl_idx.index() {
                    if let Some(direction) = net.trees[source].directions[qabl_idx.index()] {
                        if net.graph.contains_node(direction) {
                            if let Some(face) = tables.get_face(&net.graph[direction].zid) {
                                if net.distances.len() > qabl_idx.index() {
                                    let key_expr =
                                        Resource::get_best_key(expr.prefix, expr.suffix, face.id);
                                    route.push(QueryTargetQabl {
                                        direction: (
                                            face.clone(),
                                            key_expr.to_owned(),
                                            if source != 0 {
                                                Some(source as u16)
                                            } else {
                                                None
                                            },
                                        ),
                                        complete: if complete {
                                            qabl_info.complete as u64
                                        } else {
                                            0
                                        },
                                        distance: net.distances[qabl_idx.index()],
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    } else {
        log::trace!("Tree for node sid:{} not yet ready", source);
    }
}

lazy_static::lazy_static! {
    static ref EMPTY_ROUTE: Arc<QueryTargetQablSet> = Arc::new(Vec::new());
}
fn compute_query_route(
    tables: &Tables,
    expr: &mut RoutingExpr,
    source: Option<usize>,
    source_type: WhatAmI,
) -> Arc<QueryTargetQablSet> {
    let mut route = QueryTargetQablSet::new();
    let key_expr = expr.full_expr();
    if key_expr.ends_with('/') {
        return EMPTY_ROUTE.clone();
    }
    log::trace!(
        "compute_query_route({}, {:?}, {:?})",
        key_expr,
        source,
        source_type
    );
    let key_expr = match OwnedKeyExpr::try_from(key_expr) {
        Ok(ke) => ke,
        Err(e) => {
            log::warn!("Invalid KE reached the system: {}", e);
            return EMPTY_ROUTE.clone();
        }
    };
    let res = Resource::get_resource(expr.prefix, expr.suffix);
    let matches = res
        .as_ref()
        .and_then(|res| res.context.as_ref())
        .map(|ctx| Cow::from(&ctx.matches))
        .unwrap_or_else(|| Cow::from(Resource::get_matches(tables, &key_expr)));

    let master = tables.whatami != WhatAmI::Router
        || !tables.hat.full_net(WhatAmI::Peer)
        || *tables
            .hat
            .elect_router(&tables.zid, &key_expr, tables.hat.shared_nodes.iter())
            == tables.zid;

    for mres in matches.iter() {
        let mres = mres.upgrade().unwrap();
        let complete = DEFAULT_INCLUDER.includes(mres.expr().as_bytes(), key_expr.as_bytes());
        if tables.whatami == WhatAmI::Router {
            if master || source_type == WhatAmI::Router {
                let net = tables.hat.routers_net.as_ref().unwrap();
                let router_source = match source_type {
                    WhatAmI::Router => source.unwrap(),
                    _ => net.idx.index(),
                };
                insert_target_for_qabls(
                    &mut route,
                    expr,
                    tables,
                    net,
                    router_source,
                    &mres.context().router_qabls,
                    complete,
                );
            }

            if (master || source_type != WhatAmI::Router) && tables.hat.full_net(WhatAmI::Peer) {
                let net = tables.hat.peers_net.as_ref().unwrap();
                let peer_source = match source_type {
                    WhatAmI::Peer => source.unwrap(),
                    _ => net.idx.index(),
                };
                insert_target_for_qabls(
                    &mut route,
                    expr,
                    tables,
                    net,
                    peer_source,
                    &mres.context().peer_qabls,
                    complete,
                );
            }
        }

        if tables.whatami == WhatAmI::Peer && tables.hat.full_net(WhatAmI::Peer) {
            let net = tables.hat.peers_net.as_ref().unwrap();
            let peer_source = match source_type {
                WhatAmI::Router | WhatAmI::Peer => source.unwrap(),
                _ => net.idx.index(),
            };
            insert_target_for_qabls(
                &mut route,
                expr,
                tables,
                net,
                peer_source,
                &mres.context().peer_qabls,
                complete,
            );
        }

        if tables.whatami != WhatAmI::Router || master || source_type == WhatAmI::Router {
            for (sid, context) in &mres.session_ctxs {
                if match tables.whatami {
                    WhatAmI::Router => context.face.whatami != WhatAmI::Router,
                    _ => source_type == WhatAmI::Client || context.face.whatami == WhatAmI::Client,
                } {
                    let key_expr = Resource::get_best_key(expr.prefix, expr.suffix, *sid);
                    if let Some(qabl_info) = context.qabl.as_ref() {
                        route.push(QueryTargetQabl {
                            direction: (context.face.clone(), key_expr.to_owned(), None),
                            complete: if complete {
                                qabl_info.complete as u64
                            } else {
                                0
                            },
                            distance: 0.5,
                        });
                    }
                }
            }
        }
    }
    route.sort_by_key(|qabl| OrderedFloat(qabl.distance));
    Arc::new(route)
}

pub(crate) fn compute_query_routes_(tables: &Tables, res: &Arc<Resource>) -> QueryRoutes {
    let mut routes = QueryRoutes {
        routers_query_routes: vec![],
        peers_query_routes: vec![],
        peer_query_route: None,
        client_query_route: None,
    };
    let mut expr = RoutingExpr::new(res, "");
    if tables.whatami == WhatAmI::Router {
        let indexes = tables
            .hat
            .routers_net
            .as_ref()
            .unwrap()
            .graph
            .node_indices()
            .collect::<Vec<NodeIndex>>();
        let max_idx = indexes.iter().max().unwrap();
        routes
            .routers_query_routes
            .resize_with(max_idx.index() + 1, || Arc::new(QueryTargetQablSet::new()));

        for idx in &indexes {
            routes.routers_query_routes[idx.index()] =
                compute_query_route(tables, &mut expr, Some(idx.index()), WhatAmI::Router);
        }

        routes.peer_query_route = Some(compute_query_route(tables, &mut expr, None, WhatAmI::Peer));
    }
    if (tables.whatami == WhatAmI::Router || tables.whatami == WhatAmI::Peer)
        && tables.hat.full_net(WhatAmI::Peer)
    {
        let indexes = tables
            .hat
            .peers_net
            .as_ref()
            .unwrap()
            .graph
            .node_indices()
            .collect::<Vec<NodeIndex>>();
        let max_idx = indexes.iter().max().unwrap();
        routes
            .peers_query_routes
            .resize_with(max_idx.index() + 1, || Arc::new(QueryTargetQablSet::new()));

        for idx in &indexes {
            routes.peers_query_routes[idx.index()] =
                compute_query_route(tables, &mut expr, Some(idx.index()), WhatAmI::Peer);
        }
    }
    if tables.whatami == WhatAmI::Peer && !tables.hat.full_net(WhatAmI::Peer) {
        routes.client_query_route = Some(compute_query_route(
            tables,
            &mut expr,
            None,
            WhatAmI::Client,
        ));
        routes.peer_query_route = Some(compute_query_route(tables, &mut expr, None, WhatAmI::Peer));
    }
    if tables.whatami == WhatAmI::Client {
        routes.client_query_route = Some(compute_query_route(
            tables,
            &mut expr,
            None,
            WhatAmI::Client,
        ));
    }
    routes
}

pub(crate) fn compute_query_routes(tables: &mut Tables, res: &mut Arc<Resource>) {
    if res.context.is_some() {
        let mut res_mut = res.clone();
        let res_mut = get_mut_unchecked(&mut res_mut);
        let mut expr = RoutingExpr::new(res, "");
        if tables.whatami == WhatAmI::Router {
            let indexes = tables
                .hat
                .routers_net
                .as_ref()
                .unwrap()
                .graph
                .node_indices()
                .collect::<Vec<NodeIndex>>();
            let max_idx = indexes.iter().max().unwrap();
            let routers_query_routes = &mut res_mut.context_mut().routers_query_routes;
            routers_query_routes.clear();
            routers_query_routes
                .resize_with(max_idx.index() + 1, || Arc::new(QueryTargetQablSet::new()));

            for idx in &indexes {
                routers_query_routes[idx.index()] =
                    compute_query_route(tables, &mut expr, Some(idx.index()), WhatAmI::Router);
            }

            res_mut.context_mut().peer_query_route =
                Some(compute_query_route(tables, &mut expr, None, WhatAmI::Peer));
        }
        if (tables.whatami == WhatAmI::Router || tables.whatami == WhatAmI::Peer)
            && tables.hat.full_net(WhatAmI::Peer)
        {
            let indexes = tables
                .hat
                .peers_net
                .as_ref()
                .unwrap()
                .graph
                .node_indices()
                .collect::<Vec<NodeIndex>>();
            let max_idx = indexes.iter().max().unwrap();
            let peers_query_routes = &mut res_mut.context_mut().peers_query_routes;
            peers_query_routes.clear();
            peers_query_routes
                .resize_with(max_idx.index() + 1, || Arc::new(QueryTargetQablSet::new()));

            for idx in &indexes {
                peers_query_routes[idx.index()] =
                    compute_query_route(tables, &mut expr, Some(idx.index()), WhatAmI::Peer);
            }
        }
        if tables.whatami == WhatAmI::Peer && !tables.hat.full_net(WhatAmI::Peer) {
            res_mut.context_mut().client_query_route = Some(compute_query_route(
                tables,
                &mut expr,
                None,
                WhatAmI::Client,
            ));
            res_mut.context_mut().peer_query_route =
                Some(compute_query_route(tables, &mut expr, None, WhatAmI::Peer));
        }
        if tables.whatami == WhatAmI::Client {
            res_mut.context_mut().client_query_route = Some(compute_query_route(
                tables,
                &mut expr,
                None,
                WhatAmI::Client,
            ));
        }
    }
}

pub(crate) fn compute_query_routes_from(tables: &mut Tables, res: &mut Arc<Resource>) {
    compute_query_routes(tables, res);
    let res = get_mut_unchecked(res);
    for child in res.childs.values_mut() {
        compute_query_routes_from(tables, child);
    }
}

pub(crate) fn compute_matches_query_routes_(
    tables: &Tables,
    res: &Arc<Resource>,
) -> Vec<(Arc<Resource>, QueryRoutes)> {
    let mut routes = vec![];
    if res.context.is_some() {
        routes.push((res.clone(), compute_query_routes_(tables, res)));
        for match_ in &res.context().matches {
            let match_ = match_.upgrade().unwrap();
            if !Arc::ptr_eq(&match_, res) {
                let match_routes = compute_query_routes_(tables, &match_);
                routes.push((match_, match_routes));
            }
        }
    }
    routes
}

#[inline]
fn insert_pending_query(outface: &mut Arc<FaceState>, query: Arc<Query>) -> RequestId {
    let outface_mut = get_mut_unchecked(outface);
    outface_mut.next_qid += 1;
    let qid = outface_mut.next_qid;
    outface_mut.pending_queries.insert(qid, query);
    qid
}

#[inline]
fn should_route(
    tables: &Tables,
    src_face: &FaceState,
    outface: &Arc<FaceState>,
    expr: &mut RoutingExpr,
) -> bool {
    if src_face.id != outface.id {
        let dst_master = tables.whatami != WhatAmI::Router
            || outface.whatami != WhatAmI::Peer
            || tables.hat.peers_net.is_none()
            || tables.zid
                == *tables.hat.elect_router(
                    &tables.zid,
                    expr.full_expr(),
                    tables.hat.get_router_links(outface.zid),
                );

        return dst_master
            && (src_face.whatami != WhatAmI::Peer
                || outface.whatami != WhatAmI::Peer
                || tables.hat.full_net(WhatAmI::Peer)
                || tables.hat.failover_brokering(src_face.zid, outface.zid));
    }
    false
}

#[inline]
fn compute_final_route(
    tables: &Tables,
    qabls: &Arc<QueryTargetQablSet>,
    src_face: &Arc<FaceState>,
    expr: &mut RoutingExpr,
    target: &TargetType,
    query: Arc<Query>,
) -> QueryRoute {
    match target {
        TargetType::All => {
            let mut route = HashMap::new();
            for qabl in qabls.iter() {
                if should_route(tables, src_face, &qabl.direction.0, expr) {
                    #[cfg(feature = "complete_n")]
                    {
                        route.entry(qabl.direction.0.id).or_insert_with(|| {
                            let mut direction = qabl.direction.clone();
                            let qid = insert_pending_query(&mut direction.0, query.clone());
                            (direction, qid, *target)
                        });
                    }
                    #[cfg(not(feature = "complete_n"))]
                    {
                        route.entry(qabl.direction.0.id).or_insert_with(|| {
                            let mut direction = qabl.direction.clone();
                            let qid = insert_pending_query(&mut direction.0, query.clone());
                            (direction, qid)
                        });
                    }
                }
            }
            route
        }
        TargetType::AllComplete => {
            let mut route = HashMap::new();
            for qabl in qabls.iter() {
                if qabl.complete > 0 && should_route(tables, src_face, &qabl.direction.0, expr) {
                    #[cfg(feature = "complete_n")]
                    {
                        route.entry(qabl.direction.0.id).or_insert_with(|| {
                            let mut direction = qabl.direction.clone();
                            let qid = insert_pending_query(&mut direction.0, query.clone());
                            (direction, qid, *target)
                        });
                    }
                    #[cfg(not(feature = "complete_n"))]
                    {
                        route.entry(qabl.direction.0.id).or_insert_with(|| {
                            let mut direction = qabl.direction.clone();
                            let qid = insert_pending_query(&mut direction.0, query.clone());
                            (direction, qid)
                        });
                    }
                }
            }
            route
        }
        #[cfg(feature = "complete_n")]
        TargetType::Complete(n) => {
            let mut route = HashMap::new();
            let mut remaining = *n;
            if src_face.whatami == WhatAmI::Peer && !tables.full_net(WhatAmI::Peer) {
                let source_links = tables
                    .peers_net
                    .as_ref()
                    .map(|net| net.get_links(src_face.zid))
                    .unwrap_or_default();
                for qabl in qabls.iter() {
                    if qabl.direction.0.id != src_face.id
                        && qabl.complete > 0
                        && (qabl.direction.0.whatami != WhatAmI::Peer
                            || (tables.router_peers_failover_brokering
                                && Tables::failover_brokering_to(
                                    source_links,
                                    qabl.direction.0.zid,
                                )))
                    {
                        let nb = std::cmp::min(qabl.complete, remaining);
                        route.entry(qabl.direction.0.id).or_insert_with(|| {
                            let mut direction = qabl.direction.clone();
                            let qid = insert_pending_query(&mut direction.0, query.clone());
                            (direction, qid, TargetType::Complete(nb))
                        });
                        remaining -= nb;
                        if remaining == 0 {
                            break;
                        }
                    }
                }
            } else {
                for qabl in qabls.iter() {
                    if qabl.direction.0.id != src_face.id && qabl.complete > 0 {
                        let nb = std::cmp::min(qabl.complete, remaining);
                        route.entry(qabl.direction.0.id).or_insert_with(|| {
                            let mut direction = qabl.direction.clone();
                            let qid = insert_pending_query(&mut direction.0, query.clone());
                            (direction, qid, TargetType::Complete(nb))
                        });
                        remaining -= nb;
                        if remaining == 0 {
                            break;
                        }
                    }
                }
            }
            route
        }
        TargetType::BestMatching => {
            if let Some(qabl) = qabls
                .iter()
                .find(|qabl| qabl.direction.0.id != src_face.id && qabl.complete > 0)
            {
                let mut route = HashMap::new();
                #[cfg(feature = "complete_n")]
                {
                    let mut direction = qabl.direction.clone();
                    let qid = insert_pending_query(&mut direction.0, query);
                    route.insert(direction.0.id, (direction, qid, *target));
                }
                #[cfg(not(feature = "complete_n"))]
                {
                    let mut direction = qabl.direction.clone();
                    let qid = insert_pending_query(&mut direction.0, query);
                    route.insert(direction.0.id, (direction, qid));
                }
                route
            } else {
                compute_final_route(tables, qabls, src_face, expr, &TargetType::All, query)
            }
        }
    }
}

#[inline]
fn compute_local_replies(
    tables: &Tables,
    prefix: &Arc<Resource>,
    suffix: &str,
    face: &Arc<FaceState>,
) -> Vec<(WireExpr<'static>, ZBuf)> {
    let mut result = vec![];
    // Only the first routing point in the query route
    // should return the liveliness tokens
    if face.whatami == WhatAmI::Client {
        let key_expr = prefix.expr() + suffix;
        let key_expr = match OwnedKeyExpr::try_from(key_expr) {
            Ok(ke) => ke,
            Err(e) => {
                log::warn!("Invalid KE reached the system: {}", e);
                return result;
            }
        };
        if key_expr.starts_with(PREFIX_LIVELINESS) {
            let res = Resource::get_resource(prefix, suffix);
            let matches = res
                .as_ref()
                .and_then(|res| res.context.as_ref())
                .map(|ctx| Cow::from(&ctx.matches))
                .unwrap_or_else(|| Cow::from(Resource::get_matches(tables, &key_expr)));
            for mres in matches.iter() {
                let mres = mres.upgrade().unwrap();
                if (mres.context.is_some()
                    && (!mres.context().router_subs.is_empty()
                        || !mres.context().peer_subs.is_empty()))
                    || mres.session_ctxs.values().any(|ctx| ctx.subs.is_some())
                {
                    result.push((Resource::get_best_key(&mres, "", face.id), ZBuf::default()));
                }
            }
        }
    }
    result
}

#[derive(Clone)]
struct QueryCleanup {
    tables: Arc<TablesLock>,
    face: Weak<FaceState>,
    qid: RequestId,
}

#[async_trait]
impl Timed for QueryCleanup {
    async fn run(&mut self) {
        if let Some(mut face) = self.face.upgrade() {
            let tables_lock = zwrite!(self.tables.tables);
            if let Some(query) = get_mut_unchecked(&mut face)
                .pending_queries
                .remove(&self.qid)
            {
                drop(tables_lock);
                log::warn!(
                    "Didn't receive final reply {}:{} from {}: Timeout!",
                    query.src_face,
                    self.qid,
                    face
                );
                finalize_pending_query(query);
            }
        }
    }
}

pub(crate) fn disable_matches_query_routes(_tables: &mut Tables, res: &mut Arc<Resource>) {
    if res.context.is_some() {
        get_mut_unchecked(res).context_mut().valid_query_routes = false;
        for match_ in &res.context().matches {
            let mut match_ = match_.upgrade().unwrap();
            if !Arc::ptr_eq(&match_, res) {
                get_mut_unchecked(&mut match_)
                    .context_mut()
                    .valid_query_routes = false;
            }
        }
    }
}

#[inline]
fn get_query_route(
    tables: &Tables,
    face: &FaceState,
    res: &Option<Arc<Resource>>,
    expr: &mut RoutingExpr,
    routing_context: u64,
) -> Arc<QueryTargetQablSet> {
    match tables.whatami {
        WhatAmI::Router => match face.whatami {
            WhatAmI::Router => {
                let routers_net = tables.hat.routers_net.as_ref().unwrap();
                let local_context = routers_net.get_local_context(routing_context, face.link_id);
                res.as_ref()
                    .and_then(|res| res.routers_query_route(local_context))
                    .unwrap_or_else(|| {
                        compute_query_route(tables, expr, Some(local_context), face.whatami)
                    })
            }
            WhatAmI::Peer => {
                if tables.hat.full_net(WhatAmI::Peer) {
                    let peers_net = tables.hat.peers_net.as_ref().unwrap();
                    let local_context = peers_net.get_local_context(routing_context, face.link_id);
                    res.as_ref()
                        .and_then(|res| res.peers_query_route(local_context))
                        .unwrap_or_else(|| {
                            compute_query_route(tables, expr, Some(local_context), face.whatami)
                        })
                } else {
                    res.as_ref()
                        .and_then(|res| res.peer_query_route())
                        .unwrap_or_else(|| compute_query_route(tables, expr, None, face.whatami))
                }
            }
            _ => res
                .as_ref()
                .and_then(|res| res.routers_query_route(0))
                .unwrap_or_else(|| compute_query_route(tables, expr, None, face.whatami)),
        },
        WhatAmI::Peer => {
            if tables.hat.full_net(WhatAmI::Peer) {
                match face.whatami {
                    WhatAmI::Router | WhatAmI::Peer => {
                        let peers_net = tables.hat.peers_net.as_ref().unwrap();
                        let local_context =
                            peers_net.get_local_context(routing_context, face.link_id);
                        res.as_ref()
                            .and_then(|res| res.peers_query_route(local_context))
                            .unwrap_or_else(|| {
                                compute_query_route(tables, expr, Some(local_context), face.whatami)
                            })
                    }
                    _ => res
                        .as_ref()
                        .and_then(|res| res.peers_query_route(0))
                        .unwrap_or_else(|| compute_query_route(tables, expr, None, face.whatami)),
                }
            } else {
                res.as_ref()
                    .and_then(|res| match face.whatami {
                        WhatAmI::Client => res.client_query_route(),
                        _ => res.peer_query_route(),
                    })
                    .unwrap_or_else(|| compute_query_route(tables, expr, None, face.whatami))
            }
        }
        _ => res
            .as_ref()
            .and_then(|res| res.client_query_route())
            .unwrap_or_else(|| compute_query_route(tables, expr, None, face.whatami)),
    }
}

#[cfg(feature = "stats")]
macro_rules! inc_req_stats {
    (
        $face:expr,
        $txrx:ident,
        $space:ident,
        $body:expr
    ) => {
        paste::paste! {
            if let Some(stats) = $face.stats.as_ref() {
                use zenoh_buffers::SplitBuffer;
                match &$body {
                    RequestBody::Put(p) => {
                        stats.[<$txrx _z_put_msgs>].[<inc_ $space>](1);
                        stats.[<$txrx _z_put_pl_bytes>].[<inc_ $space>](p.payload.len());
                    }
                    RequestBody::Del(_) => {
                        stats.[<$txrx _z_del_msgs>].[<inc_ $space>](1);
                    }
                    RequestBody::Query(q) => {
                        stats.[<$txrx _z_query_msgs>].[<inc_ $space>](1);
                        stats.[<$txrx _z_query_pl_bytes>].[<inc_ $space>](
                            q.ext_body.as_ref().map(|b| b.payload.len()).unwrap_or(0),
                        );
                    }
                    RequestBody::Pull(_) => (),
                }
            }
        }
    };
}

#[cfg(feature = "stats")]
macro_rules! inc_res_stats {
    (
        $face:expr,
        $txrx:ident,
        $space:ident,
        $body:expr
    ) => {
        paste::paste! {
            if let Some(stats) = $face.stats.as_ref() {
                use zenoh_buffers::SplitBuffer;
                match &$body {
                    ResponseBody::Put(p) => {
                        stats.[<$txrx _z_put_msgs>].[<inc_ $space>](1);
                        stats.[<$txrx _z_put_pl_bytes>].[<inc_ $space>](p.payload.len());
                    }
                    ResponseBody::Reply(r) => {
                        stats.[<$txrx _z_reply_msgs>].[<inc_ $space>](1);
                        stats.[<$txrx _z_reply_pl_bytes>].[<inc_ $space>](r.payload.len());
                    }
                    ResponseBody::Err(e) => {
                        stats.[<$txrx _z_reply_msgs>].[<inc_ $space>](1);
                        stats.[<$txrx _z_reply_pl_bytes>].[<inc_ $space>](
                            e.ext_body.as_ref().map(|b| b.payload.len()).unwrap_or(0),
                        );
                    }
                    ResponseBody::Ack(_) => (),
                }
            }
        }
    };
}

#[allow(clippy::too_many_arguments)]
pub fn route_query(
    tables_ref: &Arc<TablesLock>,
    face: &Arc<FaceState>,
    expr: &WireExpr,
    qid: RequestId,
    target: TargetType,
    body: RequestBody,
    routing_context: u64,
) {
    let rtables = zread!(tables_ref.tables);
    match rtables.get_mapping(face, &expr.scope, expr.mapping) {
        Some(prefix) => {
            log::debug!(
                "Route query {}:{} for res {}{}",
                face,
                qid,
                prefix.expr(),
                expr.suffix.as_ref(),
            );
            let prefix = prefix.clone();
            let mut expr = RoutingExpr::new(&prefix, expr.suffix.as_ref());

            #[cfg(feature = "stats")]
            let admin = expr.full_expr().starts_with("@/");
            #[cfg(feature = "stats")]
            if !admin {
                inc_req_stats!(face, rx, user, body)
            } else {
                inc_req_stats!(face, rx, admin, body)
            }

            if rtables.whatami != WhatAmI::Router
                || face.whatami != WhatAmI::Peer
                || rtables.hat.peers_net.is_none()
                || rtables.zid
                    == *rtables.hat.elect_router(
                        &rtables.zid,
                        expr.full_expr(),
                        rtables.hat.get_router_links(face.zid),
                    )
            {
                let res = Resource::get_resource(&prefix, expr.suffix);
                let route = get_query_route(&rtables, face, &res, &mut expr, routing_context);

                let query = Arc::new(Query {
                    src_face: face.clone(),
                    src_qid: qid,
                });

                let queries_lock = zwrite!(tables_ref.queries_lock);
                let route = compute_final_route(&rtables, &route, face, &mut expr, &target, query);
                let local_replies = compute_local_replies(&rtables, &prefix, expr.suffix, face);
                let zid = rtables.zid;

                drop(queries_lock);
                drop(rtables);

                for (expr, payload) in local_replies {
                    let payload = ResponseBody::Reply(Reply {
                        timestamp: None,
                        encoding: Encoding::default(),
                        ext_sinfo: None,
                        ext_consolidation: ConsolidationType::default(),
                        #[cfg(feature = "shared-memory")]
                        ext_shm: None,
                        ext_unknown: vec![],
                        payload,
                    });
                    #[cfg(feature = "stats")]
                    if !admin {
                        inc_res_stats!(face, tx, user, payload)
                    } else {
                        inc_res_stats!(face, tx, admin, payload)
                    }

                    face.primitives.clone().send_response(Response {
                        rid: qid,
                        wire_expr: expr,
                        payload,
                        ext_qos: response::ext::QoSType::declare_default(),
                        ext_tstamp: None,
                        ext_respid: Some(response::ext::ResponderIdType {
                            zid,
                            eid: 0, // TODO
                        }),
                    });
                }

                if route.is_empty() {
                    log::debug!(
                        "Send final reply {}:{} (no matching queryables or not master)",
                        face,
                        qid
                    );
                    face.primitives.clone().send_response_final(ResponseFinal {
                        rid: qid,
                        ext_qos: response::ext::QoSType::response_final_default(),
                        ext_tstamp: None,
                    });
                } else {
                    // let timer = tables.timer.clone();
                    // let timeout = tables.queries_default_timeout;
                    #[cfg(feature = "complete_n")]
                    {
                        for ((outface, key_expr, context), qid, t) in route.values() {
                            // timer.add(TimedEvent::once(
                            //     Instant::now() + timeout,
                            //     QueryCleanup {
                            //         tables: tables_ref.clone(),
                            //         face: Arc::downgrade(&outface),
                            //         *qid,
                            //     },
                            // ));
                            #[cfg(feature = "stats")]
                            if !admin {
                                inc_req_stats!(outface, tx, user, body)
                            } else {
                                inc_req_stats!(outface, tx, admin, body)
                            }

                            log::trace!("Propagate query {}:{} to {}", face, qid, outface);
                            outface.primitives.send_request(Request {
                                id: *qid,
                                wire_expr: key_expr.into(),
                                ext_qos: ext::QoSType::request_default(), // TODO
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType {
                                    node_id: context.unwrap_or(0),
                                },
                                ext_target: *t,
                                ext_budget: None,
                                ext_timeout: None,
                                payload: body.clone(),
                            });
                        }
                    }

                    #[cfg(not(feature = "complete_n"))]
                    {
                        for ((outface, key_expr, context), qid) in route.values() {
                            // timer.add(TimedEvent::once(
                            //     Instant::now() + timeout,
                            //     QueryCleanup {
                            //         tables: tables_ref.clone(),
                            //         face: Arc::downgrade(&outface),
                            //         *qid,
                            //     },
                            // ));
                            #[cfg(feature = "stats")]
                            if !admin {
                                inc_req_stats!(outface, tx, user, body)
                            } else {
                                inc_req_stats!(outface, tx, admin, body)
                            }

                            log::trace!("Propagate query {}:{} to {}", face, qid, outface);
                            outface.primitives.send_request(Request {
                                id: *qid,
                                wire_expr: key_expr.into(),
                                ext_qos: ext::QoSType::request_default(),
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType {
                                    node_id: context.unwrap_or(0),
                                },
                                ext_target: target,
                                ext_budget: None,
                                ext_timeout: None,
                                payload: body.clone(),
                            });
                        }
                    }
                }
            } else {
                log::debug!("Send final reply {}:{} (not master)", face, qid);
                drop(rtables);
                face.primitives.clone().send_response_final(ResponseFinal {
                    rid: qid,
                    ext_qos: response::ext::QoSType::response_final_default(),
                    ext_tstamp: None,
                });
            }
        }
        None => {
            log::error!(
                "Route query with unknown scope {}! Send final reply.",
                expr.scope
            );
            drop(rtables);
            face.primitives.clone().send_response_final(ResponseFinal {
                rid: qid,
                ext_qos: response::ext::QoSType::response_final_default(),
                ext_tstamp: None,
            });
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn route_send_response(
    tables_ref: &Arc<TablesLock>,
    face: &mut Arc<FaceState>,
    qid: RequestId,
    ext_respid: Option<ResponderIdType>,
    key_expr: WireExpr,
    body: ResponseBody,
) {
    let queries_lock = zread!(tables_ref.queries_lock);
    #[cfg(feature = "stats")]
    let admin = key_expr.as_str().starts_with("@/");
    #[cfg(feature = "stats")]
    if !admin {
        inc_res_stats!(face, rx, user, body)
    } else {
        inc_res_stats!(face, rx, admin, body)
    }

    match face.pending_queries.get(&qid) {
        Some(query) => {
            drop(queries_lock);

            #[cfg(feature = "stats")]
            if !admin {
                inc_res_stats!(query.src_face, tx, user, body)
            } else {
                inc_res_stats!(query.src_face, tx, admin, body)
            }

            query.src_face.primitives.clone().send_response(Response {
                rid: query.src_qid,
                wire_expr: key_expr.to_owned(),
                payload: body,
                ext_qos: response::ext::QoSType::response_default(),
                ext_tstamp: None,
                ext_respid,
            });
        }
        None => log::warn!(
            "Route reply {}:{} from {}: Query nof found!",
            face,
            qid,
            face
        ),
    }
}

pub(crate) fn route_send_response_final(
    tables_ref: &Arc<TablesLock>,
    face: &mut Arc<FaceState>,
    qid: RequestId,
) {
    let queries_lock = zwrite!(tables_ref.queries_lock);
    match get_mut_unchecked(face).pending_queries.remove(&qid) {
        Some(query) => {
            drop(queries_lock);
            log::debug!(
                "Received final reply {}:{} from {}",
                query.src_face,
                qid,
                face
            );
            finalize_pending_query(query);
        }
        None => log::warn!(
            "Route final reply {}:{} from {}: Query nof found!",
            face,
            qid,
            face
        ),
    }
}

pub(crate) fn finalize_pending_queries(tables_ref: &TablesLock, face: &mut Arc<FaceState>) {
    let queries_lock = zwrite!(tables_ref.queries_lock);
    for (_, query) in get_mut_unchecked(face).pending_queries.drain() {
        finalize_pending_query(query);
    }
    drop(queries_lock);
}

pub(crate) fn finalize_pending_query(query: Arc<Query>) {
    if let Some(query) = Arc::into_inner(query) {
        log::debug!("Propagate final reply {}:{}", query.src_face, query.src_qid);
        query
            .src_face
            .primitives
            .clone()
            .send_response_final(ResponseFinal {
                rid: query.src_qid,
                ext_qos: response::ext::QoSType::response_final_default(),
                ext_tstamp: None,
            });
    }
}
