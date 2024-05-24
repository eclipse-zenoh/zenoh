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

use std::sync::{atomic::Ordering, Arc};

use petgraph::graph::NodeIndex;
use zenoh_config::{WhatAmI, ZenohId};
use zenoh_protocol::network::{
    declare::{common::ext::WireExprType, TokenId},
    ext,
    interest::{InterestId, InterestMode},
    Declare, DeclareBody, DeclareFinal, DeclareToken, UndeclareToken,
};
use zenoh_sync::get_mut_unchecked;

use crate::net::routing::{
    dispatcher::{face::FaceState, tables::Tables},
    hat::{CurrentFutureTrait, HatTokenTrait},
    router::{NodeId, Resource, SessionContext},
    RoutingContext,
};

use super::{
    face_hat, face_hat_mut, get_peer, get_router, hat, hat_mut, network::Network, res_hat,
    res_hat_mut, HatCode, HatContext, HatFace, HatTables,
};

#[inline]
fn send_sourced_token_to_net_childs(
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
                        let key_expr = Resource::decl_key(res, &mut someface);

                        someface.primitives.send_declare(RoutingContext::with_expr(
                            Declare {
                                ext_qos: ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType {
                                    node_id: routing_context,
                                },
                                body: DeclareBody::DeclareToken(DeclareToken {
                                    // NOTE(fuzzypixelz): In the original
                                    // subscriber-based liveliness
                                    // implementation, sourced subscriptions
                                    // don't use an id, should this be the same
                                    // for liveliness declarators?
                                    id: 0,
                                    wire_expr: key_expr,
                                }),
                                interest_id: None,
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

#[inline]
fn propagate_simple_token_to(
    tables: &mut Tables,
    dst_face: &mut Arc<FaceState>,
    res: &Arc<Resource>,
    src_face: &mut Arc<FaceState>,
    full_peer_net: bool,
) {
    if (src_face.id != dst_face.id || dst_face.whatami == WhatAmI::Client)
        && !face_hat!(dst_face).local_tokens.contains_key(res)
        && if full_peer_net {
            dst_face.whatami == WhatAmI::Client
        } else {
            dst_face.whatami != WhatAmI::Router
                && (src_face.whatami != WhatAmI::Peer
                    || dst_face.whatami != WhatAmI::Peer
                    || hat!(tables).failover_brokering(src_face.zid, dst_face.zid))
        }
    {
        if dst_face.whatami != WhatAmI::Client {
            let id = face_hat!(dst_face).next_id.fetch_add(1, Ordering::SeqCst);
            face_hat_mut!(dst_face).local_tokens.insert(res.clone(), id);
            let key_expr = Resource::decl_key(res, dst_face);
            dst_face.primitives.send_declare(RoutingContext::with_expr(
                Declare {
                    ext_qos: ext::QoSType::DECLARE,
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::DEFAULT,
                    body: DeclareBody::DeclareToken(DeclareToken {
                        id,
                        wire_expr: key_expr,
                    }),
                    interest_id: None,
                },
                res.expr(),
            ));
        } else {
            let matching_interests = face_hat!(dst_face)
                .remote_token_interests
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
                if !face_hat!(dst_face).local_tokens.contains_key(res) {
                    let id = face_hat!(dst_face).next_id.fetch_add(1, Ordering::SeqCst);
                    face_hat_mut!(dst_face).local_tokens.insert(res.clone(), id);
                    let key_expr = Resource::decl_key(res, dst_face);
                    dst_face.primitives.send_declare(RoutingContext::with_expr(
                        Declare {
                            ext_qos: ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType::DEFAULT,
                            body: DeclareBody::DeclareToken(DeclareToken {
                                id,
                                wire_expr: key_expr,
                            }),
                            interest_id: None,
                        },
                        res.expr(),
                    ));
                }
            }
        }
    }
}

fn propagate_simple_token(tables: &mut Tables, res: &Arc<Resource>, src_face: &mut Arc<FaceState>) {
    let full_peer_net = hat!(tables).full_net(WhatAmI::Peer);
    for mut dst_face in tables
        .faces
        .values()
        .cloned()
        .collect::<Vec<Arc<FaceState>>>()
    {
        propagate_simple_token_to(tables, &mut dst_face, res, src_face, full_peer_net);
    }
}

fn propagate_sourced_token(
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
                send_sourced_token_to_net_childs(
                    tables,
                    net,
                    &net.trees[tree_sid.index()].childs,
                    res,
                    src_face,
                    tree_sid.index() as NodeId,
                );
            } else {
                tracing::trace!(
                    "Propagating liveliness {}: tree for node {} sid:{} not yet ready",
                    res.expr(),
                    tree_sid.index(),
                    source
                );
            }
        }
        None => tracing::error!(
            "Error propagating token {}: cannot get index of {}!",
            res.expr(),
            source
        ),
    }
}

