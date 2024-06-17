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
use super::{face_hat, face_hat_mut, get_routes_entries, hat, hat_mut, res_hat, res_hat_mut};
use super::{get_peer, HatCode, HatContext, HatFace, HatTables};
use crate::net::routing::dispatcher::face::FaceState;
use crate::net::routing::dispatcher::queries::*;
use crate::net::routing::dispatcher::resource::{NodeId, Resource, SessionContext};
use crate::net::routing::dispatcher::tables::Tables;
use crate::net::routing::dispatcher::tables::{QueryTargetQabl, QueryTargetQablSet, RoutingExpr};
use crate::net::routing::hat::{HatQueriesTrait, SendDeclare, Sources};
use crate::net::routing::router::RoutesIndexes;
use crate::net::routing::{RoutingContext, PREFIX_LIVELINESS};
use ordered_float::OrderedFloat;
use petgraph::graph::NodeIndex;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use zenoh_buffers::ZBuf;
use zenoh_protocol::core::key_expr::include::{Includer, DEFAULT_INCLUDER};
use zenoh_protocol::core::key_expr::OwnedKeyExpr;
use zenoh_protocol::{
    core::{WhatAmI, WireExpr, ZenohId},
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

fn local_peer_qabl_info(_tables: &Tables, res: &Arc<Resource>) -> QueryableInfo {
    res.session_ctxs
        .values()
        .fold(None, |accu, ctx| {
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
    let info = if res.context.is_some() {
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
    } else {
        None
    };
    res.session_ctxs
        .values()
        .fold(info, |accu, ctx| {
            if ctx.face.id != face.id && ctx.face.whatami != WhatAmI::Peer
                || face.whatami != WhatAmI::Peer
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

#[inline]
fn send_sourced_queryable_to_net_children(
    tables: &Tables,
    net: &Network,
    children: &[NodeIndex],
    res: &Arc<Resource>,
    qabl_info: &QueryableInfo,
    src_face: Option<&mut Arc<FaceState>>,
    routing_context: NodeId,
) {
    for child in children {
        if net.graph.contains_node(*child) {
            match tables.get_face(&net.graph[*child].zid).cloned() {
                Some(mut someface) => {
                    if src_face.is_none() || someface.id != src_face.as_ref().unwrap().id {
                        let key_expr = Resource::decl_key(res, &mut someface);

                        tracing::debug!("Send queryable {} on {}", res.expr(), someface);

                        someface.primitives.send_declare(RoutingContext::with_expr(
                            Declare {
                                ext_qos: ext::QoSType::declare_default(),
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType {
                                    node_id: routing_context,
                                },
                                body: DeclareBody::DeclareQueryable(DeclareQueryable {
                                    id: 0, // @TODO use proper QueryableId (#703)
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
    send_declare: &mut SendDeclare,
) {
    let faces = tables.faces.values().cloned();
    for mut dst_face in faces {
        let info = local_qabl_info(tables, res, &dst_face);
        let current_info = face_hat!(dst_face).local_qabls.get(res);
        if (src_face.is_none() || src_face.as_ref().unwrap().id != dst_face.id)
            && (current_info.is_none() || *current_info.unwrap() != info)
            && dst_face.whatami == WhatAmI::Client
        {
            face_hat_mut!(&mut dst_face)
                .local_qabls
                .insert(res.clone(), info);
            let key_expr = Resource::decl_key(res, &mut dst_face);
            send_declare(
                &dst_face.primitives,
                RoutingContext::with_expr(
                    Declare {
                        ext_qos: ext::QoSType::declare_default(),
                        ext_tstamp: None,
                        ext_nodeid: ext::NodeIdType::default(),
                        body: DeclareBody::DeclareQueryable(DeclareQueryable {
                            id: 0, // @TODO use proper QueryableId (#703)
                            wire_expr: key_expr,
                            ext_info: info,
                        }),
                    },
                    res.expr(),
                ),
            );
        }
    }
}

fn propagate_sourced_queryable(
    tables: &Tables,
    res: &Arc<Resource>,
    qabl_info: &QueryableInfo,
    src_face: Option<&mut Arc<FaceState>>,
    source: &ZenohId,
) {
    let net = hat!(tables).peers_net.as_ref().unwrap();
    match net.get_idx(source) {
        Some(tree_sid) => {
            if net.trees.len() > tree_sid.index() {
                send_sourced_queryable_to_net_children(
                    tables,
                    net,
                    &net.trees[tree_sid.index()].children,
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

fn register_peer_queryable(
    tables: &mut Tables,
    mut face: Option<&mut Arc<FaceState>>,
    res: &mut Arc<Resource>,
    qabl_info: &QueryableInfo,
    peer: ZenohId,
    send_declare: &mut SendDeclare,
) {
    let current_info = res_hat!(res).peer_qabls.get(&peer);
    if current_info.is_none() || current_info.unwrap() != qabl_info {
        // Register peer queryable
        {
            tracing::debug!("Register peer queryable {} (peer: {})", res.expr(), peer,);
            res_hat_mut!(res).peer_qabls.insert(peer, *qabl_info);
            hat_mut!(tables).peer_qabls.insert(res.clone());
        }

        // Propagate queryable to peers
        propagate_sourced_queryable(tables, res, qabl_info, face.as_deref_mut(), &peer);
    }

    if tables.whatami == WhatAmI::Peer {
        // Propagate queryable to clients
        propagate_simple_queryable(tables, res, face, send_declare);
    }
}

fn declare_peer_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    qabl_info: &QueryableInfo,
    peer: ZenohId,
    send_declare: &mut SendDeclare,
) {
    let face = Some(face);
    register_peer_queryable(tables, face, res, qabl_info, peer, send_declare);
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
        tracing::debug!("Register queryable {} (face: {})", res.expr(), face,);
        get_mut_unchecked(res.session_ctxs.entry(face.id).or_insert_with(|| {
            Arc::new(SessionContext {
                face: face.clone(),
                local_expr_id: None,
                remote_expr_id: None,
                subs: None,
                qabl: None,
                last_values: HashMap::new(),
                in_interceptor_cache: None,
                e_interceptor_cache: None,
            })
        }))
        .qabl = Some(*qabl_info);
    }
    face_hat_mut!(face).remote_qabls.insert(res.clone());
}

fn declare_client_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    qabl_info: &QueryableInfo,
    send_declare: &mut SendDeclare,
) {
    register_client_queryable(tables, face, res, qabl_info);

    let local_details = local_peer_qabl_info(tables, res);
    let zid = tables.zid;
    register_peer_queryable(tables, Some(face), res, &local_details, zid, send_declare);
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
fn send_forget_sourced_queryable_to_net_children(
    tables: &Tables,
    net: &Network,
    children: &[NodeIndex],
    res: &Arc<Resource>,
    src_face: Option<&Arc<FaceState>>,
    routing_context: NodeId,
) {
    for child in children {
        if net.graph.contains_node(*child) {
            match tables.get_face(&net.graph[*child].zid).cloned() {
                Some(mut someface) => {
                    if src_face.is_none() || someface.id != src_face.unwrap().id {
                        let wire_expr = Resource::decl_key(res, &mut someface);

                        tracing::debug!("Send forget queryable {}  on {}", res.expr(), someface);

                        someface.primitives.send_declare(RoutingContext::with_expr(
                            Declare {
                                ext_qos: ext::QoSType::declare_default(),
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType {
                                    node_id: routing_context,
                                },
                                body: DeclareBody::UndeclareQueryable(UndeclareQueryable {
                                    id: 0, // @TODO use proper QueryableId (#703)
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

fn propagate_forget_simple_queryable(
    tables: &mut Tables,
    res: &mut Arc<Resource>,
    send_declare: &mut SendDeclare,
) {
    for face in tables.faces.values_mut() {
        if face_hat!(face).local_qabls.contains_key(res) {
            let wire_expr = Resource::get_best_key(res, "", face.id);
            send_declare(
                &face.primitives,
                RoutingContext::with_expr(
                    Declare {
                        ext_qos: ext::QoSType::declare_default(),
                        ext_tstamp: None,
                        ext_nodeid: ext::NodeIdType::default(),
                        body: DeclareBody::UndeclareQueryable(UndeclareQueryable {
                            id: 0, // @TODO use proper QueryableId (#703)
                            ext_wire_expr: WireExprType { wire_expr },
                        }),
                    },
                    res.expr(),
                ),
            );

            face_hat_mut!(face).local_qabls.remove(res);
        }
    }
}

fn propagate_forget_sourced_queryable(
    tables: &mut Tables,
    res: &mut Arc<Resource>,
    src_face: Option<&Arc<FaceState>>,
    source: &ZenohId,
) {
    let net = hat!(tables).peers_net.as_ref().unwrap();
    match net.get_idx(source) {
        Some(tree_sid) => {
            if net.trees.len() > tree_sid.index() {
                send_forget_sourced_queryable_to_net_children(
                    tables,
                    net,
                    &net.trees[tree_sid.index()].children,
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

fn unregister_peer_queryable(
    tables: &mut Tables,
    res: &mut Arc<Resource>,
    peer: &ZenohId,
    send_declare: &mut SendDeclare,
) {
    tracing::debug!("Unregister peer queryable {} (peer: {})", res.expr(), peer,);
    res_hat_mut!(res).peer_qabls.remove(peer);

    if res_hat!(res).peer_qabls.is_empty() {
        hat_mut!(tables)
            .peer_qabls
            .retain(|qabl| !Arc::ptr_eq(qabl, res));

        if tables.whatami == WhatAmI::Peer {
            propagate_forget_simple_queryable(tables, res, send_declare);
        }
    }
}

fn undeclare_peer_queryable(
    tables: &mut Tables,
    face: Option<&Arc<FaceState>>,
    res: &mut Arc<Resource>,
    peer: &ZenohId,
    send_declare: &mut SendDeclare,
) {
    if res_hat!(res).peer_qabls.contains_key(peer) {
        unregister_peer_queryable(tables, res, peer, send_declare);
        propagate_forget_sourced_queryable(tables, res, face, peer);
    }
}

fn forget_peer_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    peer: &ZenohId,
    send_declare: &mut SendDeclare,
) {
    undeclare_peer_queryable(tables, Some(face), res, peer, send_declare);
}

pub(super) fn undeclare_client_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    send_declare: &mut SendDeclare,
) {
    tracing::debug!("Unregister client queryable {} for {}", res.expr(), face);
    if let Some(ctx) = get_mut_unchecked(res).session_ctxs.get_mut(&face.id) {
        get_mut_unchecked(ctx).qabl = None;
        if ctx.qabl.is_none() {
            face_hat_mut!(face).remote_qabls.remove(res);
        }
    }

    let mut client_qabls = client_qabls(res);
    let peer_qabls = remote_peer_qabls(tables, res);

    if client_qabls.is_empty() {
        undeclare_peer_queryable(tables, None, res, &tables.zid.clone(), send_declare);
    } else {
        let local_info = local_peer_qabl_info(tables, res);
        register_peer_queryable(tables, None, res, &local_info, tables.zid, send_declare);
    }

    if client_qabls.len() == 1 && !peer_qabls {
        let face = &mut client_qabls[0];
        if face_hat!(face).local_qabls.contains_key(res) {
            let wire_expr = Resource::get_best_key(res, "", face.id);
            send_declare(
                &face.primitives,
                RoutingContext::with_expr(
                    Declare {
                        ext_qos: ext::QoSType::declare_default(),
                        ext_tstamp: None,
                        ext_nodeid: ext::NodeIdType::default(),
                        body: DeclareBody::UndeclareQueryable(UndeclareQueryable {
                            id: 0, // @TODO use proper QueryableId (#703)
                            ext_wire_expr: WireExprType { wire_expr },
                        }),
                    },
                    res.expr(),
                ),
            );

            face_hat_mut!(face).local_qabls.remove(res);
        }
    }
}

fn forget_client_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    send_declare: &mut SendDeclare,
) {
    undeclare_client_queryable(tables, face, res, send_declare);
}

pub(super) fn queries_new_face(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    send_declare: &mut SendDeclare,
) {
    if face.whatami == WhatAmI::Client {
        for qabl in &hat!(tables).peer_qabls {
            if qabl.context.is_some() {
                let info = local_qabl_info(tables, qabl, face);
                face_hat_mut!(face).local_qabls.insert(qabl.clone(), info);
                let key_expr = Resource::decl_key(qabl, face);
                send_declare(
                    &face.primitives,
                    RoutingContext::with_expr(
                        Declare {
                            ext_qos: ext::QoSType::declare_default(),
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType::default(),
                            body: DeclareBody::DeclareQueryable(DeclareQueryable {
                                id: 0, // @TODO use proper QueryableId (#703)
                                wire_expr: key_expr,
                                ext_info: info,
                            }),
                        },
                        qabl.expr(),
                    ),
                );
            }
        }
    }
}

pub(super) fn queries_remove_node(
    tables: &mut Tables,
    node: &ZenohId,
    send_declare: &mut SendDeclare,
) {
    let mut qabls = vec![];
    for res in hat!(tables).peer_qabls.iter() {
        for qabl in res_hat!(res).peer_qabls.keys() {
            if qabl == node {
                qabls.push(res.clone());
            }
        }
    }
    for mut res in qabls {
        unregister_peer_queryable(tables, &mut res, node, send_declare);

        update_matches_query_routes(tables, &res);
        Resource::clean(&mut res)
    }
}

pub(super) fn queries_tree_change(tables: &mut Tables, new_children: &[Vec<NodeIndex>]) {
    // propagate qabls to new children
    for (tree_sid, tree_children) in new_children.iter().enumerate() {
        if !tree_children.is_empty() {
            let net = hat!(tables).peers_net.as_ref().unwrap();
            let tree_idx = NodeIndex::new(tree_sid);
            if net.graph.contains_node(tree_idx) {
                let tree_id = net.graph[tree_idx].zid;

                let qabls_res = &hat!(tables).peer_qabls;

                for res in qabls_res {
                    let qabls = &res_hat!(res).peer_qabls;
                    if let Some(qabl_info) = qabls.get(&tree_id) {
                        send_sourced_queryable_to_net_children(
                            tables,
                            net,
                            tree_children,
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
        tracing::trace!("Tree for node sid:{} not yet ready", source);
    }
}

lazy_static::lazy_static! {
    static ref EMPTY_ROUTE: Arc<QueryTargetQablSet> = Arc::new(Vec::new());
}

impl HatQueriesTrait for HatCode {
    fn declare_queryable(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        res: &mut Arc<Resource>,
        qabl_info: &QueryableInfo,
        node_id: NodeId,
        send_declare: &mut SendDeclare,
    ) {
        if face.whatami != WhatAmI::Client {
            if let Some(peer) = get_peer(tables, face, node_id) {
                declare_peer_queryable(tables, face, res, qabl_info, peer, send_declare);
            }
        } else {
            declare_client_queryable(tables, face, res, qabl_info, send_declare);
        }
    }

    fn undeclare_queryable(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        res: &mut Arc<Resource>,
        node_id: NodeId,
        send_declare: &mut SendDeclare,
    ) {
        if face.whatami != WhatAmI::Client {
            if let Some(peer) = get_peer(tables, face, node_id) {
                forget_peer_queryable(tables, face, res, &peer, send_declare);
            }
        } else {
            forget_client_queryable(tables, face, res, send_declare);
        }
    }

    fn get_queryables(&self, tables: &Tables) -> Vec<(Arc<Resource>, Sources)> {
        // Compute the list of known queryables (keys)
        hat!(tables)
            .peer_qabls
            .iter()
            .map(|s| {
                (
                    s.clone(),
                    // Compute the list of routers, peers and clients that are known
                    // sources of those queryables
                    Sources {
                        routers: vec![],
                        peers: Vec::from_iter(res_hat!(s).peer_qabls.keys().cloned()),
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

        for mres in matches.iter() {
            let mres = mres.upgrade().unwrap();
            let complete = DEFAULT_INCLUDER.includes(mres.expr().as_bytes(), key_expr.as_bytes());

            let net = hat!(tables).peers_net.as_ref().unwrap();
            let peer_source = match source_type {
                WhatAmI::Router | WhatAmI::Peer => source,
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

            for (sid, context) in &mres.session_ctxs {
                if match tables.whatami {
                    WhatAmI::Router => context.face.whatami != WhatAmI::Router,
                    _ => source_type == WhatAmI::Client || context.face.whatami == WhatAmI::Client,
                } {
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
