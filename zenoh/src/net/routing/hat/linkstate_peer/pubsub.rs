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
    collections::{HashMap, HashSet},
    ops::Deref,
    sync::{atomic::Ordering, Arc},
};

use petgraph::graph::NodeIndex;
use zenoh_protocol::{
    core::{key_expr::OwnedKeyExpr, WhatAmI, ZenohIdProto},
    network::{
        declare::{
            common::ext::WireExprType, ext, Declare, DeclareBody, DeclareSubscriber, SubscriberId,
            UndeclareSubscriber,
        },
        interest::{InterestId, InterestMode},
    },
};
use zenoh_sync::get_mut_unchecked;

use super::{
    face_hat, face_hat_mut, get_peer, hat, hat_mut, network::Network, push_declaration_profile,
    res_hat, res_hat_mut, HatCode, HatContext, HatFace, HatTables,
};
#[cfg(feature = "unstable")]
use crate::key_expr::KeyExpr;
use crate::net::routing::{
    dispatcher::{
        face::{Face, FaceState},
        interests::RemoteInterest,
        pubsub::SubscriberInfo,
        resource::{NodeId, Resource, SessionContext},
        tables::{Route, RoutingExpr, Tables},
    },
    hat::{CurrentFutureTrait, HatPubSubTrait, SendDeclare, Sources},
    router::disable_matches_data_routes,
};

#[inline]
fn send_sourced_subscription_to_net_children(
    tables: &Tables,
    net: &Network,
    children: &[NodeIndex],
    res: &Arc<Resource>,
    src_face: Option<&Arc<FaceState>>,
    _sub_info: &SubscriberInfo,
    routing_context: NodeId,
) {
    for child in children {
        if net.graph.contains_node(*child) {
            match tables.get_face(&net.graph[*child].zid).cloned() {
                Some(someface) => {
                    if src_face
                        .map(|src_face| someface.state.id != src_face.id)
                        .unwrap_or(true)
                    {
                        let push_declaration = push_declaration_profile(&someface.state);
                        let key_expr = Resource::decl_key(res, &someface, push_declaration);

                        someface.intercept_declare(
                            &mut Declare {
                                interest_id: None,
                                ext_qos: ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType {
                                    node_id: routing_context,
                                },
                                body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                    id: 0, // Sourced subscriptions do not use ids
                                    wire_expr: key_expr,
                                }),
                            },
                            Some(res),
                        );
                    }
                }
                None => tracing::trace!("Unable to find face for zid {}", net.graph[*child].zid),
            }
        }
    }
}

#[inline]
fn propagate_simple_subscription_to(
    _tables: &mut Tables,
    dst_face: &mut Face,
    res: &Arc<Resource>,
    _sub_info: &SubscriberInfo,
    src_face: &mut Arc<FaceState>,
    send_declare: &mut SendDeclare,
) {
    if (src_face.id != dst_face.state.id)
        && !face_hat!(dst_face.state).local_subs.contains_key(res)
        && dst_face.state.whatami == WhatAmI::Client
    {
        if dst_face.state.whatami != WhatAmI::Client {
            let id = face_hat!(dst_face.state)
                .next_id
                .fetch_add(1, Ordering::SeqCst);
            face_hat_mut!(&mut dst_face.state)
                .local_subs
                .insert(res.clone(), id);
            let key_expr =
                Resource::decl_key(res, dst_face, push_declaration_profile(&dst_face.state));
            send_declare(
                dst_face,
                Declare {
                    interest_id: None,
                    ext_qos: ext::QoSType::DECLARE,
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::DEFAULT,
                    body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                        id,
                        wire_expr: key_expr,
                    }),
                },
                Some(res.clone()),
            );
        } else {
            let matching_interests = face_hat!(dst_face.state)
                .remote_interests
                .values()
                .filter(|i| i.options.subscribers() && i.matches(res))
                .cloned()
                .collect::<Vec<_>>();

            for RemoteInterest {
                res: int_res,
                options,
                ..
            } in matching_interests
            {
                let res = if options.aggregate() {
                    int_res.as_ref().unwrap_or(res)
                } else {
                    res
                };
                if !face_hat!(dst_face.state).local_subs.contains_key(res) {
                    let id = face_hat!(dst_face.state)
                        .next_id
                        .fetch_add(1, Ordering::SeqCst);
                    face_hat_mut!(&mut dst_face.state)
                        .local_subs
                        .insert(res.clone(), id);
                    let key_expr = Resource::decl_key(
                        res,
                        dst_face,
                        push_declaration_profile(&dst_face.state),
                    );
                    send_declare(
                        dst_face,
                        Declare {
                            interest_id: None,
                            ext_qos: ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType::DEFAULT,
                            body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                id,
                                wire_expr: key_expr,
                            }),
                        },
                        Some(res.clone()),
                    );
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
    send_declare: &mut SendDeclare,
) {
    for mut dst_face in tables.faces.values().cloned().collect::<Vec<_>>() {
        propagate_simple_subscription_to(
            tables,
            &mut dst_face,
            res,
            sub_info,
            src_face,
            send_declare,
        );
    }
}

