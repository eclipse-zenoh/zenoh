//
// Copyright (c) 2022 ZettaScale Technology
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
use petgraph::graph::NodeIndex;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;
use std::sync::Arc;
use std::sync::RwLock;
use zenoh_core::zread;
use zenoh_protocol_core::key_expr::OwnedKeyExpr;
use zenoh_sync::get_mut_unchecked;

use zenoh_protocol::io::ZBuf;
use zenoh_protocol::proto::{DataInfo, RoutingContext};
use zenoh_protocol_core::{
    Channel, CongestionControl, Priority, Reliability, SubInfo, SubMode, WhatAmI, WireExpr, ZInt,
    ZenohId,
};

use super::face::FaceState;
use super::network::Network;
use super::resource::{elect_router, PullCaches, Resource, Route, SessionContext};
use super::router::Tables;

#[inline]
fn send_sourced_subscription_to_net_childs(
    tables: &Tables,
    net: &Network,
    childs: &[NodeIndex],
    res: &Arc<Resource>,
    src_face: Option<&Arc<FaceState>>,
    sub_info: &SubInfo,
    routing_context: Option<RoutingContext>,
) {
    for child in childs {
        if net.graph.contains_node(*child) {
            match tables.get_face(&net.graph[*child].zid).cloned() {
                Some(mut someface) => {
                    if src_face.is_none() || someface.id != src_face.unwrap().id {
                        let key_expr = Resource::decl_key(res, &mut someface);

                        log::debug!("Send subscription {} on {}", res.expr(), someface);

                        someface
                            .primitives
                            .decl_subscriber(&key_expr, sub_info, routing_context);
                    }
                }
                None => log::trace!("Unable to find face for zid {}", net.graph[*child].zid),
            }
        }
    }
}

#[inline]
fn propagate_simple_subscription_to(
    whatami: WhatAmI,
    dst_face: &mut Arc<FaceState>,
    res: &Arc<Resource>,
    sub_info: &SubInfo,
    src_face: &mut Arc<FaceState>,
    full_peer_net: bool,
) {
    if src_face.id != dst_face.id
        && !dst_face.local_subs.contains(res)
        && match whatami {
            WhatAmI::Router => {
                if full_peer_net {
                    dst_face.whatami == WhatAmI::Client
                } else {
                    (src_face.whatami != WhatAmI::Peer || dst_face.whatami != WhatAmI::Peer)
                        && dst_face.whatami != WhatAmI::Router
                }
            }
            WhatAmI::Peer => {
                if full_peer_net {
                    dst_face.whatami == WhatAmI::Client
                } else {
                    src_face.whatami == WhatAmI::Client || dst_face.whatami == WhatAmI::Client
                }
            }
            _ => (src_face.whatami == WhatAmI::Client || dst_face.whatami == WhatAmI::Client),
        }
    {
        get_mut_unchecked(dst_face).local_subs.insert(res.clone());
        let key_expr = Resource::decl_key(res, dst_face);
        dst_face
            .primitives
            .decl_subscriber(&key_expr, sub_info, None);
    }
}

fn propagate_simple_subscription(
    tables: &mut Tables,
    res: &Arc<Resource>,
    sub_info: &SubInfo,
    src_face: &mut Arc<FaceState>,
) {
    let full_peer_net = tables.full_net(WhatAmI::Peer);
    for dst_face in &mut tables.faces.values_mut() {
        propagate_simple_subscription_to(
            tables.whatami,
            dst_face,
            res,
            sub_info,
            src_face,
            full_peer_net,
        );
    }
}

fn propagate_sourced_subscription(
    tables: &Tables,
    res: &Arc<Resource>,
    sub_info: &SubInfo,
    src_face: Option<&Arc<FaceState>>,
    source: &ZenohId,
    net_type: WhatAmI,
) {
    let net = tables.get_net(net_type).unwrap();
    match net.get_idx(source) {
        Some(tree_sid) => {
            if net.trees.len() > tree_sid.index() {
                send_sourced_subscription_to_net_childs(
                    tables,
                    net,
                    &net.trees[tree_sid.index()].childs,
                    res,
                    src_face,
                    sub_info,
                    Some(RoutingContext::new(tree_sid.index() as ZInt)),
                );
            } else {
                log::trace!(
                    "Propagating sub {}: tree for node {} sid:{} not yet ready",
                    res.expr(),
                    tree_sid.index(),
                    source
                );
            }
        }
        None => log::error!(
            "Error propagating sub {}: cannot get index of {}!",
            res.expr(),
            source
        ),
    }
}

