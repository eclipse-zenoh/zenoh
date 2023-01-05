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
use async_trait::async_trait;
use ordered_float::OrderedFloat;
use petgraph::graph::NodeIndex;
use std::borrow::Cow;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::sync::Arc;
use std::sync::{RwLock, Weak};
use zenoh_collections::Timed;
use zenoh_sync::get_mut_unchecked;

use zenoh_protocol::io::ZBuf;
use zenoh_protocol::proto::{DataInfo, QueryBody, RoutingContext};
use zenoh_protocol_core::key_expr::include::{Includer, DEFAULT_INCLUDER};
use zenoh_protocol_core::{
    ConsolidationMode, QueryTarget, QueryableInfo, WhatAmI, WireExpr, ZInt, ZenohId,
};

use crate::prelude::KeyExpr;

use super::face::FaceState;
use super::network::Network;
use super::resource::{
    elect_router, QueryRoute, QueryTargetQabl, QueryTargetQablSet, Resource, SessionContext,
};
use super::router::Tables;

pub(crate) struct Query {
    src_face: Arc<FaceState>,
    src_qid: ZInt,
}

#[cfg(feature = "complete_n")]
#[inline]
fn merge_qabl_infos(mut this: QueryableInfo, info: &QueryableInfo) -> QueryableInfo {
    this.complete += info.complete;
    this.distance = std::cmp::min(this.distance, info.distance);
    this
}

#[cfg(not(feature = "complete_n"))]
#[inline]
fn merge_qabl_infos(mut this: QueryableInfo, info: &QueryableInfo) -> QueryableInfo {
    this.complete = ZInt::from(this.complete != 0 || info.complete != 0);
    this.distance = std::cmp::min(this.distance, info.distance);
    this
}

fn local_router_qabl_info(tables: &Tables, res: &Arc<Resource>) -> QueryableInfo {
    let info = if tables.full_net(WhatAmI::Peer) {
        res.context.as_ref().and_then(|ctx| {
            ctx.peer_qabls.iter().fold(None, |accu, (zid, info)| {
                if *zid != tables.zid {
                    Some(match accu {
                        Some(accu) => merge_qabl_infos(accu, info),
                        None => info.clone(),
                    })
                } else {
                    accu
                }
            })
        })
    } else {
        None
    };
    res.session_ctxs
        .values()
        .fold(info, |accu, ctx| {
            if let Some(info) = ctx.qabl.as_ref() {
                Some(match accu {
                    Some(accu) => merge_qabl_infos(accu, info),
                    None => info.clone(),
                })
            } else {
                accu
            }
        })
        .unwrap_or(QueryableInfo {
            complete: 0,
            distance: 0,
        })
}

fn local_peer_qabl_info(tables: &Tables, res: &Arc<Resource>) -> QueryableInfo {
    let info = if tables.whatami == WhatAmI::Router && res.context.is_some() {
        res.context()
            .router_qabls
            .iter()
            .fold(None, |accu, (zid, info)| {
                if *zid != tables.zid {
                    Some(match accu {
                        Some(accu) => merge_qabl_infos(accu, info),
                        None => info.clone(),
                    })
                } else {
                    accu
                }
            })
    } else {
        None
    };
    res.session_ctxs
        .values()
        .fold(info, |accu, ctx| {
            if let Some(info) = ctx.qabl.as_ref() {
                Some(match accu {
                    Some(accu) => merge_qabl_infos(accu, info),
                    None => info.clone(),
                })
            } else {
                accu
            }
        })
        .unwrap_or(QueryableInfo {
            complete: 0,
            distance: 0,
        })
}

fn local_qabl_info(tables: &Tables, res: &Arc<Resource>, face: &Arc<FaceState>) -> QueryableInfo {
    let mut info = if tables.whatami == WhatAmI::Router && res.context.is_some() {
        res.context()
            .router_qabls
            .iter()
            .fold(None, |accu, (zid, info)| {
                if *zid != tables.zid {
                    Some(match accu {
                        Some(accu) => merge_qabl_infos(accu, info),
                        None => info.clone(),
                    })
                } else {
                    accu
                }
            })
    } else {
        None
    };
    if res.context.is_some() && tables.full_net(WhatAmI::Peer) {
        info = res
            .context()
            .peer_qabls
            .iter()
            .fold(info, |accu, (zid, info)| {
                if *zid != tables.zid {
                    Some(match accu {
                        Some(accu) => merge_qabl_infos(accu, info),
                        None => info.clone(),
                    })
                } else {
                    accu
                }
            })
    }
    res.session_ctxs
        .values()
        .fold(info, |accu, ctx| {
            if ctx.face.id != face.id && ctx.face.whatami != WhatAmI::Peer
                || face.whatami != WhatAmI::Peer
                || tables.failover_brokering(ctx.face.zid, face.zid)
            {
                if let Some(info) = ctx.qabl.as_ref() {
                    Some(match accu {
                        Some(accu) => merge_qabl_infos(accu, info),
                        None => info.clone(),
                    })
                } else {
                    accu
                }
            } else {
                accu
            }
        })
        .unwrap_or(QueryableInfo {
            complete: 0,
            distance: 0,
        })
}

#[allow(clippy::too_many_arguments)]
#[inline]
fn send_sourced_queryable_to_net_childs(
    tables: &Tables,
    net: &Network,
    childs: &[NodeIndex],
    res: &Arc<Resource>,
    qabl_info: &QueryableInfo,
    src_face: Option<&mut Arc<FaceState>>,
    routing_context: Option<RoutingContext>,
) {
    for child in childs {
        if net.graph.contains_node(*child) {
            match tables.get_face(&net.graph[*child].zid).cloned() {
                Some(mut someface) => {
                    if src_face.is_none() || someface.id != src_face.as_ref().unwrap().id {
                        let key_expr = Resource::decl_key(res, &mut someface);

                        log::debug!("Send queryable {} on {}", res.expr(), someface);

                        someface
                            .primitives
                            .decl_queryable(&key_expr, qabl_info, routing_context);
                    }
                }
                None => log::trace!("Unable to find face for zid {}", net.graph[*child].zid),
            }
        }
    }
}

