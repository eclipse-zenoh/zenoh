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
use crate::net::routing::dispatcher::pubsub::*;
use crate::net::routing::dispatcher::resource::{NodeId, Resource, SessionContext};
use crate::net::routing::dispatcher::tables::Tables;
use crate::net::routing::dispatcher::tables::{Route, RoutingExpr};
use crate::net::routing::hat::HatPubSubTrait;
use crate::net::routing::router::RoutesIndexes;
use crate::net::routing::{RoutingContext, PREFIX_LIVELINESS};
use petgraph::graph::NodeIndex;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use zenoh_protocol::core::key_expr::OwnedKeyExpr;
use zenoh_protocol::network::declare::{InterestId, SubscriberId};
use zenoh_protocol::{
    core::{Reliability, WhatAmI, ZenohId},
    network::declare::{
        common::ext::WireExprType, ext, subscriber::ext::SubscriberInfo, Declare, DeclareBody,
        DeclareSubscriber, UndeclareSubscriber,
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
    routing_context: NodeId,
) {
    for child in childs {
        if net.graph.contains_node(*child) {
            match tables.get_face(&net.graph[*child].zid).cloned() {
                Some(mut someface) => {
                    if src_face.is_none() || someface.id != src_face.unwrap().id {
                        let key_expr = Resource::decl_key(res, &mut someface);

                        someface.primitives.send_declare(RoutingContext::with_expr(
                            Declare {
                                ext_qos: ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType {
                                    node_id: routing_context,
                                },
                                body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                    id: 0, // Sourced subscriptions do not use ids
                                    wire_expr: key_expr,
                                    ext_info: *sub_info,
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

#[inline]
fn propagate_simple_subscription_to(
    _tables: &mut Tables,
    dst_face: &mut Arc<FaceState>,
    res: &Arc<Resource>,
    sub_info: &SubscriberInfo,
    src_face: &mut Arc<FaceState>,
) {
    if (src_face.id != dst_face.id || res.expr().starts_with(PREFIX_LIVELINESS))
        && !face_hat!(dst_face).local_subs.contains_key(res)
        && dst_face.whatami == WhatAmI::Client
    {
        if dst_face.whatami != WhatAmI::Client {
            let id = face_hat!(dst_face).next_id.fetch_add(1, Ordering::SeqCst);
            face_hat_mut!(dst_face).local_subs.insert(res.clone(), id);
            let key_expr = Resource::decl_key(res, dst_face);
            dst_face.primitives.send_declare(RoutingContext::with_expr(
                Declare {
                    ext_qos: ext::QoSType::DECLARE,
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::DEFAULT,
                    body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                        id,
                        wire_expr: key_expr,
                        ext_info: *sub_info,
                    }),
                },
                res.expr(),
            ));
        } else {
            let matching_interests = face_hat!(dst_face)
                .remote_sub_interests
                .values()
                .filter(|si| si.0.as_ref().map(|si| si.matches(res)).unwrap_or(true))
                .cloned()
                .collect::<Vec<(Option<Arc<Resource>>, bool)>>();

            for (int_res, aggregate) in matching_interests {
                let res = if aggregate {
                    int_res.as_ref().unwrap_or(res)
                } else {
                    res
                };
                if !face_hat!(dst_face).local_subs.contains_key(res) {
                    let id = face_hat!(dst_face).next_id.fetch_add(1, Ordering::SeqCst);
                    face_hat_mut!(dst_face).local_subs.insert(res.clone(), id);
                    let key_expr = Resource::decl_key(res, dst_face);
                    dst_face.primitives.send_declare(RoutingContext::with_expr(
                        Declare {
                            ext_qos: ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType::DEFAULT,
                            body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                id,
                                wire_expr: key_expr,
                                ext_info: *sub_info,
                            }),
                        },
                        res.expr(),
                    ));
                }
            }
        }
    }
}

fn propagate_simple_subscription(
    tables: &mut Tables,
    res: &Arc<Resource>,
    sub_info: &SubscriberInfo,
    src_face: &mut Arc<FaceState>,
) {
    for mut dst_face in tables
        .faces
        .values()
        .cloned()
        .collect::<Vec<Arc<FaceState>>>()
    {
        propagate_simple_subscription_to(tables, &mut dst_face, res, sub_info, src_face);
    }
}

fn propagate_sourced_subscription(
    tables: &Tables,
    res: &Arc<Resource>,
    sub_info: &SubscriberInfo,
    src_face: Option<&Arc<FaceState>>,
    source: &ZenohId,
) {
    let net = hat!(tables).peers_net.as_ref().unwrap();
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
                    tree_sid.index() as NodeId,
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
            res_hat_mut!(res).peer_subs.insert(peer);
            hat_mut!(tables).peer_subs.insert(res.clone());
        }

        // Propagate subscription to peers
        propagate_sourced_subscription(tables, res, sub_info, Some(face), &peer);
    }

    if tables.whatami == WhatAmI::Peer {
        // Propagate subscription to clients
        propagate_simple_subscription(tables, res, sub_info, face);
    }
}

fn declare_peer_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    sub_info: &SubscriberInfo,
    peer: ZenohId,
) {
    register_peer_subscription(tables, face, res, sub_info, peer);
}

fn register_client_subscription(
    _tables: &mut Tables,
    face: &mut Arc<FaceState>,
    id: SubscriberId,
    res: &mut Arc<Resource>,
    sub_info: &SubscriberInfo,
) {
    // Register subscription
    {
        let res = get_mut_unchecked(res);
        match res.session_ctxs.get_mut(&face.id) {
            Some(ctx) => {
                if ctx.subs.is_none() {
                    get_mut_unchecked(ctx).subs = Some(*sub_info);
                }
            }
            None => {
                let ctx = res
                    .session_ctxs
                    .entry(face.id)
                    .or_insert_with(|| Arc::new(SessionContext::new(face.clone())));
                get_mut_unchecked(ctx).subs = Some(*sub_info);
            }
        }
    }
    face_hat_mut!(face).remote_subs.insert(id, res.clone());
}

fn declare_client_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    id: SubscriberId,
    res: &mut Arc<Resource>,
    sub_info: &SubscriberInfo,
) {
    register_client_subscription(tables, face, id, res, sub_info);
    let zid = tables.zid;
    register_peer_subscription(tables, face, res, sub_info, zid);
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
fn remote_client_subs(res: &Arc<Resource>, face: &Arc<FaceState>) -> bool {
    res.session_ctxs
        .values()
        .any(|ctx| ctx.face.id != face.id && ctx.subs.is_some())
}

#[inline]
fn send_forget_sourced_subscription_to_net_childs(
    tables: &Tables,
    net: &Network,
    childs: &[NodeIndex],
    res: &Arc<Resource>,
    src_face: Option<&Arc<FaceState>>,
    routing_context: Option<NodeId>,
) {
    for child in childs {
        if net.graph.contains_node(*child) {
            match tables.get_face(&net.graph[*child].zid).cloned() {
                Some(mut someface) => {
                    if src_face.is_none() || someface.id != src_face.unwrap().id {
                        let wire_expr = Resource::decl_key(res, &mut someface);

                        someface.primitives.send_declare(RoutingContext::with_expr(
                            Declare {
                                ext_qos: ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType {
                                    node_id: routing_context.unwrap_or(0),
                                },
                                body: DeclareBody::UndeclareSubscriber(UndeclareSubscriber {
                                    id: 0, // Sourced subscriptions do not use ids
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

fn propagate_forget_simple_subscription(tables: &mut Tables, res: &Arc<Resource>) {
    for mut face in tables.faces.values().cloned() {
        if let Some(id) = face_hat_mut!(&mut face).local_subs.remove(res) {
            // Still send WireExpr in UndeclareSubscriber to clients for pico
            let ext_wire_expr = if face.whatami == WhatAmI::Client {
                WireExprType {
                    wire_expr: Resource::get_best_key(res, "", face.id),
                }
            } else {
                WireExprType::null()
            };
            face.primitives.send_declare(RoutingContext::with_expr(
                Declare {
                    ext_qos: ext::QoSType::DECLARE,
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::DEFAULT,
                    body: DeclareBody::UndeclareSubscriber(UndeclareSubscriber {
                        id,
                        ext_wire_expr,
                    }),
                },
                res.expr(),
            ));
        }
        for res in face_hat!(face)
            .local_subs
            .keys()
            .cloned()
            .collect::<Vec<Arc<Resource>>>()
        {
            if !res.context().matches.iter().any(|m| {
                m.upgrade().is_some_and(|m| {
                    m.context.is_some()
                        && (remote_client_subs(&m, &face) || remote_peer_subs(tables, &m))
                })
            }) {
                if let Some(id) = face_hat_mut!(&mut face).local_subs.remove(&res) {
                    // Still send WireExpr in UndeclareSubscriber to clients for pico
                    let ext_wire_expr = if face.whatami == WhatAmI::Client {
                        WireExprType {
                            wire_expr: Resource::get_best_key(&res, "", face.id),
                        }
                    } else {
                        WireExprType::null()
                    };
                    face.primitives.send_declare(RoutingContext::with_expr(
                        Declare {
                            ext_qos: ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType::DEFAULT,
                            body: DeclareBody::UndeclareSubscriber(UndeclareSubscriber {
                                id,
                                ext_wire_expr,
                            }),
                        },
                        res.expr(),
                    ));
                }
            }
        }
    }
}

fn propagate_forget_sourced_subscription(
    tables: &Tables,
    res: &Arc<Resource>,
    src_face: Option<&Arc<FaceState>>,
    source: &ZenohId,
) {
    let net = hat!(tables).peers_net.as_ref().unwrap();
    match net.get_idx(source) {
        Some(tree_sid) => {
            if net.trees.len() > tree_sid.index() {
                send_forget_sourced_subscription_to_net_childs(
                    tables,
                    net,
                    &net.trees[tree_sid.index()].childs,
                    res,
                    src_face,
                    Some(tree_sid.index() as NodeId),
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

fn unregister_peer_subscription(tables: &mut Tables, res: &mut Arc<Resource>, peer: &ZenohId) {
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
        propagate_forget_sourced_subscription(tables, res, face, peer);
    }
}

fn forget_peer_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    peer: &ZenohId,
) {
    undeclare_peer_subscription(tables, Some(face), res, peer);
}

pub(super) fn undeclare_client_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
) {
    if !face_hat_mut!(face).remote_subs.values().any(|s| *s == *res) {
        if let Some(ctx) = get_mut_unchecked(res).session_ctxs.get_mut(&face.id) {
            get_mut_unchecked(ctx).subs = None;
        }

        let mut client_subs = client_subs(res);
        let peer_subs = remote_peer_subs(tables, res);
        if client_subs.is_empty() {
            undeclare_peer_subscription(tables, None, res, &tables.zid.clone());
        }

        if client_subs.len() == 1 && !peer_subs {
            let mut face = &mut client_subs[0];
            if !(face.whatami == WhatAmI::Client && res.expr().starts_with(PREFIX_LIVELINESS)) {
                if let Some(id) = face_hat_mut!(face).local_subs.remove(res) {
                    // Still send WireExpr in UndeclareSubscriber to clients for pico
                    let ext_wire_expr = if face.whatami == WhatAmI::Client {
                        WireExprType {
                            wire_expr: Resource::get_best_key(res, "", face.id),
                        }
                    } else {
                        WireExprType::null()
                    };
                    face.primitives.send_declare(RoutingContext::with_expr(
                        Declare {
                            ext_qos: ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType::DEFAULT,
                            body: DeclareBody::UndeclareSubscriber(UndeclareSubscriber {
                                id,
                                ext_wire_expr,
                            }),
                        },
                        res.expr(),
                    ));
                }
                for res in face_hat!(face)
                    .local_subs
                    .keys()
                    .cloned()
                    .collect::<Vec<Arc<Resource>>>()
                {
                    if !res.context().matches.iter().any(|m| {
                        m.upgrade().is_some_and(|m| {
                            m.context.is_some()
                                && (remote_client_subs(&m, face) || remote_peer_subs(tables, &m))
                        })
                    }) {
                        if let Some(id) = face_hat_mut!(&mut face).local_subs.remove(&res) {
                            // Still send WireExpr in UndeclareSubscriber to clients for pico
                            let ext_wire_expr = if face.whatami == WhatAmI::Client {
                                WireExprType {
                                    wire_expr: Resource::get_best_key(&res, "", face.id),
                                }
                            } else {
                                WireExprType::null()
                            };
                            face.primitives.send_declare(RoutingContext::with_expr(
                                Declare {
                                    ext_qos: ext::QoSType::DECLARE,
                                    ext_tstamp: None,
                                    ext_nodeid: ext::NodeIdType::DEFAULT,
                                    body: DeclareBody::UndeclareSubscriber(UndeclareSubscriber {
                                        id,
                                        ext_wire_expr,
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

fn forget_client_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    id: SubscriberId,
) -> Option<Arc<Resource>> {
    if let Some(mut res) = face_hat_mut!(face).remote_subs.remove(&id) {
        undeclare_client_subscription(tables, face, &mut res);
        Some(res)
    } else {
        None
    }
}

pub(super) fn pubsub_new_face(_tables: &mut Tables, _face: &mut Arc<FaceState>) {
    // Nothing to do
}

pub(super) fn pubsub_remove_node(tables: &mut Tables, node: &ZenohId) {
    for mut res in hat!(tables)
        .peer_subs
        .iter()
        .filter(|res| res_hat!(res).peer_subs.contains(node))
        .cloned()
        .collect::<Vec<Arc<Resource>>>()
    {
        unregister_peer_subscription(tables, &mut res, node);

        update_matches_data_routes(tables, &mut res);
        Resource::clean(&mut res)
    }
}

pub(super) fn pubsub_tree_change(tables: &mut Tables, new_childs: &[Vec<NodeIndex>]) {
    // propagate subs to new childs
    for (tree_sid, tree_childs) in new_childs.iter().enumerate() {
        if !tree_childs.is_empty() {
            let net = hat!(tables).peers_net.as_ref().unwrap();
            let tree_idx = NodeIndex::new(tree_sid);
            if net.graph.contains_node(tree_idx) {
                let tree_id = net.graph[tree_idx].zid;

                let subs_res = &hat!(tables).peer_subs;

                for res in subs_res {
                    let subs = &res_hat!(res).peer_subs;
                    for sub in subs {
                        if *sub == tree_id {
                            let sub_info = SubscriberInfo {
                                reliability: Reliability::Reliable, // @TODO compute proper reliability to propagate from reliability of known subscribers
                            };
                            send_sourced_subscription_to_net_childs(
                                tables,
                                net,
                                tree_childs,
                                res,
                                None,
                                &sub_info,
                                tree_sid as NodeId,
                            );
                        }
                    }
                }
            }
        }
    }

    // recompute routes
    update_data_routes_from(tables, &mut tables.root_res.clone());
}

#[inline]
fn insert_faces_for_subs(
    route: &mut Route,
    expr: &RoutingExpr,
    tables: &Tables,
    net: &Network,
    source: NodeId,
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

impl HatPubSubTrait for HatCode {
    fn declare_sub_interest(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: InterestId,
        res: Option<&mut Arc<Resource>>,
        current: bool,
        future: bool,
        aggregate: bool,
    ) {
        if current && face.whatami == WhatAmI::Client {
            let sub_info = SubscriberInfo {
                reliability: Reliability::Reliable, // @TODO compute proper reliability to propagate from reliability of known subscribers
            };
            if let Some(res) = res.as_ref() {
                if aggregate {
                    if hat!(tables).peer_subs.iter().any(|sub| {
                        sub.context.is_some()
                            && sub.matches(res)
                            && (remote_client_subs(sub, face) || remote_peer_subs(tables, sub))
                    }) {
                        let id = face_hat!(face).next_id.fetch_add(1, Ordering::SeqCst);
                        face_hat_mut!(face).local_subs.insert((*res).clone(), id);
                        let wire_expr = Resource::decl_key(res, face);
                        face.primitives.send_declare(RoutingContext::with_expr(
                            Declare {
                                ext_qos: ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType::DEFAULT,
                                body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                    id,
                                    wire_expr,
                                    ext_info: sub_info,
                                }),
                            },
                            res.expr(),
                        ));
                    }
                } else {
                    for sub in &hat!(tables).peer_subs {
                        if sub.context.is_some()
                            && sub.matches(res)
                            && (remote_client_subs(sub, face) || remote_peer_subs(tables, sub))
                        {
                            let id = face_hat!(face).next_id.fetch_add(1, Ordering::SeqCst);
                            face_hat_mut!(face).local_subs.insert(sub.clone(), id);
                            let wire_expr = Resource::decl_key(sub, face);
                            face.primitives.send_declare(RoutingContext::with_expr(
                                Declare {
                                    ext_qos: ext::QoSType::DECLARE,
                                    ext_tstamp: None,
                                    ext_nodeid: ext::NodeIdType::DEFAULT,
                                    body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                        id,
                                        wire_expr,
                                        ext_info: sub_info,
                                    }),
                                },
                                sub.expr(),
                            ));
                        }
                    }
                }
            } else {
                for sub in &hat!(tables).peer_subs {
                    if sub.context.is_some()
                        && (remote_client_subs(sub, face) || remote_peer_subs(tables, sub))
                    {
                        let id = face_hat!(face).next_id.fetch_add(1, Ordering::SeqCst);
                        face_hat_mut!(face).local_subs.insert(sub.clone(), id);
                        let wire_expr = Resource::decl_key(sub, face);
                        face.primitives.send_declare(RoutingContext::with_expr(
                            Declare {
                                ext_qos: ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType::DEFAULT,
                                body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                    id,
                                    wire_expr,
                                    ext_info: sub_info,
                                }),
                            },
                            sub.expr(),
                        ));
                    }
                }
            }
        }
        if future {
            face_hat_mut!(face)
                .remote_sub_interests
                .insert(id, (res.cloned(), aggregate));
        }
    }

    fn undeclare_sub_interest(
        &self,
        _tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: InterestId,
    ) {
        face_hat_mut!(face).remote_sub_interests.remove(&id);
    }

    fn declare_subscription(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: SubscriberId,
        res: &mut Arc<Resource>,
        sub_info: &SubscriberInfo,
        node_id: NodeId,
    ) {
        if face.whatami != WhatAmI::Client {
            if let Some(peer) = get_peer(tables, face, node_id) {
                declare_peer_subscription(tables, face, res, sub_info, peer)
            }
        } else {
            declare_client_subscription(tables, face, id, res, sub_info)
        }
    }

    fn undeclare_subscription(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: SubscriberId,
        res: Option<Arc<Resource>>,
        node_id: NodeId,
    ) -> Option<Arc<Resource>> {
        if face.whatami != WhatAmI::Client {
            if let Some(mut res) = res {
                if let Some(peer) = get_peer(tables, face, node_id) {
                    forget_peer_subscription(tables, face, &mut res, &peer);
                    Some(res)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            forget_client_subscription(tables, face, id)
        }
    }

    fn get_subscriptions(&self, tables: &Tables) -> Vec<Arc<Resource>> {
        hat!(tables).peer_subs.iter().cloned().collect()
    }

    fn compute_data_route(
        &self,
        tables: &Tables,
        expr: &mut RoutingExpr,
        source: NodeId,
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

        for mres in matches.iter() {
            let mres = mres.upgrade().unwrap();

            let net = hat!(tables).peers_net.as_ref().unwrap();
            let peer_source = match source_type {
                WhatAmI::Router | WhatAmI::Peer => source,
                _ => net.idx.index() as NodeId,
            };
            insert_faces_for_subs(
                &mut route,
                expr,
                tables,
                net,
                peer_source,
                &res_hat!(mres).peer_subs,
            );

            for (sid, context) in &mres.session_ctxs {
                if context.subs.is_some()
                    && match tables.whatami {
                        WhatAmI::Router => context.face.whatami != WhatAmI::Router,
                        _ => {
                            source_type == WhatAmI::Client
                                || context.face.whatami == WhatAmI::Client
                        }
                    }
                {
                    route.entry(*sid).or_insert_with(|| {
                        let key_expr = Resource::get_best_key(expr.prefix, expr.suffix, *sid);
                        (context.face.clone(), key_expr.to_owned(), NodeId::default())
                    });
                }
            }
        }
        for mcast_group in &tables.mcast_groups {
            route.insert(
                mcast_group.id,
                (
                    mcast_group.clone(),
                    expr.full_expr().to_string().into(),
                    NodeId::default(),
                ),
            );
        }
        Arc::new(route)
    }

    fn get_data_routes_entries(&self, tables: &Tables) -> RoutesIndexes {
        get_routes_entries(tables)
    }
}
