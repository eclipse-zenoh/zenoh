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
    sync::{atomic::Ordering, Arc},
};

use petgraph::graph::NodeIndex;
use zenoh_protocol::{
    core::{WhatAmI, ZenohIdProto},
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
    face_hat, face_hat_mut, get_peer, get_router, hat, hat_mut, push_declaration_profile, res_hat,
    res_hat_mut, HatCode, HatContext, HatFace, HatTables,
};
use crate::{
    key_expr::KeyExpr,
    net::{
        protocol::{linkstate::LinkEdgeWeight, network::Network},
        routing::{
            dispatcher::{
                face::FaceState,
                pubsub::SubscriberInfo,
                resource::{NodeId, Resource, SessionContext},
                tables::{Route, RoutingExpr, Tables},
            },
            hat::{
                router::INITIAL_INTEREST_ID, CurrentFutureTrait, HatPubSubTrait, SendDeclare,
                Sources,
            },
            router::{disable_matches_data_routes, RouteBuilder},
            RoutingContext,
        },
    },
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
                Some(mut someface) => {
                    if src_face
                        .map(|src_face| someface.id != src_face.id)
                        .unwrap_or(true)
                    {
                        let push_declaration = push_declaration_profile(tables, &someface);
                        let key_expr = Resource::decl_key(res, &mut someface, push_declaration);

                        someface.primitives.send_declare(RoutingContext::with_expr(
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
                            res.expr().to_string(),
                        ));
                    }
                }
                None => tracing::trace!("Unable to find face for zid {}", net.graph[*child].zid),
            }
        }
    }
}

#[inline]
fn maybe_register_local_subscriber(
    tables: &Tables,
    dst_face: &mut Arc<FaceState>,
    res: &Arc<Resource>,
    initial_interest: Option<InterestId>,
    send_declare: &mut SendDeclare,
) {
    if face_hat!(dst_face).local_subs.contains_simple_resource(res) {
        return;
    }
    let (should_notify, simple_interests) = match initial_interest {
        Some(interest) => (true, HashSet::from([interest])),
        None => face_hat!(dst_face)
            .remote_interests
            .iter()
            .filter(|(_, i)| i.options.subscribers() && i.matches(res))
            .fold(
                (false, HashSet::new()),
                |(_, mut simple_interests), (id, i)| {
                    if !i.options.aggregate() {
                        simple_interests.insert(*id);
                    }
                    (true, simple_interests)
                },
            ),
    };

    if !should_notify {
        return;
    }
    let face_hat_mut = face_hat_mut!(dst_face);
    let (_, subs_to_notify) = face_hat_mut.local_subs.insert_simple_resource(
        res.clone(),
        SubscriberInfo,
        || face_hat_mut.next_id.fetch_add(1, Ordering::SeqCst),
        simple_interests,
    );

    for update in subs_to_notify {
        let key_expr = Resource::decl_key(
            &update.resource,
            dst_face,
            push_declaration_profile(tables, dst_face),
        );
        send_declare(
            &dst_face.primitives,
            RoutingContext::with_expr(
                Declare {
                    interest_id: None,
                    ext_qos: ext::QoSType::DECLARE,
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::DEFAULT,
                    body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                        id: update.id,
                        wire_expr: key_expr.clone(),
                    }),
                },
                update.resource.expr().to_string(),
            ),
        );
    }
}

#[inline]
fn maybe_unregister_local_subscriber(
    face: &mut Arc<FaceState>,
    res: &Arc<Resource>,
    send_ext_wire_expr: bool,
    send_declare: &mut SendDeclare,
) {
    for update in face_hat_mut!(face).local_subs.remove_simple_resource(res) {
        let ext_wire_expr = if send_ext_wire_expr {
            WireExprType {
                wire_expr: Resource::get_best_key(&update.resource, "", face.id),
            }
        } else {
            WireExprType::null()
        };
        send_declare(
            &face.primitives,
            RoutingContext::with_expr(
                Declare {
                    interest_id: None,
                    ext_qos: ext::QoSType::DECLARE,
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::DEFAULT,
                    body: DeclareBody::UndeclareSubscriber(UndeclareSubscriber {
                        id: update.id,
                        ext_wire_expr,
                    }),
                },
                update.resource.expr().to_string(),
            ),
        );
    }
}