fn propagate_simple_queryable(
    tables: &mut Tables,
    res: &Arc<Resource>,
    src_face: Option<&mut Arc<FaceState>>,
) {
    let full_peers_net = tables.full_net(WhatAmI::Peer);
    let faces = tables.faces.values().cloned();
    for mut dst_face in faces {
        let info = local_qabl_info(tables, res, &dst_face);
        let current_info = dst_face.local_qabls.get(res);
        if (src_face.is_none() || src_face.as_ref().unwrap().id != dst_face.id)
            && (current_info.is_none() || *current_info.unwrap() != info)
            && match tables.whatami {
                WhatAmI::Router => {
                    if full_peers_net {
                        dst_face.whatami == WhatAmI::Client
                    } else {
                        dst_face.whatami != WhatAmI::Router
                            && (src_face.is_none()
                                || src_face.as_ref().unwrap().whatami != WhatAmI::Peer
                                || dst_face.whatami != WhatAmI::Peer
                                || tables.failover_brokering(
                                    src_face.as_ref().unwrap().zid,
                                    dst_face.zid,
                                ))
                    }
                }
                WhatAmI::Peer => {
                    if full_peers_net {
                        dst_face.whatami == WhatAmI::Client
                    } else {
                        src_face.is_none()
                            || src_face.as_ref().unwrap().whatami == WhatAmI::Client
                            || dst_face.whatami == WhatAmI::Client
                    }
                }
                _ => {
                    src_face.is_none()
                        || src_face.as_ref().unwrap().whatami == WhatAmI::Client
                        || dst_face.whatami == WhatAmI::Client
                }
            }
        {
            get_mut_unchecked(&mut dst_face)
                .local_qabls
                .insert(res.clone(), info.clone());
            let key_expr = Resource::decl_key(res, &mut dst_face);
            dst_face.primitives.decl_queryable(&key_expr, &info, None);
        }
    }
}

fn propagate_sourced_queryable(
    tables: &Tables,
    res: &Arc<Resource>,
    qabl_info: &QueryableInfo,
    src_face: Option<&mut Arc<FaceState>>,
    source: &ZenohId,
    net_type: WhatAmI,
) {
    let net = tables.get_net(net_type).unwrap();
    match net.get_idx(source) {
        Some(tree_sid) => {
            if net.trees.len() > tree_sid.index() {
                send_sourced_queryable_to_net_childs(
                    tables,
                    net,
                    &net.trees[tree_sid.index()].childs,
                    res,
                    qabl_info,
                    src_face,
                    Some(RoutingContext::new(tree_sid.index() as ZInt)),
                );
            } else {
                log::trace!(
                    "Propagating qabl {}: tree for node {} sid:{} not yet ready",
                    res.expr(),
                    tree_sid.index(),
                    source
                );
            }
        }
        None => log::error!(
            "Error propagating qabl {}: cannot get index of {}!",
            res.expr(),
            source
        ),
    }
}

fn register_router_queryable(
    tables: &mut Tables,
    mut face: Option<&mut Arc<FaceState>>,
    res: &mut Arc<Resource>,
    qabl_info: &QueryableInfo,
    router: ZenohId,
) {
    let current_info = res.context().router_qabls.get(&router);
    if current_info.is_none() || current_info.unwrap() != qabl_info {
        // Register router queryable
        {
            log::debug!(
                "Register router queryable {} (router: {})",
                res.expr(),
                router,
            );
            get_mut_unchecked(res)
                .context_mut()
                .router_qabls
                .insert(router, qabl_info.clone());
            tables.router_qabls.insert(res.clone());
        }

        // Propagate queryable to routers
        propagate_sourced_queryable(
            tables,
            res,
            qabl_info,
            face.as_deref_mut(),
            &router,
            WhatAmI::Router,
        );
    }

    if tables.full_net(WhatAmI::Peer) {
        // Propagate queryable to peers
        if face.is_none() || face.as_ref().unwrap().whatami != WhatAmI::Peer {
            let local_info = local_peer_qabl_info(tables, res);
            register_peer_queryable(tables, face.as_deref_mut(), res, &local_info, tables.zid)
        }
    }

    // Propagate queryable to clients
    propagate_simple_queryable(tables, res, face);
}

pub fn declare_router_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    expr: &WireExpr,
    qabl_info: &QueryableInfo,
    router: ZenohId,
) {
    match tables.get_mapping(face, &expr.scope).cloned() {
        Some(mut prefix) => {
            let mut res = Resource::make_resource(tables, &mut prefix, expr.suffix.as_ref());
            Resource::match_resource(tables, &mut res);
            register_router_queryable(tables, Some(face), &mut res, qabl_info, router);

            compute_matches_query_routes(tables, &mut res);
        }
        None => log::error!("Declare router queryable for unknown scope {}!", expr.scope),
    }
}

fn register_peer_queryable(
    tables: &mut Tables,
    mut face: Option<&mut Arc<FaceState>>,
    res: &mut Arc<Resource>,
    qabl_info: &QueryableInfo,
    peer: ZenohId,
) {
    let current_info = res.context().peer_qabls.get(&peer);
    if current_info.is_none() || current_info.unwrap() != qabl_info {
        // Register peer queryable
        {
            log::debug!("Register peer queryable {} (peer: {})", res.expr(), peer,);
            get_mut_unchecked(res)
                .context_mut()
                .peer_qabls
                .insert(peer, qabl_info.clone());
            tables.peer_qabls.insert(res.clone());
        }

        // Propagate queryable to peers
        propagate_sourced_queryable(
            tables,
            res,
            qabl_info,
            face.as_deref_mut(),
            &peer,
            WhatAmI::Peer,
        );
    }

    if tables.whatami == WhatAmI::Peer {
        // Propagate queryable to clients
        propagate_simple_queryable(tables, res, face);
    }
}