fn register_router_token(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    router: ZenohId,
) {
    if !res_hat!(res).router_tokens.contains(&router) {
        // Register router liveliness
        {
            res_hat_mut!(res).router_tokens.insert(router);
            hat_mut!(tables).router_tokens.insert(res.clone());
        }

        // Propagate liveliness to routers
        propagate_sourced_token(tables, res, Some(face), &router, WhatAmI::Router);
    }
    // Propagate liveliness to peers
    if hat!(tables).full_net(WhatAmI::Peer) && face.whatami != WhatAmI::Peer {
        register_peer_token(tables, face, res, tables.zid)
    }

    // Propagate liveliness to clients
    propagate_simple_token(tables, res, face);
}

fn declare_router_token(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    router: ZenohId,
) {
    register_router_token(tables, face, res, router);
}

fn register_peer_token(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    peer: ZenohId,
) {
    if !res_hat!(res).peer_tokens.contains(&peer) {
        // Register peer liveliness
        {
            res_hat_mut!(res).peer_tokens.insert(peer);
            hat_mut!(tables).peer_tokens.insert(res.clone());
        }

        // Propagate liveliness to peers
        propagate_sourced_token(tables, res, Some(face), &peer, WhatAmI::Peer);
    }
}

fn declare_peer_token(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    peer: ZenohId,
) {
    register_peer_token(tables, face, res, peer);
    let zid = tables.zid;
    register_router_token(tables, face, res, zid);
}

fn register_client_token(
    _tables: &mut Tables,
    face: &mut Arc<FaceState>,
    id: TokenId,
    res: &mut Arc<Resource>,
) {
    // Register liveliness
    {
        let res = get_mut_unchecked(res);
        match res.session_ctxs.get_mut(&face.id) {
            Some(ctx) => {
                if !ctx.token {
                    get_mut_unchecked(ctx).token = true;
                }
            }
            None => {
                let ctx = res
                    .session_ctxs
                    .entry(face.id)
                    .or_insert_with(|| Arc::new(SessionContext::new(face.clone())));
                get_mut_unchecked(ctx).token = true;
            }
        }
    }
    face_hat_mut!(face).remote_tokens.insert(id, res.clone());
}

fn declare_client_token(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    id: TokenId,
    res: &mut Arc<Resource>,
) {
    register_client_token(tables, face, id, res);
    let zid = tables.zid;
    register_router_token(tables, face, res, zid);
}

#[inline]
fn remote_router_tokens(tables: &Tables, res: &Arc<Resource>) -> bool {
    res.context.is_some()
        && res_hat!(res)
            .router_tokens
            .iter()
            .any(|peer| peer != &tables.zid)
}

#[inline]
fn remote_peer_tokens(tables: &Tables, res: &Arc<Resource>) -> bool {
    res.context.is_some()
        && res_hat!(res)
            .peer_tokens
            .iter()
            .any(|peer| peer != &tables.zid)
}

#[inline]
fn client_tokens(res: &Arc<Resource>) -> Vec<Arc<FaceState>> {
    res.session_ctxs
        .values()
        .filter_map(|ctx| {
            if ctx.token {
                Some(ctx.face.clone())
            } else {
                None
            }
        })
        .collect()
}

#[inline]
fn remote_client_tokens(res: &Arc<Resource>, face: &Arc<FaceState>) -> bool {
    res.session_ctxs
        .values()
        .any(|ctx| ctx.face.id != face.id && ctx.token)
}