#[inline]
fn propagate_simple_subscription_to(
    tables: &mut Tables,
    dst_face: &mut Arc<FaceState>,
    res: &Arc<Resource>,
    _sub_info: &SubscriberInfo,
    src_face: &mut Arc<FaceState>,
    full_peer_net: bool,
    send_declare: &mut SendDeclare,
) {
    if src_face.id != dst_face.id
        && !face_hat!(dst_face).local_subs.contains_simple_resource(res)
        && if full_peer_net {
            dst_face.whatami == WhatAmI::Client
        } else {
            dst_face.whatami != WhatAmI::Router
                && (src_face.whatami != WhatAmI::Peer
                    || dst_face.whatami != WhatAmI::Peer
                    || hat!(tables).failover_brokering(src_face.zid, dst_face.zid))
        }
    {
        maybe_register_local_subscriber(tables, dst_face, res, None, send_declare);
    }
}

fn propagate_simple_subscription(
    tables: &mut Tables,
    res: &Arc<Resource>,
    sub_info: &SubscriberInfo,
    src_face: &mut Arc<FaceState>,
    send_declare: &mut SendDeclare,
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
    net_type: WhatAmI,
) {
    let net = hat!(tables).get_net(net_type).unwrap();
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

fn register_router_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    sub_info: &SubscriberInfo,
    router: ZenohIdProto,
    send_declare: &mut SendDeclare,
) {
    if !res_hat!(res).router_subs.contains(&router) {
        // Register router subscription
        {
            res_hat_mut!(res).router_subs.insert(router);
            hat_mut!(tables).router_subs.insert(res.clone());
        }

        // Propagate subscription to routers
        propagate_sourced_subscription(tables, res, sub_info, Some(face), &router, WhatAmI::Router);
    }
    // Propagate subscription to peers
    if hat!(tables).full_net(WhatAmI::Peer) && face.whatami != WhatAmI::Peer {
        register_linkstatepeer_subscription(tables, face, res, sub_info, tables.zid)
    }

    // Propagate subscription to clients
    propagate_simple_subscription(tables, res, sub_info, face, send_declare);
}

fn declare_router_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    sub_info: &SubscriberInfo,
    router: ZenohIdProto,
    send_declare: &mut SendDeclare,
) {
    register_router_subscription(tables, face, res, sub_info, router, send_declare);
}

fn register_linkstatepeer_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    sub_info: &SubscriberInfo,
    peer: ZenohIdProto,
) {
    if !res_hat!(res).linkstatepeer_subs.contains(&peer) {
        // Register peer subscription
        {
            res_hat_mut!(res).linkstatepeer_subs.insert(peer);
            hat_mut!(tables).linkstatepeer_subs.insert(res.clone());
        }

        // Propagate subscription to peers
        propagate_sourced_subscription(tables, res, sub_info, Some(face), &peer, WhatAmI::Peer);
    }
}

fn declare_linkstatepeer_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    sub_info: &SubscriberInfo,
    peer: ZenohIdProto,
    send_declare: &mut SendDeclare,
) {
    register_linkstatepeer_subscription(tables, face, res, sub_info, peer);
    let propa_sub_info = *sub_info;
    let zid = tables.zid;
    register_router_subscription(tables, face, res, &propa_sub_info, zid, send_declare);
}