pub fn declare_peer_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    expr: &WireExpr,
    qabl_info: &QueryableInfo,
    peer: ZenohId,
) {
    match tables.get_mapping(face, &expr.scope).cloned() {
        Some(mut prefix) => {
            let mut face = Some(face);
            let mut res = Resource::make_resource(tables, &mut prefix, expr.suffix.as_ref());
            Resource::match_resource(tables, &mut res);
            register_peer_queryable(tables, face.as_deref_mut(), &mut res, qabl_info, peer);

            if tables.whatami == WhatAmI::Router {
                let local_info = local_router_qabl_info(tables, &res);
                register_router_queryable(tables, face, &mut res, &local_info, tables.zid);
            }

            compute_matches_query_routes(tables, &mut res);
        }
        None => log::error!("Declare router queryable for unknown scope {}!", expr.scope),
    }
}

fn register_client_queryable(
    _tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    qabl_info: &QueryableInfo,
) {
    // Register queryable
    {
        let res = get_mut_unchecked(res);
        log::debug!("Register queryable {} (face: {})", res.expr(), face,);
        get_mut_unchecked(res.session_ctxs.entry(face.id).or_insert_with(|| {
            Arc::new(SessionContext {
                face: face.clone(),
                local_expr_id: None,
                remote_expr_id: None,
                subs: None,
                qabl: None,
                last_values: HashMap::new(),
            })
        }))
        .qabl = Some(qabl_info.clone());
    }
    get_mut_unchecked(face).remote_qabls.insert(res.clone());
}

pub fn declare_client_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    expr: &WireExpr,
    qabl_info: &QueryableInfo,
) {
    match tables.get_mapping(face, &expr.scope).cloned() {
        Some(mut prefix) => {
            let mut res = Resource::make_resource(tables, &mut prefix, expr.suffix.as_ref());
            Resource::match_resource(tables, &mut res);

            register_client_queryable(tables, face, &mut res, qabl_info);

            match tables.whatami {
                WhatAmI::Router => {
                    let local_details = local_router_qabl_info(tables, &res);
                    register_router_queryable(
                        tables,
                        Some(face),
                        &mut res,
                        &local_details,
                        tables.zid,
                    );
                }
                WhatAmI::Peer => {
                    if tables.full_net(WhatAmI::Peer) {
                        let local_details = local_peer_qabl_info(tables, &res);
                        register_peer_queryable(
                            tables,
                            Some(face),
                            &mut res,
                            &local_details,
                            tables.zid,
                        );
                    } else {
                        propagate_simple_queryable(tables, &res, Some(face));
                    }
                }
                _ => {
                    propagate_simple_queryable(tables, &res, Some(face));
                }
            }

            compute_matches_query_routes(tables, &mut res);
        }
        None => log::error!("Declare queryable for unknown scope {}!", expr.scope),
    }
}

#[inline]
fn remote_router_qabls(tables: &Tables, res: &Arc<Resource>) -> bool {
    res.context.is_some()
        && res
            .context()
            .router_qabls
            .keys()
            .any(|router| router != &tables.zid)
}

#[inline]
fn remote_peer_qabls(tables: &Tables, res: &Arc<Resource>) -> bool {
    res.context.is_some()
        && res
            .context()
            .peer_qabls
            .keys()
            .any(|peer| peer != &tables.zid)
}

#[inline]
fn client_qabls(res: &Arc<Resource>) -> Vec<Arc<FaceState>> {
    res.session_ctxs
        .values()
        .filter_map(|ctx| {
            if ctx.qabl.is_some() {
                Some(ctx.face.clone())
            } else {
                None
            }
        })
        .collect()
}

#[inline]
fn send_forget_sourced_queryable_to_net_childs(
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

                        log::debug!("Send forget queryable {}  on {}", res.expr(), someface);

                        someface
                            .primitives
                            .forget_queryable(&key_expr, routing_context);
                    }
                }
                None => log::trace!("Unable to find face for zid {}", net.graph[*child].zid),
            }
        }
    }
}

fn propagate_forget_simple_queryable(tables: &mut Tables, res: &mut Arc<Resource>) {
    for face in tables.faces.values_mut() {
        if face.local_qabls.contains_key(res) {
            let key_expr = Resource::get_best_key(res, "", face.id);
            face.primitives.forget_queryable(&key_expr, None);

            get_mut_unchecked(face).local_qabls.remove(res);
        }
    }
}

fn propagate_forget_simple_queryable_to_peers(tables: &mut Tables, res: &mut Arc<Resource>) {
    if !tables.full_net(WhatAmI::Peer)
        && res.context().router_qabls.len() == 1
        && res.context().router_qabls.contains_key(&tables.zid)
    {
        for mut face in tables
            .faces
            .values()
            .cloned()
            .collect::<Vec<Arc<FaceState>>>()
        {
            if face.whatami == WhatAmI::Peer
                && face.local_qabls.contains_key(res)
                && !res.session_ctxs.values().any(|s| {
                    face.zid != s.face.zid
                        && s.qabl.is_some()
                        && (s.face.whatami == WhatAmI::Client
                            || (s.face.whatami == WhatAmI::Peer
                                && tables.failover_brokering(s.face.zid, face.zid)))
                })
            {
                let key_expr = Resource::get_best_key(res, "", face.id);
                face.primitives.forget_queryable(&key_expr, None);

                get_mut_unchecked(&mut face).local_qabls.remove(res);
            }
        }
    }
}

fn propagate_forget_sourced_queryable(
    tables: &mut Tables,
    res: &mut Arc<Resource>,
    src_face: Option<&Arc<FaceState>>,
    source: &ZenohId,
    net_type: WhatAmI,
) {
    let net = tables.get_net(net_type).unwrap();
    match net.get_idx(source) {
        Some(tree_sid) => {
            if net.trees.len() > tree_sid.index() {
                send_forget_sourced_queryable_to_net_childs(
                    tables,
                    net,
                    &net.trees[tree_sid.index()].childs,
                    res,
                    src_face,
                    Some(RoutingContext::new(tree_sid.index() as ZInt)),
                );
            } else {
                log::trace!(
                    "Propagating forget qabl {}: tree for node {} sid:{} not yet ready",
                    res.expr(),
                    tree_sid.index(),
                    source
                );
            }
        }
        None => log::error!(
            "Error propagating forget qabl {}: cannot get index of {}!",
            res.expr(),
            source
        ),
    }
}

