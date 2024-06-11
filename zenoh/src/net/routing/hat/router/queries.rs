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
    collections::HashMap,
    sync::{atomic::Ordering, Arc},
};

use ordered_float::OrderedFloat;
use petgraph::graph::NodeIndex;
use zenoh_buffers::ZBuf;
use zenoh_protocol::{
    core::{
        key_expr::{
            include::{Includer, DEFAULT_INCLUDER},
            OwnedKeyExpr,
        },
        WhatAmI, WireExpr, ZenohIdInner,
    },
    network::{
        declare::{
            common::ext::WireExprType, ext, queryable::ext::QueryableInfoType, Declare,
            DeclareBody, DeclareQueryable, QueryableId, UndeclareQueryable,
        },
        interest::{InterestId, InterestMode},
    },
};
use zenoh_sync::get_mut_unchecked;

use super::{
    face_hat, face_hat_mut, get_peer, get_router, get_routes_entries, hat, hat_mut,
    network::Network, res_hat, res_hat_mut, HatCode, HatContext, HatFace, HatTables,
};
use crate::net::routing::{
    dispatcher::{
        face::FaceState,
        queries::*,
        resource::{NodeId, Resource, SessionContext},
        tables::{QueryTargetQabl, QueryTargetQablSet, RoutingExpr, Tables},
    },
    hat::{CurrentFutureTrait, HatQueriesTrait, Sources},
    router::RoutesIndexes,
    RoutingContext, PREFIX_LIVELINESS,
};

#[inline]
fn merge_qabl_infos(mut this: QueryableInfoType, info: &QueryableInfoType) -> QueryableInfoType {
    this.complete = this.complete || info.complete;
    this.distance = std::cmp::min(this.distance, info.distance);
    this
}

fn local_router_qabl_info(tables: &Tables, res: &Arc<Resource>) -> QueryableInfoType {
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
        .unwrap_or(QueryableInfoType::DEFAULT)
}

fn local_peer_qabl_info(tables: &Tables, res: &Arc<Resource>) -> QueryableInfoType {
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
        .unwrap_or(QueryableInfoType::DEFAULT)
}

fn local_qabl_info(
    tables: &Tables,
    res: &Arc<Resource>,
    face: &Arc<FaceState>,
) -> QueryableInfoType {
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
        .unwrap_or(QueryableInfoType::DEFAULT)
}