fn register_simple_subscription(
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

fn declare_simple_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    id: SubscriberId,
    res: &mut Arc<Resource>,
    sub_info: &SubscriberInfo,
    send_declare: &mut SendDeclare,
) {
    register_simple_subscription(tables, face, id, res, sub_info);
    let zid = tables.zid;
    register_router_subscription(tables, face, res, sub_info, zid, send_declare);
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
fn remote_linkstatepeer_subs(tables: &Tables, res: &Arc<Resource>) -> bool {
    res.context.is_some()
        && res_hat!(res)
            .linkstatepeer_subs
            .iter()
            .any(|peer| peer != &tables.zid)
}

#[inline]
fn simple_subs(res: &Arc<Resource>) -> Vec<Arc<FaceState>> {
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
                Some(mut someface) => {
                    if src_face
                        .map(|src_face| someface.id != src_face.id)
                        .unwrap_or(true)
                    {
                        let push_declaration = push_declaration_profile(tables, &someface);
                        let wire_expr = Resource::decl_key(res, &mut someface, push_declaration);

                        someface.primitives.send_declare(RoutingContext::with_expr(
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
                            res.expr().to_string(),
                        ));
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
        maybe_unregister_local_subscriber(&mut face, res, false, send_declare);
    }
}

fn propagate_forget_simple_subscription_to_peers(
    tables: &mut Tables,
    res: &Arc<Resource>,
    send_declare: &mut SendDeclare,
) {
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
                && face_hat!(face).local_subs.contains_simple_resource(res)
                && !res.session_ctxs.values().any(|s| {
                    face.zid != s.face.zid
                        && s.subs.is_some()
                        && (s.face.whatami == WhatAmI::Client
                            || (s.face.whatami == WhatAmI::Peer
                                && hat!(tables).failover_brokering(s.face.zid, face.zid)))
                })
            {
                maybe_unregister_local_subscriber(&mut face, res, false, send_declare);
            }
        }
    }
}

fn propagate_forget_sourced_subscription(
    tables: &Tables,
    res: &Arc<Resource>,
    src_face: Option<&Arc<FaceState>>,
    source: &ZenohIdProto,
    net_type: WhatAmI,
) {
    let net = hat!(tables).get_net(net_type).unwrap();
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

fn unregister_router_subscription(
    tables: &mut Tables,
    res: &mut Arc<Resource>,
    router: &ZenohIdProto,
    send_declare: &mut SendDeclare,
) {
    res_hat_mut!(res).router_subs.retain(|sub| sub != router);

    if res_hat!(res).router_subs.is_empty() {
        hat_mut!(tables)
            .router_subs
            .retain(|sub| !Arc::ptr_eq(sub, res));

        if hat_mut!(tables).full_net(WhatAmI::Peer) {
            undeclare_linkstatepeer_subscription(tables, None, res, &tables.zid.clone());
        }
        propagate_forget_simple_subscription(tables, res, send_declare);
    }

    propagate_forget_simple_subscription_to_peers(tables, res, send_declare);
}

fn undeclare_router_subscription(
    tables: &mut Tables,
    face: Option<&Arc<FaceState>>,
    res: &mut Arc<Resource>,
    router: &ZenohIdProto,
    send_declare: &mut SendDeclare,
) {
    if res_hat!(res).router_subs.contains(router) {
        unregister_router_subscription(tables, res, router, send_declare);
        propagate_forget_sourced_subscription(tables, res, face, router, WhatAmI::Router);
    }
}

fn forget_router_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    router: &ZenohIdProto,
    send_declare: &mut SendDeclare,
) {
    undeclare_router_subscription(tables, Some(face), res, router, send_declare);
}

fn unregister_peer_subscription(tables: &mut Tables, res: &mut Arc<Resource>, peer: &ZenohIdProto) {
    res_hat_mut!(res)
        .linkstatepeer_subs
        .retain(|sub| sub != peer);

    if res_hat!(res).linkstatepeer_subs.is_empty() {
        hat_mut!(tables)
            .linkstatepeer_subs
            .retain(|sub| !Arc::ptr_eq(sub, res));
    }
}

fn undeclare_linkstatepeer_subscription(
    tables: &mut Tables,
    face: Option<&Arc<FaceState>>,
    res: &mut Arc<Resource>,
    peer: &ZenohIdProto,
) {
    if res_hat!(res).linkstatepeer_subs.contains(peer) {
        unregister_peer_subscription(tables, res, peer);
        propagate_forget_sourced_subscription(tables, res, face, peer, WhatAmI::Peer);
    }
}