fn unregister_router_queryable(tables: &mut Tables, res: &mut Arc<Resource>, router: &ZenohId) {
    log::debug!(
        "Unregister router queryable {} (router: {})",
        res.expr(),
        router,
    );
    get_mut_unchecked(res)
        .context_mut()
        .router_qabls
        .remove(router);

    if res.context().router_qabls.is_empty() {
        tables.router_qabls.retain(|qabl| !Arc::ptr_eq(qabl, res));

        if tables.full_net(WhatAmI::Peer) {
            undeclare_peer_queryable(tables, None, res, &tables.zid.clone());
        }
        propagate_forget_simple_queryable(tables, res);
    }

    propagate_forget_simple_queryable_to_peers(tables, res);
}

fn undeclare_router_queryable(
    tables: &mut Tables,
    face: Option<&Arc<FaceState>>,
    res: &mut Arc<Resource>,
    router: &ZenohId,
) {
    if res.context().router_qabls.contains_key(router) {
        unregister_router_queryable(tables, res, router);
        propagate_forget_sourced_queryable(tables, res, face, router, WhatAmI::Router);
    }
}

pub fn forget_router_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    expr: &WireExpr,
    router: &ZenohId,
) {
    match tables.get_mapping(face, &expr.scope) {
        Some(prefix) => match Resource::get_resource(prefix, expr.suffix.as_ref()) {
            Some(mut res) => {
                undeclare_router_queryable(tables, Some(face), &mut res, router);

                compute_matches_query_routes(tables, &mut res);
                Resource::clean(&mut res)
            }
            None => log::error!("Undeclare unknown router queryable!"),
        },
        None => log::error!("Undeclare router queryable with unknown scope!"),
    }
}

fn unregister_peer_queryable(tables: &mut Tables, res: &mut Arc<Resource>, peer: &ZenohId) {
    log::debug!("Unregister peer queryable {} (peer: {})", res.expr(), peer,);
    get_mut_unchecked(res).context_mut().peer_qabls.remove(peer);

    if res.context().peer_qabls.is_empty() {
        tables.peer_qabls.retain(|qabl| !Arc::ptr_eq(qabl, res));

        if tables.whatami == WhatAmI::Peer {
            propagate_forget_simple_queryable(tables, res);
        }
    }
}

fn undeclare_peer_queryable(
    tables: &mut Tables,
    face: Option<&Arc<FaceState>>,
    res: &mut Arc<Resource>,
    peer: &ZenohId,
) {
    if res.context().peer_qabls.contains_key(peer) {
        unregister_peer_queryable(tables, res, peer);
        propagate_forget_sourced_queryable(tables, res, face, peer, WhatAmI::Peer);
    }
}

pub fn forget_peer_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    expr: &WireExpr,
    peer: &ZenohId,
) {
    match tables.get_mapping(face, &expr.scope) {
        Some(prefix) => match Resource::get_resource(prefix, expr.suffix.as_ref()) {
            Some(mut res) => {
                undeclare_peer_queryable(tables, Some(face), &mut res, peer);

                if tables.whatami == WhatAmI::Router {
                    let client_qabls = res.session_ctxs.values().any(|ctx| ctx.qabl.is_some());
                    let peer_qabls = remote_peer_qabls(tables, &res);
                    if !client_qabls && !peer_qabls {
                        undeclare_router_queryable(tables, None, &mut res, &tables.zid.clone());
                    } else {
                        let local_info = local_router_qabl_info(tables, &res);
                        register_router_queryable(tables, None, &mut res, &local_info, tables.zid);
                    }
                }

                compute_matches_query_routes(tables, &mut res);
                Resource::clean(&mut res)
            }
            None => log::error!("Undeclare unknown peer queryable!"),
        },
        None => log::error!("Undeclare peer queryable with unknown scope!"),
    }
}

pub(crate) fn undeclare_client_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
) {
    log::debug!("Unregister client queryable {} for {}", res.expr(), face);
    if let Some(ctx) = get_mut_unchecked(res).session_ctxs.get_mut(&face.id) {
        get_mut_unchecked(ctx).qabl = None;
        if ctx.qabl.is_none() {
            get_mut_unchecked(face).remote_qabls.remove(res);
        }
    }

    let mut client_qabls = client_qabls(res);
    let router_qabls = remote_router_qabls(tables, res);
    let peer_qabls = remote_peer_qabls(tables, res);

    match tables.whatami {
        WhatAmI::Router => {
            if client_qabls.is_empty() && !peer_qabls {
                undeclare_router_queryable(tables, None, res, &tables.zid.clone());
            } else {
                let local_info = local_router_qabl_info(tables, res);
                register_router_queryable(tables, None, res, &local_info, tables.zid);
                propagate_forget_simple_queryable_to_peers(tables, res);
            }
        }
        WhatAmI::Peer => {
            if tables.full_net(WhatAmI::Peer) {
                if client_qabls.is_empty() {
                    undeclare_peer_queryable(tables, None, res, &tables.zid.clone());
                } else {
                    let local_info = local_peer_qabl_info(tables, res);
                    register_peer_queryable(tables, None, res, &local_info, tables.zid);
                }
            } else if client_qabls.is_empty() {
                propagate_forget_simple_queryable(tables, res);
            } else {
                propagate_simple_queryable(tables, res, None);
            }
        }
        _ => {
            if client_qabls.is_empty() {
                propagate_forget_simple_queryable(tables, res);
            } else {
                propagate_simple_queryable(tables, res, None);
            }
        }
    }

    if client_qabls.len() == 1 && !router_qabls && !peer_qabls {
        let face = &mut client_qabls[0];
        if face.local_qabls.contains_key(res) {
            let key_expr = Resource::get_best_key(res, "", face.id);
            face.primitives.forget_queryable(&key_expr, None);

            get_mut_unchecked(face).local_qabls.remove(res);
        }
    }

    compute_matches_query_routes(tables, res);
    Resource::clean(res)
}

