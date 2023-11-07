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
use super::super::dispatcher::face::FaceState;
use super::super::dispatcher::pubsub::*;
use super::super::dispatcher::resource::{Resource, RoutingContext, SessionContext};
use super::super::dispatcher::tables::{Tables, TablesLock};
use super::super::PREFIX_LIVELINESS;
use super::network::Network;
use super::{get_peer, get_router, HatContext, HatFace, HatTables};
use crate::net::routing::dispatcher::tables::{DataRoutes, PullCaches, Route, RoutingExpr};
use crate::{face_hat, face_hat_mut, hat, hat_mut, res_hat, res_hat_mut};
use petgraph::graph::NodeIndex;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLockReadGuard};
use zenoh_core::zread;
use zenoh_protocol::core::key_expr::OwnedKeyExpr;
use zenoh_protocol::{
    core::{key_expr::keyexpr, Reliability, WhatAmI, WireExpr, ZenohId},
    network::declare::{
        common::ext::WireExprType, ext, subscriber::ext::SubscriberInfo, Declare, DeclareBody,
        DeclareSubscriber, Mode, UndeclareSubscriber,
    },
};
use zenoh_sync::get_mut_unchecked;

#[inline]
fn send_sourced_subscription_to_net_childs(
    tables: &Tables,
    net: &Network,
    childs: &[NodeIndex],
    res: &Arc<Resource>,
    src_face: Option<&Arc<FaceState>>,
    sub_info: &SubscriberInfo,
    routing_context: RoutingContext,
) {
    for child in childs {
        if net.graph.contains_node(*child) {
            match tables.get_face(&net.graph[*child].zid).cloned() {
                Some(mut someface) => {
                    if src_face.is_none() || someface.id != src_face.unwrap().id {
                        let key_expr = Resource::decl_key(res, &mut someface);

                        log::debug!("Send subscription {} on {}", res.expr(), someface);

                        someface.primitives.send_declare(Declare {
                            ext_qos: ext::QoSType::declare_default(),
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType {
                                node_id: routing_context,
                            },
                            body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                id: 0, // TODO
                                wire_expr: key_expr,
                                ext_info: *sub_info,
                            }),
                        });
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
    sub_info: &SubscriberInfo,
    src_face: &mut Arc<FaceState>,
    full_peer_net: bool,
) {
    if (src_face.id != dst_face.id || res.expr().starts_with(PREFIX_LIVELINESS))
        && !face_hat!(dst_face).local_subs.contains(res)
        && match tables.whatami {
            WhatAmI::Router => {
                if full_peer_net {
                    dst_face.whatami == WhatAmI::Client
                } else {
                    dst_face.whatami != WhatAmI::Router
                        && (src_face.whatami != WhatAmI::Peer
                            || dst_face.whatami != WhatAmI::Peer
                            || hat!(tables).failover_brokering(src_face.zid, dst_face.zid))
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
        face_hat_mut!(dst_face).local_subs.insert(res.clone());
        let key_expr = Resource::decl_key(res, dst_face);
        dst_face.primitives.send_declare(Declare {
            ext_qos: ext::QoSType::declare_default(),
            ext_tstamp: None,
            ext_nodeid: ext::NodeIdType::default(),
            body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                id: 0, // TODO
                wire_expr: key_expr,
                ext_info: *sub_info,
            }),
        });
    }
}

fn propagate_simple_subscription(
    tables: &mut Tables,
    res: &Arc<Resource>,
    sub_info: &SubscriberInfo,
    src_face: &mut Arc<FaceState>,
) {
    let full_peer_net = hat!(tables).full_net(WhatAmI::Peer);
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
    sub_info: &SubscriberInfo,
    src_face: Option<&Arc<FaceState>>,
    source: &ZenohId,
    net_type: WhatAmI,
) {
    let net = hat!(tables).get_net(net_type).unwrap();
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
                    tree_sid.index() as RoutingContext,
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
    sub_info: &SubscriberInfo,
    router: ZenohId,
) {
    if !res_hat!(res).router_subs.contains(&router) {
        // Register router subscription
        {
            log::debug!(
                "Register router subscription {} (router: {})",
                res.expr(),
                router
            );
            res_hat_mut!(res).router_subs.insert(router);
            hat_mut!(tables).router_subs.insert(res.clone());
        }

        // Propagate subscription to routers
        propagate_sourced_subscription(tables, res, sub_info, Some(face), &router, WhatAmI::Router);
    }
    // Propagate subscription to peers
    if hat!(tables).full_net(WhatAmI::Peer) && face.whatami != WhatAmI::Peer {
        register_peer_subscription(tables, face, res, sub_info, tables.zid)
    }

    // Propagate subscription to clients
    propagate_simple_subscription(tables, res, sub_info, face);
}

fn declare_router_subscription(
    tables: &TablesLock,
    rtables: RwLockReadGuard<Tables>,
    face: &mut Arc<FaceState>,
    expr: &WireExpr,
    sub_info: &SubscriberInfo,
    router: ZenohId,
) {
    match rtables
        .get_mapping(face, &expr.scope, expr.mapping)
        .cloned()
    {
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
    sub_info: &SubscriberInfo,
    peer: ZenohId,
) {
    if !res_hat!(res).peer_subs.contains(&peer) {
        // Register peer subscription
        {
            log::debug!("Register peer subscription {} (peer: {})", res.expr(), peer);
            res_hat_mut!(res).peer_subs.insert(peer);
            hat_mut!(tables).peer_subs.insert(res.clone());
        }

        // Propagate subscription to peers
        propagate_sourced_subscription(tables, res, sub_info, Some(face), &peer, WhatAmI::Peer);
    }

    if tables.whatami == WhatAmI::Peer {
        // Propagate subscription to clients
        propagate_simple_subscription(tables, res, sub_info, face);
    }
}

fn declare_peer_subscription(
    tables: &TablesLock,
    rtables: RwLockReadGuard<Tables>,
    face: &mut Arc<FaceState>,
    expr: &WireExpr,
    sub_info: &SubscriberInfo,
    peer: ZenohId,
) {
    match rtables
        .get_mapping(face, &expr.scope, expr.mapping)
        .cloned()
    {
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
                let mut propa_sub_info = *sub_info;
                propa_sub_info.mode = Mode::Push;
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
    sub_info: &SubscriberInfo,
) {
    // Register subscription
    {
        let res = get_mut_unchecked(res);
        log::debug!("Register subscription {} for {}", res.expr(), face);
        match res.session_ctxs.get_mut(&face.id) {
            Some(ctx) => match &ctx.subs {
                Some(info) => {
                    if Mode::Pull == info.mode {
                        get_mut_unchecked(ctx).subs = Some(*sub_info);
                    }
                }
                None => {
                    get_mut_unchecked(ctx).subs = Some(*sub_info);
                }
            },
            None => {
                res.session_ctxs.insert(
                    face.id,
                    Arc::new(SessionContext {
                        face: face.clone(),
                        local_expr_id: None,
                        remote_expr_id: None,
                        subs: Some(*sub_info),
                        qabl: None,
                        last_values: HashMap::new(),
                    }),
                );
            }
        }
    }
    face_hat_mut!(face).remote_subs.insert(res.clone());
}

fn declare_client_subscription(
    tables: &TablesLock,
    rtables: RwLockReadGuard<Tables>,
    face: &mut Arc<FaceState>,
    expr: &WireExpr,
    sub_info: &SubscriberInfo,
) {
    log::debug!("Register client subscription");
    match rtables
        .get_mapping(face, &expr.scope, expr.mapping)
        .cloned()
    {
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
            let mut propa_sub_info = *sub_info;
            propa_sub_info.mode = Mode::Push;
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
                    if hat!(wtables).full_net(WhatAmI::Peer) {
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
                        // This introduced a buffer overflow on windows
                        // TODO: Let's deactivate this on windows until Fixed
                        #[cfg(not(windows))]
                        for mcast_group in &wtables.mcast_groups {
                            mcast_group.primitives.send_declare(Declare {
                                ext_qos: ext::QoSType::declare_default(),
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType::default(),
                                body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                    id: 0, // TODO
                                    wire_expr: res.expr().into(),
                                    ext_info: *sub_info,
                                }),
                            })
                        }
                    }
                }
                _ => {
                    propagate_simple_subscription(&mut wtables, &res, &propa_sub_info, face);
                    // This introduced a buffer overflow on windows
                    // TODO: Let's deactivate this on windows until Fixed
                    #[cfg(not(windows))]
                    for mcast_group in &wtables.mcast_groups {
                        mcast_group.primitives.send_declare(Declare {
                            ext_qos: ext::QoSType::declare_default(),
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType::default(),
                            body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                id: 0, // TODO
                                wire_expr: res.expr().into(),
                                ext_info: *sub_info,
                            }),
                        })
                    }
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

pub fn declare_subscription(
    tables: &TablesLock,
    face: &mut Arc<FaceState>,
    expr: &WireExpr,
    sub_info: &SubscriberInfo,
    node_id: RoutingContext,
) {
    let rtables = zread!(tables.tables);
    match (rtables.whatami, face.whatami) {
        (WhatAmI::Router, WhatAmI::Router) => {
            if let Some(router) = get_router(&rtables, face, node_id) {
                declare_router_subscription(tables, rtables, face, expr, sub_info, router)
            }
        }
        (WhatAmI::Router, WhatAmI::Peer)
        | (WhatAmI::Peer, WhatAmI::Router)
        | (WhatAmI::Peer, WhatAmI::Peer) => {
            if hat!(rtables).full_net(WhatAmI::Peer) {
                if let Some(peer) = get_peer(&rtables, face, node_id) {
                    declare_peer_subscription(tables, rtables, face, expr, sub_info, peer)
                }
            } else {
                declare_client_subscription(tables, rtables, face, expr, sub_info)
            }
        }
        _ => declare_client_subscription(tables, rtables, face, expr, sub_info),
    }
}

#[inline]
fn remote_router_subs(tables: &Tables, res: &Arc<Resource>) -> bool {
    res.context.is_some()
        && res_hat!(res)
            .router_subs
            .iter()
            .any(|peer| peer != &tables.zid)
}

#[inline]
fn remote_peer_subs(tables: &Tables, res: &Arc<Resource>) -> bool {
    res.context.is_some()
        && res_hat!(res)
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
                        let wire_expr = Resource::decl_key(res, &mut someface);

                        log::debug!("Send forget subscription {} on {}", res.expr(), someface);

                        someface.primitives.send_declare(Declare {
                            ext_qos: ext::QoSType::declare_default(),
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType {
                                node_id: routing_context.unwrap_or(0),
                            },
                            body: DeclareBody::UndeclareSubscriber(UndeclareSubscriber {
                                id: 0, // TODO
                                ext_wire_expr: WireExprType { wire_expr },
                            }),
                        });
                    }
                }
                None => log::trace!("Unable to find face for zid {}", net.graph[*child].zid),
            }
        }
    }
}