fn propagate_sourced_subscription(
    tables: &Tables,
    res: &Arc<Resource>,
    sub_info: &SubscriberInfo,
    src_face: Option<&Arc<FaceState>>,
    source: &ZenohIdProto,
) {
    let net = hat!(tables).linkstatepeers_net.as_ref().unwrap();
    match net.get_idx(source) {
        Some(tree_sid) => {
            if net.trees.len() > tree_sid.index() {
                send_sourced_subscription_to_net_children(
                    tables,
                    net,
                    &net.trees[tree_sid.index()].children,
                    res,
                    src_face,
                    sub_info,
                    tree_sid.index() as NodeId,
                );
            } else {
                tracing::trace!(
                    "Propagating sub {}: tree for node {} sid:{} not yet ready",
                    res.expr(),
                    tree_sid.index(),
                    source
                );
            }
        }
        None => tracing::error!(
            "Error propagating sub {}: cannot get index of {}!",
            res.expr(),
            source
        ),
    }
}

fn register_linkstatepeer_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    sub_info: &SubscriberInfo,
    peer: ZenohIdProto,
    send_declare: &mut SendDeclare,
) {
    if !res_hat!(res).linkstatepeer_subs.contains(&peer) {
        // Register peer subscription
        {
            res_hat_mut!(res).linkstatepeer_subs.insert(peer);
            hat_mut!(tables).linkstatepeer_subs.insert(res.clone());
        }

        // Propagate subscription to peers
        propagate_sourced_subscription(tables, res, sub_info, Some(face), &peer);
    }

    // Propagate subscription to clients
    propagate_simple_subscription(tables, res, sub_info, face, send_declare);
}

fn declare_linkstatepeer_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    sub_info: &SubscriberInfo,
    peer: ZenohIdProto,
    send_declare: &mut SendDeclare,
) {
    register_linkstatepeer_subscription(tables, face, res, sub_info, peer, send_declare);
}

fn register_simple_subscription(
    _tables: &mut Tables,
    face: &Face,
    id: SubscriberId,
    res: &mut Arc<Resource>,
    sub_info: &SubscriberInfo,
) {
    // Register subscription
    {
        let res = get_mut_unchecked(res);
        match res.session_ctxs.get_mut(&face.state.id) {
            Some(ctx) => {
                if ctx.subs.is_none() {
                    get_mut_unchecked(ctx).subs = Some(*sub_info);
                }
            }
            None => {
                let ctx = res
                    .session_ctxs
                    .entry(face.state.id)
                    .or_insert_with(|| Arc::new(SessionContext::new(face.clone())));
                get_mut_unchecked(ctx).subs = Some(*sub_info);
            }
        }
    }
    face_hat_mut!(&mut face.state.clone())
        .remote_subs
        .insert(id, res.clone());
}