pub fn forget_client_queryable(tables: &mut Tables, face: &mut Arc<FaceState>, expr: &WireExpr) {
    match tables.get_mapping(face, &expr.scope) {
        Some(prefix) => match Resource::get_resource(prefix, expr.suffix.as_ref()) {
            Some(mut res) => {
                undeclare_client_queryable(tables, face, &mut res);
            }
            None => log::error!("Undeclare unknown queryable!"),
        },
        None => log::error!("Undeclare queryable with unknown scope!"),
    }
}

pub(crate) fn queries_new_face(tables: &mut Tables, face: &mut Arc<FaceState>) {
    match tables.whatami {
        WhatAmI::Router => {
            if face.whatami == WhatAmI::Client {
                for qabl in tables.router_qabls.iter() {
                    if qabl.context.is_some() {
                        let info = local_qabl_info(tables, qabl, face);
                        get_mut_unchecked(face)
                            .local_qabls
                            .insert(qabl.clone(), info.clone());
                        let key_expr = Resource::decl_key(qabl, face);
                        face.primitives.decl_queryable(&key_expr, &info, None);
                    }
                }
            } else if face.whatami == WhatAmI::Peer && !tables.full_net(WhatAmI::Peer) {
                for qabl in tables.router_qabls.iter() {
                    if qabl.context.is_some()
                        && (qabl.context().router_qabls.keys().any(|r| *r != tables.zid)
                            || qabl.session_ctxs.values().any(|s| {
                                s.qabl.is_some()
                                    && (s.face.whatami == WhatAmI::Client
                                        || (s.face.whatami == WhatAmI::Peer
                                            && tables.failover_brokering(s.face.zid, face.zid)))
                            }))
                    {
                        let info = local_qabl_info(tables, qabl, face);
                        get_mut_unchecked(face)
                            .local_qabls
                            .insert(qabl.clone(), info.clone());
                        let key_expr = Resource::decl_key(qabl, face);
                        face.primitives.decl_queryable(&key_expr, &info, None);
                    }
                }
            }
        }
        WhatAmI::Peer => {
            if tables.full_net(WhatAmI::Peer) {
                if face.whatami == WhatAmI::Client {
                    for qabl in &tables.peer_qabls {
                        if qabl.context.is_some() {
                            let info = local_qabl_info(tables, qabl, face);
                            get_mut_unchecked(face)
                                .local_qabls
                                .insert(qabl.clone(), info.clone());
                            let key_expr = Resource::decl_key(qabl, face);
                            face.primitives.decl_queryable(&key_expr, &info, None);
                        }
                    }
                }
            } else {
                for face in tables
                    .faces
                    .values()
                    .cloned()
                    .collect::<Vec<Arc<FaceState>>>()
                {
                    for qabl in face.remote_qabls.iter() {
                        propagate_simple_queryable(tables, qabl, Some(&mut face.clone()));
                    }
                }
            }
        }
        WhatAmI::Client => {
            for face in tables
                .faces
                .values()
                .cloned()
                .collect::<Vec<Arc<FaceState>>>()
            {
                for qabl in face.remote_qabls.iter() {
                    propagate_simple_queryable(tables, qabl, Some(&mut face.clone()));
                }
            }
        }
    }
}

pub(crate) fn queries_remove_node(tables: &mut Tables, node: &ZenohId, net_type: WhatAmI) {
    match net_type {
        WhatAmI::Router => {
            let mut qabls = vec![];
            for res in tables.router_qabls.iter() {
                for qabl in res.context().router_qabls.keys() {
                    if qabl == node {
                        qabls.push(res.clone());
                    }
                }
            }
            for mut res in qabls {
                unregister_router_queryable(tables, &mut res, node);

                compute_matches_query_routes(tables, &mut res);
                Resource::clean(&mut res);
            }
        }
        WhatAmI::Peer => {
            let mut qabls = vec![];
            for res in tables.router_qabls.iter() {
                for qabl in res.context().router_qabls.keys() {
                    if qabl == node {
                        qabls.push(res.clone());
                    }
                }
            }
            for mut res in qabls {
                unregister_peer_queryable(tables, &mut res, node);

                if tables.whatami == WhatAmI::Router {
                    let client_qabls = res.session_ctxs.values().any(|ctx| ctx.qabl.is_some());
                    let peer_qabls = remote_peer_qabls(tables, &res);
                    if !client_qabls && !peer_qabls {
                        undeclare_router_queryable(tables, None, &mut res, &tables.zid.clone());
                    } else {
                        let local_info = local_router_qabl_info(tables, &res);
                        register_router_queryable(tables, None, &mut res, &local_info, tables.zid);
                    }
                }

                compute_matches_query_routes(tables, &mut res);
                Resource::clean(&mut res)
            }
        }
        _ => (),
    }
}

pub(crate) fn queries_linkstate_change(tables: &mut Tables, zid: &ZenohId, links: &[ZenohId]) {
    if let Some(src_face) = tables.get_face(zid) {
        if tables.router_peers_failover_brokering
            && tables.whatami == WhatAmI::Router
            && src_face.whatami == WhatAmI::Peer
        {
            for res in &src_face.remote_qabls {
                if !remote_router_qabls(tables, res) {
                    for mut dst_face in tables
                        .faces
                        .values()
                        .cloned()
                        .collect::<Vec<Arc<FaceState>>>()
                    {
                        if dst_face.whatami == WhatAmI::Peer && src_face.zid != dst_face.zid {
                            if !Tables::failover_brokering_to(links, dst_face.zid) {
                                if dst_face.local_qabls.contains_key(res) {
                                    let key_expr = Resource::get_best_key(res, "", dst_face.id);
                                    dst_face.primitives.forget_queryable(&key_expr, None);

                                    get_mut_unchecked(&mut dst_face).local_qabls.remove(res);
                                }
                            } else {
                                get_mut_unchecked(&mut dst_face)
                                    .local_subs
                                    .insert(res.clone());
                                let key_expr = Resource::decl_key(res, &mut dst_face);
                                let info = local_qabl_info(tables, res, &dst_face);
                                dst_face.primitives.decl_queryable(&key_expr, &info, None);
                            }
                        }
                    }
                }
            }
        }
    }
}