fn register_router_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    sub_info: &SubInfo,
    router: ZenohId,
) {
    if !res.context().router_subs.contains(&router) {
        // Register router subscription
        {
            log::debug!(
                "Register router subscription {} (router: {})",
                res.expr(),
                router
            );
            get_mut_unchecked(res)
                .context_mut()
                .router_subs
                .insert(router);
            tables.router_subs.insert(res.clone());
        }

        // Propagate subscription to routers
        propagate_sourced_subscription(tables, res, sub_info, Some(face), &router, WhatAmI::Router);
    }
    // Propagate subscription to peers
    if tables.full_net(WhatAmI::Peer) && face.whatami != WhatAmI::Peer {
        register_peer_subscription(tables, face, res, sub_info, tables.zid)
    }

    // Propagate subscription to clients
    propagate_simple_subscription(tables, res, sub_info, face);
}

pub fn declare_router_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    expr: &WireExpr,
    sub_info: &SubInfo,
    router: ZenohId,
) {
    match tables.get_mapping(face, &expr.scope).cloned() {
        Some(mut prefix) => {
            let mut res = Resource::make_resource(tables, &mut prefix, expr.suffix.as_ref());
            Resource::match_resource(tables, &mut res);
            register_router_subscription(tables, face, &mut res, sub_info, router);

            compute_matches_data_routes(tables, &mut res);
        }
        None => log::error!(
            "Declare router subscription for unknown scope {}!",
            expr.scope
        ),
    }
}

fn register_peer_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    sub_info: &SubInfo,
    peer: ZenohId,
) {
    if !res.context().peer_subs.contains(&peer) {
        // Register peer subscription
        {
            log::debug!("Register peer subscription {} (peer: {})", res.expr(), peer);
            get_mut_unchecked(res).context_mut().peer_subs.insert(peer);
            tables.peer_subs.insert(res.clone());
        }

        // Propagate subscription to peers
        propagate_sourced_subscription(tables, res, sub_info, Some(face), &peer, WhatAmI::Peer);
    }

    if tables.whatami == WhatAmI::Peer {
        // Propagate subscription to clients
        propagate_simple_subscription(tables, res, sub_info, face);
    }
}

pub fn declare_peer_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    expr: &WireExpr,
    sub_info: &SubInfo,
    peer: ZenohId,
) {
    match tables.get_mapping(face, &expr.scope).cloned() {
        Some(mut prefix) => {
            let mut res = Resource::make_resource(tables, &mut prefix, expr.suffix.as_ref());
            Resource::match_resource(tables, &mut res);
            register_peer_subscription(tables, face, &mut res, sub_info, peer);

            if tables.whatami == WhatAmI::Router {
                let mut propa_sub_info = sub_info.clone();
                propa_sub_info.mode = SubMode::Push;
                register_router_subscription(tables, face, &mut res, &propa_sub_info, tables.zid);
            }

            compute_matches_data_routes(tables, &mut res);
        }
        None => log::error!(
            "Declare router subscription for unknown scope {}!",
            expr.scope
        ),
    }
}

fn register_client_subscription(
    _tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    sub_info: &SubInfo,
) {
    // Register subscription
    {
        let res = get_mut_unchecked(res);
        log::debug!("Register subscription {} for {}", res.expr(), face);
        match res.session_ctxs.get_mut(&face.id) {
            Some(ctx) => match &ctx.subs {
                Some(info) => {
                    if SubMode::Pull == info.mode {
                        get_mut_unchecked(ctx).subs = Some(sub_info.clone());
                    }
                }
                None => {
                    get_mut_unchecked(ctx).subs = Some(sub_info.clone());
                }
            },
            None => {
                res.session_ctxs.insert(
                    face.id,
                    Arc::new(SessionContext {
                        face: face.clone(),
                        local_expr_id: None,
                        remote_expr_id: None,
                        subs: Some(sub_info.clone()),
                        qabl: HashMap::new(),
                        last_values: HashMap::new(),
                    }),
                );
            }
        }
    }
    get_mut_unchecked(face).remote_subs.insert(res.clone());
}

pub fn declare_client_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    expr: &WireExpr,
    sub_info: &SubInfo,
) {
    log::debug!("Register client subscription");
    match tables.get_mapping(face, &expr.scope).cloned() {
        Some(mut prefix) => {
            let mut res = Resource::make_resource(tables, &mut prefix, expr.suffix.as_ref());
            log::debug!("Register client subscription {}", res.expr());
            Resource::match_resource(tables, &mut res);

            register_client_subscription(tables, face, &mut res, sub_info);
            match tables.whatami {
                WhatAmI::Router => {
                    let mut propa_sub_info = sub_info.clone();
                    propa_sub_info.mode = SubMode::Push;
                    register_router_subscription(
                        tables,
                        face,
                        &mut res,
                        &propa_sub_info,
                        tables.zid,
                    );
                }
                WhatAmI::Peer => {
                    if tables.full_net(WhatAmI::Peer) {
                        let mut propa_sub_info = sub_info.clone();
                        propa_sub_info.mode = SubMode::Push;
                        register_peer_subscription(
                            tables,
                            face,
                            &mut res,
                            &propa_sub_info,
                            tables.zid,
                        );
                    } else {
                        propagate_simple_subscription(tables, &res, sub_info, face);
                    }
                }
                _ => {
                    propagate_simple_subscription(tables, &res, sub_info, face);
                }
            }

            compute_matches_data_routes(tables, &mut res);
        }
        None => log::error!("Declare subscription for unknown scope {}!", expr.scope),
    }
}