#[inline]
fn send_forget_sourced_token_to_net_childs(
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
                                body: DeclareBody::UndeclareToken(UndeclareToken {
                                    // NOTE(fuzzypixelz): In the original
                                    // subscriber-based liveliness
                                    // implementation, sourced subscriptions
                                    // don't use an id, should this be the same
                                    // for liveliness declarators?
                                    id: 0,
                                    ext_wire_expr: WireExprType { wire_expr },
                                }),
                                interest_id: None,
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

fn propagate_forget_simple_token(tables: &mut Tables, res: &Arc<Resource>) {
    for mut face in tables.faces.values().cloned() {
        if let Some(id) = face_hat_mut!(&mut face).local_tokens.remove(res) {
            face.primitives.send_declare(RoutingContext::with_expr(
                Declare {
                    ext_qos: ext::QoSType::DECLARE,
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::DEFAULT,
                    body: DeclareBody::UndeclareToken(UndeclareToken {
                        id,
                        ext_wire_expr: WireExprType::null(),
                    }),
                    interest_id: None,
                },
                res.expr(),
            ));
        }
        for res in face_hat!(&mut face)
            .local_tokens
            .keys()
            .cloned()
            .collect::<Vec<Arc<Resource>>>()
        {
            if !res.context().matches.iter().any(|m| {
                m.upgrade().is_some_and(|m| {
                    m.context.is_some()
                        && (remote_client_tokens(&m, &face)
                            || remote_peer_tokens(tables, &m)
                            || remote_router_tokens(tables, &m))
                })
            }) {
                if let Some(id) = face_hat_mut!(&mut face).local_tokens.remove(&res) {
                    face.primitives.send_declare(RoutingContext::with_expr(
                        Declare {
                            ext_qos: ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType::DEFAULT,
                            body: DeclareBody::UndeclareToken(UndeclareToken {
                                id,
                                ext_wire_expr: WireExprType::null(),
                            }),
                            interest_id: None,
                        },
                        res.expr(),
                    ));
                }
            }
        }
    }
}

fn propagate_forget_simple_token_to_peers(tables: &mut Tables, res: &Arc<Resource>) {
    if !hat!(tables).full_net(WhatAmI::Peer)
        && res_hat!(res).router_tokens.len() == 1
        && res_hat!(res).router_tokens.contains(&tables.zid)
    {
        for mut face in tables
            .faces
            .values()
            .cloned()
            .collect::<Vec<Arc<FaceState>>>()
        {
            if face.whatami == WhatAmI::Peer
                && face_hat!(face).local_tokens.contains_key(res)
                && !res.session_ctxs.values().any(|s| {
                    face.zid != s.face.zid
                        && s.token
                        && (s.face.whatami == WhatAmI::Client
                            || (s.face.whatami == WhatAmI::Peer
                                && hat!(tables).failover_brokering(s.face.zid, face.zid)))
                })
            {
                if let Some(id) = face_hat_mut!(&mut face).local_tokens.remove(res) {
                    face.primitives.send_declare(RoutingContext::with_expr(
                        Declare {
                            ext_qos: ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType::DEFAULT,
                            body: DeclareBody::UndeclareToken(UndeclareToken {
                                id,
                                ext_wire_expr: WireExprType::null(),
                            }),
                            interest_id: None,
                        },
                        res.expr(),
                    ));
                }
            }
        }
    }
}

fn propagate_forget_sourced_token(
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
                send_forget_sourced_token_to_net_childs(
                    tables,
                    net,
                    &net.trees[tree_sid.index()].childs,
                    res,
                    src_face,
                    Some(tree_sid.index() as NodeId),
                );
            } else {
                tracing::trace!(
                    "Propagating forget token {}: tree for node {} sid:{} not yet ready",
                    res.expr(),
                    tree_sid.index(),
                    source
                );
            }
        }
        None => tracing::error!(
            "Error propagating forget token {}: cannot get index of {}!",
            res.expr(),
            source
        ),
    }
}

fn unregister_router_token(tables: &mut Tables, res: &mut Arc<Resource>, router: &ZenohId) {
    res_hat_mut!(res)
        .router_tokens
        .retain(|token| token != router);

    if res_hat!(res).router_tokens.is_empty() {
        hat_mut!(tables)
            .router_tokens
            .retain(|token| !Arc::ptr_eq(token, res));

        if hat_mut!(tables).full_net(WhatAmI::Peer) {
            undeclare_peer_token(tables, None, res, &tables.zid.clone());
        }
        propagate_forget_simple_token(tables, res);
    }

    propagate_forget_simple_token_to_peers(tables, res);
}

fn undeclare_router_token(
    tables: &mut Tables,
    face: Option<&Arc<FaceState>>,
    res: &mut Arc<Resource>,
    router: &ZenohId,
) {
    if res_hat!(res).router_tokens.contains(router) {
        unregister_router_token(tables, res, router);
        propagate_forget_sourced_token(tables, res, face, router, WhatAmI::Router);
    }
}

