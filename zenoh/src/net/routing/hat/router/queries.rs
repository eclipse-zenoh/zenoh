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
use super::network::Network;
use super::{face_hat, face_hat_mut, hat, hat_mut, res_hat, res_hat_mut};
use super::{get_peer, get_router, HatCode, HatContext, HatFace, HatTables};
use crate::net::routing::dispatcher::face::FaceState;
use crate::net::routing::dispatcher::queries::*;
use crate::net::routing::dispatcher::resource::{NodeId, Resource, SessionContext};
use crate::net::routing::dispatcher::tables::{
    QueryRoutes, QueryTargetQabl, QueryTargetQablSet, RoutingExpr,
};
use crate::net::routing::dispatcher::tables::{Tables, TablesLock};
use crate::net::routing::hat::HatQueriesTrait;
use crate::net::routing::{RoutingContext, PREFIX_LIVELINESS};
use ordered_float::OrderedFloat;
use petgraph::graph::NodeIndex;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, RwLockReadGuard};
use zenoh_buffers::ZBuf;
use zenoh_protocol::core::key_expr::include::{Includer, DEFAULT_INCLUDER};
use zenoh_protocol::core::key_expr::OwnedKeyExpr;
use zenoh_protocol::{
    core::{key_expr::keyexpr, WhatAmI, WireExpr, ZenohId},
    network::declare::{
        common::ext::WireExprType, ext, queryable::ext::QueryableInfo, Declare, DeclareBody,
        DeclareQueryable, UndeclareQueryable,
    },
};
use zenoh_sync::get_mut_unchecked;

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
    this.complete = u8::from(this.complete != 0 || info.complete != 0);
    this.distance = std::cmp::min(this.distance, info.distance);
    this
}