#[inline]
fn send_sourced_queryable_to_net_childs(
    tables: &Tables,
    net: &Network,
    childs: &[NodeIndex],
    res: &Arc<Resource>,
    qabl_info: &QueryableInfoType,
    src_face: Option<&mut Arc<FaceState>>,
    routing_context: NodeId,
) {
    for child in childs {
        if net.graph.contains_node(*child) {
            match tables.get_face(&net.graph[*child].zid).cloned() {
                Some(mut someface) => {
                    if src_face.is_none() || someface.id != src_face.as_ref().unwrap().id {
                        let key_expr = Resource::decl_key(res, &mut someface);

                        someface.primitives.send_declare(RoutingContext::with_expr(
                            Declare {
                                interest_id: None,
                                ext_qos: ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType {
                                    node_id: routing_context,
                                },
                                body: DeclareBody::DeclareQueryable(DeclareQueryable {
                                    id: 0, // Sourced queryables do not use ids
                                    wire_expr: key_expr,
                                    ext_info: *qabl_info,
                                }),
                            },
                            res.expr(),
                        ));
                    }
                }
                None => tracing::trace!("Unable to find face for zid {}", net.graph[*child].zid),
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
        let current = face_hat!(dst_face).local_qabls.get(res);
        if (src_face.is_none() || src_face.as_ref().unwrap().id != dst_face.id)
            && (current.is_none() || current.unwrap().1 != info)
            && face_hat!(dst_face)
                .remote_qabl_interests
                .values()
                .any(|si| si.as_ref().map(|si| si.matches(res)).unwrap_or(true))
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
            let id = current
                .map(|c| c.0)
                .unwrap_or(face_hat!(dst_face).next_id.fetch_add(1, Ordering::SeqCst));
            face_hat_mut!(&mut dst_face)
                .local_qabls
                .insert(res.clone(), (id, info));
            let key_expr = Resource::decl_key(res, &mut dst_face);
            dst_face.primitives.send_declare(RoutingContext::with_expr(
                Declare {
                    interest_id: None,
                    ext_qos: ext::QoSType::DECLARE,
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::DEFAULT,
                    body: DeclareBody::DeclareQueryable(DeclareQueryable {
                        id,
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
    qabl_info: &QueryableInfoType,
    src_face: Option<&mut Arc<FaceState>>,
    source: &ZenohIdInner,
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
                tracing::trace!(
                    "Propagating qabl {}: tree for node {} sid:{} not yet ready",
                    res.expr(),
                    tree_sid.index(),
                    source
                );
            }
        }
        None => tracing::error!(
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
    qabl_info: &QueryableInfoType,
    router: ZenohIdInner,
) {
    let current_info = res_hat!(res).router_qabls.get(&router);
    if current_info.is_none() || current_info.unwrap() != qabl_info {
        // Register router queryable
        {
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
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    qabl_info: &QueryableInfoType,
    router: ZenohIdInner,
) {
    register_router_queryable(tables, Some(face), res, qabl_info, router);
}

fn register_peer_queryable(
    tables: &mut Tables,
    face: Option<&mut Arc<FaceState>>,
    res: &mut Arc<Resource>,
    qabl_info: &QueryableInfoType,
    peer: ZenohIdInner,
) {
    let current_info = res_hat!(res).peer_qabls.get(&peer);
    if current_info.is_none() || current_info.unwrap() != qabl_info {
        // Register peer queryable
        {
            res_hat_mut!(res).peer_qabls.insert(peer, *qabl_info);
            hat_mut!(tables).peer_qabls.insert(res.clone());
        }

        // Propagate queryable to peers
        propagate_sourced_queryable(tables, res, qabl_info, face, &peer, WhatAmI::Peer);
    }
}

fn declare_peer_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    qabl_info: &QueryableInfoType,
    peer: ZenohIdInner,
) {
    let mut face = Some(face);
    register_peer_queryable(tables, face.as_deref_mut(), res, qabl_info, peer);
    let local_info = local_router_qabl_info(tables, res);
    let zid = tables.zid;
    register_router_queryable(tables, face, res, &local_info, zid);
}

fn register_client_queryable(
    _tables: &mut Tables,
    face: &mut Arc<FaceState>,
    id: QueryableId,
    res: &mut Arc<Resource>,
    qabl_info: &QueryableInfoType,
) {
    // Register queryable
    {
        let res = get_mut_unchecked(res);
        get_mut_unchecked(
            res.session_ctxs
                .entry(face.id)
                .or_insert_with(|| Arc::new(SessionContext::new(face.clone()))),
        )
        .qabl = Some(*qabl_info);
    }
    face_hat_mut!(face).remote_qabls.insert(id, res.clone());
}

fn declare_client_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    id: QueryableId,
    res: &mut Arc<Resource>,
    qabl_info: &QueryableInfoType,
) {
    register_client_queryable(tables, face, id, res, qabl_info);
    let local_details = local_router_qabl_info(tables, res);
    let zid = tables.zid;
    register_router_queryable(tables, Some(face), res, &local_details, zid);
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
fn remote_client_qabls(res: &Arc<Resource>, face: &Arc<FaceState>) -> bool {
    res.session_ctxs
        .values()
        .any(|ctx| ctx.face.id != face.id && ctx.qabl.is_some())
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

                        someface.primitives.send_declare(RoutingContext::with_expr(
                            Declare {
                                interest_id: None,
                                ext_qos: ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType {
                                    node_id: routing_context,
                                },
                                body: DeclareBody::UndeclareQueryable(UndeclareQueryable {
                                    id: 0, // Sourced queryables do not use ids
                                    ext_wire_expr: WireExprType { wire_expr },
                                }),
                            },
                            res.expr(),
                        ));
                    }
                }
                None => tracing::trace!("Unable to find face for zid {}", net.graph[*child].zid),
            }
        }
    }
}

fn propagate_forget_simple_queryable(tables: &mut Tables, res: &mut Arc<Resource>) {
    for mut face in tables.faces.values().cloned() {
        if let Some((id, _)) = face_hat_mut!(&mut face).local_qabls.remove(res) {
            face.primitives.send_declare(RoutingContext::with_expr(
                Declare {
                    interest_id: None,
                    ext_qos: ext::QoSType::DECLARE,
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::DEFAULT,
                    body: DeclareBody::UndeclareQueryable(UndeclareQueryable {
                        id,
                        ext_wire_expr: WireExprType::null(),
                    }),
                },
                res.expr(),
            ));
        }
        for res in face_hat!(&mut face)
            .local_qabls
            .keys()
            .cloned()
            .collect::<Vec<Arc<Resource>>>()
        {
            if !res.context().matches.iter().any(|m| {
                m.upgrade().is_some_and(|m| {
                    m.context.is_some()
                        && (remote_client_qabls(&m, &face)
                            || remote_peer_qabls(tables, &m)
                            || remote_router_qabls(tables, &m))
                })
            }) {
                if let Some((id, _)) = face_hat_mut!(&mut face).local_qabls.remove(&res) {
                    face.primitives.send_declare(RoutingContext::with_expr(
                        Declare {
                            interest_id: None,
                            ext_qos: ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType::DEFAULT,
                            body: DeclareBody::UndeclareQueryable(UndeclareQueryable {
                                id,
                                ext_wire_expr: WireExprType::null(),
                            }),
                        },
                        res.expr(),
                    ));
                }
            }
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
                if let Some((id, _)) = face_hat_mut!(&mut face).local_qabls.remove(res) {
                    face.primitives.send_declare(RoutingContext::with_expr(
                        Declare {
                            interest_id: None,
                            ext_qos: ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType::DEFAULT,
                            body: DeclareBody::UndeclareQueryable(UndeclareQueryable {
                                id,
                                ext_wire_expr: WireExprType::null(),
                            }),
                        },
                        res.expr(),
                    ));
                }
            }
        }
    }
}

fn propagate_forget_sourced_queryable(
    tables: &mut Tables,
    res: &mut Arc<Resource>,
    src_face: Option<&Arc<FaceState>>,
    source: &ZenohIdInner,
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
                tracing::trace!(
                    "Propagating forget qabl {}: tree for node {} sid:{} not yet ready",
                    res.expr(),
                    tree_sid.index(),
                    source
                );
            }
        }
        None => tracing::error!(
            "Error propagating forget qabl {}: cannot get index of {}!",
            res.expr(),
            source
        ),
    }
}