fn declare_simple_subscription(
    tables: &mut Tables,
    face: &Face,
    id: SubscriberId,
    res: &mut Arc<Resource>,
    sub_info: &SubscriberInfo,
    send_declare: &mut SendDeclare,
) {
    register_simple_subscription(tables, face, id, res, sub_info);
    let zid = tables.zid;
    register_linkstatepeer_subscription(
        tables,
        &mut face.state.clone(),
        res,
        sub_info,
        zid,
        send_declare,
    );
}

#[inline]
fn remote_linkstatepeer_subs(tables: &Tables, res: &Arc<Resource>) -> bool {
    res.context.is_some()
        && res_hat!(res)
            .linkstatepeer_subs
            .iter()
            .any(|peer| peer != &tables.zid)
}

#[inline]
fn simple_subs(res: &Arc<Resource>) -> Vec<Face> {
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
fn remote_simple_subs(res: &Arc<Resource>, face: &Arc<FaceState>) -> bool {
    res.session_ctxs
        .values()
        .any(|ctx| ctx.face.state.id != face.id && ctx.subs.is_some())
}

#[inline]
fn send_forget_sourced_subscription_to_net_children(
    tables: &Tables,
    net: &Network,
    children: &[NodeIndex],
    res: &Arc<Resource>,
    src_face: Option<&Arc<FaceState>>,
    routing_context: Option<NodeId>,
) {
    for child in children {
        if net.graph.contains_node(*child) {
            match tables.get_face(&net.graph[*child].zid).cloned() {
                Some(someface) => {
                    if src_face
                        .map(|src_face| someface.state.id != src_face.id)
                        .unwrap_or(true)
                    {
                        let push_declaration = push_declaration_profile(&someface.state);
                        let wire_expr = Resource::decl_key(res, &someface, push_declaration);

                        someface.intercept_declare(
                            &mut Declare {
                                interest_id: None,
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
                            Some(res),
                        );
                    }
                }
                None => tracing::trace!("Unable to find face for zid {}", net.graph[*child].zid),
            }
        }
    }
}

fn propagate_forget_simple_subscription(
    tables: &mut Tables,
    res: &Arc<Resource>,
    send_declare: &mut SendDeclare,
) {
    for mut face in tables.faces.values().cloned() {
        if let Some(id) = face_hat_mut!(&mut face.state).local_subs.remove(res) {
            send_declare(
                &face,
                Declare {
                    interest_id: None,
                    ext_qos: ext::QoSType::DECLARE,
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::DEFAULT,
                    body: DeclareBody::UndeclareSubscriber(UndeclareSubscriber {
                        id,
                        ext_wire_expr: WireExprType::null(),
                    }),
                },
                Some(res.clone()),
            );
        }
        for res in face_hat!(face.state)
            .local_subs
            .keys()
            .cloned()
            .collect::<Vec<Arc<Resource>>>()
        {
            if !res.context().matches.iter().any(|m| {
                m.upgrade().is_some_and(|m| {
                    m.context.is_some()
                        && (remote_simple_subs(&m, &face.state)
                            || remote_linkstatepeer_subs(tables, &m))
                })
            }) {
                if let Some(id) = face_hat_mut!(&mut face.state).local_subs.remove(&res) {
                    send_declare(
                        &face,
                        Declare {
                            interest_id: None,
                            ext_qos: ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType::DEFAULT,
                            body: DeclareBody::UndeclareSubscriber(UndeclareSubscriber {
                                id,
                                ext_wire_expr: WireExprType::null(),
                            }),
                        },
                        Some(res.clone()),
                    );
                }
            }
        }
    }
}

fn propagate_forget_sourced_subscription(
    tables: &Tables,
    res: &Arc<Resource>,
    src_face: Option<&Arc<FaceState>>,
    source: &ZenohIdProto,
) {
    let net = hat!(tables).linkstatepeers_net.as_ref().unwrap();
    match net.get_idx(source) {
        Some(tree_sid) => {
            if net.trees.len() > tree_sid.index() {
                send_forget_sourced_subscription_to_net_children(
                    tables,
                    net,
                    &net.trees[tree_sid.index()].children,
                    res,
                    src_face,
                    Some(tree_sid.index() as NodeId),
                );
            } else {
                tracing::trace!(
                    "Propagating forget sub {}: tree for node {} sid:{} not yet ready",
                    res.expr(),
                    tree_sid.index(),
                    source
                );
            }
        }
        None => tracing::error!(
            "Error propagating forget sub {}: cannot get index of {}!",
            res.expr(),
            source
        ),
    }
}

fn unregister_peer_subscription(
    tables: &mut Tables,
    res: &mut Arc<Resource>,
    peer: &ZenohIdProto,
    send_declare: &mut SendDeclare,
) {
    res_hat_mut!(res)
        .linkstatepeer_subs
        .retain(|sub| sub != peer);

    if res_hat!(res).linkstatepeer_subs.is_empty() {
        hat_mut!(tables)
            .linkstatepeer_subs
            .retain(|sub| !Arc::ptr_eq(sub, res));

        propagate_forget_simple_subscription(tables, res, send_declare);
    }
}

fn undeclare_linkstatepeer_subscription(
    tables: &mut Tables,
    face: Option<&Arc<FaceState>>,
    res: &mut Arc<Resource>,
    peer: &ZenohIdProto,
    send_declare: &mut SendDeclare,
) {
    if res_hat!(res).linkstatepeer_subs.contains(peer) {
        unregister_peer_subscription(tables, res, peer, send_declare);
        propagate_forget_sourced_subscription(tables, res, face, peer);
    }
}

fn forget_linkstatepeer_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    peer: &ZenohIdProto,
    send_declare: &mut SendDeclare,
) {
    undeclare_linkstatepeer_subscription(tables, Some(face), res, peer, send_declare);
}

pub(super) fn undeclare_simple_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    send_declare: &mut SendDeclare,
) {
    if !face_hat_mut!(face).remote_subs.values().any(|s| *s == *res) {
        if let Some(ctx) = get_mut_unchecked(res).session_ctxs.get_mut(&face.id) {
            get_mut_unchecked(ctx).subs = None;
        }

        let mut simple_subs = simple_subs(res);
        let linkstatepeer_subs = remote_linkstatepeer_subs(tables, res);
        if simple_subs.is_empty() {
            undeclare_linkstatepeer_subscription(
                tables,
                None,
                res,
                &tables.zid.clone(),
                send_declare,
            );
        }

        if simple_subs.len() == 1 && !linkstatepeer_subs {
            let face = &mut simple_subs[0];
            if face.state.whatami != WhatAmI::Client {
                if let Some(id) = face_hat_mut!(&mut face.state).local_subs.remove(res) {
                    send_declare(
                        face,
                        Declare {
                            interest_id: None,
                            ext_qos: ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType::DEFAULT,
                            body: DeclareBody::UndeclareSubscriber(UndeclareSubscriber {
                                id,
                                ext_wire_expr: WireExprType::null(),
                            }),
                        },
                        Some(res.clone()),
                    );
                }
                for res in face_hat!(face.state)
                    .local_subs
                    .keys()
                    .cloned()
                    .collect::<Vec<Arc<Resource>>>()
                {
                    if !res.context().matches.iter().any(|m| {
                        m.upgrade().is_some_and(|m| {
                            m.context.is_some()
                                && (remote_simple_subs(&m, &face.state)
                                    || remote_linkstatepeer_subs(tables, &m))
                        })
                    }) {
                        if let Some(id) = face_hat_mut!(&mut face.state).local_subs.remove(&res) {
                            send_declare(
                                face,
                                Declare {
                                    interest_id: None,
                                    ext_qos: ext::QoSType::DECLARE,
                                    ext_tstamp: None,
                                    ext_nodeid: ext::NodeIdType::DEFAULT,
                                    body: DeclareBody::UndeclareSubscriber(UndeclareSubscriber {
                                        id,
                                        ext_wire_expr: WireExprType::null(),
                                    }),
                                },
                                Some(res.clone()),
                            );
                        }
                    }
                }
            }
        }
    }
}