#[inline]
fn remote_router_subs(tables: &Tables, res: &Arc<Resource>) -> bool {
    res.context.is_some()
        && res
            .context()
            .router_subs
            .iter()
            .any(|peer| peer != &tables.zid)
}

#[inline]
fn remote_peer_subs(tables: &Tables, res: &Arc<Resource>) -> bool {
    res.context.is_some()
        && res
            .context()
            .peer_subs
            .iter()
            .any(|peer| peer != &tables.zid)
}

#[inline]
fn client_subs(res: &Arc<Resource>) -> Vec<Arc<FaceState>> {
    res.session_ctxs
        .values()
        .filter_map(|ctx| {
            if ctx.subs.is_some() {
                Some(ctx.face.clone())
            } else {
                None
            }
        })
        .collect()
}

#[inline]
fn send_forget_sourced_subscription_to_net_childs(
    tables: &Tables,
    net: &Network,
    childs: &[NodeIndex],
    res: &Arc<Resource>,
    src_face: Option<&Arc<FaceState>>,
    routing_context: Option<RoutingContext>,
) {
    for child in childs {
        if net.graph.contains_node(*child) {
            match tables.get_face(&net.graph[*child].zid).cloned() {
                Some(mut someface) => {
                    if src_face.is_none() || someface.id != src_face.unwrap().id {
                        let key_expr = Resource::decl_key(res, &mut someface);

                        log::debug!("Send forget subscription {} on {}", res.expr(), someface);

                        someface
                            .primitives
                            .forget_subscriber(&key_expr, routing_context);
                    }
                }
                None => log::trace!("Unable to find face for zid {}", net.graph[*child].zid),
            }
        }
    }
}

fn propagate_forget_simple_subscription(tables: &mut Tables, res: &Arc<Resource>) {
    for face in tables.faces.values_mut() {
        if face.local_subs.contains(res) {
            let key_expr = Resource::get_best_key(res, "", face.id);
            face.primitives.forget_subscriber(&key_expr, None);

            get_mut_unchecked(face).local_subs.remove(res);
        }
    }
}

fn propagate_forget_sourced_subscription(
    tables: &Tables,
    res: &Arc<Resource>,
    src_face: Option<&Arc<FaceState>>,
    source: &ZenohId,
    net_type: WhatAmI,
) {
    let net = tables.get_net(net_type).unwrap();
    match net.get_idx(source) {
        Some(tree_sid) => {
            if net.trees.len() > tree_sid.index() {
                send_forget_sourced_subscription_to_net_childs(
                    tables,
                    net,
                    &net.trees[tree_sid.index()].childs,
                    res,
                    src_face,
                    Some(RoutingContext::new(tree_sid.index() as ZInt)),
                );
            } else {
                log::trace!(
                    "Propagating forget sub {}: tree for node {} sid:{} not yet ready",
                    res.expr(),
                    tree_sid.index(),
                    source
                );
            }
        }
        None => log::error!(
            "Error propagating forget sub {}: cannot get index of {}!",
            res.expr(),
            source
        ),
    }
}

fn unregister_router_subscription(tables: &mut Tables, res: &mut Arc<Resource>, router: &ZenohId) {
    log::debug!(
        "Unregister router subscription {} (router: {})",
        res.expr(),
        router
    );
    get_mut_unchecked(res)
        .context_mut()
        .router_subs
        .retain(|sub| sub != router);

    if res.context().router_subs.is_empty() {
        tables.router_subs.retain(|sub| !Arc::ptr_eq(sub, res));

        if tables.full_net(WhatAmI::Peer) {
            undeclare_peer_subscription(tables, None, res, &tables.zid.clone());
        }
        propagate_forget_simple_subscription(tables, res);
    }
}

fn undeclare_router_subscription(
    tables: &mut Tables,
    face: Option<&Arc<FaceState>>,
    res: &mut Arc<Resource>,
    router: &ZenohId,
) {
    if res.context().router_subs.contains(router) {
        unregister_router_subscription(tables, res, router);
        propagate_forget_sourced_subscription(tables, res, face, router, WhatAmI::Router);
    }
}

pub fn forget_router_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    expr: &WireExpr,
    router: &ZenohId,
) {
    match tables.get_mapping(face, &expr.scope) {
        Some(prefix) => match Resource::get_resource(prefix, expr.suffix.as_ref()) {
            Some(mut res) => {
                undeclare_router_subscription(tables, Some(face), &mut res, router);

                compute_matches_data_routes(tables, &mut res);
                Resource::clean(&mut res)
            }
            None => log::error!("Undeclare unknown router subscription!"),
        },
        None => log::error!("Undeclare router subscription with unknown scope!"),
    }
}