fn forget_router_token(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    router: &ZenohId,
) {
    undeclare_router_token(tables, Some(face), res, router);
}

fn unregister_peer_token(tables: &mut Tables, res: &mut Arc<Resource>, peer: &ZenohId) {
    res_hat_mut!(res).peer_tokens.retain(|token| token != peer);

    if res_hat!(res).peer_tokens.is_empty() {
        hat_mut!(tables)
            .peer_tokens
            .retain(|token| !Arc::ptr_eq(token, res));
    }
}

fn undeclare_peer_token(
    tables: &mut Tables,
    face: Option<&Arc<FaceState>>,
    res: &mut Arc<Resource>,
    peer: &ZenohId,
) {
    if res_hat!(res).peer_tokens.contains(peer) {
        unregister_peer_token(tables, res, peer);
        propagate_forget_sourced_token(tables, res, face, peer, WhatAmI::Peer);
    }
}

fn forget_peer_token(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    peer: &ZenohId,
) {
    undeclare_peer_token(tables, Some(face), res, peer);
    let client_tokens = res.session_ctxs.values().any(|ctx| ctx.token);
    let peer_tokens = remote_peer_tokens(tables, res);
    let zid = tables.zid;
    if !client_tokens && !peer_tokens {
        undeclare_router_token(tables, None, res, &zid);
    }
}

