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
use super::face::FaceState;
use super::network::Network;
use super::resource::{DataRoutes, Direction, PullCaches, Resource, Route, SessionContext};
use super::router::{RoutingExpr, Tables, TablesLock};
use petgraph::graph::NodeIndex;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;
use std::sync::RwLock;
use std::sync::{Arc, RwLockReadGuard};
use zenoh_buffers::ZBuf;
use zenoh_core::zread;
use zenoh_protocol::core::key_expr::keyexpr;
use zenoh_protocol::{
    core::{
        key_expr::OwnedKeyExpr, Channel, CongestionControl, Priority, Reliability, SubInfo,
        SubMode, WhatAmI, WireExpr, ZInt, ZenohId,
    },
    zenoh::{DataInfo, RoutingContext},
};
use zenoh_sync::get_mut_unchecked;

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
    tables: &mut Tables,
    dst_face: &mut Arc<FaceState>,
    res: &Arc<Resource>,
    sub_info: &SubInfo,
    src_face: &mut Arc<FaceState>,
    full_peer_net: bool,
) {
    if (src_face.id != dst_face.id || res.expr().starts_with(super::PREFIX_LIVELINESS))
        && !dst_face.local_subs.contains(res)
        && match tables.whatami {
            WhatAmI::Router => {
                if full_peer_net {
                    dst_face.whatami == WhatAmI::Client
                } else {
                    dst_face.whatami != WhatAmI::Router
                        && (src_face.whatami != WhatAmI::Peer
                            || dst_face.whatami != WhatAmI::Peer
                            || tables.failover_brokering(src_face.zid, dst_face.zid))
                }
            }
            WhatAmI::Peer => {
                if full_peer_net {
                    dst_face.whatami == WhatAmI::Client
                } else {
                    src_face.whatami == WhatAmI::Client || dst_face.whatami == WhatAmI::Client
                }
            }
            _ => src_face.whatami == WhatAmI::Client || dst_face.whatami == WhatAmI::Client,
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
    for mut dst_face in tables
        .faces
        .values()
        .cloned()
        .collect::<Vec<Arc<FaceState>>>()
    {
        propagate_simple_subscription_to(
            tables,
            &mut dst_face,
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
    tables: &TablesLock,
    rtables: RwLockReadGuard<Tables>,
    face: &mut Arc<FaceState>,
    expr: &WireExpr,
    sub_info: &SubInfo,
    router: ZenohId,
) {
    match rtables.get_mapping(face, &expr.scope).cloned() {
        Some(mut prefix) => {
            let res = Resource::get_resource(&prefix, &expr.suffix);
            let (mut res, mut wtables) =
                if res.as_ref().map(|r| r.context.is_some()).unwrap_or(false) {
                    drop(rtables);
                    let wtables = zwrite!(tables.tables);
                    (res.unwrap(), wtables)
                } else {
                    let mut fullexpr = prefix.expr();
                    fullexpr.push_str(expr.suffix.as_ref());
                    let mut matches = keyexpr::new(fullexpr.as_str())
                        .map(|ke| Resource::get_matches(&rtables, ke))
                        .unwrap_or_default();
                    drop(rtables);
                    let mut wtables = zwrite!(tables.tables);
                    let mut res =
                        Resource::make_resource(&mut wtables, &mut prefix, expr.suffix.as_ref());
                    matches.push(Arc::downgrade(&res));
                    Resource::match_resource(&wtables, &mut res, matches);
                    (res, wtables)
                };
            register_router_subscription(&mut wtables, face, &mut res, sub_info, router);
            disable_matches_data_routes(&mut wtables, &mut res);
            drop(wtables);

            let rtables = zread!(tables.tables);
            let matches_data_routes = compute_matches_data_routes_(&rtables, &res);
            drop(rtables);

            let wtables = zwrite!(tables.tables);
            for (mut res, data_routes) in matches_data_routes {
                get_mut_unchecked(&mut res)
                    .context_mut()
                    .update_data_routes(data_routes);
            }
            drop(wtables);
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
    tables: &TablesLock,
    rtables: RwLockReadGuard<Tables>,
    face: &mut Arc<FaceState>,
    expr: &WireExpr,
    sub_info: &SubInfo,
    peer: ZenohId,
) {
    match rtables.get_mapping(face, &expr.scope).cloned() {
        Some(mut prefix) => {
            let res = Resource::get_resource(&prefix, &expr.suffix);
            let (mut res, mut wtables) =
                if res.as_ref().map(|r| r.context.is_some()).unwrap_or(false) {
                    drop(rtables);
                    let wtables = zwrite!(tables.tables);
                    (res.unwrap(), wtables)
                } else {
                    let mut fullexpr = prefix.expr();
                    fullexpr.push_str(expr.suffix.as_ref());
                    let mut matches = keyexpr::new(fullexpr.as_str())
                        .map(|ke| Resource::get_matches(&rtables, ke))
                        .unwrap_or_default();
                    drop(rtables);
                    let mut wtables = zwrite!(tables.tables);
                    let mut res =
                        Resource::make_resource(&mut wtables, &mut prefix, expr.suffix.as_ref());
                    matches.push(Arc::downgrade(&res));
                    Resource::match_resource(&wtables, &mut res, matches);
                    (res, wtables)
                };
            register_peer_subscription(&mut wtables, face, &mut res, sub_info, peer);
            if wtables.whatami == WhatAmI::Router {
                let mut propa_sub_info = sub_info.clone();
                propa_sub_info.mode = SubMode::Push;
                let zid = wtables.zid;
                register_router_subscription(&mut wtables, face, &mut res, &propa_sub_info, zid);
            }
            disable_matches_data_routes(&mut wtables, &mut res);
            drop(wtables);

            let rtables = zread!(tables.tables);
            let matches_data_routes = compute_matches_data_routes_(&rtables, &res);
            drop(rtables);

            let wtables = zwrite!(tables.tables);
            for (mut res, data_routes) in matches_data_routes {
                get_mut_unchecked(&mut res)
                    .context_mut()
                    .update_data_routes(data_routes);
            }
            drop(wtables);
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
                        qabl: None,
                        last_values: HashMap::new(),
                    }),
                );
            }
        }
    }
    get_mut_unchecked(face).remote_subs.insert(res.clone());
}

pub fn declare_client_subscription(
    tables: &TablesLock,
    rtables: RwLockReadGuard<Tables>,
    face: &mut Arc<FaceState>,
    expr: &WireExpr,
    sub_info: &SubInfo,
) {
    log::debug!("Register client subscription");
    match rtables.get_mapping(face, &expr.scope).cloned() {
        Some(mut prefix) => {
            let res = Resource::get_resource(&prefix, &expr.suffix);
            let (mut res, mut wtables) =
                if res.as_ref().map(|r| r.context.is_some()).unwrap_or(false) {
                    drop(rtables);
                    let wtables = zwrite!(tables.tables);
                    (res.unwrap(), wtables)
                } else {
                    let mut fullexpr = prefix.expr();
                    fullexpr.push_str(expr.suffix.as_ref());
                    let mut matches = keyexpr::new(fullexpr.as_str())
                        .map(|ke| Resource::get_matches(&rtables, ke))
                        .unwrap_or_default();
                    drop(rtables);
                    let mut wtables = zwrite!(tables.tables);
                    let mut res =
                        Resource::make_resource(&mut wtables, &mut prefix, expr.suffix.as_ref());
                    matches.push(Arc::downgrade(&res));
                    Resource::match_resource(&wtables, &mut res, matches);
                    (res, wtables)
                };

            register_client_subscription(&mut wtables, face, &mut res, sub_info);
            let mut propa_sub_info = sub_info.clone();
            propa_sub_info.mode = SubMode::Push;
            match wtables.whatami {
                WhatAmI::Router => {
                    let zid = wtables.zid;
                    register_router_subscription(
                        &mut wtables,
                        face,
                        &mut res,
                        &propa_sub_info,
                        zid,
                    );
                }
                WhatAmI::Peer => {
                    if wtables.full_net(WhatAmI::Peer) {
                        let zid = wtables.zid;
                        register_peer_subscription(
                            &mut wtables,
                            face,
                            &mut res,
                            &propa_sub_info,
                            zid,
                        );
                    } else {
                        propagate_simple_subscription(&mut wtables, &res, &propa_sub_info, face);
                    }
                }
                _ => {
                    propagate_simple_subscription(&mut wtables, &res, &propa_sub_info, face);
                }
            }
            disable_matches_data_routes(&mut wtables, &mut res);
            drop(wtables);

            let rtables = zread!(tables.tables);
            let matches_data_routes = compute_matches_data_routes_(&rtables, &res);
            drop(rtables);

            let wtables = zwrite!(tables.tables);
            for (mut res, data_routes) in matches_data_routes {
                get_mut_unchecked(&mut res)
                    .context_mut()
                    .update_data_routes(data_routes);
            }
            drop(wtables);
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

fn propagate_forget_simple_subscription_to_peers(tables: &mut Tables, res: &Arc<Resource>) {
    if !tables.full_net(WhatAmI::Peer)
        && res.context().router_subs.len() == 1
        && res.context().router_subs.contains(&tables.zid)
    {
        for mut face in tables
            .faces
            .values()
            .cloned()
            .collect::<Vec<Arc<FaceState>>>()
        {
            if face.whatami == WhatAmI::Peer
                && face.local_subs.contains(res)
                && !res.session_ctxs.values().any(|s| {
                    face.zid != s.face.zid
                        && s.subs.is_some()
                        && (s.face.whatami == WhatAmI::Client
                            || (s.face.whatami == WhatAmI::Peer
                                && tables.failover_brokering(s.face.zid, face.zid)))
                })
            {
                let key_expr = Resource::get_best_key(res, "", face.id);
                face.primitives.forget_subscriber(&key_expr, None);

                get_mut_unchecked(&mut face).local_subs.remove(res);
            }
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

    propagate_forget_simple_subscription_to_peers(tables, res);
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
    tables: &TablesLock,
    rtables: RwLockReadGuard<Tables>,
    face: &mut Arc<FaceState>,
    expr: &WireExpr,
    router: &ZenohId,
) {
    match rtables.get_mapping(face, &expr.scope) {
        Some(prefix) => match Resource::get_resource(prefix, expr.suffix.as_ref()) {
            Some(mut res) => {
                drop(rtables);
                let mut wtables = zwrite!(tables.tables);
                undeclare_router_subscription(&mut wtables, Some(face), &mut res, router);
                disable_matches_data_routes(&mut wtables, &mut res);
                drop(wtables);

                let rtables = zread!(tables.tables);
                let matches_data_routes = compute_matches_data_routes_(&rtables, &res);
                drop(rtables);
                let wtables = zwrite!(tables.tables);
                for (mut res, data_routes) in matches_data_routes {
                    get_mut_unchecked(&mut res)
                        .context_mut()
                        .update_data_routes(data_routes);
                }
                Resource::clean(&mut res);
                drop(wtables);
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
    tables: &TablesLock,
    rtables: RwLockReadGuard<Tables>,
    face: &mut Arc<FaceState>,
    expr: &WireExpr,
    peer: &ZenohId,
) {
    match rtables.get_mapping(face, &expr.scope) {
        Some(prefix) => match Resource::get_resource(prefix, expr.suffix.as_ref()) {
            Some(mut res) => {
                drop(rtables);
                let mut wtables = zwrite!(tables.tables);
                undeclare_peer_subscription(&mut wtables, Some(face), &mut res, peer);
                if wtables.whatami == WhatAmI::Router {
                    let client_subs = res.session_ctxs.values().any(|ctx| ctx.subs.is_some());
                    let peer_subs = remote_peer_subs(&wtables, &res);
                    let zid = wtables.zid;
                    if !client_subs && !peer_subs {
                        undeclare_router_subscription(&mut wtables, None, &mut res, &zid);
                    }
                }
                disable_matches_data_routes(&mut wtables, &mut res);
                drop(wtables);

                let rtables = zread!(tables.tables);
                let matches_data_routes = compute_matches_data_routes_(&rtables, &res);
                drop(rtables);
                let wtables = zwrite!(tables.tables);
                for (mut res, data_routes) in matches_data_routes {
                    get_mut_unchecked(&mut res)
                        .context_mut()
                        .update_data_routes(data_routes);
                }
                Resource::clean(&mut res);
                drop(wtables);
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
            } else {
                propagate_forget_simple_subscription_to_peers(tables, res);
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
        if face.local_subs.contains(res)
            && !(face.whatami == WhatAmI::Client
                && res.expr().starts_with(super::PREFIX_LIVELINESS))
        {
            let key_expr = Resource::get_best_key(res, "", face.id);
            face.primitives.forget_subscriber(&key_expr, None);

            get_mut_unchecked(face).local_subs.remove(res);
        }
    }
}

pub fn forget_client_subscription(
    tables: &TablesLock,
    rtables: RwLockReadGuard<Tables>,
    face: &mut Arc<FaceState>,
    expr: &WireExpr,
) {
    match rtables.get_mapping(face, &expr.scope) {
        Some(prefix) => match Resource::get_resource(prefix, expr.suffix.as_ref()) {
            Some(mut res) => {
                drop(rtables);
                let mut wtables = zwrite!(tables.tables);
                undeclare_client_subscription(&mut wtables, face, &mut res);
                disable_matches_data_routes(&mut wtables, &mut res);
                drop(wtables);

                let rtables = zread!(tables.tables);
                let matches_data_routes = compute_matches_data_routes_(&rtables, &res);
                drop(rtables);

                let wtables = zwrite!(tables.tables);
                for (mut res, data_routes) in matches_data_routes {
                    get_mut_unchecked(&mut res)
                        .context_mut()
                        .update_data_routes(data_routes);
                }
                Resource::clean(&mut res);
                drop(wtables);
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
            } else if face.whatami == WhatAmI::Peer && !tables.full_net(WhatAmI::Peer) {
                for sub in &tables.router_subs {
                    if sub.context.is_some()
                        && (sub.context().router_subs.iter().any(|r| *r != tables.zid)
                            || sub.session_ctxs.values().any(|s| {
                                s.subs.is_some()
                                    && (s.face.whatami == WhatAmI::Client
                                        || (s.face.whatami == WhatAmI::Peer
                                            && tables.failover_brokering(s.face.zid, face.zid)))
                            }))
                    {
                        get_mut_unchecked(face).local_subs.insert(sub.clone());
                        let key_expr = Resource::decl_key(sub, face);
                        face.primitives.decl_subscriber(&key_expr, &sub_info, None);
                    }
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
                            tables,
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
                        tables,
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

                let matches_data_routes = compute_matches_data_routes_(tables, &res);
                for (mut res, data_routes) in matches_data_routes {
                    get_mut_unchecked(&mut res)
                        .context_mut()
                        .update_data_routes(data_routes);
                }
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

                // compute_matches_data_routes(tables, &mut res);
                let matches_data_routes = compute_matches_data_routes_(tables, &res);
                for (mut res, data_routes) in matches_data_routes {
                    get_mut_unchecked(&mut res)
                        .context_mut()
                        .update_data_routes(data_routes);
                }
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

pub(crate) fn pubsub_linkstate_change(tables: &mut Tables, zid: &ZenohId, links: &[ZenohId]) {
    if let Some(src_face) = tables.get_face(zid).cloned() {
        if tables.router_peers_failover_brokering
            && tables.whatami == WhatAmI::Router
            && src_face.whatami == WhatAmI::Peer
        {
            for res in &src_face.remote_subs {
                let client_subs = res
                    .session_ctxs
                    .values()
                    .any(|ctx| ctx.face.whatami == WhatAmI::Client && ctx.subs.is_some());
                if !remote_router_subs(tables, res) && !client_subs {
                    for ctx in get_mut_unchecked(&mut res.clone())
                        .session_ctxs
                        .values_mut()
                    {
                        let dst_face = &mut get_mut_unchecked(ctx).face;
                        if dst_face.whatami == WhatAmI::Peer && src_face.zid != dst_face.zid {
                            if dst_face.local_subs.contains(res) {
                                let forget = !Tables::failover_brokering_to(links, dst_face.zid)
                                    && {
                                        let ctx_links = tables
                                            .peers_net
                                            .as_ref()
                                            .map(|net| net.get_links(dst_face.zid))
                                            .unwrap_or_else(|| &[]);
                                        res.session_ctxs.values().any(|ctx2| {
                                            ctx2.face.whatami == WhatAmI::Peer
                                                && ctx2.subs.is_some()
                                                && Tables::failover_brokering_to(
                                                    ctx_links,
                                                    ctx2.face.zid,
                                                )
                                        })
                                    };
                                if forget {
                                    let key_expr = Resource::get_best_key(res, "", dst_face.id);
                                    dst_face.primitives.forget_subscriber(&key_expr, None);

                                    get_mut_unchecked(dst_face).local_subs.remove(res);
                                }
                            } else if Tables::failover_brokering_to(links, ctx.face.zid) {
                                let dst_face = &mut get_mut_unchecked(ctx).face;
                                get_mut_unchecked(dst_face).local_subs.insert(res.clone());
                                let key_expr = Resource::decl_key(res, dst_face);
                                let sub_info = SubInfo {
                                    reliability: Reliability::Reliable, // TODO
                                    mode: SubMode::Push,
                                };
                                dst_face
                                    .primitives
                                    .decl_subscriber(&key_expr, &sub_info, None);
                            }
                        }
                    }
                }
            }
        }
    }
}

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
        || !tables.full_net(WhatAmI::Peer)
        || *tables.elect_router(&key_expr, tables.shared_nodes.iter()) == tables.zid;

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
                    expr,
                    tables,
                    net,
                    router_source,
                    &mres.context().router_subs,
                );
            }

            if (master || source_type != WhatAmI::Router) && tables.full_net(WhatAmI::Peer) {
                let net = tables.peers_net.as_ref().unwrap();
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

        if tables.whatami == WhatAmI::Peer && tables.full_net(WhatAmI::Peer) {
            let net = tables.peers_net.as_ref().unwrap();
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
                    } && subinfo.mode == SubMode::Push
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
                if subinfo.mode == SubMode::Pull {
                    pull_caches.push(context.clone());
                }
            }
        }
    }
    Arc::new(pull_caches)
}

pub(super) fn compute_data_routes_(tables: &Tables, res: &Arc<Resource>) -> DataRoutes {
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
        routes
            .peers_data_routes
            .resize_with(max_idx.index() + 1, || Arc::new(HashMap::new()));

        for idx in &indexes {
            routes.peers_data_routes[idx.index()] =
                compute_data_route(tables, &mut expr, Some(idx.index()), WhatAmI::Peer);
        }
    }
    if tables.whatami == WhatAmI::Peer && !tables.full_net(WhatAmI::Peer) {
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
                    compute_data_route(tables, &mut expr, Some(idx.index()), WhatAmI::Peer);
            }
        }
        if tables.whatami == WhatAmI::Peer && !tables.full_net(WhatAmI::Peer) {
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

fn compute_data_routes_from(tables: &mut Tables, res: &mut Arc<Resource>) {
    compute_data_routes(tables, res);
    let res = get_mut_unchecked(res);
    for child in res.childs.values_mut() {
        compute_data_routes_from(tables, child);
    }
}

pub(super) fn compute_matches_data_routes_<'a>(
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

pub(super) fn disable_matches_data_routes(_tables: &mut Tables, res: &mut Arc<Resource>) {
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
                    let data_info = DataInfo {
                        timestamp: Some(hlc.new_timestamp()),
                        ..Default::default()
                    };
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
    expr: &mut RoutingExpr,
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
                        compute_data_route(tables, expr, Some(local_context), face.whatami)
                    })
            }
            WhatAmI::Peer => {
                if tables.full_net(WhatAmI::Peer) {
                    let peers_net = tables.peers_net.as_ref().unwrap();
                    let local_context = peers_net
                        .get_local_context(routing_context.map(|rc| rc.tree_id), face.link_id);
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
            if tables.full_net(WhatAmI::Peer) {
                match face.whatami {
                    WhatAmI::Router | WhatAmI::Peer => {
                        let peers_net = tables.peers_net.as_ref().unwrap();
                        let local_context = peers_net
                            .get_local_context(routing_context.map(|rc| rc.tree_id), face.link_id);
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
        $payload:expr,
        $info:expr
    ) => {
        for context in $matching_pulls.iter() {
            get_mut_unchecked(&mut context.clone()).last_values.insert(
                $expr.full_expr().to_string(),
                ($info.clone(), $payload.clone()),
            );
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
    if src_face.id != outface.id {
        let dst_master = tables.whatami != WhatAmI::Router
            || outface.whatami != WhatAmI::Peer
            || tables.peers_net.is_none()
            || tables.zid
                == *tables.elect_router(expr.full_expr(), tables.get_router_links(outface.zid));

        return dst_master
            && (src_face.whatami != WhatAmI::Peer
                || outface.whatami != WhatAmI::Peer
                || tables.full_net(WhatAmI::Peer)
                || tables.failover_brokering(src_face.zid, outface.zid));
    }
    false
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
            let mut expr = RoutingExpr::new(&prefix, expr.suffix.as_ref());

            if tables.whatami != WhatAmI::Router
                || face.whatami != WhatAmI::Peer
                || tables.peers_net.is_none()
                || tables.zid
                    == *tables.elect_router(expr.full_expr(), tables.get_router_links(face.zid))
            {
                let res = Resource::get_resource(&prefix, expr.suffix);
                let route = get_data_route(&tables, face, &res, &mut expr, routing_context);
                let matching_pulls = get_matching_pulls(&tables, &res, &mut expr);

                if !(route.is_empty() && matching_pulls.is_empty()) {
                    let data_info =
                        treat_timestamp!(&tables.hlc, info, tables.drop_future_timestamp);

                    if route.len() == 1 && matching_pulls.len() == 0 {
                        let (outface, key_expr, context) = route.values().next().unwrap();
                        if should_route(&tables, face, outface, &mut expr) {
                            drop(tables);
                            outface.primitives.send_data(
                                key_expr,
                                payload,
                                channel, // @TODO: Need to check the active subscriptions to determine the right reliability value
                                congestion_control,
                                data_info,
                                *context,
                            )
                        }
                    } else {
                        if !matching_pulls.is_empty() {
                            let lock = zlock!(tables.pull_caches_lock);
                            cache_data!(matching_pulls, expr, payload, data_info);
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
                                outface.primitives.send_data(
                                    &key_expr,
                                    payload.clone(),
                                    channel, // @TODO: Need to check the active subscriptions to determine the right reliability value
                                    congestion_control,
                                    data_info.clone(),
                                    context,
                                )
                            }
                        } else {
                            drop(tables);
                            for (outface, key_expr, context) in route.values() {
                                if face.id != outface.id {
                                    outface.primitives.send_data(
                                        key_expr,
                                        payload.clone(),
                                        channel, // @TODO: Need to check the active subscriptions to determine the right reliability value
                                        congestion_control,
                                        data_info.clone(),
                                        *context,
                                    )
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

pub fn pull_data(
    tables_ref: &RwLock<Tables>,
    face: &Arc<FaceState>,
    _is_final: bool,
    expr: &WireExpr,
    _pull_id: ZInt,
    _max_samples: &Option<ZInt>,
) {
    let tables = zread!(tables_ref);
    match tables.get_mapping(face, &expr.scope) {
        Some(prefix) => match Resource::get_resource(prefix, expr.suffix.as_ref()) {
            Some(mut res) => {
                let res = get_mut_unchecked(&mut res);
                match res.session_ctxs.get_mut(&face.id) {
                    Some(ctx) => match &ctx.subs {
                        Some(subinfo) => {
                            let reliability = subinfo.reliability;
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
                                .collect::<Vec<(WireExpr, (Option<DataInfo>, ZBuf))>>();
                            drop(lock);
                            drop(tables);
                            for (key_expr, (info, data)) in route {
                                face.primitives.send_data(
                                    &key_expr,
                                    data,
                                    Channel {
                                        priority: Priority::default(), // @TODO: Default value for the time being
                                        reliability,
                                    },
                                    CongestionControl::default(), // @TODO: Default value for the time being
                                    info,
                                    None,
                                );
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