fn unregister_peer_subscription(tables: &mut Tables, res: &mut Arc<Resource>, peer: &ZenohId) {
    log::debug!(
        "Unregister peer subscription {} (peer: {})",
        res.expr(),
        peer
    );
    get_mut_unchecked(res)
        .context_mut()
        .peer_subs
        .retain(|sub| sub != peer);

    if res.context().peer_subs.is_empty() {
        tables.peer_subs.retain(|sub| !Arc::ptr_eq(sub, res));

        if tables.whatami == WhatAmI::Peer {
            propagate_forget_simple_subscription(tables, res);
        }
    }
}

fn undeclare_peer_subscription(
    tables: &mut Tables,
    face: Option<&Arc<FaceState>>,
    res: &mut Arc<Resource>,
    peer: &ZenohId,
) {
    if res.context().peer_subs.contains(peer) {
        unregister_peer_subscription(tables, res, peer);
        propagate_forget_sourced_subscription(tables, res, face, peer, WhatAmI::Peer);
    }
}

pub fn forget_peer_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    expr: &WireExpr,
    peer: &ZenohId,
) {
    match tables.get_mapping(face, &expr.scope) {
        Some(prefix) => match Resource::get_resource(prefix, expr.suffix.as_ref()) {
            Some(mut res) => {
                undeclare_peer_subscription(tables, Some(face), &mut res, peer);

                if tables.whatami == WhatAmI::Router {
                    let client_subs = res.session_ctxs.values().any(|ctx| ctx.subs.is_some());
                    let peer_subs = remote_peer_subs(tables, &res);
                    if !client_subs && !peer_subs {
                        undeclare_router_subscription(tables, None, &mut res, &tables.zid.clone());
                    }
                }

                compute_matches_data_routes(tables, &mut res);
                Resource::clean(&mut res)
            }
            None => log::error!("Undeclare unknown peer subscription!"),
        },
        None => log::error!("Undeclare peer subscription with unknown scope!"),
    }
}

pub(crate) fn undeclare_client_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
) {
    log::debug!("Unregister client subscription {} for {}", res.expr(), face);
    if let Some(ctx) = get_mut_unchecked(res).session_ctxs.get_mut(&face.id) {
        get_mut_unchecked(ctx).subs = None;
    }
    get_mut_unchecked(face).remote_subs.remove(res);

    let mut client_subs = client_subs(res);
    let router_subs = remote_router_subs(tables, res);
    let peer_subs = remote_peer_subs(tables, res);
    match tables.whatami {
        WhatAmI::Router => {
            if client_subs.is_empty() && !peer_subs {
                undeclare_router_subscription(tables, None, res, &tables.zid.clone());
            }
        }
        WhatAmI::Peer => {
            if client_subs.is_empty() {
                if tables.full_net(WhatAmI::Peer) {
                    undeclare_peer_subscription(tables, None, res, &tables.zid.clone());
                } else {
                    propagate_forget_simple_subscription(tables, res);
                }
            }
        }
        _ => {
            if client_subs.is_empty() {
                propagate_forget_simple_subscription(tables, res);
            }
        }
    }
    if client_subs.len() == 1 && !router_subs && !peer_subs {
        let face = &mut client_subs[0];
        if face.local_subs.contains(res) {
            let key_expr = Resource::get_best_key(res, "", face.id);
            face.primitives.forget_subscriber(&key_expr, None);

            get_mut_unchecked(face).local_subs.remove(res);
        }
    }

    compute_matches_data_routes(tables, res);
    Resource::clean(res)
}

pub fn forget_client_subscription(tables: &mut Tables, face: &mut Arc<FaceState>, expr: &WireExpr) {
    match tables.get_mapping(face, &expr.scope) {
        Some(prefix) => match Resource::get_resource(prefix, expr.suffix.as_ref()) {
            Some(mut res) => {
                undeclare_client_subscription(tables, face, &mut res);
            }
            None => log::error!("Undeclare unknown subscription!"),
        },
        None => log::error!("Undeclare subscription with unknown scope!"),
    }
}

pub(crate) fn pubsub_new_face(tables: &mut Tables, face: &mut Arc<FaceState>) {
    let sub_info = SubInfo {
        reliability: Reliability::Reliable, // @TODO
        mode: SubMode::Push,
    };
    match tables.whatami {
        WhatAmI::Router => {
            if face.whatami == WhatAmI::Client {
                for sub in &tables.router_subs {
                    get_mut_unchecked(face).local_subs.insert(sub.clone());
                    let key_expr = Resource::decl_key(sub, face);
                    face.primitives.decl_subscriber(&key_expr, &sub_info, None);
                }
            }
        }
        WhatAmI::Peer => {
            if tables.full_net(WhatAmI::Peer) {
                if face.whatami == WhatAmI::Client {
                    for sub in &tables.peer_subs {
                        get_mut_unchecked(face).local_subs.insert(sub.clone());
                        let key_expr = Resource::decl_key(sub, face);
                        face.primitives.decl_subscriber(&key_expr, &sub_info, None);
                    }
                }
            } else {
                for src_face in tables
                    .faces
                    .values()
                    .cloned()
                    .collect::<Vec<Arc<FaceState>>>()
                {
                    for sub in &src_face.remote_subs {
                        propagate_simple_subscription_to(
                            tables.whatami,
                            face,
                            sub,
                            &sub_info,
                            &mut src_face.clone(),
                            false,
                        );
                    }
                }
            }
        }
        WhatAmI::Client => {
            for src_face in tables
                .faces
                .values()
                .cloned()
                .collect::<Vec<Arc<FaceState>>>()
            {
                for sub in &src_face.remote_subs {
                    propagate_simple_subscription_to(
                        tables.whatami,
                        face,
                        sub,
                        &sub_info,
                        &mut src_face.clone(),
                        false,
                    );
                }
            }
        }
    }
}