fn forget_simple_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    id: SubscriberId,
    send_declare: &mut SendDeclare,
) -> Option<Arc<Resource>> {
    if let Some(mut res) = face_hat_mut!(face).remote_subs.remove(&id) {
        undeclare_simple_subscription(tables, face, &mut res, send_declare);
        Some(res)
    } else {
        None
    }
}

pub(super) fn pubsub_remove_node(
    tables: &mut Tables,
    node: &ZenohIdProto,
    send_declare: &mut SendDeclare,
) {
    for mut res in hat!(tables)
        .linkstatepeer_subs
        .iter()
        .filter(|res| res_hat!(res).linkstatepeer_subs.contains(node))
        .cloned()
        .collect::<Vec<Arc<Resource>>>()
    {
        unregister_peer_subscription(tables, &mut res, node, send_declare);

        disable_matches_data_routes(tables, &mut res);
        Resource::clean(&mut res)
    }
}

pub(super) fn pubsub_tree_change(tables: &mut Tables, new_children: &[Vec<NodeIndex>]) {
    let net = match hat!(tables).linkstatepeers_net.as_ref() {
        Some(net) => net,
        None => {
            tracing::error!("Error accessing peers_net in pubsub_tree_change!");
            return;
        }
    };
    // propagate subs to new children
    for (tree_sid, tree_children) in new_children.iter().enumerate() {
        if !tree_children.is_empty() {
            let tree_idx = NodeIndex::new(tree_sid);
            if net.graph.contains_node(tree_idx) {
                let tree_id = net.graph[tree_idx].zid;

                let subs_res = &hat!(tables).linkstatepeer_subs;

                for res in subs_res {
                    let subs = &res_hat!(res).linkstatepeer_subs;
                    for sub in subs {
                        if *sub == tree_id {
                            let sub_info = SubscriberInfo;
                            send_sourced_subscription_to_net_children(
                                tables,
                                net,
                                tree_children,
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
}

#[inline]
fn make_sub_id(res: &Arc<Resource>, face: &mut Arc<FaceState>, mode: InterestMode) -> u32 {
    if mode.future() {
        if let Some(id) = face_hat!(face).local_subs.get(res) {
            *id
        } else {
            let id = face_hat!(face).next_id.fetch_add(1, Ordering::SeqCst);
            face_hat_mut!(face).local_subs.insert(res.clone(), id);
            id
        }
    } else {
        0
    }
}

pub(super) fn declare_sub_interest(
    tables: &mut Tables,
    face: &Face,
    id: InterestId,
    res: Option<&mut Arc<Resource>>,
    mode: InterestMode,
    aggregate: bool,
    send_declare: &mut SendDeclare,
) {
    if mode.current() && face.state.whatami == WhatAmI::Client {
        let interest_id = Some(id);
        if let Some(res) = res.as_ref() {
            if aggregate {
                if hat!(tables).linkstatepeer_subs.iter().any(|sub| {
                    sub.context.is_some()
                        && sub.matches(res)
                        && (remote_simple_subs(sub, &face.state.clone())
                            || remote_linkstatepeer_subs(tables, sub))
                }) {
                    let id = make_sub_id(res, &mut face.state.clone(), mode);
                    let wire_expr =
                        Resource::decl_key(res, face, push_declaration_profile(&face.state));
                    send_declare(
                        face,
                        Declare {
                            interest_id,
                            ext_qos: ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType::DEFAULT,
                            body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                id,
                                wire_expr,
                            }),
                        },
                        Some(res.deref().clone()),
                    );
                }
            } else {
                for sub in &hat!(tables).linkstatepeer_subs {
                    if sub.context.is_some()
                        && sub.matches(res)
                        && (remote_simple_subs(sub, &face.state)
                            || remote_linkstatepeer_subs(tables, sub))
                    {
                        let id = make_sub_id(sub, &mut face.state.clone(), mode);
                        let wire_expr =
                            Resource::decl_key(sub, face, push_declaration_profile(&face.state));
                        send_declare(
                            face,
                            Declare {
                                interest_id,
                                ext_qos: ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType::DEFAULT,
                                body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                    id,
                                    wire_expr,
                                }),
                            },
                            Some(sub.clone()),
                        );
                    }
                }
            }
        } else {
            for sub in &hat!(tables).linkstatepeer_subs {
                if sub.context.is_some()
                    && (remote_simple_subs(sub, &face.state)
                        || remote_linkstatepeer_subs(tables, sub))
                {
                    let id = make_sub_id(sub, &mut face.state.clone(), mode);
                    let wire_expr =
                        Resource::decl_key(sub, face, push_declaration_profile(&face.state));
                    send_declare(
                        face,
                        Declare {
                            interest_id,
                            ext_qos: ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType::DEFAULT,
                            body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                id,
                                wire_expr,
                            }),
                        },
                        Some(sub.clone()),
                    );
                }
            }
        }
    }
}

impl HatPubSubTrait for HatCode {
    fn declare_subscription(
        &self,
        tables: &mut Tables,
        face: &Face,
        id: SubscriberId,
        res: &mut Arc<Resource>,
        sub_info: &SubscriberInfo,
        node_id: NodeId,
        send_declare: &mut SendDeclare,
    ) {
        if face.state.whatami != WhatAmI::Client {
            if let Some(peer) = get_peer(tables, &face.state, node_id) {
                declare_linkstatepeer_subscription(
                    tables,
                    &mut face.state.clone(),
                    res,
                    sub_info,
                    peer,
                    send_declare,
                )
            }
        } else {
            declare_simple_subscription(tables, face, id, res, sub_info, send_declare)
        }
    }

    fn undeclare_subscription(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: SubscriberId,
        res: Option<Arc<Resource>>,
        node_id: NodeId,
        send_declare: &mut SendDeclare,
    ) -> Option<Arc<Resource>> {
        if face.whatami != WhatAmI::Client {
            if let Some(mut res) = res {
                if let Some(peer) = get_peer(tables, face, node_id) {
                    forget_linkstatepeer_subscription(tables, face, &mut res, &peer, send_declare);
                    Some(res)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            forget_simple_subscription(tables, face, id, send_declare)
        }
    }

    fn get_subscriptions(&self, tables: &Tables) -> Vec<(Arc<Resource>, Sources)> {
        // Compute the list of known suscriptions (keys)
        hat!(tables)
            .linkstatepeer_subs
            .iter()
            .map(|s| {
                (
                    s.clone(),
                    // Compute the list of routers, peers and clients that are known
                    // sources of those subscriptions
                    Sources {
                        routers: vec![],
                        peers: Vec::from_iter(res_hat!(s).linkstatepeer_subs.iter().cloned()),
                        clients: s
                            .session_ctxs
                            .values()
                            .filter_map(|f| {
                                (f.face.state.whatami == WhatAmI::Client && f.subs.is_some())
                                    .then_some(f.face.state.zid)
                            })
                            .collect(),
                    },
                )
            })
            .collect()
    }

    fn get_publications(&self, tables: &Tables) -> Vec<(Arc<Resource>, Sources)> {
        let mut result = HashMap::new();
        for face in tables.faces.values() {
            for interest in face_hat!(face.state).remote_interests.values() {
                if interest.options.subscribers() {
                    if let Some(res) = interest.res.as_ref() {
                        let sources = result.entry(res.clone()).or_insert_with(Sources::default);
                        match face.state.whatami {
                            WhatAmI::Router => sources.routers.push(face.state.zid),
                            WhatAmI::Peer => sources.peers.push(face.state.zid),
                            WhatAmI::Client => sources.clients.push(face.state.zid),
                        }
                    }
                }
            }
        }
        result.into_iter().collect()
    }

    fn compute_data_route(
        &self,
        tables: &Tables,
        expr: &mut RoutingExpr,
        source: NodeId,
        source_type: WhatAmI,
    ) -> Arc<Route> {
        #[inline]
        fn insert_faces_for_subs(
            route: &mut Route,
            expr: &RoutingExpr,
            tables: &Tables,
            net: &Network,
            source: NodeId,
            subs: &HashSet<ZenohIdProto>,
        ) {
            if net.trees.len() > source as usize {
                for sub in subs {
                    if let Some(sub_idx) = net.get_idx(sub) {
                        if net.trees[source as usize].directions.len() > sub_idx.index() {
                            if let Some(direction) =
                                net.trees[source as usize].directions[sub_idx.index()]
                            {
                                if net.graph.contains_node(direction) {
                                    if let Some(face) = tables.get_face(&net.graph[direction].zid) {
                                        route.entry(face.state.id).or_insert_with(|| {
                                            let key_expr = Resource::get_best_key(
                                                expr.prefix,
                                                expr.suffix,
                                                face.state.id,
                                            );
                                            (face.clone(), key_expr.to_owned(), source)
                                        });
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

        let mut route = HashMap::new();
        let key_expr = expr.full_expr();
        if key_expr.ends_with('/') {
            return Arc::new(route);
        }
        tracing::trace!(
            "compute_data_route({}, {:?}, {:?})",
            key_expr,
            source,
            source_type
        );
        let key_expr = match OwnedKeyExpr::try_from(key_expr) {
            Ok(ke) => ke,
            Err(e) => {
                tracing::warn!("Invalid KE reached the system: {}", e);
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

            let net = hat!(tables).linkstatepeers_net.as_ref().unwrap();
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
                &res_hat!(mres).linkstatepeer_subs,
            );

            for (sid, context) in &mres.session_ctxs {
                if context.subs.is_some()
                    && (source_type == WhatAmI::Client
                        || context.face.state.whatami == WhatAmI::Client)
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
                mcast_group.state.id,
                (
                    mcast_group.clone(),
                    expr.full_expr().to_string().into(),
                    NodeId::default(),
                ),
            );
        }
        Arc::new(route)
    }

    #[zenoh_macros::unstable]
    fn get_matching_subscriptions(
        &self,
        tables: &Tables,
        key_expr: &KeyExpr<'_>,
    ) -> HashMap<usize, Arc<FaceState>> {
        #[inline]
        fn insert_faces_for_subs(
            route: &mut HashMap<usize, Arc<FaceState>>,
            tables: &Tables,
            net: &Network,
            source: usize,
            subs: &HashSet<ZenohIdProto>,
        ) {
            if net.trees.len() > source {
                for sub in subs {
                    if let Some(sub_idx) = net.get_idx(sub) {
                        if net.trees[source].directions.len() > sub_idx.index() {
                            if let Some(direction) = net.trees[source].directions[sub_idx.index()] {
                                if net.graph.contains_node(direction) {
                                    if let Some(face) = tables.get_face(&net.graph[direction].zid) {
                                        route
                                            .entry(face.state.id)
                                            .or_insert_with(|| face.state.clone());
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

        let mut matching_subscriptions = HashMap::new();
        if key_expr.ends_with('/') {
            return matching_subscriptions;
        }
        tracing::trace!("get_matching_subscriptions({})", key_expr,);

        let res = Resource::get_resource(&tables.root_res, key_expr);
        let matches = res
            .as_ref()
            .and_then(|res| res.context.as_ref())
            .map(|ctx| Cow::from(&ctx.matches))
            .unwrap_or_else(|| Cow::from(Resource::get_matches(tables, key_expr)));

        for mres in matches.iter() {
            let mres = mres.upgrade().unwrap();

            let net = hat!(tables).linkstatepeers_net.as_ref().unwrap();
            insert_faces_for_subs(
                &mut matching_subscriptions,
                tables,
                net,
                net.idx.index(),
                &res_hat!(mres).linkstatepeer_subs,
            );

            for (sid, context) in &mres.session_ctxs {
                if context.subs.is_some() {
                    matching_subscriptions
                        .entry(*sid)
                        .or_insert_with(|| context.face.state.clone());
                }
            }
        }
        matching_subscriptions
    }
}
