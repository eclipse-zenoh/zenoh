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
use super::resource::{DataRoutes, Direction, PullCaches, Resource, Route};
use super::tables::{RoutingExpr, Tables};
use petgraph::graph::NodeIndex;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;
use std::sync::Arc;
use std::sync::RwLock;
use zenoh_core::zread;
use zenoh_protocol::{
    core::{key_expr::OwnedKeyExpr, WhatAmI, WireExpr, ZenohId},
    network::{
        declare::{ext, Mode},
        Push,
    },
    zenoh::PushBody,
};
use zenoh_sync::get_mut_unchecked;

#[inline]
fn insert_faces_for_subs(
    route: &mut Route,
    expr: &RoutingExpr,
    tables: &Tables,
    net: &Network,
    source: usize,
    subs: &HashSet<ZenohId>,
) {
    if net.trees.len() > source {
        for sub in subs {
            if let Some(sub_idx) = net.get_idx(sub) {
                if net.trees[source].directions.len() > sub_idx.index() {
                    if let Some(direction) = net.trees[source].directions[sub_idx.index()] {
                        if net.graph.contains_node(direction) {
                            if let Some(face) = tables.get_face(&net.graph[direction].zid) {
                                route.entry(face.id).or_insert_with(|| {
                                    let key_expr =
                                        Resource::get_best_key(expr.prefix, expr.suffix, face.id);
                                    (
                                        face.clone(),
                                        key_expr.to_owned(),
                                        if source != 0 {
                                            Some(source as u16)
                                        } else {
                                            None
                                        },
                                    )
                                });
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

fn compute_data_route(
    tables: &Tables,
    expr: &mut RoutingExpr,
    source: Option<usize>,
    source_type: WhatAmI,
) -> Arc<Route> {
    let mut route = HashMap::new();
    let key_expr = expr.full_expr();
    if key_expr.ends_with('/') {
        return Arc::new(route);
    }
    log::trace!(
        "compute_data_route({}, {:?}, {:?})",
        key_expr,
        source,
        source_type
    );
    let key_expr = match OwnedKeyExpr::try_from(key_expr) {
        Ok(ke) => ke,
        Err(e) => {
            log::warn!("Invalid KE reached the system: {}", e);
            return Arc::new(route);
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
        if tables.whatami == WhatAmI::Router {
            if master || source_type == WhatAmI::Router {
                let net = tables.hat.routers_net.as_ref().unwrap();
                let router_source = match source_type {
                    WhatAmI::Router => source.unwrap(),
                    _ => net.idx.index(),
                };
                insert_faces_for_subs(
                    &mut route,
                    expr,
                    tables,
                    net,
                    router_source,
                    &mres.context().router_subs,
                );
            }

            if (master || source_type != WhatAmI::Router) && tables.hat.full_net(WhatAmI::Peer) {
                let net = tables.hat.peers_net.as_ref().unwrap();
                let peer_source = match source_type {
                    WhatAmI::Peer => source.unwrap(),
                    _ => net.idx.index(),
                };
                insert_faces_for_subs(
                    &mut route,
                    expr,
                    tables,
                    net,
                    peer_source,
                    &mres.context().peer_subs,
                );
            }
        }

        if tables.whatami == WhatAmI::Peer && tables.hat.full_net(WhatAmI::Peer) {
            let net = tables.hat.peers_net.as_ref().unwrap();
            let peer_source = match source_type {
                WhatAmI::Router | WhatAmI::Peer => source.unwrap(),
                _ => net.idx.index(),
            };
            insert_faces_for_subs(
                &mut route,
                expr,
                tables,
                net,
                peer_source,
                &mres.context().peer_subs,
            );
        }

        if tables.whatami != WhatAmI::Router || master || source_type == WhatAmI::Router {
            for (sid, context) in &mres.session_ctxs {
                if let Some(subinfo) = &context.subs {
                    if match tables.whatami {
                        WhatAmI::Router => context.face.whatami != WhatAmI::Router,
                        _ => {
                            source_type == WhatAmI::Client
                                || context.face.whatami == WhatAmI::Client
                        }
                    } && subinfo.mode == Mode::Push
                    {
                        route.entry(*sid).or_insert_with(|| {
                            let key_expr = Resource::get_best_key(expr.prefix, expr.suffix, *sid);
                            (context.face.clone(), key_expr.to_owned(), None)
                        });
                    }
                }
            }
        }
    }
    for mcast_group in &tables.mcast_groups {
        route.insert(
            mcast_group.id,
            (
                mcast_group.clone(),
                expr.full_expr().to_string().into(),
                None,
            ),
        );
    }
    Arc::new(route)
}

fn compute_matching_pulls(tables: &Tables, expr: &mut RoutingExpr) -> Arc<PullCaches> {
    let mut pull_caches = vec![];
    let ke = if let Ok(ke) = OwnedKeyExpr::try_from(expr.full_expr()) {
        ke
    } else {
        return Arc::new(pull_caches);
    };
    let res = Resource::get_resource(expr.prefix, expr.suffix);
    let matches = res
        .as_ref()
        .and_then(|res| res.context.as_ref())
        .map(|ctx| Cow::from(&ctx.matches))
        .unwrap_or_else(|| Cow::from(Resource::get_matches(tables, &ke)));

    for mres in matches.iter() {
        let mres = mres.upgrade().unwrap();
        for context in mres.session_ctxs.values() {
            if let Some(subinfo) = &context.subs {
                if subinfo.mode == Mode::Pull {
                    pull_caches.push(context.clone());
                }
            }
        }
    }
    Arc::new(pull_caches)
}

pub(crate) fn compute_data_routes_(tables: &Tables, res: &Arc<Resource>) -> DataRoutes {
    let mut routes = DataRoutes {
        matching_pulls: None,
        routers_data_routes: vec![],
        peers_data_routes: vec![],
        peer_data_route: None,
        client_data_route: None,
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
            .routers_data_routes
            .resize_with(max_idx.index() + 1, || Arc::new(HashMap::new()));

        for idx in &indexes {
            routes.routers_data_routes[idx.index()] =
                compute_data_route(tables, &mut expr, Some(idx.index()), WhatAmI::Router);
        }

        routes.peer_data_route = Some(compute_data_route(tables, &mut expr, None, WhatAmI::Peer));
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
            .peers_data_routes
            .resize_with(max_idx.index() + 1, || Arc::new(HashMap::new()));

        for idx in &indexes {
            routes.peers_data_routes[idx.index()] =
                compute_data_route(tables, &mut expr, Some(idx.index()), WhatAmI::Peer);
        }
    }
    if tables.whatami == WhatAmI::Peer && !tables.hat.full_net(WhatAmI::Peer) {
        routes.client_data_route =
            Some(compute_data_route(tables, &mut expr, None, WhatAmI::Client));
        routes.peer_data_route = Some(compute_data_route(tables, &mut expr, None, WhatAmI::Peer));
    }
    if tables.whatami == WhatAmI::Client {
        routes.client_data_route =
            Some(compute_data_route(tables, &mut expr, None, WhatAmI::Client));
    }
    routes.matching_pulls = Some(compute_matching_pulls(tables, &mut expr));
    routes
}

pub(crate) fn compute_data_routes(tables: &mut Tables, res: &mut Arc<Resource>) {
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
            let routers_data_routes = &mut res_mut.context_mut().routers_data_routes;
            routers_data_routes.clear();
            routers_data_routes.resize_with(max_idx.index() + 1, || Arc::new(HashMap::new()));

            for idx in &indexes {
                routers_data_routes[idx.index()] =
                    compute_data_route(tables, &mut expr, Some(idx.index()), WhatAmI::Router);
            }

            res_mut.context_mut().peer_data_route =
                Some(compute_data_route(tables, &mut expr, None, WhatAmI::Peer));
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
            let peers_data_routes = &mut res_mut.context_mut().peers_data_routes;
            peers_data_routes.clear();
            peers_data_routes.resize_with(max_idx.index() + 1, || Arc::new(HashMap::new()));

            for idx in &indexes {
                peers_data_routes[idx.index()] =
                    compute_data_route(tables, &mut expr, Some(idx.index()), WhatAmI::Peer);
            }
        }
        if tables.whatami == WhatAmI::Peer && !tables.hat.full_net(WhatAmI::Peer) {
            res_mut.context_mut().client_data_route =
                Some(compute_data_route(tables, &mut expr, None, WhatAmI::Client));
            res_mut.context_mut().peer_data_route =
                Some(compute_data_route(tables, &mut expr, None, WhatAmI::Peer));
        }
        if tables.whatami == WhatAmI::Client {
            res_mut.context_mut().client_data_route =
                Some(compute_data_route(tables, &mut expr, None, WhatAmI::Client));
        }
        res_mut.context_mut().matching_pulls = compute_matching_pulls(tables, &mut expr);
    }
}

pub(crate) fn compute_data_routes_from(tables: &mut Tables, res: &mut Arc<Resource>) {
    compute_data_routes(tables, res);
    let res = get_mut_unchecked(res);
    for child in res.childs.values_mut() {
        compute_data_routes_from(tables, child);
    }
}

pub(crate) fn compute_matches_data_routes_<'a>(
    tables: &'a Tables,
    res: &'a Arc<Resource>,
) -> Vec<(Arc<Resource>, DataRoutes)> {
    let mut routes = vec![];
    if res.context.is_some() {
        routes.push((res.clone(), compute_data_routes_(tables, res)));
        for match_ in &res.context().matches {
            let match_ = match_.upgrade().unwrap();
            if !Arc::ptr_eq(&match_, res) {
                let match_routes = compute_data_routes_(tables, &match_);
                routes.push((match_, match_routes));
            }
        }
    }
    routes
}

pub(crate) fn disable_matches_data_routes(_tables: &mut Tables, res: &mut Arc<Resource>) {
    if res.context.is_some() {
        get_mut_unchecked(res).context_mut().valid_data_routes = false;
        for match_ in &res.context().matches {
            let mut match_ = match_.upgrade().unwrap();
            if !Arc::ptr_eq(&match_, res) {
                get_mut_unchecked(&mut match_)
                    .context_mut()
                    .valid_data_routes = false;
            }
        }
    }
}

macro_rules! treat_timestamp {
    ($hlc:expr, $payload:expr, $drop:expr) => {
        // if an HLC was configured (via Config.add_timestamp),
        // check DataInfo and add a timestamp if there isn't
        if let Some(hlc) = $hlc {
            if let PushBody::Put(data) = &mut $payload {
                if let Some(ref ts) = data.timestamp {
                    // Timestamp is present; update HLC with it (possibly raising error if delta exceed)
                    match hlc.update_with_timestamp(ts) {
                        Ok(()) => (),
                        Err(e) => {
                            if $drop {
                                log::error!(
                                    "Error treating timestamp for received Data ({}). Drop it!",
                                    e
                                );
                                return;
                            } else {
                                data.timestamp = Some(hlc.new_timestamp());
                                log::error!(
                                    "Error treating timestamp for received Data ({}). Replace timestamp: {:?}",
                                    e,
                                    data.timestamp);
                            }
                        }
                    }
                } else {
                    // Timestamp not present; add one
                    data.timestamp = Some(hlc.new_timestamp());
                    log::trace!("Adding timestamp to DataInfo: {:?}", data.timestamp);
                }
            }
        }
    }
}

#[inline]
fn get_data_route(
    tables: &Tables,
    face: &FaceState,
    res: &Option<Arc<Resource>>,
    expr: &mut RoutingExpr,
    routing_context: u64,
) -> Arc<Route> {
    match tables.whatami {
        WhatAmI::Router => match face.whatami {
            WhatAmI::Router => {
                let routers_net = tables.hat.routers_net.as_ref().unwrap();
                let local_context = routers_net.get_local_context(routing_context, face.link_id);
                res.as_ref()
                    .and_then(|res| res.routers_data_route(local_context))
                    .unwrap_or_else(|| {
                        compute_data_route(tables, expr, Some(local_context), face.whatami)
                    })
            }
            WhatAmI::Peer => {
                if tables.hat.full_net(WhatAmI::Peer) {
                    let peers_net = tables.hat.peers_net.as_ref().unwrap();
                    let local_context = peers_net.get_local_context(routing_context, face.link_id);
                    res.as_ref()
                        .and_then(|res| res.peers_data_route(local_context))
                        .unwrap_or_else(|| {
                            compute_data_route(tables, expr, Some(local_context), face.whatami)
                        })
                } else {
                    res.as_ref()
                        .and_then(|res| res.peer_data_route())
                        .unwrap_or_else(|| compute_data_route(tables, expr, None, face.whatami))
                }
            }
            _ => res
                .as_ref()
                .and_then(|res| res.routers_data_route(0))
                .unwrap_or_else(|| compute_data_route(tables, expr, None, face.whatami)),
        },
        WhatAmI::Peer => {
            if tables.hat.full_net(WhatAmI::Peer) {
                match face.whatami {
                    WhatAmI::Router | WhatAmI::Peer => {
                        let peers_net = tables.hat.peers_net.as_ref().unwrap();
                        let local_context =
                            peers_net.get_local_context(routing_context, face.link_id);
                        res.as_ref()
                            .and_then(|res| res.peers_data_route(local_context))
                            .unwrap_or_else(|| {
                                compute_data_route(tables, expr, Some(local_context), face.whatami)
                            })
                    }
                    _ => res
                        .as_ref()
                        .and_then(|res| res.peers_data_route(0))
                        .unwrap_or_else(|| compute_data_route(tables, expr, None, face.whatami)),
                }
            } else {
                res.as_ref()
                    .and_then(|res| match face.whatami {
                        WhatAmI::Client => res.client_data_route(),
                        _ => res.peer_data_route(),
                    })
                    .unwrap_or_else(|| compute_data_route(tables, expr, None, face.whatami))
            }
        }
        _ => res
            .as_ref()
            .and_then(|res| res.client_data_route())
            .unwrap_or_else(|| compute_data_route(tables, expr, None, face.whatami)),
    }
}

#[inline]
fn get_matching_pulls(
    tables: &Tables,
    res: &Option<Arc<Resource>>,
    expr: &mut RoutingExpr,
) -> Arc<PullCaches> {
    res.as_ref()
        .and_then(|res| res.context.as_ref())
        .map(|ctx| ctx.matching_pulls.clone())
        .unwrap_or_else(|| compute_matching_pulls(tables, expr))
}

macro_rules! cache_data {
    (
        $matching_pulls:expr,
        $expr:expr,
        $payload:expr
    ) => {
        for context in $matching_pulls.iter() {
            get_mut_unchecked(&mut context.clone())
                .last_values
                .insert($expr.full_expr().to_string(), $payload.clone());
        }
    };
}

#[inline]
fn should_route(
    tables: &Tables,
    src_face: &FaceState,
    outface: &Arc<FaceState>,
    expr: &mut RoutingExpr,
) -> bool {
    if src_face.id != outface.id
        && match (src_face.mcast_group.as_ref(), outface.mcast_group.as_ref()) {
            (Some(l), Some(r)) => l != r,
            _ => true,
        }
    {
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

#[cfg(feature = "stats")]
macro_rules! inc_stats {
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
                    PushBody::Put(p) => {
                        stats.[<$txrx _z_put_msgs>].[<inc_ $space>](1);
                        stats.[<$txrx _z_put_pl_bytes>].[<inc_ $space>](p.payload.len());
                    }
                    PushBody::Del(_) => {
                        stats.[<$txrx _z_del_msgs>].[<inc_ $space>](1);
                    }
                }
            }
        }
    };
}

#[allow(clippy::too_many_arguments)]
pub fn full_reentrant_route_data(
    tables_ref: &RwLock<Tables>,
    face: &FaceState,
    expr: &WireExpr,
    ext_qos: ext::QoSType,
    mut payload: PushBody,
    routing_context: u64,
) {
    let tables = zread!(tables_ref);
    match tables.get_mapping(face, &expr.scope, expr.mapping).cloned() {
        Some(prefix) => {
            log::trace!(
                "Route data for res {}{}",
                prefix.expr(),
                expr.suffix.as_ref()
            );
            let mut expr = RoutingExpr::new(&prefix, expr.suffix.as_ref());

            #[cfg(feature = "stats")]
            let admin = expr.full_expr().starts_with("@/");
            #[cfg(feature = "stats")]
            if !admin {
                inc_stats!(face, rx, user, payload)
            } else {
                inc_stats!(face, rx, admin, payload)
            }

            if tables.whatami != WhatAmI::Router
                || face.whatami != WhatAmI::Peer
                || tables.hat.peers_net.is_none()
                || tables.zid
                    == *tables.hat.elect_router(
                        &tables.zid,
                        expr.full_expr(),
                        tables.hat.get_router_links(face.zid),
                    )
            {
                let res = Resource::get_resource(&prefix, expr.suffix);
                let route = get_data_route(&tables, face, &res, &mut expr, routing_context);
                let matching_pulls = get_matching_pulls(&tables, &res, &mut expr);

                if !(route.is_empty() && matching_pulls.is_empty()) {
                    treat_timestamp!(&tables.hlc, payload, tables.drop_future_timestamp);

                    if route.len() == 1 && matching_pulls.len() == 0 {
                        let (outface, key_expr, context) = route.values().next().unwrap();
                        if should_route(&tables, face, outface, &mut expr) {
                            drop(tables);
                            #[cfg(feature = "stats")]
                            if !admin {
                                inc_stats!(face, tx, user, payload)
                            } else {
                                inc_stats!(face, tx, admin, payload)
                            }

                            outface.primitives.send_push(Push {
                                wire_expr: key_expr.into(),
                                ext_qos,
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType {
                                    node_id: context.unwrap_or(0),
                                },
                                payload,
                            })
                        }
                    } else {
                        if !matching_pulls.is_empty() {
                            let lock = zlock!(tables.pull_caches_lock);
                            cache_data!(matching_pulls, expr, payload);
                            drop(lock);
                        }

                        if tables.whatami == WhatAmI::Router {
                            let route = route
                                .values()
                                .filter(|(outface, _key_expr, _context)| {
                                    should_route(&tables, face, outface, &mut expr)
                                })
                                .cloned()
                                .collect::<Vec<Direction>>();

                            drop(tables);
                            for (outface, key_expr, context) in route {
                                #[cfg(feature = "stats")]
                                if !admin {
                                    inc_stats!(face, tx, user, payload)
                                } else {
                                    inc_stats!(face, tx, admin, payload)
                                }

                                outface.primitives.send_push(Push {
                                    wire_expr: key_expr,
                                    ext_qos,
                                    ext_tstamp: None,
                                    ext_nodeid: ext::NodeIdType {
                                        node_id: context.unwrap_or(0),
                                    },
                                    payload: payload.clone(),
                                })
                            }
                        } else {
                            drop(tables);
                            for (outface, key_expr, context) in route.values() {
                                if face.id != outface.id
                                    && match (
                                        face.mcast_group.as_ref(),
                                        outface.mcast_group.as_ref(),
                                    ) {
                                        (Some(l), Some(r)) => l != r,
                                        _ => true,
                                    }
                                {
                                    #[cfg(feature = "stats")]
                                    if !admin {
                                        inc_stats!(face, tx, user, payload)
                                    } else {
                                        inc_stats!(face, tx, admin, payload)
                                    }

                                    outface.primitives.send_push(Push {
                                        wire_expr: key_expr.into(),
                                        ext_qos,
                                        ext_tstamp: None,
                                        ext_nodeid: ext::NodeIdType {
                                            node_id: context.unwrap_or(0),
                                        },
                                        payload: payload.clone(),
                                    })
                                }
                            }
                        }
                    }
                }
            }
        }
        None => {
            log::error!("Route data with unknown scope {}!", expr.scope);
        }
    }
}

pub fn pull_data(tables_ref: &RwLock<Tables>, face: &Arc<FaceState>, expr: WireExpr) {
    let tables = zread!(tables_ref);
    match tables.get_mapping(face, &expr.scope, expr.mapping) {
        Some(prefix) => match Resource::get_resource(prefix, expr.suffix.as_ref()) {
            Some(mut res) => {
                let res = get_mut_unchecked(&mut res);
                match res.session_ctxs.get_mut(&face.id) {
                    Some(ctx) => match &ctx.subs {
                        Some(_subinfo) => {
                            // let reliability = subinfo.reliability;
                            let lock = zlock!(tables.pull_caches_lock);
                            let route = get_mut_unchecked(ctx)
                                .last_values
                                .drain()
                                .map(|(name, sample)| {
                                    (
                                        Resource::get_best_key(&tables.root_res, &name, face.id)
                                            .to_owned(),
                                        sample,
                                    )
                                })
                                .collect::<Vec<(WireExpr, PushBody)>>();
                            drop(lock);
                            drop(tables);
                            for (key_expr, payload) in route {
                                face.primitives.send_push(Push {
                                    wire_expr: key_expr,
                                    ext_qos: ext::QoSType::push_default(),
                                    ext_tstamp: None,
                                    ext_nodeid: ext::NodeIdType::default(),
                                    payload,
                                });
                            }
                        }
                        None => {
                            log::error!(
                                "Pull data for unknown subscription {} (no info)!",
                                prefix.expr() + expr.suffix.as_ref()
                            );
                        }
                    },
                    None => {
                        log::error!(
                            "Pull data for unknown subscription {} (no context)!",
                            prefix.expr() + expr.suffix.as_ref()
                        );
                    }
                }
            }
            None => {
                log::error!(
                    "Pull data for unknown subscription {} (no resource)!",
                    prefix.expr() + expr.suffix.as_ref()
                );
            }
        },
        None => {
            log::error!("Pull data with unknown scope {}!", expr.scope);
        }
    };
}