pub(crate) fn pubsub_remove_node(tables: &mut Tables, node: &ZenohId, net_type: WhatAmI) {
    match net_type {
        WhatAmI::Router => {
            for mut res in tables
                .router_subs
                .iter()
                .filter(|res| res.context().router_subs.contains(node))
                .cloned()
                .collect::<Vec<Arc<Resource>>>()
            {
                unregister_router_subscription(tables, &mut res, node);

                compute_matches_data_routes(tables, &mut res);
                Resource::clean(&mut res)
            }
        }
        WhatAmI::Peer => {
            for mut res in tables
                .peer_subs
                .iter()
                .filter(|res| res.context().peer_subs.contains(node))
                .cloned()
                .collect::<Vec<Arc<Resource>>>()
            {
                unregister_peer_subscription(tables, &mut res, node);

                if tables.whatami == WhatAmI::Router {
                    let client_subs = res.session_ctxs.values().any(|ctx| ctx.subs.is_some());
                    let peer_subs = remote_peer_subs(tables, &res);
                    if !client_subs && !peer_subs {
                        undeclare_router_subscription(tables, None, &mut res, &tables.zid.clone());
                    }
                }

                compute_matches_data_routes(tables, &mut res);
                Resource::clean(&mut res)
            }
        }
        _ => (),
    }
}

pub(crate) fn pubsub_tree_change(
    tables: &mut Tables,
    new_childs: &[Vec<NodeIndex>],
    net_type: WhatAmI,
) {
    // propagate subs to new childs
    for (tree_sid, tree_childs) in new_childs.iter().enumerate() {
        if !tree_childs.is_empty() {
            let net = tables.get_net(net_type).unwrap();
            let tree_idx = NodeIndex::new(tree_sid);
            if net.graph.contains_node(tree_idx) {
                let tree_id = net.graph[tree_idx].zid;

                let subs_res = match net_type {
                    WhatAmI::Router => &tables.router_subs,
                    _ => &tables.peer_subs,
                };

                for res in subs_res {
                    let subs = match net_type {
                        WhatAmI::Router => &res.context().router_subs,
                        _ => &res.context().peer_subs,
                    };
                    for sub in subs {
                        if *sub == tree_id {
                            let sub_info = SubInfo {
                                reliability: Reliability::Reliable, // @TODO
                                mode: SubMode::Push,
                            };
                            send_sourced_subscription_to_net_childs(
                                tables,
                                net,
                                tree_childs,
                                res,
                                None,
                                &sub_info,
                                Some(RoutingContext::new(tree_sid as ZInt)),
                            );
                        }
                    }
                }
            }
        }
    }

    // recompute routes
    compute_data_routes_from(tables, &mut tables.root_res.clone());
}