pub(crate) fn queries_tree_change(
    tables: &mut Tables,
    new_childs: &[Vec<NodeIndex>],
    net_type: WhatAmI,
) {
    // propagate qabls to new childs
    for (tree_sid, tree_childs) in new_childs.iter().enumerate() {
        if !tree_childs.is_empty() {
            let net = tables.get_net(net_type).unwrap();
            let tree_idx = NodeIndex::new(tree_sid);
            if net.graph.contains_node(tree_idx) {
                let tree_id = net.graph[tree_idx].zid;

                let qabls_res = match net_type {
                    WhatAmI::Router => &tables.router_qabls,
                    _ => &tables.peer_qabls,
                };

                for res in qabls_res {
                    let qabls = match net_type {
                        WhatAmI::Router => &res.context().router_qabls,
                        _ => &res.context().peer_qabls,
                    };
                    if let Some(qabl_info) = qabls.get(&tree_id) {
                        send_sourced_queryable_to_net_childs(
                            tables,
                            net,
                            tree_childs,
                            res,
                            qabl_info,
                            None,
                            Some(RoutingContext::new(tree_sid as ZInt)),
                        );
                    }
                }
            }
        }
    }

    // recompute routes
    compute_query_routes_from(tables, &mut tables.root_res.clone());
}