fn propagate_forget_simple_subscription(tables: &mut Tables, res: &Arc<Resource>) {
    for face in tables.faces.values_mut() {
        if face_hat!(face).local_subs.contains(res) {
            let wire_expr = Resource::get_best_key(res, "", face.id);
            face.primitives.send_declare(Declare {
                ext_qos: ext::QoSType::declare_default(),
                ext_tstamp: None,
                ext_nodeid: ext::NodeIdType::default(),
                body: DeclareBody::UndeclareSubscriber(UndeclareSubscriber {
                    id: 0, // TODO
                    ext_wire_expr: WireExprType { wire_expr },
                }),
            });
            face_hat_mut!(face).local_subs.remove(res);
        }
    }
}

fn propagate_forget_simple_subscription_to_peers(tables: &mut Tables, res: &Arc<Resource>) {
    if !hat!(tables).full_net(WhatAmI::Peer)
        && res_hat!(res).router_subs.len() == 1
        && res_hat!(res).router_subs.contains(&tables.zid)
    {
        for mut face in tables
            .faces
            .values()
            .cloned()
            .collect::<Vec<Arc<FaceState>>>()
        {
            if face.whatami == WhatAmI::Peer
                && face_hat!(face).local_subs.contains(res)
                && !res.session_ctxs.values().any(|s| {
                    face.zid != s.face.zid
                        && s.subs.is_some()
                        && (s.face.whatami == WhatAmI::Client
                            || (s.face.whatami == WhatAmI::Peer
                                && hat!(tables).failover_brokering(s.face.zid, face.zid)))
                })
            {
                let wire_expr = Resource::get_best_key(res, "", face.id);
                face.primitives.send_declare(Declare {
                    ext_qos: ext::QoSType::declare_default(),
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::default(),
                    body: DeclareBody::UndeclareSubscriber(UndeclareSubscriber {
                        id: 0, // TODO
                        ext_wire_expr: WireExprType { wire_expr },
                    }),
                });

                face_hat_mut!(&mut face).local_subs.remove(res);
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
    let net = hat!(tables).get_net(net_type).unwrap();
    match net.get_idx(source) {
        Some(tree_sid) => {
            if net.trees.len() > tree_sid.index() {
                send_forget_sourced_subscription_to_net_childs(
                    tables,
                    net,
                    &net.trees[tree_sid.index()].childs,
                    res,
                    src_face,
                    Some(tree_sid.index() as RoutingContext),
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
    res_hat_mut!(res).router_subs.retain(|sub| sub != router);

    if res_hat!(res).router_subs.is_empty() {
        hat_mut!(tables)
            .router_subs
            .retain(|sub| !Arc::ptr_eq(sub, res));

        if hat_mut!(tables).full_net(WhatAmI::Peer) {
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
    if res_hat!(res).router_subs.contains(router) {
        unregister_router_subscription(tables, res, router);
        propagate_forget_sourced_subscription(tables, res, face, router, WhatAmI::Router);
    }
}

fn forget_router_subscription(
    tables: &TablesLock,
    rtables: RwLockReadGuard<Tables>,
    face: &mut Arc<FaceState>,
    expr: &WireExpr,
    router: &ZenohId,
) {
    match rtables.get_mapping(face, &expr.scope, expr.mapping) {
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
    res_hat_mut!(res).peer_subs.retain(|sub| sub != peer);

    if res_hat!(res).peer_subs.is_empty() {
        hat_mut!(tables)
            .peer_subs
            .retain(|sub| !Arc::ptr_eq(sub, res));

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
    if res_hat!(res).peer_subs.contains(peer) {
        unregister_peer_subscription(tables, res, peer);
        propagate_forget_sourced_subscription(tables, res, face, peer, WhatAmI::Peer);
    }
}

fn forget_peer_subscription(
    tables: &TablesLock,
    rtables: RwLockReadGuard<Tables>,
    face: &mut Arc<FaceState>,
    expr: &WireExpr,
    peer: &ZenohId,
) {
    match rtables.get_mapping(face, &expr.scope, expr.mapping) {
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
    face_hat_mut!(face).remote_subs.remove(res);

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
                if hat!(tables).full_net(WhatAmI::Peer) {
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
        if face_hat!(face).local_subs.contains(res)
            && !(face.whatami == WhatAmI::Client && res.expr().starts_with(PREFIX_LIVELINESS))
        {
            let wire_expr = Resource::get_best_key(res, "", face.id);
            face.primitives.send_declare(Declare {
                ext_qos: ext::QoSType::declare_default(),
                ext_tstamp: None,
                ext_nodeid: ext::NodeIdType::default(),
                body: DeclareBody::UndeclareSubscriber(UndeclareSubscriber {
                    id: 0, // TODO
                    ext_wire_expr: WireExprType { wire_expr },
                }),
            });

            face_hat_mut!(face).local_subs.remove(res);
        }
    }
}

fn forget_client_subscription(
    tables: &TablesLock,
    rtables: RwLockReadGuard<Tables>,
    face: &mut Arc<FaceState>,
    expr: &WireExpr,
) {
    match rtables.get_mapping(face, &expr.scope, expr.mapping) {
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

pub fn forget_subscription(
    tables: &TablesLock,
    face: &mut Arc<FaceState>,
    expr: &WireExpr,
    node_id: RoutingContext,
) {
    let rtables = zread!(tables.tables);
    match (rtables.whatami, face.whatami) {
        (WhatAmI::Router, WhatAmI::Router) => {
            if let Some(router) = get_router(&rtables, face, node_id) {
                forget_router_subscription(tables, rtables, face, expr, &router)
            }
        }
        (WhatAmI::Router, WhatAmI::Peer)
        | (WhatAmI::Peer, WhatAmI::Router)
        | (WhatAmI::Peer, WhatAmI::Peer) => {
            if hat!(rtables).full_net(WhatAmI::Peer) {
                if let Some(peer) = get_peer(&rtables, face, node_id) {
                    forget_peer_subscription(tables, rtables, face, expr, &peer)
                }
            } else {
                forget_client_subscription(tables, rtables, face, expr)
            }
        }
        _ => forget_client_subscription(tables, rtables, face, expr),
    }
}

pub(crate) fn pubsub_new_face(tables: &mut Tables, face: &mut Arc<FaceState>) {
    let sub_info = SubscriberInfo {
        reliability: Reliability::Reliable, // @TODO
        mode: Mode::Push,
    };
    match tables.whatami {
        WhatAmI::Router => {
            if face.whatami == WhatAmI::Client {
                for sub in &hat!(tables).router_subs {
                    face_hat_mut!(face).local_subs.insert(sub.clone());
                    let key_expr = Resource::decl_key(sub, face);
                    face.primitives.send_declare(Declare {
                        ext_qos: ext::QoSType::declare_default(),
                        ext_tstamp: None,
                        ext_nodeid: ext::NodeIdType::default(),
                        body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                            id: 0, // TODO
                            wire_expr: key_expr,
                            ext_info: sub_info,
                        }),
                    });
                }
            } else if face.whatami == WhatAmI::Peer && !hat!(tables).full_net(WhatAmI::Peer) {
                for sub in &hat!(tables).router_subs {
                    if sub.context.is_some()
                        && (res_hat!(sub).router_subs.iter().any(|r| *r != tables.zid)
                            || sub.session_ctxs.values().any(|s| {
                                s.subs.is_some()
                                    && (s.face.whatami == WhatAmI::Client
                                        || (s.face.whatami == WhatAmI::Peer
                                            && hat!(tables)
                                                .failover_brokering(s.face.zid, face.zid)))
                            }))
                    {
                        face_hat_mut!(face).local_subs.insert(sub.clone());
                        let key_expr = Resource::decl_key(sub, face);
                        face.primitives.send_declare(Declare {
                            ext_qos: ext::QoSType::declare_default(),
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType::default(),
                            body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                id: 0, // TODO
                                wire_expr: key_expr,
                                ext_info: sub_info,
                            }),
                        });
                    }
                }
            }
        }
        WhatAmI::Peer => {
            if hat!(tables).full_net(WhatAmI::Peer) {
                if face.whatami == WhatAmI::Client {
                    for sub in &hat!(tables).peer_subs {
                        face_hat_mut!(face).local_subs.insert(sub.clone());
                        let key_expr = Resource::decl_key(sub, face);
                        face.primitives.send_declare(Declare {
                            ext_qos: ext::QoSType::declare_default(),
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType::default(),
                            body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                id: 0, // TODO
                                wire_expr: key_expr,
                                ext_info: sub_info,
                            }),
                        });
                    }
                }
            } else {
                for src_face in tables
                    .faces
                    .values()
                    .cloned()
                    .collect::<Vec<Arc<FaceState>>>()
                {
                    for sub in &face_hat!(src_face).remote_subs {
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
                for sub in &face_hat!(src_face).remote_subs {
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
            for mut res in hat!(tables)
                .router_subs
                .iter()
                .filter(|res| res_hat!(res).router_subs.contains(node))
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
            for mut res in hat!(tables)
                .peer_subs
                .iter()
                .filter(|res| res_hat!(res).peer_subs.contains(node))
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
            let net = hat!(tables).get_net(net_type).unwrap();
            let tree_idx = NodeIndex::new(tree_sid);
            if net.graph.contains_node(tree_idx) {
                let tree_id = net.graph[tree_idx].zid;

                let subs_res = match net_type {
                    WhatAmI::Router => &hat!(tables).router_subs,
                    _ => &hat!(tables).peer_subs,
                };

                for res in subs_res {
                    let subs = match net_type {
                        WhatAmI::Router => &res_hat!(res).router_subs,
                        _ => &res_hat!(res).peer_subs,
                    };
                    for sub in subs {
                        if *sub == tree_id {
                            let sub_info = SubscriberInfo {
                                reliability: Reliability::Reliable, // @TODO
                                mode: Mode::Push,
                            };
                            send_sourced_subscription_to_net_childs(
                                tables,
                                net,
                                tree_childs,
                                res,
                                None,
                                &sub_info,
                                tree_sid as RoutingContext,
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
        if hat!(tables).router_peers_failover_brokering
            && tables.whatami == WhatAmI::Router
            && src_face.whatami == WhatAmI::Peer
        {
            for res in &face_hat!(src_face).remote_subs {
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
                            if face_hat!(dst_face).local_subs.contains(res) {
                                let forget = !HatTables::failover_brokering_to(links, dst_face.zid)
                                    && {
                                        let ctx_links = hat!(tables)
                                            .peers_net
                                            .as_ref()
                                            .map(|net| net.get_links(dst_face.zid))
                                            .unwrap_or_else(|| &[]);
                                        res.session_ctxs.values().any(|ctx2| {
                                            ctx2.face.whatami == WhatAmI::Peer
                                                && ctx2.subs.is_some()
                                                && HatTables::failover_brokering_to(
                                                    ctx_links,
                                                    ctx2.face.zid,
                                                )
                                        })
                                    };
                                if forget {
                                    let wire_expr = Resource::get_best_key(res, "", dst_face.id);
                                    dst_face.primitives.send_declare(Declare {
                                        ext_qos: ext::QoSType::declare_default(),
                                        ext_tstamp: None,
                                        ext_nodeid: ext::NodeIdType::default(),
                                        body: DeclareBody::UndeclareSubscriber(
                                            UndeclareSubscriber {
                                                id: 0, // TODO
                                                ext_wire_expr: WireExprType { wire_expr },
                                            },
                                        ),
                                    });

                                    face_hat_mut!(dst_face).local_subs.remove(res);
                                }
                            } else if HatTables::failover_brokering_to(links, ctx.face.zid) {
                                let dst_face = &mut get_mut_unchecked(ctx).face;
                                face_hat_mut!(dst_face).local_subs.insert(res.clone());
                                let key_expr = Resource::decl_key(res, dst_face);
                                let sub_info = SubscriberInfo {
                                    reliability: Reliability::Reliable, // TODO
                                    mode: Mode::Push,
                                };
                                dst_face.primitives.send_declare(Declare {
                                    ext_qos: ext::QoSType::declare_default(),
                                    ext_tstamp: None,
                                    ext_nodeid: ext::NodeIdType::default(),
                                    body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                        id: 0, // TODO
                                        wire_expr: key_expr,
                                        ext_info: sub_info,
                                    }),
                                });
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
    source: RoutingContext,
    subs: &HashSet<ZenohId>,
) {
    if net.trees.len() > source as usize {
        for sub in subs {
            if let Some(sub_idx) = net.get_idx(sub) {
                if net.trees[source as usize].directions.len() > sub_idx.index() {
                    if let Some(direction) = net.trees[source as usize].directions[sub_idx.index()]
                    {
                        if net.graph.contains_node(direction) {
                            if let Some(face) = tables.get_face(&net.graph[direction].zid) {
                                route.entry(face.id).or_insert_with(|| {
                                    let key_expr =
                                        Resource::get_best_key(expr.prefix, expr.suffix, face.id);
                                    (face.clone(), key_expr.to_owned(), source)
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

pub(crate) fn compute_data_route(
    tables: &Tables,
    expr: &mut RoutingExpr,
    source: RoutingContext,
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
        || !hat!(tables).full_net(WhatAmI::Peer)
        || *hat!(tables).elect_router(&tables.zid, &key_expr, hat!(tables).shared_nodes.iter())
            == tables.zid;

    for mres in matches.iter() {
        let mres = mres.upgrade().unwrap();
        if tables.whatami == WhatAmI::Router {
            if master || source_type == WhatAmI::Router {
                let net = hat!(tables).routers_net.as_ref().unwrap();
                let router_source = match source_type {
                    WhatAmI::Router => source,
                    _ => net.idx.index() as RoutingContext,
                };
                insert_faces_for_subs(
                    &mut route,
                    expr,
                    tables,
                    net,
                    router_source,
                    &res_hat!(mres).router_subs,
                );
            }

            if (master || source_type != WhatAmI::Router) && hat!(tables).full_net(WhatAmI::Peer) {
                let net = hat!(tables).peers_net.as_ref().unwrap();
                let peer_source = match source_type {
                    WhatAmI::Peer => source,
                    _ => net.idx.index() as RoutingContext,
                };
                insert_faces_for_subs(
                    &mut route,
                    expr,
                    tables,
                    net,
                    peer_source,
                    &res_hat!(mres).peer_subs,
                );
            }
        }

        if tables.whatami == WhatAmI::Peer && hat!(tables).full_net(WhatAmI::Peer) {
            let net = hat!(tables).peers_net.as_ref().unwrap();
            let peer_source = match source_type {
                WhatAmI::Router | WhatAmI::Peer => source,
                _ => net.idx.index() as RoutingContext,
            };
            insert_faces_for_subs(
                &mut route,
                expr,
                tables,
                net,
                peer_source,
                &res_hat!(mres).peer_subs,
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
                            (
                                context.face.clone(),
                                key_expr.to_owned(),
                                RoutingContext::default(),
                            )
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
                RoutingContext::default(),
            ),
        );
    }
    Arc::new(route)
}

pub(crate) fn compute_matching_pulls(tables: &Tables, expr: &mut RoutingExpr) -> Arc<PullCaches> {
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
        let indexes = hat!(tables)
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
            routes.routers_data_routes[idx.index()] = compute_data_route(
                tables,
                &mut expr,
                idx.index() as RoutingContext,
                WhatAmI::Router,
            );
        }

        routes.peer_data_route = Some(compute_data_route(
            tables,
            &mut expr,
            RoutingContext::default(),
            WhatAmI::Peer,
        ));
    }
    if (tables.whatami == WhatAmI::Router || tables.whatami == WhatAmI::Peer)
        && hat!(tables).full_net(WhatAmI::Peer)
    {
        let indexes = hat!(tables)
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
            routes.peers_data_routes[idx.index()] = compute_data_route(
                tables,
                &mut expr,
                idx.index() as RoutingContext,
                WhatAmI::Peer,
            );
        }
    }
    if tables.whatami == WhatAmI::Peer && !hat!(tables).full_net(WhatAmI::Peer) {
        routes.client_data_route = Some(compute_data_route(
            tables,
            &mut expr,
            RoutingContext::default(),
            WhatAmI::Client,
        ));
        routes.peer_data_route = Some(compute_data_route(
            tables,
            &mut expr,
            RoutingContext::default(),
            WhatAmI::Peer,
        ));
    }
    if tables.whatami == WhatAmI::Client {
        routes.client_data_route = Some(compute_data_route(
            tables,
            &mut expr,
            RoutingContext::default(),
            WhatAmI::Client,
        ));
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
            let indexes = hat!(tables)
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
                routers_data_routes[idx.index()] = compute_data_route(
                    tables,
                    &mut expr,
                    idx.index() as RoutingContext,
                    WhatAmI::Router,
                );
            }

            res_mut.context_mut().peer_data_route = Some(compute_data_route(
                tables,
                &mut expr,
                RoutingContext::default(),
                WhatAmI::Peer,
            ));
        }
        if (tables.whatami == WhatAmI::Router || tables.whatami == WhatAmI::Peer)
            && hat!(tables).full_net(WhatAmI::Peer)
        {
            let indexes = hat!(tables)
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
                peers_data_routes[idx.index()] = compute_data_route(
                    tables,
                    &mut expr,
                    idx.index() as RoutingContext,
                    WhatAmI::Peer,
                );
            }
        }
        if tables.whatami == WhatAmI::Peer && !hat!(tables).full_net(WhatAmI::Peer) {
            res_mut.context_mut().client_data_route = Some(compute_data_route(
                tables,
                &mut expr,
                RoutingContext::default(),
                WhatAmI::Client,
            ));
            res_mut.context_mut().peer_data_route = Some(compute_data_route(
                tables,
                &mut expr,
                RoutingContext::default(),
                WhatAmI::Peer,
            ));
        }
        if tables.whatami == WhatAmI::Client {
            res_mut.context_mut().client_data_route = Some(compute_data_route(
                tables,
                &mut expr,
                RoutingContext::default(),
                WhatAmI::Client,
            ));
        }
        res_mut.context_mut().matching_pulls = compute_matching_pulls(tables, &mut expr);
    }
}

#[inline]
pub(crate) fn ingress_filter(tables: &Tables, face: &FaceState, expr: &mut RoutingExpr) -> bool {
    tables.whatami != WhatAmI::Router
        || face.whatami != WhatAmI::Peer
        || hat!(tables).peers_net.is_none()
        || tables.zid
            == *hat!(tables).elect_router(
                &tables.zid,
                expr.full_expr(),
                hat!(tables).get_router_links(face.zid),
            )
}

#[inline]
pub(crate) fn egress_filter(
    tables: &Tables,
    src_face: &FaceState,
    out_face: &Arc<FaceState>,
    expr: &mut RoutingExpr,
) -> bool {
    if src_face.id != out_face.id
        && match (src_face.mcast_group.as_ref(), out_face.mcast_group.as_ref()) {
            (Some(l), Some(r)) => l != r,
            _ => true,
        }
    {
        let dst_master = tables.whatami != WhatAmI::Router
            || out_face.whatami != WhatAmI::Peer
            || hat!(tables).peers_net.is_none()
            || tables.zid
                == *hat!(tables).elect_router(
                    &tables.zid,
                    expr.full_expr(),
                    hat!(tables).get_router_links(out_face.zid),
                );

        return dst_master
            && (src_face.whatami != WhatAmI::Peer
                || out_face.whatami != WhatAmI::Peer
                || hat!(tables).full_net(WhatAmI::Peer)
                || hat!(tables).failover_brokering(src_face.zid, out_face.zid));
    }
    false
}