#[inline]
fn insert_faces_for_subs(
    route: &mut Route,
    prefix: &Arc<Resource>,
    suffix: &str,
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
                                    let key_expr = Resource::get_best_key(prefix, suffix, face.id);
                                    (
                                        face.clone(),
                                        key_expr.to_owned(),
                                        if source != 0 {
                                            Some(RoutingContext::new(source as ZInt))
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
    prefix: &Arc<Resource>,
    suffix: &str,
    source: Option<usize>,
    source_type: WhatAmI,
) -> Arc<Route> {
    let mut route = HashMap::new();
    let key_expr = prefix.expr() + suffix;
    if key_expr.ends_with('/') {
        return Arc::new(route);
    }
    let key_expr = match OwnedKeyExpr::try_from(key_expr) {
        Ok(ke) => ke,
        Err(e) => {
            log::warn!("Invalid KE reached the system: {}", e);
            return Arc::new(route);
        }
    };
    let res = Resource::get_resource(prefix, suffix);
    let matches = res
        .as_ref()
        .and_then(|res| res.context.as_ref())
        .map(|ctx| Cow::from(&ctx.matches))
        .unwrap_or_else(|| Cow::from(Resource::get_matches(tables, &key_expr)));

    let master = tables.whatami != WhatAmI::Router
        || !tables.full_net(WhatAmI::Peer)
        || *elect_router(&key_expr, &tables.shared_nodes) == tables.zid;

    for mres in matches.iter() {
        let mres = mres.upgrade().unwrap();
        if tables.whatami == WhatAmI::Router {
            if master || source_type == WhatAmI::Router {
                let net = tables.routers_net.as_ref().unwrap();
                let router_source = match source_type {
                    WhatAmI::Router => source.unwrap(),
                    _ => net.idx.index(),
                };
                insert_faces_for_subs(
                    &mut route,
                    prefix,
                    suffix,
                    tables,
                    net,
                    router_source,
                    &mres.context().router_subs,
                );
            }

            if master || source_type != WhatAmI::Router {
                let net = tables.peers_net.as_ref().unwrap();
                let peer_source = match source_type {
                    WhatAmI::Peer => source.unwrap(),
                    _ => net.idx.index(),
                };
                insert_faces_for_subs(
                    &mut route,
                    prefix,
                    suffix,
                    tables,
                    net,
                    peer_source,
                    &mres.context().peer_subs,
                );
            }
        }

        if tables.whatami == WhatAmI::Peer && tables.full_net(WhatAmI::Peer) {
            let net = tables.peers_net.as_ref().unwrap();
            let peer_source = match source_type {
                WhatAmI::Router | WhatAmI::Peer => source.unwrap(),
                _ => net.idx.index(),
            };
            insert_faces_for_subs(
                &mut route,
                prefix,
                suffix,
                tables,
                net,
                peer_source,
                &mres.context().peer_subs,
            );
        }

        // println!("compute data route for {}{}", prefix.expr(), suffix);
        if tables.whatami != WhatAmI::Router || master || source_type == WhatAmI::Router {
            for (sid, context) in &mres.session_ctxs {
                if let Some(subinfo) = &context.subs {
                    // println!("compute data route for {}{} ({} {} {})", prefix.expr(), suffix, tables.whatami, source_type, context.face.whatami);
                    if match tables.whatami {
                        WhatAmI::Router => {
                            (source_type != WhatAmI::Peer || context.face.whatami != WhatAmI::Peer)
                                && context.face.whatami != WhatAmI::Router
                        }
                        _ => {
                            source_type == WhatAmI::Client
                                || context.face.whatami == WhatAmI::Client
                        }
                    } && subinfo.mode == SubMode::Push
                    {
                        // println!("compute data route for {}{} add entry", prefix.expr(), suffix);
                        route.entry(*sid).or_insert_with(|| {
                            let key_expr = Resource::get_best_key(prefix, suffix, *sid);
                            (context.face.clone(), key_expr.to_owned(), None)
                        });
                    }
                }
            }
        }
    }
    Arc::new(route)
}

fn compute_matching_pulls(
    tables: &Tables,
    prefix: &Arc<Resource>,
    suffix: &str,
) -> Arc<PullCaches> {
    let mut pull_caches = vec![];
    let ke = if let Ok(ke) = OwnedKeyExpr::try_from(prefix.expr() + suffix) {
        ke
    } else {
        return Arc::new(pull_caches);
    };
    let res = Resource::get_resource(prefix, suffix);
    let matches = res
        .as_ref()
        .and_then(|res| res.context.as_ref())
        .map(|ctx| Cow::from(&ctx.matches))
        .unwrap_or_else(|| Cow::from(Resource::get_matches(tables, &ke)));

    for mres in matches.iter() {
        let mres = mres.upgrade().unwrap();
        for context in mres.session_ctxs.values() {
            if let Some(subinfo) = &context.subs {
                if subinfo.mode == SubMode::Pull {
                    pull_caches.push(context.clone());
                }
            }
        }
    }
    Arc::new(pull_caches)
}

pub(crate) fn compute_data_routes(tables: &mut Tables, res: &mut Arc<Resource>) {
    if res.context.is_some() {
        let mut res_mut = res.clone();
        let res_mut = get_mut_unchecked(&mut res_mut);
        if tables.whatami == WhatAmI::Router {
            let indexes = tables
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
                    compute_data_route(tables, res, "", Some(idx.index()), WhatAmI::Router);
            }
        }
        if (tables.whatami == WhatAmI::Router || tables.whatami == WhatAmI::Peer)
            && tables.full_net(WhatAmI::Peer)
        {
            let indexes = tables
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
                    compute_data_route(tables, res, "", Some(idx.index()), WhatAmI::Peer);
            }
        }
        if tables.whatami == WhatAmI::Client
            || (tables.whatami == WhatAmI::Peer && !tables.full_net(WhatAmI::Peer))
        {
            res_mut.context_mut().client_data_route =
                Some(compute_data_route(tables, res, "", None, WhatAmI::Client));
        }
        res_mut.context_mut().matching_pulls = compute_matching_pulls(tables, res, "");
    }
}

fn compute_data_routes_from(tables: &mut Tables, res: &mut Arc<Resource>) {
    compute_data_routes(tables, res);
    let res = get_mut_unchecked(res);
    for child in res.childs.values_mut() {
        compute_data_routes_from(tables, child);
    }
}