fn unregister_router_queryable(tables: &mut Tables, res: &mut Arc<Resource>, router: &ZenohIdInner) {
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
    router: &ZenohIdInner,
) {
    if res_hat!(res).router_qabls.contains_key(router) {
        unregister_router_queryable(tables, res, router);
        propagate_forget_sourced_queryable(tables, res, face, router, WhatAmI::Router);
    }
}

fn forget_router_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    router: &ZenohIdInner,
) {
    undeclare_router_queryable(tables, Some(face), res, router);
}

fn unregister_peer_queryable(tables: &mut Tables, res: &mut Arc<Resource>, peer: &ZenohIdInner) {
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
    peer: &ZenohIdInner,
) {
    if res_hat!(res).peer_qabls.contains_key(peer) {
        unregister_peer_queryable(tables, res, peer);
        propagate_forget_sourced_queryable(tables, res, face, peer, WhatAmI::Peer);
    }
}

fn forget_peer_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    peer: &ZenohIdInner,
) {
    undeclare_peer_queryable(tables, Some(face), res, peer);

    let client_qabls = res.session_ctxs.values().any(|ctx| ctx.qabl.is_some());
    let peer_qabls = remote_peer_qabls(tables, res);
    let zid = tables.zid;
    if !client_qabls && !peer_qabls {
        undeclare_router_queryable(tables, None, res, &zid);
    } else {
        let local_info = local_router_qabl_info(tables, res);
        register_router_queryable(tables, None, res, &local_info, zid);
    }
}