fn local_router_qabl_info(tables: &Tables, res: &Arc<Resource>) -> QueryableInfo {
    let info = if hat!(tables).full_net(WhatAmI::Peer) {
        res.context.as_ref().and_then(|_| {
            res_hat!(res)
                .peer_qabls
                .iter()
                .fold(None, |accu, (zid, info)| {
                    if *zid != tables.zid {
                        Some(match accu {
                            Some(accu) => merge_qabl_infos(accu, info),
                            None => *info,
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
                    None => *info,
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
    let info = if res.context.is_some() {
        res_hat!(res)
            .router_qabls
            .iter()
            .fold(None, |accu, (zid, info)| {
                if *zid != tables.zid {
                    Some(match accu {
                        Some(accu) => merge_qabl_infos(accu, info),
                        None => *info,
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
                    None => *info,
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
    let mut info = if res.context.is_some() {
        res_hat!(res)
            .router_qabls
            .iter()
            .fold(None, |accu, (zid, info)| {
                if *zid != tables.zid {
                    Some(match accu {
                        Some(accu) => merge_qabl_infos(accu, info),
                        None => *info,
                    })
                } else {
                    accu
                }
            })
    } else {
        None
    };
    if res.context.is_some() && hat!(tables).full_net(WhatAmI::Peer) {
        info = res_hat!(res)
            .peer_qabls
            .iter()
            .fold(info, |accu, (zid, info)| {
                if *zid != tables.zid {
                    Some(match accu {
                        Some(accu) => merge_qabl_infos(accu, info),
                        None => *info,
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
                || hat!(tables).failover_brokering(ctx.face.zid, face.zid)
            {
                if let Some(info) = ctx.qabl.as_ref() {
                    Some(match accu {
                        Some(accu) => merge_qabl_infos(accu, info),
                        None => *info,
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
    routing_context: NodeId,
) {
    for child in childs {
        if net.graph.contains_node(*child) {
            match tables.get_face(&net.graph[*child].zid).cloned() {
                Some(mut someface) => {
                    if src_face.is_none() || someface.id != src_face.as_ref().unwrap().id {
                        let key_expr = Resource::decl_key(res, &mut someface);

                        log::debug!("Send queryable {} on {}", res.expr(), someface);

                        someface.primitives.send_declare(RoutingContext::with_expr(
                            Declare {
                                ext_qos: ext::QoSType::declare_default(),
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType {
                                    node_id: routing_context,
                                },
                                body: DeclareBody::DeclareQueryable(DeclareQueryable {
                                    id: 0, // TODO
                                    wire_expr: key_expr,
                                    ext_info: *qabl_info,
                                }),
                            },
                            res.expr(),
                        ));
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
    let full_peers_net = hat!(tables).full_net(WhatAmI::Peer);
    let faces = tables.faces.values().cloned();
    for mut dst_face in faces {
        let info = local_qabl_info(tables, res, &dst_face);
        let current_info = face_hat!(dst_face).local_qabls.get(res);
        if (src_face.is_none() || src_face.as_ref().unwrap().id != dst_face.id)
            && (current_info.is_none() || *current_info.unwrap() != info)
            && if full_peers_net {
                dst_face.whatami == WhatAmI::Client
            } else {
                dst_face.whatami != WhatAmI::Router
                    && (src_face.is_none()
                        || src_face.as_ref().unwrap().whatami != WhatAmI::Peer
                        || dst_face.whatami != WhatAmI::Peer
                        || hat!(tables)
                            .failover_brokering(src_face.as_ref().unwrap().zid, dst_face.zid))
            }
        {
            face_hat_mut!(&mut dst_face)
                .local_qabls
                .insert(res.clone(), info);
            let key_expr = Resource::decl_key(res, &mut dst_face);
            dst_face.primitives.send_declare(RoutingContext::with_expr(
                Declare {
                    ext_qos: ext::QoSType::declare_default(),
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::default(),
                    body: DeclareBody::DeclareQueryable(DeclareQueryable {
                        id: 0, // TODO
                        wire_expr: key_expr,
                        ext_info: info,
                    }),
                },
                res.expr(),
            ));
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
    let net = hat!(tables).get_net(net_type).unwrap();
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
                    tree_sid.index() as NodeId,
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
    let current_info = res_hat!(res).router_qabls.get(&router);
    if current_info.is_none() || current_info.unwrap() != qabl_info {
        // Register router queryable
        {
            log::debug!(
                "Register router queryable {} (router: {})",
                res.expr(),
                router,
            );
            res_hat_mut!(res).router_qabls.insert(router, *qabl_info);
            hat_mut!(tables).router_qabls.insert(res.clone());
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

    if hat!(tables).full_net(WhatAmI::Peer) {
        // Propagate queryable to peers
        if face.is_none() || face.as_ref().unwrap().whatami != WhatAmI::Peer {
            let local_info = local_peer_qabl_info(tables, res);
            register_peer_queryable(tables, face.as_deref_mut(), res, &local_info, tables.zid)
        }
    }

    // Propagate queryable to clients
    propagate_simple_queryable(tables, res, face);
}

fn declare_router_queryable(
    tables: &TablesLock,
    rtables: RwLockReadGuard<Tables>,
    face: &mut Arc<FaceState>,
    expr: &WireExpr,
    qabl_info: &QueryableInfo,
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
                    log::debug!("Register router queryable {}", fullexpr);
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
            register_router_queryable(&mut wtables, Some(face), &mut res, qabl_info, router);
            disable_matches_query_routes(&mut wtables, &mut res);
            drop(wtables);

            let rtables = zread!(tables.tables);
            let matches_query_routes = compute_matches_query_routes_(&rtables, &res);
            drop(rtables);

            let wtables = zwrite!(tables.tables);
            for (mut res, query_routes) in matches_query_routes {
                get_mut_unchecked(&mut res)
                    .context_mut()
                    .update_query_routes(query_routes);
            }
            drop(wtables);
        }
        None => log::error!("Declare router queryable for unknown scope {}!", expr.scope),
    }
}

fn register_peer_queryable(
    tables: &mut Tables,
    face: Option<&mut Arc<FaceState>>,
    res: &mut Arc<Resource>,
    qabl_info: &QueryableInfo,
    peer: ZenohId,
) {
    let current_info = res_hat!(res).peer_qabls.get(&peer);
    if current_info.is_none() || current_info.unwrap() != qabl_info {
        // Register peer queryable
        {
            log::debug!("Register peer queryable {} (peer: {})", res.expr(), peer,);
            res_hat_mut!(res).peer_qabls.insert(peer, *qabl_info);
            hat_mut!(tables).peer_qabls.insert(res.clone());
        }

        // Propagate queryable to peers
        propagate_sourced_queryable(tables, res, qabl_info, face, &peer, WhatAmI::Peer);
    }
}

fn declare_peer_queryable(
    tables: &TablesLock,
    rtables: RwLockReadGuard<Tables>,
    face: &mut Arc<FaceState>,
    expr: &WireExpr,
    qabl_info: &QueryableInfo,
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
                    log::debug!("Register peer queryable {}", fullexpr);
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
            let mut face = Some(face);
            register_peer_queryable(&mut wtables, face.as_deref_mut(), &mut res, qabl_info, peer);
            let local_info = local_router_qabl_info(&wtables, &res);
            let zid = wtables.zid;
            register_router_queryable(&mut wtables, face, &mut res, &local_info, zid);
            disable_matches_query_routes(&mut wtables, &mut res);
            drop(wtables);

            let rtables = zread!(tables.tables);
            let matches_query_routes = compute_matches_query_routes_(&rtables, &res);
            drop(rtables);

            let wtables = zwrite!(tables.tables);
            for (mut res, query_routes) in matches_query_routes {
                get_mut_unchecked(&mut res)
                    .context_mut()
                    .update_query_routes(query_routes);
            }
            drop(wtables);
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
        .qabl = Some(*qabl_info);
    }
    face_hat_mut!(face).remote_qabls.insert(res.clone());
}

fn declare_client_queryable(
    tables: &TablesLock,
    rtables: RwLockReadGuard<Tables>,
    face: &mut Arc<FaceState>,
    expr: &WireExpr,
    qabl_info: &QueryableInfo,
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
                    log::debug!("Register client queryable {}", fullexpr);
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

            register_client_queryable(&mut wtables, face, &mut res, qabl_info);
            let local_details = local_router_qabl_info(&wtables, &res);
            let zid = wtables.zid;
            register_router_queryable(&mut wtables, Some(face), &mut res, &local_details, zid);
            disable_matches_query_routes(&mut wtables, &mut res);
            drop(wtables);

            let rtables = zread!(tables.tables);
            let matches_query_routes = compute_matches_query_routes_(&rtables, &res);
            drop(rtables);

            let wtables = zwrite!(tables.tables);
            for (mut res, query_routes) in matches_query_routes {
                get_mut_unchecked(&mut res)
                    .context_mut()
                    .update_query_routes(query_routes);
            }
            drop(wtables);
        }
        None => log::error!("Declare queryable for unknown scope {}!", expr.scope),
    }
}

#[inline]
fn remote_router_qabls(tables: &Tables, res: &Arc<Resource>) -> bool {
    res.context.is_some()
        && res_hat!(res)
            .router_qabls
            .keys()
            .any(|router| router != &tables.zid)
}

#[inline]
fn remote_peer_qabls(tables: &Tables, res: &Arc<Resource>) -> bool {
    res.context.is_some()
        && res_hat!(res)
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
    routing_context: NodeId,
) {
    for child in childs {
        if net.graph.contains_node(*child) {
            match tables.get_face(&net.graph[*child].zid).cloned() {
                Some(mut someface) => {
                    if src_face.is_none() || someface.id != src_face.unwrap().id {
                        let wire_expr = Resource::decl_key(res, &mut someface);

                        log::debug!("Send forget queryable {}  on {}", res.expr(), someface);

                        someface.primitives.send_declare(RoutingContext::with_expr(
                            Declare {
                                ext_qos: ext::QoSType::declare_default(),
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType {
                                    node_id: routing_context,
                                },
                                body: DeclareBody::UndeclareQueryable(UndeclareQueryable {
                                    id: 0, // TODO
                                    ext_wire_expr: WireExprType { wire_expr },
                                }),
                            },
                            res.expr(),
                        ));
                    }
                }
                None => log::trace!("Unable to find face for zid {}", net.graph[*child].zid),
            }
        }
    }
}

fn propagate_forget_simple_queryable(tables: &mut Tables, res: &mut Arc<Resource>) {
    for face in tables.faces.values_mut() {
        if face_hat!(face).local_qabls.contains_key(res) {
            let wire_expr = Resource::get_best_key(res, "", face.id);
            face.primitives.send_declare(RoutingContext::with_expr(
                Declare {
                    ext_qos: ext::QoSType::declare_default(),
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::default(),
                    body: DeclareBody::UndeclareQueryable(UndeclareQueryable {
                        id: 0, // TODO
                        ext_wire_expr: WireExprType { wire_expr },
                    }),
                },
                res.expr(),
            ));

            face_hat_mut!(face).local_qabls.remove(res);
        }
    }
}

fn propagate_forget_simple_queryable_to_peers(tables: &mut Tables, res: &mut Arc<Resource>) {
    if !hat!(tables).full_net(WhatAmI::Peer)
        && res_hat!(res).router_qabls.len() == 1
        && res_hat!(res).router_qabls.contains_key(&tables.zid)
    {
        for mut face in tables
            .faces
            .values()
            .cloned()
            .collect::<Vec<Arc<FaceState>>>()
        {
            if face.whatami == WhatAmI::Peer
                && face_hat!(face).local_qabls.contains_key(res)
                && !res.session_ctxs.values().any(|s| {
                    face.zid != s.face.zid
                        && s.qabl.is_some()
                        && (s.face.whatami == WhatAmI::Client
                            || (s.face.whatami == WhatAmI::Peer
                                && hat!(tables).failover_brokering(s.face.zid, face.zid)))
                })
            {
                let wire_expr = Resource::get_best_key(res, "", face.id);
                face.primitives.send_declare(RoutingContext::with_expr(
                    Declare {
                        ext_qos: ext::QoSType::declare_default(),
                        ext_tstamp: None,
                        ext_nodeid: ext::NodeIdType::default(),
                        body: DeclareBody::UndeclareQueryable(UndeclareQueryable {
                            id: 0, // TODO
                            ext_wire_expr: WireExprType { wire_expr },
                        }),
                    },
                    res.expr(),
                ));

                face_hat_mut!(&mut face).local_qabls.remove(res);
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
    let net = hat!(tables).get_net(net_type).unwrap();
    match net.get_idx(source) {
        Some(tree_sid) => {
            if net.trees.len() > tree_sid.index() {
                send_forget_sourced_queryable_to_net_childs(
                    tables,
                    net,
                    &net.trees[tree_sid.index()].childs,
                    res,
                    src_face,
                    tree_sid.index() as NodeId,
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
    res_hat_mut!(res).router_qabls.remove(router);

    if res_hat!(res).router_qabls.is_empty() {
        hat_mut!(tables)
            .router_qabls
            .retain(|qabl| !Arc::ptr_eq(qabl, res));

        if hat!(tables).full_net(WhatAmI::Peer) {
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
    if res_hat!(res).router_qabls.contains_key(router) {
        unregister_router_queryable(tables, res, router);
        propagate_forget_sourced_queryable(tables, res, face, router, WhatAmI::Router);
    }
}

fn forget_router_queryable(
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
                undeclare_router_queryable(&mut wtables, Some(face), &mut res, router);
                disable_matches_query_routes(&mut wtables, &mut res);
                drop(wtables);

                let rtables = zread!(tables.tables);
                let matches_query_routes = compute_matches_query_routes_(&rtables, &res);
                drop(rtables);

                let wtables = zwrite!(tables.tables);
                for (mut res, query_routes) in matches_query_routes {
                    get_mut_unchecked(&mut res)
                        .context_mut()
                        .update_query_routes(query_routes);
                }
                Resource::clean(&mut res);
                drop(wtables);
            }
            None => log::error!("Undeclare unknown router queryable!"),
        },
        None => log::error!("Undeclare router queryable with unknown scope!"),
    }
}

fn unregister_peer_queryable(tables: &mut Tables, res: &mut Arc<Resource>, peer: &ZenohId) {
    log::debug!("Unregister peer queryable {} (peer: {})", res.expr(), peer,);
    res_hat_mut!(res).peer_qabls.remove(peer);

    if res_hat!(res).peer_qabls.is_empty() {
        hat_mut!(tables)
            .peer_qabls
            .retain(|qabl| !Arc::ptr_eq(qabl, res));
    }
}

fn undeclare_peer_queryable(
    tables: &mut Tables,
    face: Option<&Arc<FaceState>>,
    res: &mut Arc<Resource>,
    peer: &ZenohId,
) {
    if res_hat!(res).peer_qabls.contains_key(peer) {
        unregister_peer_queryable(tables, res, peer);
        propagate_forget_sourced_queryable(tables, res, face, peer, WhatAmI::Peer);
    }
}

fn forget_peer_queryable(
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
                undeclare_peer_queryable(&mut wtables, Some(face), &mut res, peer);

                let client_qabls = res.session_ctxs.values().any(|ctx| ctx.qabl.is_some());
                let peer_qabls = remote_peer_qabls(&wtables, &res);
                let zid = wtables.zid;
                if !client_qabls && !peer_qabls {
                    undeclare_router_queryable(&mut wtables, None, &mut res, &zid);
                } else {
                    let local_info = local_router_qabl_info(&wtables, &res);
                    register_router_queryable(&mut wtables, None, &mut res, &local_info, zid);
                }
                drop(wtables);

                let rtables = zread!(tables.tables);
                let matches_query_routes = compute_matches_query_routes_(&rtables, &res);
                drop(rtables);

                let wtables = zwrite!(tables.tables);
                for (mut res, query_routes) in matches_query_routes {
                    get_mut_unchecked(&mut res)
                        .context_mut()
                        .update_query_routes(query_routes);
                }
                Resource::clean(&mut res);
                drop(wtables);
            }
            None => log::error!("Undeclare unknown peer queryable!"),
        },
        None => log::error!("Undeclare peer queryable with unknown scope!"),
    }
}

pub(super) fn undeclare_client_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
) {
    log::debug!("Unregister client queryable {} for {}", res.expr(), face);
    if let Some(ctx) = get_mut_unchecked(res).session_ctxs.get_mut(&face.id) {
        get_mut_unchecked(ctx).qabl = None;
        if ctx.qabl.is_none() {
            face_hat_mut!(face).remote_qabls.remove(res);
        }
    }

    let mut client_qabls = client_qabls(res);
    let router_qabls = remote_router_qabls(tables, res);
    let peer_qabls = remote_peer_qabls(tables, res);

    if client_qabls.is_empty() && !peer_qabls {
        undeclare_router_queryable(tables, None, res, &tables.zid.clone());
    } else {
        let local_info = local_router_qabl_info(tables, res);
        register_router_queryable(tables, None, res, &local_info, tables.zid);
        propagate_forget_simple_queryable_to_peers(tables, res);
    }

    if client_qabls.len() == 1 && !router_qabls && !peer_qabls {
        let face = &mut client_qabls[0];
        if face_hat!(face).local_qabls.contains_key(res) {
            let wire_expr = Resource::get_best_key(res, "", face.id);
            face.primitives.send_declare(RoutingContext::with_expr(
                Declare {
                    ext_qos: ext::QoSType::declare_default(),
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::default(),
                    body: DeclareBody::UndeclareQueryable(UndeclareQueryable {
                        id: 0, // TODO
                        ext_wire_expr: WireExprType { wire_expr },
                    }),
                },
                res.expr(),
            ));

            face_hat_mut!(face).local_qabls.remove(res);
        }
    }
}

fn forget_client_queryable(
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
                undeclare_client_queryable(&mut wtables, face, &mut res);
                disable_matches_query_routes(&mut wtables, &mut res);
                drop(wtables);

                let rtables = zread!(tables.tables);
                let matches_query_routes = compute_matches_query_routes_(&rtables, &res);
                drop(rtables);

                let wtables = zwrite!(tables.tables);
                for (mut res, query_routes) in matches_query_routes {
                    get_mut_unchecked(&mut res)
                        .context_mut()
                        .update_query_routes(query_routes);
                }
                Resource::clean(&mut res);
                drop(wtables);
            }
            None => log::error!("Undeclare unknown queryable!"),
        },
        None => log::error!("Undeclare queryable with unknown scope!"),
    }
}

pub(super) fn queries_new_face(tables: &mut Tables, face: &mut Arc<FaceState>) {
    if face.whatami == WhatAmI::Client {
        for qabl in hat!(tables).router_qabls.iter() {
            if qabl.context.is_some() {
                let info = local_qabl_info(tables, qabl, face);
                face_hat_mut!(face).local_qabls.insert(qabl.clone(), info);
                let key_expr = Resource::decl_key(qabl, face);
                face.primitives.send_declare(RoutingContext::with_expr(
                    Declare {
                        ext_qos: ext::QoSType::declare_default(),
                        ext_tstamp: None,
                        ext_nodeid: ext::NodeIdType::default(),
                        body: DeclareBody::DeclareQueryable(DeclareQueryable {
                            id: 0, // TODO
                            wire_expr: key_expr,
                            ext_info: info,
                        }),
                    },
                    qabl.expr(),
                ));
            }
        }
    } else if face.whatami == WhatAmI::Peer && !hat!(tables).full_net(WhatAmI::Peer) {
        for qabl in hat!(tables).router_qabls.iter() {
            if qabl.context.is_some()
                && (res_hat!(qabl).router_qabls.keys().any(|r| *r != tables.zid)
                    || qabl.session_ctxs.values().any(|s| {
                        s.qabl.is_some()
                            && (s.face.whatami == WhatAmI::Client
                                || (s.face.whatami == WhatAmI::Peer
                                    && hat!(tables).failover_brokering(s.face.zid, face.zid)))
                    }))
            {
                let info = local_qabl_info(tables, qabl, face);
                face_hat_mut!(face).local_qabls.insert(qabl.clone(), info);
                let key_expr = Resource::decl_key(qabl, face);
                face.primitives.send_declare(RoutingContext::with_expr(
                    Declare {
                        ext_qos: ext::QoSType::declare_default(),
                        ext_tstamp: None,
                        ext_nodeid: ext::NodeIdType::default(),
                        body: DeclareBody::DeclareQueryable(DeclareQueryable {
                            id: 0, // TODO
                            wire_expr: key_expr,
                            ext_info: info,
                        }),
                    },
                    qabl.expr(),
                ));
            }
        }
    }
}

pub(super) fn queries_remove_node(tables: &mut Tables, node: &ZenohId, net_type: WhatAmI) {
    match net_type {
        WhatAmI::Router => {
            let mut qabls = vec![];
            for res in hat!(tables).router_qabls.iter() {
                for qabl in res_hat!(res).router_qabls.keys() {
                    if qabl == node {
                        qabls.push(res.clone());
                    }
                }
            }
            for mut res in qabls {
                unregister_router_queryable(tables, &mut res, node);

                let matches_query_routes = compute_matches_query_routes_(tables, &res);
                for (mut res, query_routes) in matches_query_routes {
                    get_mut_unchecked(&mut res)
                        .context_mut()
                        .update_query_routes(query_routes);
                }
                Resource::clean(&mut res);
            }
        }
        WhatAmI::Peer => {
            let mut qabls = vec![];
            for res in hat!(tables).router_qabls.iter() {
                for qabl in res_hat!(res).router_qabls.keys() {
                    if qabl == node {
                        qabls.push(res.clone());
                    }
                }
            }
            for mut res in qabls {
                unregister_peer_queryable(tables, &mut res, node);

                let client_qabls = res.session_ctxs.values().any(|ctx| ctx.qabl.is_some());
                let peer_qabls = remote_peer_qabls(tables, &res);
                if !client_qabls && !peer_qabls {
                    undeclare_router_queryable(tables, None, &mut res, &tables.zid.clone());
                } else {
                    let local_info = local_router_qabl_info(tables, &res);
                    register_router_queryable(tables, None, &mut res, &local_info, tables.zid);
                }

                let matches_query_routes = compute_matches_query_routes_(tables, &res);
                for (mut res, query_routes) in matches_query_routes {
                    get_mut_unchecked(&mut res)
                        .context_mut()
                        .update_query_routes(query_routes);
                }
                Resource::clean(&mut res)
            }
        }
        _ => (),
    }
}

pub(super) fn queries_linkstate_change(tables: &mut Tables, zid: &ZenohId, links: &[ZenohId]) {
    if let Some(src_face) = tables.get_face(zid) {
        if hat!(tables).router_peers_failover_brokering && src_face.whatami == WhatAmI::Peer {
            for res in &face_hat!(src_face).remote_qabls {
                let client_qabls = res
                    .session_ctxs
                    .values()
                    .any(|ctx| ctx.face.whatami == WhatAmI::Client && ctx.qabl.is_some());
                if !remote_router_qabls(tables, res) && !client_qabls {
                    for ctx in get_mut_unchecked(&mut res.clone())
                        .session_ctxs
                        .values_mut()
                    {
                        let dst_face = &mut get_mut_unchecked(ctx).face;
                        if dst_face.whatami == WhatAmI::Peer && src_face.zid != dst_face.zid {
                            if face_hat!(dst_face).local_qabls.contains_key(res) {
                                let forget = !HatTables::failover_brokering_to(links, dst_face.zid)
                                    && {
                                        let ctx_links = hat!(tables)
                                            .peers_net
                                            .as_ref()
                                            .map(|net| net.get_links(dst_face.zid))
                                            .unwrap_or_else(|| &[]);
                                        res.session_ctxs.values().any(|ctx2| {
                                            ctx2.face.whatami == WhatAmI::Peer
                                                && ctx2.qabl.is_some()
                                                && HatTables::failover_brokering_to(
                                                    ctx_links,
                                                    ctx2.face.zid,
                                                )
                                        })
                                    };
                                if forget {
                                    let wire_expr = Resource::get_best_key(res, "", dst_face.id);
                                    dst_face.primitives.send_declare(RoutingContext::with_expr(
                                        Declare {
                                            ext_qos: ext::QoSType::declare_default(),
                                            ext_tstamp: None,
                                            ext_nodeid: ext::NodeIdType::default(),
                                            body: DeclareBody::UndeclareQueryable(
                                                UndeclareQueryable {
                                                    id: 0, // TODO
                                                    ext_wire_expr: WireExprType { wire_expr },
                                                },
                                            ),
                                        },
                                        res.expr(),
                                    ));

                                    face_hat_mut!(dst_face).local_qabls.remove(res);
                                }
                            } else if HatTables::failover_brokering_to(links, ctx.face.zid) {
                                let dst_face = &mut get_mut_unchecked(ctx).face;
                                let info = local_qabl_info(tables, res, dst_face);
                                face_hat_mut!(dst_face)
                                    .local_qabls
                                    .insert(res.clone(), info);
                                let key_expr = Resource::decl_key(res, dst_face);
                                dst_face.primitives.send_declare(RoutingContext::with_expr(
                                    Declare {
                                        ext_qos: ext::QoSType::declare_default(),
                                        ext_tstamp: None,
                                        ext_nodeid: ext::NodeIdType::default(),
                                        body: DeclareBody::DeclareQueryable(DeclareQueryable {
                                            id: 0, // TODO
                                            wire_expr: key_expr,
                                            ext_info: info,
                                        }),
                                    },
                                    res.expr(),
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
}

pub(super) fn queries_tree_change(
    tables: &mut Tables,
    new_childs: &[Vec<NodeIndex>],
    net_type: WhatAmI,
) {
    // propagate qabls to new childs
    for (tree_sid, tree_childs) in new_childs.iter().enumerate() {
        if !tree_childs.is_empty() {
            let net = hat!(tables).get_net(net_type).unwrap();
            let tree_idx = NodeIndex::new(tree_sid);
            if net.graph.contains_node(tree_idx) {
                let tree_id = net.graph[tree_idx].zid;

                let qabls_res = match net_type {
                    WhatAmI::Router => &hat!(tables).router_qabls,
                    _ => &hat!(tables).peer_qabls,
                };

                for res in qabls_res {
                    let qabls = match net_type {
                        WhatAmI::Router => &res_hat!(res).router_qabls,
                        _ => &res_hat!(res).peer_qabls,
                    };
                    if let Some(qabl_info) = qabls.get(&tree_id) {
                        send_sourced_queryable_to_net_childs(
                            tables,
                            net,
                            tree_childs,
                            res,
                            qabl_info,
                            None,
                            tree_sid as NodeId,
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
    expr: &mut RoutingExpr,
    tables: &Tables,
    net: &Network,
    source: NodeId,
    qabls: &HashMap<ZenohId, QueryableInfo>,
    complete: bool,
) {
    if net.trees.len() > source as usize {
        for (qabl, qabl_info) in qabls {
            if let Some(qabl_idx) = net.get_idx(qabl) {
                if net.trees[source as usize].directions.len() > qabl_idx.index() {
                    if let Some(direction) = net.trees[source as usize].directions[qabl_idx.index()]
                    {
                        if net.graph.contains_node(direction) {
                            if let Some(face) = tables.get_face(&net.graph[direction].zid) {
                                if net.distances.len() > qabl_idx.index() {
                                    let key_expr =
                                        Resource::get_best_key(expr.prefix, expr.suffix, face.id);
                                    route.push(QueryTargetQabl {
                                        direction: (face.clone(), key_expr.to_owned(), source),
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

impl HatQueriesTrait for HatCode {
    fn declare_queryable(
        &self,
        tables: &TablesLock,
        face: &mut Arc<FaceState>,
        expr: &WireExpr,
        qabl_info: &QueryableInfo,
        node_id: NodeId,
    ) {
        let rtables = zread!(tables.tables);
        match face.whatami {
            WhatAmI::Router => {
                if let Some(router) = get_router(&rtables, face, node_id) {
                    declare_router_queryable(tables, rtables, face, expr, qabl_info, router)
                }
            }
            WhatAmI::Peer => {
                if hat!(rtables).full_net(WhatAmI::Peer) {
                    if let Some(peer) = get_peer(&rtables, face, node_id) {
                        declare_peer_queryable(tables, rtables, face, expr, qabl_info, peer)
                    }
                } else {
                    declare_client_queryable(tables, rtables, face, expr, qabl_info)
                }
            }
            _ => declare_client_queryable(tables, rtables, face, expr, qabl_info),
        }
    }

    fn forget_queryable(
        &self,
        tables: &TablesLock,
        face: &mut Arc<FaceState>,
        expr: &WireExpr,
        node_id: NodeId,
    ) {
        let rtables = zread!(tables.tables);
        match face.whatami {
            WhatAmI::Router => {
                if let Some(router) = get_router(&rtables, face, node_id) {
                    forget_router_queryable(tables, rtables, face, expr, &router)
                }
            }
            WhatAmI::Peer => {
                if hat!(rtables).full_net(WhatAmI::Peer) {
                    if let Some(peer) = get_peer(&rtables, face, node_id) {
                        forget_peer_queryable(tables, rtables, face, expr, &peer)
                    }
                } else {
                    forget_client_queryable(tables, rtables, face, expr)
                }
            }
            _ => forget_client_queryable(tables, rtables, face, expr),
        }
    }

    fn compute_query_route(
        &self,
        tables: &Tables,
        expr: &mut RoutingExpr,
        source: NodeId,
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

        let master = !hat!(tables).full_net(WhatAmI::Peer)
            || *hat!(tables).elect_router(&tables.zid, &key_expr, hat!(tables).shared_nodes.iter())
                == tables.zid;

        for mres in matches.iter() {
            let mres = mres.upgrade().unwrap();
            let complete = DEFAULT_INCLUDER.includes(mres.expr().as_bytes(), key_expr.as_bytes());
            if master || source_type == WhatAmI::Router {
                let net = hat!(tables).routers_net.as_ref().unwrap();
                let router_source = match source_type {
                    WhatAmI::Router => source,
                    _ => net.idx.index() as NodeId,
                };
                insert_target_for_qabls(
                    &mut route,
                    expr,
                    tables,
                    net,
                    router_source,
                    &res_hat!(mres).router_qabls,
                    complete,
                );
            }

            if (master || source_type != WhatAmI::Router) && hat!(tables).full_net(WhatAmI::Peer) {
                let net = hat!(tables).peers_net.as_ref().unwrap();
                let peer_source = match source_type {
                    WhatAmI::Peer => source,
                    _ => net.idx.index() as NodeId,
                };
                insert_target_for_qabls(
                    &mut route,
                    expr,
                    tables,
                    net,
                    peer_source,
                    &res_hat!(mres).peer_qabls,
                    complete,
                );
            }

            if master || source_type == WhatAmI::Router {
                for (sid, context) in &mres.session_ctxs {
                    if context.face.whatami != WhatAmI::Router {
                        let key_expr = Resource::get_best_key(expr.prefix, expr.suffix, *sid);
                        if let Some(qabl_info) = context.qabl.as_ref() {
                            route.push(QueryTargetQabl {
                                direction: (
                                    context.face.clone(),
                                    key_expr.to_owned(),
                                    NodeId::default(),
                                ),
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

    #[inline]
    fn compute_local_replies(
        &self,
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
                        && (!res_hat!(mres).router_subs.is_empty()
                            || !res_hat!(mres).peer_subs.is_empty()))
                        || mres.session_ctxs.values().any(|ctx| ctx.subs.is_some())
                    {
                        result.push((Resource::get_best_key(&mres, "", face.id), ZBuf::default()));
                    }
                }
            }
        }
        result
    }

    fn compute_query_routes_(&self, tables: &Tables, res: &Arc<Resource>) -> QueryRoutes {
        let mut routes = QueryRoutes {
            routers_query_routes: vec![],
            peers_query_routes: vec![],
            peer_query_route: None,
            client_query_route: None,
        };
        let mut expr = RoutingExpr::new(res, "");

        let indexes = hat!(tables)
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
                self.compute_query_route(tables, &mut expr, idx.index() as NodeId, WhatAmI::Router);
        }

        routes.peer_query_route =
            Some(self.compute_query_route(tables, &mut expr, NodeId::default(), WhatAmI::Peer));

        if hat!(tables).full_net(WhatAmI::Peer) {
            let indexes = hat!(tables)
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
                routes.peers_query_routes[idx.index()] = self.compute_query_route(
                    tables,
                    &mut expr,
                    idx.index() as NodeId,
                    WhatAmI::Peer,
                );
            }
        }
        routes
    }

    fn compute_query_routes(&self, tables: &mut Tables, res: &mut Arc<Resource>) {
        if res.context.is_some() {
            let mut res_mut = res.clone();
            let res_mut = get_mut_unchecked(&mut res_mut);
            let mut expr = RoutingExpr::new(res, "");

            let indexes = hat!(tables)
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
                routers_query_routes[idx.index()] = self.compute_query_route(
                    tables,
                    &mut expr,
                    idx.index() as NodeId,
                    WhatAmI::Router,
                );
            }

            res_mut.context_mut().peer_query_route =
                Some(self.compute_query_route(tables, &mut expr, NodeId::default(), WhatAmI::Peer));

            if hat!(tables).full_net(WhatAmI::Peer) {
                let indexes = hat!(tables)
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
                    peers_query_routes[idx.index()] = self.compute_query_route(
                        tables,
                        &mut expr,
                        idx.index() as NodeId,
                        WhatAmI::Peer,
                    );
                }
            }
        }
    }
}