pub(crate) fn compute_matches_data_routes(tables: &mut Tables, res: &mut Arc<Resource>) {
    if res.context.is_some() {
        compute_data_routes(tables, res);
        let resclone = res.clone();
        for match_ in &mut get_mut_unchecked(res).context_mut().matches {
            if !Arc::ptr_eq(&match_.upgrade().unwrap(), &resclone) {
                compute_data_routes(tables, &mut match_.upgrade().unwrap());
            }
        }
    }
}

macro_rules! treat_timestamp {
    ($hlc:expr, $info:expr, $drop:expr) => {
        // if an HLC was configured (via Config.add_timestamp),
        // check DataInfo and add a timestamp if there isn't
        match $hlc {
            Some(hlc) => {
                if let Some(mut data_info) = $info {
                    if let Some(ref ts) = data_info.timestamp {
                        // Timestamp is present; update HLC with it (possibly raising error if delta exceed)
                        match hlc.update_with_timestamp(ts) {
                            Ok(()) => Some(data_info),
                            Err(e) => {
                                if $drop {
                                    log::error!(
                                        "Error treating timestamp for received Data ({}). Drop it!",
                                        e
                                    );
                                    return;
                                } else {
                                    data_info.timestamp = Some(hlc.new_timestamp());
                                    log::error!(
                                        "Error treating timestamp for received Data ({}). Replace timestamp: {:?}",
                                        e,
                                        data_info.timestamp);
                                    Some(data_info)
                                }
                            }
                        }
                    } else {
                        // Timestamp not present; add one
                        data_info.timestamp = Some(hlc.new_timestamp());
                        log::trace!("Adding timestamp to DataInfo: {:?}", data_info.timestamp);
                        Some(data_info)
                    }
                } else {
                    // No DataInfo; add one with a Timestamp
                    let mut data_info = DataInfo::new();
                    data_info.timestamp = Some(hlc.new_timestamp());
                    Some(data_info)
                }
            },
            None => $info,
        }
    }
}

#[inline]
fn get_data_route(
    tables: &Tables,
    face: &FaceState,
    res: &Option<Arc<Resource>>,
    prefix: &Arc<Resource>,
    suffix: &str,
    routing_context: Option<RoutingContext>,
) -> Arc<Route> {
    match tables.whatami {
        WhatAmI::Router => match face.whatami {
            WhatAmI::Router => {
                let routers_net = tables.routers_net.as_ref().unwrap();
                let local_context = routers_net
                    .get_local_context(routing_context.map(|rc| rc.tree_id), face.link_id);
                res.as_ref()
                    .and_then(|res| res.routers_data_route(local_context))
                    .unwrap_or_else(|| {
                        compute_data_route(
                            tables,
                            prefix,
                            suffix,
                            Some(local_context),
                            face.whatami,
                        )
                    })
            }
            WhatAmI::Peer => {
                let peers_net = tables.peers_net.as_ref().unwrap();
                let local_context =
                    peers_net.get_local_context(routing_context.map(|rc| rc.tree_id), face.link_id);
                res.as_ref()
                    .and_then(|res| res.peers_data_route(local_context))
                    .unwrap_or_else(|| {
                        compute_data_route(
                            tables,
                            prefix,
                            suffix,
                            Some(local_context),
                            face.whatami,
                        )
                    })
            }
            _ => res
                .as_ref()
                .and_then(|res| res.routers_data_route(0))
                .unwrap_or_else(|| compute_data_route(tables, prefix, suffix, None, face.whatami)),
        },
        WhatAmI::Peer => {
            if tables.full_net(WhatAmI::Peer) {
                match face.whatami {
                    WhatAmI::Router | WhatAmI::Peer => {
                        let peers_net = tables.peers_net.as_ref().unwrap();
                        let local_context = peers_net
                            .get_local_context(routing_context.map(|rc| rc.tree_id), face.link_id);
                        res.as_ref()
                            .and_then(|res| res.peers_data_route(local_context))
                            .unwrap_or_else(|| {
                                compute_data_route(
                                    tables,
                                    prefix,
                                    suffix,
                                    Some(local_context),
                                    face.whatami,
                                )
                            })
                    }
                    _ => res
                        .as_ref()
                        .and_then(|res| res.peers_data_route(0))
                        .unwrap_or_else(|| {
                            compute_data_route(tables, prefix, suffix, None, face.whatami)
                        }),
                }
            } else {
                res.as_ref()
                    .and_then(|res| res.client_data_route())
                    .unwrap_or_else(|| {
                        compute_data_route(tables, prefix, suffix, None, face.whatami)
                    })
            }
        }
        _ => res
            .as_ref()
            .and_then(|res| res.client_data_route())
            .unwrap_or_else(|| compute_data_route(tables, prefix, suffix, None, face.whatami)),
    }
}

