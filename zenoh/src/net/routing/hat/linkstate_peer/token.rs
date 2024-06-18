//
// Copyright (c) 2024 ZettaScale Technology
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
use zenoh_protocol::{
    core::{WhatAmI, ZenohIdProto},
    network::{
        declare::{common::ext::WireExprType, TokenId},
        ext,
        interest::{InterestId, InterestMode, InterestOptions},
        Declare, DeclareBody, DeclareToken, UndeclareToken,
    },
};
use zenoh_sync::get_mut_unchecked;

use super::{
    face_hat, face_hat_mut, get_peer, hat, hat_mut, network::Network, res_hat, res_hat_mut,
    HatCode, HatContext, HatFace, HatTables,
};
use crate::net::routing::{
    dispatcher::{face::FaceState, tables::Tables},
    hat::{CurrentFutureTrait, HatTokenTrait, SendDeclare},
    router::{NodeId, Resource, SessionContext},
    RoutingContext,
};

#[inline]
fn send_sourced_token_to_net_clildren(
    tables: &Tables,
    net: &Network,
    clildren: &[NodeIndex],
    res: &Arc<Resource>,
    src_face: Option<&Arc<FaceState>>,
    routing_context: NodeId,
) {
    for child in clildren {
        if net.graph.contains_node(*child) {
            match tables.get_face(&net.graph[*child].zid).cloned() {
                Some(mut someface) => {
                    if src_face
                        .map(|src_face| someface.id != src_face.id)
                        .unwrap_or(true)
                    {
                        let key_expr = Resource::decl_key(res, &mut someface);

                        someface.primitives.send_declare(RoutingContext::with_expr(
                            Declare {
                                interest_id: None,
                                ext_qos: ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType {
                                    node_id: routing_context,
                                },
                                body: DeclareBody::DeclareToken(DeclareToken {
                                    id: 0, // Sourced tokens do not use ids
                                    wire_expr: key_expr,
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

#[inline]
fn propagate_simple_token_to(
    _tables: &mut Tables,
    dst_face: &mut Arc<FaceState>,
    res: &Arc<Resource>,
    _src_face: &mut Arc<FaceState>,
    send_declare: &mut SendDeclare,
) {
    if !face_hat!(dst_face).local_tokens.contains_key(res) && dst_face.whatami == WhatAmI::Client {
        if dst_face.whatami != WhatAmI::Client {
            let id = face_hat!(dst_face).next_id.fetch_add(1, Ordering::SeqCst);
            face_hat_mut!(dst_face).local_tokens.insert(res.clone(), id);
            let key_expr = Resource::decl_key(res, dst_face);
            send_declare(
                &dst_face.primitives,
                RoutingContext::with_expr(
                    Declare {
                        interest_id: None,
                        ext_qos: ext::QoSType::DECLARE,
                        ext_tstamp: None,
                        ext_nodeid: ext::NodeIdType::DEFAULT,
                        body: DeclareBody::DeclareToken(DeclareToken {
                            id,
                            wire_expr: key_expr,
                        }),
                    },
                    res.expr(),
                ),
            );
        } else {
            let matching_interests = face_hat!(dst_face)
                .remote_interests
                .values()
                .filter(|(r, o)| o.tokens() && r.as_ref().map(|r| r.matches(res)).unwrap_or(true))
                .cloned()
                .collect::<Vec<(Option<Arc<Resource>>, InterestOptions)>>();

            for (int_res, options) in matching_interests {
                let res = if options.aggregate() {
                    int_res.as_ref().unwrap_or(res)
                } else {
                    res
                };
                if !face_hat!(dst_face).local_tokens.contains_key(res) {
                    let id = face_hat!(dst_face).next_id.fetch_add(1, Ordering::SeqCst);
                    face_hat_mut!(dst_face).local_tokens.insert(res.clone(), id);
                    let key_expr = Resource::decl_key(res, dst_face);
                    send_declare(
                        &dst_face.primitives,
                        RoutingContext::with_expr(
                            Declare {
                                interest_id: None,
                                ext_qos: ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType::DEFAULT,
                                body: DeclareBody::DeclareToken(DeclareToken {
                                    id,
                                    wire_expr: key_expr,
                                }),
                            },
                            res.expr(),
                        ),
                    );
                }
            }
        }
    }
}

fn propagate_simple_token(
    tables: &mut Tables,
    res: &Arc<Resource>,
    src_face: &mut Arc<FaceState>,
    send_declare: &mut SendDeclare,
) {
    for mut dst_face in tables
        .faces
        .values()
        .cloned()
        .collect::<Vec<Arc<FaceState>>>()
    {
        propagate_simple_token_to(tables, &mut dst_face, res, src_face, send_declare);
    }
}

fn propagate_sourced_token(
    tables: &Tables,
    res: &Arc<Resource>,
    src_face: Option<&Arc<FaceState>>,
    source: &ZenohIdProto,
) {
    let net = hat!(tables).peers_net.as_ref().unwrap();
    match net.get_idx(source) {
        Some(tree_sid) => {
            if net.trees.len() > tree_sid.index() {
                send_sourced_token_to_net_clildren(
                    tables,
                    net,
                    &net.trees[tree_sid.index()].children,
                    res,
                    src_face,
                    tree_sid.index() as NodeId,
                );
            } else {
                tracing::trace!(
                    "Propagating token {}: tree for node {} sid:{} not yet ready",
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

fn register_peer_token(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    peer: ZenohIdProto,
    send_declare: &mut SendDeclare,
) {
    if !res_hat!(res).peer_tokens.contains(&peer) {
        // Register peer liveliness
        {
            res_hat_mut!(res).peer_tokens.insert(peer);
            hat_mut!(tables).peer_tokens.insert(res.clone());
        }

        // Propagate liveliness to peers
        propagate_sourced_token(tables, res, Some(face), &peer);
    }

    // Propagate liveliness to clients
    propagate_simple_token(tables, res, face, send_declare);
}

fn declare_peer_token(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    peer: ZenohIdProto,
    send_declare: &mut SendDeclare,
) {
    register_peer_token(tables, face, res, peer, send_declare);
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
    send_declare: &mut SendDeclare,
) {
    register_client_token(tables, face, id, res);
    let zid = tables.zid;
    register_peer_token(tables, face, res, zid, send_declare);
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
fn send_forget_sourced_token_to_net_clildren(
    tables: &Tables,
    net: &Network,
    clildren: &[NodeIndex],
    res: &Arc<Resource>,
    src_face: Option<&Arc<FaceState>>,
    routing_context: Option<NodeId>,
) {
    for child in clildren {
        if net.graph.contains_node(*child) {
            match tables.get_face(&net.graph[*child].zid).cloned() {
                Some(mut someface) => {
                    if src_face
                        .map(|src_face| someface.id != src_face.id)
                        .unwrap_or(true)
                    {
                        let wire_expr = Resource::decl_key(res, &mut someface);

                        someface.primitives.send_declare(RoutingContext::with_expr(
                            Declare {
                                interest_id: None,
                                ext_qos: ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType {
                                    node_id: routing_context.unwrap_or(0),
                                },
                                body: DeclareBody::UndeclareToken(UndeclareToken {
                                    id: 0, // Sourced tokens do not use ids
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

fn propagate_forget_simple_token(
    tables: &mut Tables,
    res: &Arc<Resource>,
    send_declare: &mut SendDeclare,
) {
    for mut face in tables.faces.values().cloned() {
        if let Some(id) = face_hat_mut!(&mut face).local_tokens.remove(res) {
            send_declare(
                &face.primitives,
                RoutingContext::with_expr(
                    Declare {
                        interest_id: None,
                        ext_qos: ext::QoSType::DECLARE,
                        ext_tstamp: None,
                        ext_nodeid: ext::NodeIdType::DEFAULT,
                        body: DeclareBody::UndeclareToken(UndeclareToken {
                            id,
                            ext_wire_expr: WireExprType::null(),
                        }),
                    },
                    res.expr(),
                ),
            );
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
                        && (remote_client_tokens(&m, &face) || remote_peer_tokens(tables, &m))
                })
            }) {
                if let Some(id) = face_hat_mut!(&mut face).local_tokens.remove(&res) {
                    send_declare(
                        &face.primitives,
                        RoutingContext::with_expr(
                            Declare {
                                interest_id: None,
                                ext_qos: ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType::DEFAULT,
                                body: DeclareBody::UndeclareToken(UndeclareToken {
                                    id,
                                    ext_wire_expr: WireExprType::null(),
                                }),
                            },
                            res.expr(),
                        ),
                    );
                }
            }
        }
    }
}

fn propagate_forget_sourced_token(
    tables: &Tables,
    res: &Arc<Resource>,
    src_face: Option<&Arc<FaceState>>,
    source: &ZenohIdProto,
) {
    let net = hat!(tables).peers_net.as_ref().unwrap();
    match net.get_idx(source) {
        Some(tree_sid) => {
            if net.trees.len() > tree_sid.index() {
                send_forget_sourced_token_to_net_clildren(
                    tables,
                    net,
                    &net.trees[tree_sid.index()].children,
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

fn unregister_peer_token(
    tables: &mut Tables,
    res: &mut Arc<Resource>,
    peer: &ZenohIdProto,
    send_declare: &mut SendDeclare,
) {
    res_hat_mut!(res).peer_tokens.retain(|token| token != peer);

    if res_hat!(res).peer_tokens.is_empty() {
        hat_mut!(tables)
            .peer_tokens
            .retain(|token| !Arc::ptr_eq(token, res));

        propagate_forget_simple_token(tables, res, send_declare);
    }
}

fn undeclare_peer_token(
    tables: &mut Tables,
    face: Option<&Arc<FaceState>>,
    res: &mut Arc<Resource>,
    peer: &ZenohIdProto,
    send_declare: &mut SendDeclare,
) {
    if res_hat!(res).peer_tokens.contains(peer) {
        unregister_peer_token(tables, res, peer, send_declare);
        propagate_forget_sourced_token(tables, res, face, peer);
    }
}

fn forget_peer_token(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    peer: &ZenohIdProto,
    send_declare: &mut SendDeclare,
) {
    undeclare_peer_token(tables, Some(face), res, peer, send_declare);
}

pub(super) fn undeclare_client_token(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    send_declare: &mut SendDeclare,
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
        let peer_tokens = remote_peer_tokens(tables, res);
        if client_tokens.is_empty() {
            undeclare_peer_token(tables, None, res, &tables.zid.clone(), send_declare);
        }

        if client_tokens.len() == 1 && !peer_tokens {
            let mut face = &mut client_tokens[0];
            if face.whatami != WhatAmI::Client {
                if let Some(id) = face_hat_mut!(face).local_tokens.remove(res) {
                    send_declare(
                        &face.primitives,
                        RoutingContext::with_expr(
                            Declare {
                                interest_id: None,
                                ext_qos: ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType::DEFAULT,
                                body: DeclareBody::UndeclareToken(UndeclareToken {
                                    id,
                                    ext_wire_expr: WireExprType::null(),
                                }),
                            },
                            res.expr(),
                        ),
                    );
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
                                    || remote_peer_tokens(tables, &m))
                        })
                    }) {
                        if let Some(id) = face_hat_mut!(&mut face).local_tokens.remove(&res) {
                            send_declare(
                                &face.primitives,
                                RoutingContext::with_expr(
                                    Declare {
                                        interest_id: None,
                                        ext_qos: ext::QoSType::DECLARE,
                                        ext_tstamp: None,
                                        ext_nodeid: ext::NodeIdType::DEFAULT,
                                        body: DeclareBody::UndeclareToken(UndeclareToken {
                                            id,
                                            ext_wire_expr: WireExprType::null(),
                                        }),
                                    },
                                    res.expr(),
                                ),
                            );
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
    send_declare: &mut SendDeclare,
) -> Option<Arc<Resource>> {
    if let Some(mut res) = face_hat_mut!(face).remote_tokens.remove(&id) {
        undeclare_client_token(tables, face, &mut res, send_declare);
        Some(res)
    } else {
        None
    }
}

pub(super) fn token_remove_node(
    tables: &mut Tables,
    node: &ZenohIdProto,
    send_declare: &mut SendDeclare,
) {
    for mut res in hat!(tables)
        .peer_tokens
        .iter()
        .filter(|res| res_hat!(res).peer_tokens.contains(node))
        .cloned()
        .collect::<Vec<Arc<Resource>>>()
    {
        unregister_peer_token(tables, &mut res, node, send_declare);
        Resource::clean(&mut res)
    }
}

pub(super) fn token_tree_change(tables: &mut Tables, new_clildren: &[Vec<NodeIndex>]) {
    let net = match hat!(tables).peers_net.as_ref() {
        Some(net) => net,
        None => {
            tracing::error!("Error accessing peers_net in token_tree_change!");
            return;
        }
    };
    // propagate tokens to new clildren
    for (tree_sid, tree_clildren) in new_clildren.iter().enumerate() {
        if !tree_clildren.is_empty() {
            let tree_idx = NodeIndex::new(tree_sid);
            if net.graph.contains_node(tree_idx) {
                let tree_id = net.graph[tree_idx].zid;

                let tokens_res = &hat!(tables).peer_tokens;

                for res in tokens_res {
                    let tokens = &res_hat!(res).peer_tokens;
                    for token in tokens {
                        if *token == tree_id {
                            send_sourced_token_to_net_clildren(
                                tables,
                                net,
                                tree_clildren,
                                res,
                                None,
                                tree_sid as NodeId,
                            );
                        }
                    }
                }
            }
        }
    }
}

pub(crate) fn declare_token_interest(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    id: InterestId,
    res: Option<&mut Arc<Resource>>,
    mode: InterestMode,
    aggregate: bool,
    send_declare: &mut SendDeclare,
) {
    if mode.current() && face.whatami == WhatAmI::Client {
        let interest_id = (!mode.future()).then_some(id);
        if let Some(res) = res.as_ref() {
            if aggregate {
                if hat!(tables).peer_tokens.iter().any(|token| {
                    token.context.is_some()
                        && token.matches(res)
                        && (remote_client_tokens(token, face) || remote_peer_tokens(tables, token))
                }) {
                    let id = if mode.future() {
                        let id = face_hat!(face).next_id.fetch_add(1, Ordering::SeqCst);
                        face_hat_mut!(face).local_tokens.insert((*res).clone(), id);
                        id
                    } else {
                        0
                    };
                    let wire_expr = Resource::decl_key(res, face);
                    send_declare(
                        &face.primitives,
                        RoutingContext::with_expr(
                            Declare {
                                interest_id,
                                ext_qos: ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType::DEFAULT,
                                body: DeclareBody::DeclareToken(DeclareToken { id, wire_expr }),
                            },
                            res.expr(),
                        ),
                    );
                }
            } else {
                for token in &hat!(tables).peer_tokens {
                    if token.context.is_some()
                        && token.matches(res)
                        && (remote_client_tokens(token, face) || remote_peer_tokens(tables, token))
                    {
                        let id = if mode.future() {
                            let id = face_hat!(face).next_id.fetch_add(1, Ordering::SeqCst);
                            face_hat_mut!(face).local_tokens.insert(token.clone(), id);
                            id
                        } else {
                            0
                        };
                        let wire_expr = Resource::decl_key(token, face);
                        send_declare(
                            &face.primitives,
                            RoutingContext::with_expr(
                                Declare {
                                    interest_id,
                                    ext_qos: ext::QoSType::DECLARE,
                                    ext_tstamp: None,
                                    ext_nodeid: ext::NodeIdType::DEFAULT,
                                    body: DeclareBody::DeclareToken(DeclareToken { id, wire_expr }),
                                },
                                token.expr(),
                            ),
                        );
                    }
                }
            }
        } else {
            for token in &hat!(tables).peer_tokens {
                if token.context.is_some()
                    && (remote_client_tokens(token, face) || remote_peer_tokens(tables, token))
                {
                    let id = if mode.future() {
                        let id = face_hat!(face).next_id.fetch_add(1, Ordering::SeqCst);
                        face_hat_mut!(face).local_tokens.insert(token.clone(), id);
                        id
                    } else {
                        0
                    };
                    let wire_expr = Resource::decl_key(token, face);
                    send_declare(
                        &face.primitives,
                        RoutingContext::with_expr(
                            Declare {
                                interest_id,
                                ext_qos: ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType::DEFAULT,
                                body: DeclareBody::DeclareToken(DeclareToken { id, wire_expr }),
                            },
                            token.expr(),
                        ),
                    );
                }
            }
        }
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
        send_declare: &mut SendDeclare,
    ) {
        if face.whatami != WhatAmI::Client {
            if let Some(peer) = get_peer(tables, face, node_id) {
                declare_peer_token(tables, face, res, peer, send_declare)
            }
        } else {
            declare_client_token(tables, face, id, res, send_declare)
        }
    }

    fn undeclare_token(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: TokenId,
        res: Option<Arc<Resource>>,
        node_id: NodeId,
        send_declare: &mut SendDeclare,
    ) -> Option<Arc<Resource>> {
        if face.whatami != WhatAmI::Client {
            if let Some(mut res) = res {
                if let Some(peer) = get_peer(tables, face, node_id) {
                    forget_peer_token(tables, face, &mut res, &peer, send_declare);
                    Some(res)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            forget_client_token(tables, face, id, send_declare)
        }
    }
}