fn forget_linkstatepeer_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    peer: &ZenohIdProto,
    send_declare: &mut SendDeclare,
) {
    undeclare_linkstatepeer_subscription(tables, Some(face), res, peer);
    let simple_subs = res.session_ctxs.values().any(|ctx| ctx.subs.is_some());
    let linkstatepeer_subs = remote_linkstatepeer_subs(tables, res);
    let zid = tables.zid;
    if !simple_subs && !linkstatepeer_subs {
        undeclare_router_subscription(tables, None, res, &zid, send_declare);
    }
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
        let router_subs = remote_router_subs(tables, res);
        let linkstatepeer_subs = remote_linkstatepeer_subs(tables, res);
        if simple_subs.is_empty() && !linkstatepeer_subs {
            undeclare_router_subscription(tables, None, res, &tables.zid.clone(), send_declare);
        } else {
            propagate_forget_simple_subscription_to_peers(tables, res, send_declare);
        }

        if simple_subs.len() == 1 && !router_subs && !linkstatepeer_subs {
            maybe_unregister_local_subscriber(&mut simple_subs[0], res, false, send_declare);
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
    net_type: WhatAmI,
    send_declare: &mut SendDeclare,
) {
    match net_type {
        WhatAmI::Router => {
            for mut res in hat!(tables)
                .router_subs
                .iter()
                .filter(|res| res_hat!(res).router_subs.contains(node))
                .cloned()
                .collect::<Vec<Arc<Resource>>>()
            {
                unregister_router_subscription(tables, &mut res, node, send_declare);

                disable_matches_data_routes(tables, &mut res);
                Resource::clean(&mut res)
            }
        }
        WhatAmI::Peer => {
            for mut res in hat!(tables)
                .linkstatepeer_subs
                .iter()
                .filter(|res| res_hat!(res).linkstatepeer_subs.contains(node))
                .cloned()
                .collect::<Vec<Arc<Resource>>>()
            {
                unregister_peer_subscription(tables, &mut res, node);
                let simple_subs = res.session_ctxs.values().any(|ctx| ctx.subs.is_some());
                let linkstatepeer_subs = remote_linkstatepeer_subs(tables, &res);
                if !simple_subs && !linkstatepeer_subs {
                    undeclare_router_subscription(
                        tables,
                        None,
                        &mut res,
                        &tables.zid.clone(),
                        send_declare,
                    );
                }

                disable_matches_data_routes(tables, &mut res);
                Resource::clean(&mut res)
            }
        }
        _ => (),
    }
}

pub(super) fn pubsub_tree_change(
    tables: &mut Tables,
    new_children: &[Vec<NodeIndex>],
    net_type: WhatAmI,
) {
    let net = match hat!(tables).get_net(net_type) {
        Some(net) => net,
        None => {
            tracing::error!("Error accessing net in pubsub_tree_change!");
            return;
        }
    };
    // propagate subs to new children
    for (tree_sid, tree_children) in new_children.iter().enumerate() {
        if !tree_children.is_empty() {
            let tree_idx = NodeIndex::new(tree_sid);
            if net.graph.contains_node(tree_idx) {
                let tree_id = net.graph[tree_idx].zid;

                let subs_res = match net_type {
                    WhatAmI::Router => &hat!(tables).router_subs,
                    _ => &hat!(tables).linkstatepeer_subs,
                };

                for res in subs_res {
                    let subs = match net_type {
                        WhatAmI::Router => &res_hat!(res).router_subs,
                        _ => &res_hat!(res).linkstatepeer_subs,
                    };
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

pub(super) fn pubsub_linkstate_change(
    tables: &mut Tables,
    zid: &ZenohIdProto,
    links: &HashMap<ZenohIdProto, LinkEdgeWeight>,
    send_declare: &mut SendDeclare,
) {
    if let Some(mut src_face) = tables.get_face(zid).cloned() {
        if hat!(tables).router_peers_failover_brokering && src_face.whatami == WhatAmI::Peer {
            let to_forget = face_hat!(src_face)
                .local_subs
                .simple_resources()
                .filter(|res| {
                    let client_subs = res
                        .session_ctxs
                        .values()
                        .any(|ctx| ctx.face.whatami == WhatAmI::Client && ctx.subs.is_some());
                    !remote_router_subs(tables, res)
                        && !client_subs
                        && !res.session_ctxs.values().any(|ctx| {
                            ctx.face.whatami == WhatAmI::Peer
                                && src_face.id != ctx.face.id
                                && HatTables::failover_brokering_to(links, &ctx.face.zid)
                        })
                })
                .cloned()
                .collect::<Vec<Arc<Resource>>>();
            for res in to_forget {
                maybe_unregister_local_subscriber(&mut src_face, &res, true, send_declare);
            }

            for mut dst_face in tables.faces.values().cloned() {
                if src_face.id != dst_face.id
                    && HatTables::failover_brokering_to(links, &dst_face.zid)
                {
                    for res in face_hat!(src_face).remote_subs.values() {
                        maybe_register_local_subscriber(
                            tables,
                            &mut dst_face,
                            res,
                            Some(INITIAL_INTEREST_ID),
                            send_declare,
                        );
                    }
                }
            }
        }
    }
}

fn get_subscribers_matching_resource<'a>(
    tables: &'a Tables,
    face: &Arc<FaceState>,
    res: Option<&'a Arc<Resource>>,
) -> impl Iterator<Item = &'a Arc<Resource>> {
    let face_id = face.id;
    let face_zid = face.zid;
    let face_what_am_i = face.whatami;

    hat!(tables).router_subs.iter().filter(move |sub| {
        sub.context.is_some()
            && res.as_ref().map(|r| sub.matches(r)).unwrap_or(true)
            && (res_hat!(sub).router_subs.iter().any(|r| *r != tables.zid)
                || res_hat!(sub)
                    .linkstatepeer_subs
                    .iter()
                    .any(|r| *r != tables.zid)
                || sub.session_ctxs.values().any(|s| {
                    s.face.id != face_id
                        && s.subs.is_some()
                        && (s.face.whatami == WhatAmI::Client
                            || face_what_am_i == WhatAmI::Client
                            || (s.face.whatami == WhatAmI::Peer
                                && hat!(tables).failover_brokering(s.face.zid, face_zid)))
                }))
    })
}

pub(crate) fn declare_sub_interest(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    interest_id: InterestId,
    res: Option<&mut Arc<Resource>>,
    mode: InterestMode,
    aggregate: bool,
    send_declare: &mut SendDeclare,
) {
    let res = res.map(|r| r.clone());
    let mut matching_subs = get_subscribers_matching_resource(tables, face, res.as_ref());

    if aggregate && (mode.current() || mode.future()) {
        if let Some(aggregated_res) = &res {
            let (resource_id, sub_info) = if mode.future() {
                let face_hat_mut = face_hat_mut!(face);
                for sub in matching_subs {
                    face_hat_mut.local_subs.insert_simple_resource(
                        sub.clone(),
                        SubscriberInfo,
                        || face_hat_mut.next_id.fetch_add(1, Ordering::SeqCst),
                        HashSet::new(),
                    );
                }
                let face_hat_mut = face_hat_mut!(face);
                face_hat_mut.local_subs.insert_aggregated_resource(
                    aggregated_res.clone(),
                    || face_hat_mut.next_id.fetch_add(1, Ordering::SeqCst),
                    HashSet::from_iter([interest_id]),
                )
            } else {
                (0, matching_subs.next().map(|_| SubscriberInfo))
            };
            if mode.current() && sub_info.is_some() {
                // send declare only if there is at least one resource matching the aggregate
                let wire_expr = Resource::decl_key(
                    aggregated_res,
                    face,
                    push_declaration_profile(tables, face),
                );
                send_declare(
                    &face.primitives,
                    RoutingContext::with_expr(
                        Declare {
                            interest_id: Some(interest_id),
                            ext_qos: ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType::DEFAULT,
                            body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                id: resource_id,
                                wire_expr,
                            }),
                        },
                        aggregated_res.expr().to_string(),
                    ),
                );
            }
        }
    } else if !aggregate && mode.current() {
        for sub in matching_subs {
            let resource_id = if mode.future() {
                let face_hat_mut = face_hat_mut!(face);
                face_hat_mut
                    .local_subs
                    .insert_simple_resource(
                        sub.clone(),
                        SubscriberInfo,
                        || face_hat_mut.next_id.fetch_add(1, Ordering::SeqCst),
                        HashSet::from([interest_id]),
                    )
                    .0
            } else {
                0
            };
            let wire_expr = Resource::decl_key(sub, face, push_declaration_profile(tables, face));
            send_declare(
                &face.primitives,
                RoutingContext::with_expr(
                    Declare {
                        interest_id: Some(interest_id),
                        ext_qos: ext::QoSType::DECLARE,
                        ext_tstamp: None,
                        ext_nodeid: ext::NodeIdType::DEFAULT,
                        body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                            id: resource_id,
                            wire_expr,
                        }),
                    },
                    sub.expr().to_string(),
                ),
            );
        }
    }
}