#[inline]
fn get_matching_pulls(
    tables: &Tables,
    res: &Option<Arc<Resource>>,
    prefix: &Arc<Resource>,
    suffix: &str,
) -> Arc<PullCaches> {
    res.as_ref()
        .and_then(|res| res.context.as_ref())
        .map(|ctx| ctx.matching_pulls.clone())
        .unwrap_or_else(|| compute_matching_pulls(tables, prefix, suffix))
}

macro_rules! send_to_first {
    ($route:expr, $srcface:expr, $payload:expr, $channel:expr, $cong_ctrl:expr, $data_info:expr) => {
        let (outface, key_expr, context) = $route.values().next().unwrap();
        if $srcface.id != outface.id {
            outface
                .primitives
                .send_data(
                    &key_expr,
                    $payload,
                    $channel, // @TODO: Need to check the active subscriptions to determine the right reliability value
                    $cong_ctrl,
                    $data_info,
                    *context,
                )
        }
    }
}

macro_rules! send_to_all {
    ($route:expr, $srcface:expr, $payload:expr, $channel:expr, $cong_ctrl:expr, $data_info:expr) => {
        for (outface, key_expr, context) in $route.values() {
            if $srcface.id != outface.id {
                outface
                    .primitives
                    .send_data(
                        &key_expr,
                        $payload.clone(),
                        $channel, // @TODO: Need to check the active subscriptions to determine the right reliability value
                        $cong_ctrl,
                        $data_info.clone(),
                        *context,
                    )
            }
        }
    }
}

macro_rules! cache_data {
    (
        $matching_pulls:expr,
        $prefix:expr,
        $suffix:expr,
        $payload:expr,
        $info:expr
    ) => {
        for context in $matching_pulls.iter() {
            get_mut_unchecked(&mut context.clone())
                .last_values
                .insert($prefix.expr() + $suffix, ($info.clone(), $payload.clone()));
        }
    };
}

#[allow(clippy::too_many_arguments)]
pub fn full_reentrant_route_data(
    tables_ref: &RwLock<Tables>,
    face: &FaceState,
    expr: &WireExpr,
    channel: Channel,
    congestion_control: CongestionControl,
    info: Option<DataInfo>,
    payload: ZBuf,
    routing_context: Option<RoutingContext>,
) {
    let tables = zread!(tables_ref);
    match tables.get_mapping(face, &expr.scope).cloned() {
        Some(prefix) => {
            log::trace!(
                "Route data for res {}{}",
                prefix.expr(),
                expr.suffix.as_ref()
            );

            let res = Resource::get_resource(&prefix, expr.suffix.as_ref());
            let route = get_data_route(
                &tables,
                face,
                &res,
                &prefix,
                expr.suffix.as_ref(),
                routing_context,
            );
            let matching_pulls = get_matching_pulls(&tables, &res, &prefix, expr.suffix.as_ref());

            if !(route.is_empty() && matching_pulls.is_empty()) {
                let data_info = treat_timestamp!(&tables.hlc, info, tables.drop_future_timestamp);

                if route.len() == 1 && matching_pulls.len() == 0 {
                    drop(tables);
                    send_to_first!(route, face, payload, channel, congestion_control, data_info);
                } else {
                    if !matching_pulls.is_empty() {
                        let lock = zlock!(tables.pull_caches_lock);
                        cache_data!(
                            matching_pulls,
                            prefix,
                            expr.suffix.as_ref(),
                            payload,
                            data_info
                        );
                        drop(lock);
                    }
                    drop(tables);
                    send_to_all!(route, face, payload, channel, congestion_control, data_info);
                }
            }
        }
        None => {
            log::error!("Route data with unknown scope {}!", expr.scope);
        }
    }
}

pub fn pull_data(
    tables: &mut Tables,
    face: &Arc<FaceState>,
    _is_final: bool,
    expr: &WireExpr,
    _pull_id: ZInt,
    _max_samples: &Option<ZInt>,
) {
    match tables.get_mapping(face, &expr.scope) {
        Some(prefix) => match Resource::get_resource(prefix, expr.suffix.as_ref()) {
            Some(mut res) => {
                let res = get_mut_unchecked(&mut res);
                match res.session_ctxs.get_mut(&face.id) {
                    Some(ctx) => match &ctx.subs {
                        Some(subinfo) => {
                            let lock = zlock!(tables.pull_caches_lock);
                            for (name, (info, data)) in &ctx.last_values {
                                let key_expr =
                                    Resource::get_best_key(&tables.root_res, name, face.id);
                                face.primitives.send_data(
                                    &key_expr,
                                    data.clone(),
                                    Channel {
                                        priority: Priority::default(), // @TODO: Default value for the time being
                                        reliability: subinfo.reliability,
                                    },
                                    CongestionControl::default(), // @TODO: Default value for the time being
                                    info.clone(),
                                    None,
                                );
                            }
                            get_mut_unchecked(ctx).last_values.clear();
                            drop(lock);
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