#[inline]
#[allow(clippy::too_many_arguments)]
fn insert_target_for_qabls(
    route: &mut QueryTargetQablSet,
    prefix: &Arc<Resource>,
    suffix: &str,
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
                                    let key_expr = Resource::get_best_key(prefix, suffix, face.id);
                                    route.push(QueryTargetQabl {
                                        direction: (
                                            face.clone(),
                                            key_expr.to_owned(),
                                            if source != 0 {
                                                Some(RoutingContext::new(source as ZInt))
                                            } else {
                                                None
                                            },
                                        ),
                                        complete: if complete { qabl_info.complete } else { 0 },
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
    prefix: &Arc<Resource>,
    suffix: &str,
    source: Option<usize>,
    source_type: WhatAmI,
) -> Arc<QueryTargetQablSet> {
    let mut route = QueryTargetQablSet::new();
    let key_expr = prefix.expr() + suffix;
    if key_expr.ends_with('/') {
        return EMPTY_ROUTE.clone();
    }
    log::trace!(
        "compute_query_route({}, {:?}, {:?})",
        key_expr,
        source,
        source_type
    );
    let key_expr = match KeyExpr::try_from(key_expr) {
        Ok(ke) => ke,
        Err(e) => {
            log::warn!("Invalid KE reached the system: {}", e);
            return EMPTY_ROUTE.clone();
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
        let complete = DEFAULT_INCLUDER.includes(mres.expr().as_bytes(), key_expr.as_bytes());
        if tables.whatami == WhatAmI::Router {
            if master || source_type == WhatAmI::Router {
                let net = tables.routers_net.as_ref().unwrap();
                let router_source = match source_type {
                    WhatAmI::Router => source.unwrap(),
                    _ => net.idx.index(),
                };
                insert_target_for_qabls(
                    &mut route,
                    prefix,
                    suffix,
                    tables,
                    net,
                    router_source,
                    &mres.context().router_qabls,
                    complete,
                );
            }

            if (master || source_type != WhatAmI::Router) && tables.full_net(WhatAmI::Peer) {
                let net = tables.peers_net.as_ref().unwrap();
                let peer_source = match source_type {
                    WhatAmI::Peer => source.unwrap(),
                    _ => net.idx.index(),
                };
                insert_target_for_qabls(
                    &mut route,
                    prefix,
                    suffix,
                    tables,
                    net,
                    peer_source,
                    &mres.context().peer_qabls,
                    complete,
                );
            }
        }

        if tables.whatami == WhatAmI::Peer && tables.full_net(WhatAmI::Peer) {
            let net = tables.peers_net.as_ref().unwrap();
            let peer_source = match source_type {
                WhatAmI::Router | WhatAmI::Peer => source.unwrap(),
                _ => net.idx.index(),
            };
            insert_target_for_qabls(
                &mut route,
                prefix,
                suffix,
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
                    let key_expr = Resource::get_best_key(prefix, suffix, *sid);
                    if let Some(qabl_info) = context.qabl.as_ref() {
                        route.push(QueryTargetQabl {
                            direction: (context.face.clone(), key_expr.to_owned(), None),
                            complete: if complete { qabl_info.complete } else { 0 },
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

pub(crate) fn compute_query_routes(tables: &mut Tables, res: &mut Arc<Resource>) {
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
            let routers_query_routes = &mut res_mut.context_mut().routers_query_routes;
            routers_query_routes.clear();
            routers_query_routes
                .resize_with(max_idx.index() + 1, || Arc::new(QueryTargetQablSet::new()));

            for idx in &indexes {
                routers_query_routes[idx.index()] =
                    compute_query_route(tables, res, "", Some(idx.index()), WhatAmI::Router);
            }

            res_mut.context_mut().peer_query_route =
                Some(compute_query_route(tables, res, "", None, WhatAmI::Peer));
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
            let peers_query_routes = &mut res_mut.context_mut().peers_query_routes;
            peers_query_routes.clear();
            peers_query_routes
                .resize_with(max_idx.index() + 1, || Arc::new(QueryTargetQablSet::new()));

            for idx in &indexes {
                peers_query_routes[idx.index()] =
                    compute_query_route(tables, res, "", Some(idx.index()), WhatAmI::Peer);
            }
        }
        if tables.whatami == WhatAmI::Peer && !tables.full_net(WhatAmI::Peer) {
            res_mut.context_mut().client_query_route =
                Some(compute_query_route(tables, res, "", None, WhatAmI::Client));
            res_mut.context_mut().peer_query_route =
                Some(compute_query_route(tables, res, "", None, WhatAmI::Peer));
        }
        if tables.whatami == WhatAmI::Client {
            res_mut.context_mut().client_query_route =
                Some(compute_query_route(tables, res, "", None, WhatAmI::Client));
        }
    }
}

fn compute_query_routes_from(tables: &mut Tables, res: &mut Arc<Resource>) {
    compute_query_routes(tables, res);
    let res = get_mut_unchecked(res);
    for child in res.childs.values_mut() {
        compute_query_routes_from(tables, child);
    }
}

pub(crate) fn compute_matches_query_routes(tables: &mut Tables, res: &mut Arc<Resource>) {
    if res.context.is_some() {
        compute_query_routes(tables, res);

        let resclone = res.clone();
        for match_ in &mut get_mut_unchecked(res).context_mut().matches {
            if !Arc::ptr_eq(&match_.upgrade().unwrap(), &resclone) {
                compute_query_routes(tables, &mut match_.upgrade().unwrap());
            }
        }
    }
}

#[inline]
fn insert_pending_query(outface: &mut Arc<FaceState>, query: Arc<Query>) -> ZInt {
    let outface_mut = get_mut_unchecked(outface);
    outface_mut.next_qid += 1;
    let qid = outface_mut.next_qid;
    outface_mut.pending_queries.insert(qid, query);
    qid
}

#[inline]
fn compute_final_route(
    tables: &Tables,
    qabls: &Arc<QueryTargetQablSet>,
    src_face: &Arc<FaceState>,
    target: &QueryTarget,
    query: Arc<Query>,
) -> QueryRoute {
    match target {
        QueryTarget::All => {
            let mut route = HashMap::new();
            if src_face.whatami == WhatAmI::Peer && !tables.full_net(WhatAmI::Peer) {
                let source_links = tables
                    .peers_net
                    .as_ref()
                    .map(|net| net.get_links(src_face.zid))
                    .unwrap_or_default();
                for qabl in qabls.iter() {
                    if qabl.direction.0.id != src_face.id
                        && (qabl.direction.0.whatami != WhatAmI::Peer
                            || (tables.router_peers_failover_brokering
                                && Tables::failover_brokering_to(
                                    &source_links,
                                    qabl.direction.0.zid,
                                )))
                    {
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
            } else {
                for qabl in qabls.iter() {
                    if qabl.direction.0.id != src_face.id {
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
            }
            route
        }
        QueryTarget::AllComplete => {
            let mut route = HashMap::new();
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
                                    &source_links,
                                    qabl.direction.0.zid,
                                )))
                    {
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
            } else {
                for qabl in qabls.iter() {
                    if qabl.direction.0.id != src_face.id && qabl.complete > 0 {
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
            }
            route
        }
        #[cfg(feature = "complete_n")]
        QueryTarget::Complete(n) => {
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
                                    &source_links,
                                    qabl.direction.0.zid,
                                )))
                    {
                        let nb = std::cmp::min(qabl.complete, remaining);
                        route.entry(qabl.direction.0.id).or_insert_with(|| {
                            let mut direction = qabl.direction.clone();
                            let qid = insert_pending_query(&mut direction.0, query.clone());
                            (direction, qid, QueryTarget::Complete(nb))
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
                            (direction, qid, QueryTarget::Complete(nb))
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
        QueryTarget::BestMatching => {
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
                compute_final_route(tables, qabls, src_face, &QueryTarget::All, query)
            }
        }
    }
}

#[derive(Clone)]
struct QueryCleanup {
    tables: Arc<RwLock<Tables>>,
    face: Weak<FaceState>,
    qid: ZInt,
}

#[async_trait]
impl Timed for QueryCleanup {
    async fn run(&mut self) {
        if let Some(mut face) = self.face.upgrade() {
            let tables_lock = zwrite!(self.tables);
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

#[allow(clippy::too_many_arguments)]
pub fn route_query(
    tables_ref: &Arc<RwLock<Tables>>,
    face: &Arc<FaceState>,
    expr: &WireExpr,
    parameters: &str,
    qid: ZInt,
    target: QueryTarget,
    consolidation: ConsolidationMode,
    body: Option<QueryBody>,
    routing_context: Option<RoutingContext>,
) {
    let tables = zwrite!(tables_ref);
    match tables.get_mapping(face, &expr.scope) {
        Some(prefix) => {
            log::debug!(
                "Route query {}:{} for res {}{}",
                face,
                qid,
                prefix.expr(),
                expr.suffix.as_ref(),
            );

            let route = match tables.whatami {
                WhatAmI::Router => match face.whatami {
                    WhatAmI::Router => {
                        let routers_net = tables.routers_net.as_ref().unwrap();
                        let local_context = routers_net
                            .get_local_context(routing_context.map(|rc| rc.tree_id), face.link_id);
                        Resource::get_resource(prefix, expr.suffix.as_ref())
                            .and_then(|res| res.routers_query_route(local_context))
                            .unwrap_or_else(|| {
                                compute_query_route(
                                    &tables,
                                    prefix,
                                    expr.suffix.as_ref(),
                                    Some(local_context),
                                    face.whatami,
                                )
                            })
                    }
                    WhatAmI::Peer => {
                        if tables.full_net(WhatAmI::Peer) {
                            let peers_net = tables.peers_net.as_ref().unwrap();
                            let local_context = peers_net.get_local_context(
                                routing_context.map(|rc| rc.tree_id),
                                face.link_id,
                            );
                            Resource::get_resource(prefix, expr.suffix.as_ref())
                                .and_then(|res| res.peers_query_route(local_context))
                                .unwrap_or_else(|| {
                                    compute_query_route(
                                        &tables,
                                        prefix,
                                        expr.suffix.as_ref(),
                                        Some(local_context),
                                        face.whatami,
                                    )
                                })
                        } else {
                            Resource::get_resource(prefix, expr.suffix.as_ref())
                                .and_then(|res| res.peer_query_route())
                                .unwrap_or_else(|| {
                                    compute_query_route(
                                        &tables,
                                        prefix,
                                        expr.suffix.as_ref(),
                                        None,
                                        face.whatami,
                                    )
                                })
                        }
                    }
                    _ => Resource::get_resource(prefix, expr.suffix.as_ref())
                        .and_then(|res| res.routers_query_route(0))
                        .unwrap_or_else(|| {
                            compute_query_route(
                                &tables,
                                prefix,
                                expr.suffix.as_ref(),
                                None,
                                face.whatami,
                            )
                        }),
                },
                WhatAmI::Peer => {
                    if tables.full_net(WhatAmI::Peer) {
                        match face.whatami {
                            WhatAmI::Router | WhatAmI::Peer => {
                                let peers_net = tables.peers_net.as_ref().unwrap();
                                let local_context = peers_net.get_local_context(
                                    routing_context.map(|rc| rc.tree_id),
                                    face.link_id,
                                );
                                Resource::get_resource(prefix, expr.suffix.as_ref())
                                    .and_then(|res| res.peers_query_route(local_context))
                                    .unwrap_or_else(|| {
                                        compute_query_route(
                                            &tables,
                                            prefix,
                                            expr.suffix.as_ref(),
                                            Some(local_context),
                                            face.whatami,
                                        )
                                    })
                            }
                            _ => Resource::get_resource(prefix, expr.suffix.as_ref())
                                .and_then(|res| res.peers_query_route(0))
                                .unwrap_or_else(|| {
                                    compute_query_route(
                                        &tables,
                                        prefix,
                                        expr.suffix.as_ref(),
                                        None,
                                        face.whatami,
                                    )
                                }),
                        }
                    } else {
                        Resource::get_resource(prefix, expr.suffix.as_ref())
                            .and_then(|res| match face.whatami {
                                WhatAmI::Client => res.client_query_route(),
                                _ => res.peer_query_route(),
                            })
                            .unwrap_or_else(|| {
                                compute_query_route(
                                    &tables,
                                    prefix,
                                    expr.suffix.as_ref(),
                                    None,
                                    face.whatami,
                                )
                            })
                    }
                }
                _ => Resource::get_resource(prefix, expr.suffix.as_ref())
                    .and_then(|res| res.client_query_route())
                    .unwrap_or_else(|| {
                        compute_query_route(
                            &tables,
                            prefix,
                            expr.suffix.as_ref(),
                            None,
                            face.whatami,
                        )
                    }),
            };

            let query = Arc::new(Query {
                src_face: face.clone(),
                src_qid: qid,
            });

            let route = compute_final_route(&tables, &route, face, &target, query);

            drop(tables);

            if route.is_empty() {
                log::debug!("Send final reply {}:{} (no matching queryables)", face, qid);
                face.primitives.clone().send_reply_final(qid)
            } else {
                // let timer = tables.timer.clone();
                // let timeout = tables.queries_default_timeout;
                #[cfg(feature = "complete_n")]
                {
                    for ((outface, key_expr, context), qid, t) in route.values() {
                        // timer.add(TimedEvent::once(
                        //     Instant::now() + timout,
                        //     QueryCleanup {
                        //         tables: tables_ref.clone(),
                        //         face: Arc::downgrade(&outface),
                        //         *qid,
                        //     },
                        // ));
                        log::trace!("Propagate query {}:{} to {}", face, qid, outface);
                        outface.primitives.send_query(
                            key_expr,
                            parameters,
                            *qid,
                            *t,
                            consolidation,
                            body.clone(),
                            *context,
                        );
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
                        log::trace!("Propagate query {}:{} to {}", face, qid, outface);
                        outface.primitives.send_query(
                            key_expr,
                            parameters,
                            *qid,
                            target,
                            consolidation,
                            body.clone(),
                            *context,
                        );
                    }
                }
            }
        }
        None => {
            log::error!(
                "Route query with unknown scope {}! Send final reply.",
                expr.scope
            );
            drop(tables);
            face.primitives.clone().send_reply_final(qid)
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn route_send_reply_data(
    tables_ref: &RwLock<Tables>,
    face: &mut Arc<FaceState>,
    qid: ZInt,
    replier_id: ZenohId,
    key_expr: WireExpr,
    info: Option<DataInfo>,
    payload: ZBuf,
) {
    let tables_lock = zread!(tables_ref);
    match face.pending_queries.get(&qid) {
        Some(query) => {
            drop(tables_lock);
            query.src_face.primitives.clone().send_reply_data(
                query.src_qid,
                replier_id,
                key_expr,
                info,
                payload,
            );
        }
        None => log::warn!(
            "Route reply {}:{} from {}: Query nof found!",
            face,
            qid,
            face
        ),
    }
}

pub(crate) fn route_send_reply_final(
    tables_ref: &RwLock<Tables>,
    face: &mut Arc<FaceState>,
    qid: ZInt,
) {
    let tables_lock = zwrite!(tables_ref);
    match get_mut_unchecked(face).pending_queries.remove(&qid) {
        Some(query) => {
            drop(tables_lock);
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

pub(crate) fn finalize_pending_queries(_tables: &mut Tables, face: &mut Arc<FaceState>) {
    for (_, query) in get_mut_unchecked(face).pending_queries.drain() {
        finalize_pending_query(query);
    }
}

pub(crate) fn finalize_pending_query(query: Arc<Query>) {
    if let Ok(query) = Arc::try_unwrap(query) {
        log::debug!("Propagate final reply {}:{}", query.src_face, query.src_qid);
        query
            .src_face
            .primitives
            .clone()
            .send_reply_final(query.src_qid);
    }
}