impl HatPubSubTrait for HatCode {
    fn declare_subscription(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: SubscriberId,
        res: &mut Arc<Resource>,
        sub_info: &SubscriberInfo,
        node_id: NodeId,
        send_declare: &mut SendDeclare,
    ) {
        match face.whatami {
            WhatAmI::Router => {
                if let Some(router) = get_router(tables, face, node_id) {
                    declare_router_subscription(tables, face, res, sub_info, router, send_declare)
                }
            }
            WhatAmI::Peer => {
                if hat!(tables).full_net(WhatAmI::Peer) {
                    if let Some(peer) = get_peer(tables, face, node_id) {
                        declare_linkstatepeer_subscription(
                            tables,
                            face,
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
            _ => declare_simple_subscription(tables, face, id, res, sub_info, send_declare),
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
        match face.whatami {
            WhatAmI::Router => {
                if let Some(mut res) = res {
                    if let Some(router) = get_router(tables, face, node_id) {
                        forget_router_subscription(tables, face, &mut res, &router, send_declare);
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
                            forget_linkstatepeer_subscription(
                                tables,
                                face,
                                &mut res,
                                &peer,
                                send_declare,
                            );
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
            _ => forget_simple_subscription(tables, face, id, send_declare),
        }
    }

    fn get_subscriptions(&self, tables: &Tables) -> Vec<(Arc<Resource>, Sources)> {
        // Compute the list of known subscriptions (keys)
        hat!(tables)
            .router_subs
            .iter()
            .map(|s| {
                // Compute the list of routers, peers and clients that are known
                // sources of those subscriptions
                let routers = Vec::from_iter(res_hat!(s).router_subs.iter().cloned());
                let mut peers = if hat!(tables).full_net(WhatAmI::Peer) {
                    Vec::from_iter(res_hat!(s).linkstatepeer_subs.iter().cloned())
                } else {
                    vec![]
                };
                let mut clients = vec![];
                for ctx in s
                    .session_ctxs
                    .values()
                    .filter(|ctx| ctx.subs.is_some() && !ctx.face.is_local)
                {
                    match ctx.face.whatami {
                        WhatAmI::Router => (),
                        WhatAmI::Peer => {
                            if !hat!(tables).full_net(WhatAmI::Peer) {
                                peers.push(ctx.face.zid);
                            }
                        }
                        WhatAmI::Client => clients.push(ctx.face.zid),
                    }
                }
                (
                    s.clone(),
                    Sources {
                        routers,
                        peers,
                        clients,
                    },
                )
            })
            .collect()
    }

    fn get_publications(&self, tables: &Tables) -> Vec<(Arc<Resource>, Sources)> {
        let mut result = HashMap::new();
        for face in tables.faces.values() {
            for interest in face_hat!(face).remote_interests.values() {
                if interest.options.subscribers() {
                    if let Some(res) = interest.res.as_ref() {
                        let sources = result.entry(res.clone()).or_insert_with(Sources::default);
                        let whatami = if face.is_local {
                            tables.whatami
                        } else {
                            face.whatami
                        };
                        match whatami {
                            WhatAmI::Router => sources.routers.push(face.zid),
                            WhatAmI::Peer => sources.peers.push(face.zid),
                            WhatAmI::Client => sources.clients.push(face.zid),
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
        expr: &RoutingExpr,
        source: NodeId,
        source_type: WhatAmI,
    ) -> Arc<Route> {
        #[inline]
        fn insert_faces_for_subs(
            route: &mut RouteBuilder,
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
                                        route.insert(face.id, || {
                                            let wire_expr = expr.get_best_key(face.id);
                                            (face.clone(), wire_expr.to_owned(), source)
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

        let mut route = RouteBuilder::new();
        let Some(key_expr) = expr.key_expr() else {
            return Arc::new(route.build());
        };
        tracing::trace!(
            "compute_data_route({}, {:?}, {:?})",
            key_expr,
            source,
            source_type
        );
        let matches = expr
            .resource()
            .as_ref()
            .and_then(|res| res.context.as_ref())
            .map(|ctx| Cow::from(&ctx.matches))
            .unwrap_or_else(|| Cow::from(Resource::get_matches(tables, key_expr)));

        let master = !hat!(tables).full_net(WhatAmI::Peer)
            || *hat!(tables).elect_router(&tables.zid, key_expr, hat!(tables).shared_nodes.iter())
                == tables.zid;

        for mres in matches.iter() {
            let mres = mres.upgrade().unwrap();

            if master || source_type == WhatAmI::Router {
                let net = hat!(tables).routers_net.as_ref().unwrap();
                let router_source = match source_type {
                    WhatAmI::Router => source,
                    _ => net.idx.index() as NodeId,
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
                let net = hat!(tables).linkstatepeers_net.as_ref().unwrap();
                let peer_source = match source_type {
                    WhatAmI::Peer => source,
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
            }

            if master || source_type == WhatAmI::Router {
                for (sid, context) in &mres.session_ctxs {
                    if context.subs.is_some() && context.face.whatami != WhatAmI::Router {
                        route.insert(*sid, || {
                            let wire_expr = expr.get_best_key(*sid);
                            (
                                context.face.clone(),
                                wire_expr.to_owned(),
                                NodeId::default(),
                            )
                        });
                    }
                }
            }
        }
        for mcast_group in &tables.mcast_groups {
            route.insert(mcast_group.id, || {
                (
                    mcast_group.clone(),
                    key_expr.to_string().into(),
                    NodeId::default(),
                )
            });
        }
        Arc::new(route.build())
    }

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
                                        route.entry(face.id).or_insert_with(|| face.clone());
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

        tracing::trace!("get_matching_subscriptions({})", key_expr,);

        let res = Resource::get_resource(&tables.root_res, key_expr);
        let matches = res
            .as_ref()
            .and_then(|res| res.context.as_ref())
            .map(|ctx| Cow::from(&ctx.matches))
            .unwrap_or_else(|| Cow::from(Resource::get_matches(tables, key_expr)));

        let master = !hat!(tables).full_net(WhatAmI::Peer)
            || *hat!(tables).elect_router(&tables.zid, key_expr, hat!(tables).shared_nodes.iter())
                == tables.zid;

        for mres in matches.iter() {
            let mres = mres.upgrade().unwrap();

            if master {
                let net = hat!(tables).routers_net.as_ref().unwrap();
                insert_faces_for_subs(
                    &mut matching_subscriptions,
                    tables,
                    net,
                    net.idx.index(),
                    &res_hat!(mres).router_subs,
                );
            }

            if hat!(tables).full_net(WhatAmI::Peer) {
                let net = hat!(tables).linkstatepeers_net.as_ref().unwrap();
                insert_faces_for_subs(
                    &mut matching_subscriptions,
                    tables,
                    net,
                    net.idx.index(),
                    &res_hat!(mres).linkstatepeer_subs,
                );
            }

            if master {
                for (sid, context) in &mres.session_ctxs {
                    if context.subs.is_some() && context.face.whatami != WhatAmI::Router {
                        matching_subscriptions
                            .entry(*sid)
                            .or_insert_with(|| context.face.clone());
                    }
                }
            }
        }
        matching_subscriptions
    }
}