pub(super) fn undeclare_client_token(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
) {
    if !face_hat_mut!(face)
        .remote_tokens
        .values()
        .any(|s| *s == *res)
    {
        if let Some(ctx) = get_mut_unchecked(res).session_ctxs.get_mut(&face.id) {
            get_mut_unchecked(ctx).token = false;
        }

        let mut client_tokens = client_tokens(res);
        let router_tokens = remote_router_tokens(tables, res);
        let peer_tokens = remote_peer_tokens(tables, res);
        if client_tokens.is_empty() && !peer_tokens {
            undeclare_router_token(tables, None, res, &tables.zid.clone());
        } else {
            propagate_forget_simple_token_to_peers(tables, res);
        }

        if client_tokens.len() == 1 && !router_tokens && !peer_tokens {
            let mut face = &mut client_tokens[0];
            if face.whatami != WhatAmI::Client {
                if let Some(id) = face_hat_mut!(face).local_tokens.remove(res) {
                    face.primitives.send_declare(RoutingContext::with_expr(
                        Declare {
                            ext_qos: ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType::DEFAULT,
                            body: DeclareBody::UndeclareToken(UndeclareToken {
                                id,
                                ext_wire_expr: WireExprType::null(),
                            }),
                            interest_id: None,
                        },
                        res.expr(),
                    ));
                }
                for res in face_hat!(face)
                    .local_tokens
                    .keys()
                    .cloned()
                    .collect::<Vec<Arc<Resource>>>()
                {
                    if !res.context().matches.iter().any(|m| {
                        m.upgrade().is_some_and(|m| {
                            m.context.is_some()
                                && (remote_client_tokens(&m, face)
                                    || remote_peer_tokens(tables, &m)
                                    || remote_router_tokens(tables, &m))
                        })
                    }) {
                        if let Some(id) = face_hat_mut!(&mut face).local_tokens.remove(&res) {
                            face.primitives.send_declare(RoutingContext::with_expr(
                                Declare {
                                    ext_qos: ext::QoSType::DECLARE,
                                    ext_tstamp: None,
                                    ext_nodeid: ext::NodeIdType::DEFAULT,
                                    body: DeclareBody::UndeclareToken(UndeclareToken {
                                        id,
                                        ext_wire_expr: WireExprType::null(),
                                    }),
                                    interest_id: None,
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

fn forget_client_token(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    id: TokenId,
) -> Option<Arc<Resource>> {
    if let Some(mut res) = face_hat_mut!(face).remote_tokens.remove(&id) {
        undeclare_client_token(tables, face, &mut res);
        Some(res)
    } else {
        None
    }
}

impl HatTokenTrait for HatCode {
    fn declare_token(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: TokenId,
        res: &mut Arc<Resource>,
        node_id: NodeId,
        _interest_id: Option<InterestId>,
    ) {
        match face.whatami {
            WhatAmI::Router => {
                if let Some(router) = get_router(tables, face, node_id) {
                    declare_router_token(tables, face, res, router)
                }
            }
            WhatAmI::Peer => {
                if hat!(tables).full_net(WhatAmI::Peer) {
                    if let Some(peer) = get_peer(tables, face, node_id) {
                        declare_peer_token(tables, face, res, peer)
                    }
                } else {
                    declare_client_token(tables, face, id, res)
                }
            }
            _ => declare_client_token(tables, face, id, res),
        }
    }

    fn undeclare_token(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: TokenId,
        res: Option<Arc<Resource>>,
        node_id: NodeId,
    ) -> Option<Arc<Resource>> {
        match face.whatami {
            WhatAmI::Router => {
                if let Some(mut res) = res {
                    if let Some(router) = get_router(tables, face, node_id) {
                        forget_router_token(tables, face, &mut res, &router);
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
                            forget_peer_token(tables, face, &mut res, &peer);
                            Some(res)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    forget_client_token(tables, face, id)
                }
            }
            _ => forget_client_token(tables, face, id),
        }
    }

    fn declare_token_interest(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: InterestId,
        res: Option<&mut Arc<Resource>>,
        mode: InterestMode,
        aggregate: bool,
    ) {
        if mode.current() && face.whatami == WhatAmI::Client {
            // NOTE(fuzzypixelz): The pub/sub routing logic only sets the interest_id to Some(..)
            // if mode.future(). For token queries (i.e. if mode.current()) an id is needed.
            let interest_id = Some(id);
            if let Some(res) = res.as_ref() {
                if aggregate {
                    if hat!(tables).router_tokens.iter().any(|token| {
                        token.context.is_some()
                            && token.matches(res)
                            && (remote_client_tokens(token, face)
                                || remote_peer_tokens(tables, token)
                                || remote_router_tokens(tables, token))
                    }) {
                        let id = face_hat!(face).next_id.fetch_add(1, Ordering::SeqCst);
                        face_hat_mut!(face).local_tokens.insert((*res).clone(), id);
                        let wire_expr = Resource::decl_key(res, face);
                        face.primitives.send_declare(RoutingContext::with_expr(
                            Declare {
                                ext_qos: ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType::DEFAULT,
                                body: DeclareBody::DeclareToken(DeclareToken { id, wire_expr }),
                                interest_id,
                            },
                            res.expr(),
                        ));
                    }
                } else {
                    for token in &hat!(tables).router_tokens {
                        if token.context.is_some()
                            && token.matches(res)
                            && (remote_client_tokens(token, face)
                                || remote_peer_tokens(tables, token)
                                || remote_router_tokens(tables, token))
                        {
                            let id = face_hat!(face).next_id.fetch_add(1, Ordering::SeqCst);
                            face_hat_mut!(face).local_tokens.insert(token.clone(), id);
                            let wire_expr = Resource::decl_key(token, face);
                            face.primitives.send_declare(RoutingContext::with_expr(
                                Declare {
                                    ext_qos: ext::QoSType::DECLARE,
                                    ext_tstamp: None,
                                    ext_nodeid: ext::NodeIdType::DEFAULT,
                                    body: DeclareBody::DeclareToken(DeclareToken { id, wire_expr }),
                                    interest_id,
                                },
                                token.expr(),
                            ));
                        }
                    }
                }
            } else {
                for token in &hat!(tables).router_tokens {
                    if token.context.is_some()
                        && (remote_client_tokens(token, face)
                            || remote_peer_tokens(tables, token)
                            || remote_router_tokens(tables, token))
                    {
                        let id = face_hat!(face).next_id.fetch_add(1, Ordering::SeqCst);
                        face_hat_mut!(face).local_tokens.insert(token.clone(), id);
                        let wire_expr = Resource::decl_key(token, face);
                        face.primitives.send_declare(RoutingContext::with_expr(
                            Declare {
                                ext_qos: ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType::DEFAULT,
                                body: DeclareBody::DeclareToken(DeclareToken { id, wire_expr }),
                                interest_id,
                            },
                            token.expr(),
                        ));
                    }
                }
            }

            face.primitives.send_declare(RoutingContext::with_expr(
                Declare {
                    interest_id,
                    ext_qos: ext::QoSType::default(),
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::default(),
                    body: DeclareBody::DeclareFinal(DeclareFinal),
                },
                res.as_ref().map(|res| res.expr()).unwrap_or_default(),
            ));
        }
        if mode.future() {
            face_hat_mut!(face)
                .remote_token_interests
                .insert(id, (res.cloned(), aggregate));
        }
    }

    fn undeclare_token_interest(
        &self,
        _tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: InterestId,
    ) {
        face_hat_mut!(face).remote_token_interests.remove(&id);
    }
}