pub(super) fn undeclare_client_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
) {
    if !face_hat_mut!(face)
        .remote_qabls
        .values()
        .any(|s| *s == *res)
    {
        if let Some(ctx) = get_mut_unchecked(res).session_ctxs.get_mut(&face.id) {
            get_mut_unchecked(ctx).qabl = None;
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
            let mut face = &mut client_qabls[0];
            if let Some((id, _)) = face_hat_mut!(face).local_qabls.remove(res) {
                face.primitives.send_declare(RoutingContext::with_expr(
                    Declare {
                        interest_id: None,
                        ext_qos: ext::QoSType::DECLARE,
                        ext_tstamp: None,
                        ext_nodeid: ext::NodeIdType::DEFAULT,
                        body: DeclareBody::UndeclareQueryable(UndeclareQueryable {
                            id,
                            ext_wire_expr: WireExprType::null(),
                        }),
                    },
                    res.expr(),
                ));
            }
            for res in face_hat!(face)
                .local_qabls
                .keys()
                .cloned()
                .collect::<Vec<Arc<Resource>>>()
            {
                if !res.context().matches.iter().any(|m| {
                    m.upgrade().is_some_and(|m| {
                        m.context.is_some()
                            && (remote_client_qabls(&m, face)
                                || remote_peer_qabls(tables, &m)
                                || remote_router_qabls(tables, &m))
                    })
                }) {
                    if let Some((id, _)) = face_hat_mut!(&mut face).local_qabls.remove(&res) {
                        face.primitives.send_declare(RoutingContext::with_expr(
                            Declare {
                                interest_id: None,
                                ext_qos: ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType::DEFAULT,
                                body: DeclareBody::UndeclareQueryable(UndeclareQueryable {
                                    id,
                                    ext_wire_expr: WireExprType::null(),
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

fn forget_client_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    id: QueryableId,
) -> Option<Arc<Resource>> {
    if let Some(mut res) = face_hat_mut!(face).remote_qabls.remove(&id) {
        undeclare_client_queryable(tables, face, &mut res);
        Some(res)
    } else {
        None
    }
}

pub(super) fn queries_remove_node(tables: &mut Tables, node: &ZenohIdInner, net_type: WhatAmI) {
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

                update_matches_query_routes(tables, &res);
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

                update_matches_query_routes(tables, &res);
                Resource::clean(&mut res)
            }
        }
        _ => (),
    }
}

pub(super) fn queries_linkstate_change(tables: &mut Tables, zid: &ZenohIdInner, links: &[ZenohIdInner]) {
    if let Some(src_face) = tables.get_face(zid) {
        if hat!(tables).router_peers_failover_brokering && src_face.whatami == WhatAmI::Peer {
            for res in face_hat!(src_face).remote_qabls.values() {
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
                            if let Some((id, _)) = face_hat!(dst_face).local_qabls.get(res).cloned()
                            {
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
                                    dst_face.primitives.send_declare(RoutingContext::with_expr(
                                        Declare {
                                            interest_id: None,
                                            ext_qos: ext::QoSType::DECLARE,
                                            ext_tstamp: None,
                                            ext_nodeid: ext::NodeIdType::DEFAULT,
                                            body: DeclareBody::UndeclareQueryable(
                                                UndeclareQueryable {
                                                    id,
                                                    ext_wire_expr: WireExprType::null(),
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
                                let id = face_hat!(dst_face).next_id.fetch_add(1, Ordering::SeqCst);
                                face_hat_mut!(dst_face)
                                    .local_qabls
                                    .insert(res.clone(), (id, info));
                                let key_expr = Resource::decl_key(res, dst_face);
                                dst_face.primitives.send_declare(RoutingContext::with_expr(
                                    Declare {
                                        interest_id: None,
                                        ext_qos: ext::QoSType::DECLARE,
                                        ext_tstamp: None,
                                        ext_nodeid: ext::NodeIdType::DEFAULT,
                                        body: DeclareBody::DeclareQueryable(DeclareQueryable {
                                            id,
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
    update_query_routes_from(tables, &mut tables.root_res.clone());
}

#[inline]
fn insert_target_for_qabls(
    route: &mut QueryTargetQablSet,
    expr: &mut RoutingExpr,
    tables: &Tables,
    net: &Network,
    source: NodeId,
    qabls: &HashMap<ZenohIdInner, QueryableInfoType>,
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
        tracing::trace!("Tree for node sid:{} not yet ready", source);
    }
}

lazy_static::lazy_static! {
    static ref EMPTY_ROUTE: Arc<QueryTargetQablSet> = Arc::new(Vec::new());
}

impl HatQueriesTrait for HatCode {
    fn declare_qabl_interest(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: InterestId,
        res: Option<&mut Arc<Resource>>,
        mode: InterestMode,
        mut aggregate: bool,
    ) {
        if aggregate && face.whatami == WhatAmI::Peer {
            tracing::warn!(
                "Received Interest with aggregate=true from peer {}. Not supported!",
                face.zid
            );
            aggregate = true;
        }
        if mode.current() && face.whatami == WhatAmI::Client {
            let interest_id = (!mode.future()).then_some(id);
            if let Some(res) = res.as_ref() {
                if aggregate {
                    if hat!(tables).router_qabls.iter().any(|qabl| {
                        qabl.context.is_some()
                            && qabl.matches(res)
                            && (res_hat!(qabl).router_qabls.keys().any(|r| *r != tables.zid)
                                || res_hat!(qabl).peer_qabls.keys().any(|r| *r != tables.zid)
                                || qabl.session_ctxs.values().any(|s| {
                                    s.face.id != face.id
                                        && s.qabl.is_some()
                                        && (s.face.whatami == WhatAmI::Client
                                            || face.whatami == WhatAmI::Client
                                            || (s.face.whatami == WhatAmI::Peer
                                                && hat!(tables)
                                                    .failover_brokering(s.face.zid, face.zid)))
                                }))
                    }) {
                        let info = local_qabl_info(tables, res, face);
                        let id = if mode.future() {
                            let id = face_hat!(face).next_id.fetch_add(1, Ordering::SeqCst);
                            face_hat_mut!(face)
                                .local_qabls
                                .insert((*res).clone(), (id, info));
                            id
                        } else {
                            0
                        };
                        let wire_expr = Resource::decl_key(res, face);
                        face.primitives.send_declare(RoutingContext::with_expr(
                            Declare {
                                interest_id,
                                ext_qos: ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType::DEFAULT,
                                body: DeclareBody::DeclareQueryable(DeclareQueryable {
                                    id,
                                    wire_expr,
                                    ext_info: info,
                                }),
                            },
                            res.expr(),
                        ));
                    }
                } else {
                    for qabl in hat!(tables).router_qabls.iter() {
                        if qabl.context.is_some()
                            && qabl.matches(res)
                            && (res_hat!(qabl).router_qabls.keys().any(|r| *r != tables.zid)
                                || res_hat!(qabl).peer_qabls.keys().any(|r| *r != tables.zid)
                                || qabl.session_ctxs.values().any(|s| {
                                    s.qabl.is_some()
                                        && (s.face.whatami != WhatAmI::Peer
                                            || face.whatami != WhatAmI::Peer
                                            || hat!(tables)
                                                .failover_brokering(s.face.zid, face.zid))
                                }))
                        {
                            let info = local_qabl_info(tables, qabl, face);
                            let id = if mode.future() {
                                let id = face_hat!(face).next_id.fetch_add(1, Ordering::SeqCst);
                                face_hat_mut!(face)
                                    .local_qabls
                                    .insert(qabl.clone(), (id, info));
                                id
                            } else {
                                0
                            };
                            let key_expr = Resource::decl_key(qabl, face);
                            face.primitives.send_declare(RoutingContext::with_expr(
                                Declare {
                                    interest_id,
                                    ext_qos: ext::QoSType::DECLARE,
                                    ext_tstamp: None,
                                    ext_nodeid: ext::NodeIdType::DEFAULT,
                                    body: DeclareBody::DeclareQueryable(DeclareQueryable {
                                        id,
                                        wire_expr: key_expr,
                                        ext_info: info,
                                    }),
                                },
                                qabl.expr(),
                            ));
                        }
                    }
                }
            } else {
                for qabl in hat!(tables).router_qabls.iter() {
                    if qabl.context.is_some()
                        && (remote_client_qabls(qabl, face)
                            || remote_peer_qabls(tables, qabl)
                            || remote_router_qabls(tables, qabl))
                    {
                        let info = local_qabl_info(tables, qabl, face);
                        let id = if mode.future() {
                            let id = face_hat!(face).next_id.fetch_add(1, Ordering::SeqCst);
                            face_hat_mut!(face)
                                .local_qabls
                                .insert(qabl.clone(), (id, info));
                            id
                        } else {
                            0
                        };
                        let key_expr = Resource::decl_key(qabl, face);
                        face.primitives.send_declare(RoutingContext::with_expr(
                            Declare {
                                interest_id,
                                ext_qos: ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType::DEFAULT,
                                body: DeclareBody::DeclareQueryable(DeclareQueryable {
                                    id,
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
        if mode.future() {
            face_hat_mut!(face)
                .remote_qabl_interests
                .insert(id, res.cloned());
        }
    }

    fn undeclare_qabl_interest(
        &self,
        _tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: InterestId,
    ) {
        face_hat_mut!(face).remote_qabl_interests.remove(&id);
    }

    fn declare_queryable(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: QueryableId,
        res: &mut Arc<Resource>,
        qabl_info: &QueryableInfoType,
        node_id: NodeId,
    ) {
        match face.whatami {
            WhatAmI::Router => {
                if let Some(router) = get_router(tables, face, node_id) {
                    declare_router_queryable(tables, face, res, qabl_info, router)
                }
            }
            WhatAmI::Peer => {
                if hat!(tables).full_net(WhatAmI::Peer) {
                    if let Some(peer) = get_peer(tables, face, node_id) {
                        declare_peer_queryable(tables, face, res, qabl_info, peer)
                    }
                } else {
                    declare_client_queryable(tables, face, id, res, qabl_info)
                }
            }
            _ => declare_client_queryable(tables, face, id, res, qabl_info),
        }
    }

    fn undeclare_queryable(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: QueryableId,
        res: Option<Arc<Resource>>,
        node_id: NodeId,
    ) -> Option<Arc<Resource>> {
        match face.whatami {
            WhatAmI::Router => {
                if let Some(mut res) = res {
                    if let Some(router) = get_router(tables, face, node_id) {
                        forget_router_queryable(tables, face, &mut res, &router);
                        Some(res)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            WhatAmI::Peer => {
                if hat!(tables).full_net(WhatAmI::Peer) {
                    if let Some(mut res) = res {
                        if let Some(peer) = get_peer(tables, face, node_id) {
                            forget_peer_queryable(tables, face, &mut res, &peer);
                            Some(res)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    forget_client_queryable(tables, face, id)
                }
            }
            _ => forget_client_queryable(tables, face, id),
        }
    }

    fn get_queryables(&self, tables: &Tables) -> Vec<(Arc<Resource>, Sources)> {
        // Compute the list of known queryables (keys)
        hat!(tables)
            .router_qabls
            .iter()
            .map(|s| {
                (
                    s.clone(),
                    // Compute the list of routers, peers and clients that are known
                    // sources of those queryables
                    Sources {
                        routers: Vec::from_iter(res_hat!(s).router_qabls.keys().cloned()),
                        peers: if hat!(tables).full_net(WhatAmI::Peer) {
                            Vec::from_iter(res_hat!(s).peer_qabls.keys().cloned())
                        } else {
                            s.session_ctxs
                                .values()
                                .filter_map(|f| {
                                    (f.face.whatami == WhatAmI::Peer && f.qabl.is_some())
                                        .then_some(f.face.zid)
                                })
                                .collect()
                        },
                        clients: s
                            .session_ctxs
                            .values()
                            .filter_map(|f| {
                                (f.face.whatami == WhatAmI::Client && f.qabl.is_some())
                                    .then_some(f.face.zid)
                            })
                            .collect(),
                    },
                )
            })
            .collect()
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
        tracing::trace!(
            "compute_query_route({}, {:?}, {:?})",
            key_expr,
            source,
            source_type
        );
        let key_expr = match OwnedKeyExpr::try_from(key_expr) {
            Ok(ke) => ke,
            Err(e) => {
                tracing::warn!("Invalid KE reached the system: {}", e);
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
                    tracing::warn!("Invalid KE reached the system: {}", e);
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

    fn get_query_routes_entries(&self, tables: &Tables) -> RoutesIndexes {
        get_routes_entries(tables)
    }
}
