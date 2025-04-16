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

use std::{
    ops::Deref,
    sync::{atomic::Ordering, Arc},
};

use petgraph::graph::NodeIndex;
use zenoh_protocol::{
    core::{WhatAmI, ZenohIdProto},
    network::{
        declare::{common::ext::WireExprType, TokenId},
        ext,
        interest::{InterestId, InterestMode},
        Declare, DeclareBody, DeclareToken, UndeclareToken,
    },
};
use zenoh_sync::get_mut_unchecked;

use super::{
    face_hat, face_hat_mut, get_peer, hat, hat_mut, network::Network, push_declaration_profile,
    res_hat, res_hat_mut, HatCode, HatContext, HatFace, HatTables,
};
use crate::net::routing::{
    dispatcher::{
        face::{Face, FaceState},
        interests::RemoteInterest,
        tables::Tables,
    },
    hat::{CurrentFutureTrait, HatTokenTrait, SendDeclare},
    router::{NodeId, Resource, SessionContext},
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
                                body: DeclareBody::DeclareToken(DeclareToken {
                                    id: 0, // Sourced tokens do not use ids
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
fn propagate_simple_token_to(
    tables: &mut Tables,
    dst_face: &mut Face,
    res: &Arc<Resource>,
    src_face: &mut Arc<FaceState>,
    send_declare: &mut SendDeclare,
) {
    if (src_face.id != dst_face.state.id || dst_face.state.zid == tables.zid)
        && !face_hat!(dst_face.state).local_tokens.contains_key(res)
        && dst_face.state.whatami == WhatAmI::Client
    {
        if dst_face.state.whatami != WhatAmI::Client {
            let id = face_hat!(dst_face.state)
                .next_id
                .fetch_add(1, Ordering::SeqCst);
            face_hat_mut!(&mut dst_face.state)
                .local_tokens
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
                    body: DeclareBody::DeclareToken(DeclareToken {
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
                .filter(|i| i.options.tokens() && i.matches(res))
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
                if !face_hat!(dst_face.state).local_tokens.contains_key(res) {
                    let id = face_hat!(dst_face.state)
                        .next_id
                        .fetch_add(1, Ordering::SeqCst);
                    face_hat_mut!(&mut dst_face.state)
                        .local_tokens
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
                            body: DeclareBody::DeclareToken(DeclareToken {
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

fn propagate_simple_token(
    tables: &mut Tables,
    res: &Arc<Resource>,
    src_face: &mut Arc<FaceState>,
    send_declare: &mut SendDeclare,
) {
    for mut dst_face in tables.faces.values().cloned().collect::<Vec<_>>() {
        propagate_simple_token_to(tables, &mut dst_face, res, src_face, send_declare);
    }
}

fn propagate_sourced_token(
    tables: &Tables,
    res: &Arc<Resource>,
    src_face: Option<&Arc<FaceState>>,
    source: &ZenohIdProto,
) {
    let net = hat!(tables).linkstatepeers_net.as_ref().unwrap();
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

fn register_linkstatepeer_token(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    peer: ZenohIdProto,
    send_declare: &mut SendDeclare,
) {
    if !res_hat!(res).linkstatepeer_tokens.contains(&peer) {
        // Register peer liveliness
        {
            res_hat_mut!(res).linkstatepeer_tokens.insert(peer);
            hat_mut!(tables).linkstatepeer_tokens.insert(res.clone());
        }

        // Propagate liveliness to peers
        propagate_sourced_token(tables, res, Some(face), &peer);
    }

    // Propagate liveliness to clients
    propagate_simple_token(tables, res, face, send_declare);
}

fn declare_linkstatepeer_token(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    peer: ZenohIdProto,
    send_declare: &mut SendDeclare,
) {
    register_linkstatepeer_token(tables, face, res, peer, send_declare);
}

fn register_simple_token(_tables: &mut Tables, face: &Face, id: TokenId, res: &mut Arc<Resource>) {
    // Register liveliness
    {
        let res = get_mut_unchecked(res);
        match res.session_ctxs.get_mut(&face.state.id) {
            Some(ctx) => {
                if !ctx.token {
                    get_mut_unchecked(ctx).token = true;
                }
            }
            None => {
                let ctx = res
                    .session_ctxs
                    .entry(face.state.id)
                    .or_insert_with(|| Arc::new(SessionContext::new(face.clone())));
                get_mut_unchecked(ctx).token = true;
            }
        }
    }
    face_hat_mut!(&mut face.state.clone())
        .remote_tokens
        .insert(id, res.clone());
}

fn declare_simple_token(
    tables: &mut Tables,
    face: &Face,
    id: TokenId,
    res: &mut Arc<Resource>,
    send_declare: &mut SendDeclare,
) {
    register_simple_token(tables, face, id, res);
    let zid = tables.zid;
    register_linkstatepeer_token(tables, &mut face.state.clone(), res, zid, send_declare);
}

#[inline]
fn remote_linkstatepeer_tokens(tables: &Tables, res: &Arc<Resource>) -> bool {
    res.context.is_some()
        && res_hat!(res)
            .linkstatepeer_tokens
            .iter()
            .any(|peer| peer != &tables.zid)
}

#[inline]
fn simple_tokens(res: &Arc<Resource>) -> Vec<Face> {
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
fn remote_simple_tokens(tables: &Tables, res: &Arc<Resource>, face: &Arc<FaceState>) -> bool {
    res.session_ctxs
        .values()
        .any(|ctx| (ctx.face.state.id != face.id || face.zid == tables.zid) && ctx.token)
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
                                body: DeclareBody::UndeclareToken(UndeclareToken {
                                    id: 0, // Sourced tokens do not use ids
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

fn propagate_forget_simple_token(
    tables: &mut Tables,
    res: &Arc<Resource>,
    send_declare: &mut SendDeclare,
) {
    for mut face in tables.faces.values().cloned() {
        if let Some(id) = face_hat_mut!(&mut face.state).local_tokens.remove(res) {
            send_declare(
                &face,
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
                Some(res.clone()),
            );
        }
        for res in face_hat!(face.state)
            .local_tokens
            .keys()
            .cloned()
            .collect::<Vec<Arc<Resource>>>()
        {
            if !res.context().matches.iter().any(|m| {
                m.upgrade().is_some_and(|m| {
                    m.context.is_some()
                        && (remote_simple_tokens(tables, &m, &face.state)
                            || remote_linkstatepeer_tokens(tables, &m))
                })
            }) {
                if let Some(id) = face_hat_mut!(&mut face.state).local_tokens.remove(&res) {
                    send_declare(
                        &face,
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
                        Some(res.clone()),
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
    let net = hat!(tables).linkstatepeers_net.as_ref().unwrap();
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
                    res.expr().to_string(),
                    tree_sid.index(),
                    source
                );
            }
        }
        None => tracing::error!(
            "Error propagating forget token {}: cannot get index of {}!",
            res.expr().to_string(),
            source
        ),
    }
}

fn unregister_linkstatepeer_token(
    tables: &mut Tables,
    res: &mut Arc<Resource>,
    peer: &ZenohIdProto,
    send_declare: &mut SendDeclare,
) {
    res_hat_mut!(res)
        .linkstatepeer_tokens
        .retain(|token| token != peer);

    if res_hat!(res).linkstatepeer_tokens.is_empty() {
        hat_mut!(tables)
            .linkstatepeer_tokens
            .retain(|token| !Arc::ptr_eq(token, res));

        propagate_forget_simple_token(tables, res, send_declare);
    }
}

fn undeclare_linkstatepeer_token(
    tables: &mut Tables,
    face: Option<&Arc<FaceState>>,
    res: &mut Arc<Resource>,
    peer: &ZenohIdProto,
    send_declare: &mut SendDeclare,
) {
    if res_hat!(res).linkstatepeer_tokens.contains(peer) {
        unregister_linkstatepeer_token(tables, res, peer, send_declare);
        propagate_forget_sourced_token(tables, res, face, peer);
    }
}

fn forget_linkstatepeer_token(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    peer: &ZenohIdProto,
    send_declare: &mut SendDeclare,
) {
    undeclare_linkstatepeer_token(tables, Some(face), res, peer, send_declare);
}

pub(super) fn undeclare_simple_token(
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

        let mut simple_tokens = simple_tokens(res);
        let linkstatepeer_tokens = remote_linkstatepeer_tokens(tables, res);
        if simple_tokens.is_empty() {
            undeclare_linkstatepeer_token(tables, None, res, &tables.zid.clone(), send_declare);
        }

        if simple_tokens.len() == 1 && !linkstatepeer_tokens {
            let face = &mut simple_tokens[0];
            if face.state.whatami != WhatAmI::Client {
                if let Some(id) = face_hat_mut!(&mut face.state).local_tokens.remove(res) {
                    send_declare(
                        face,
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
                        Some(res.clone()),
                    );
                }
                for res in face_hat!(face.state)
                    .local_tokens
                    .keys()
                    .cloned()
                    .collect::<Vec<Arc<Resource>>>()
                {
                    if !res.context().matches.iter().any(|m| {
                        m.upgrade().is_some_and(|m| {
                            m.context.is_some()
                                && (remote_simple_tokens(tables, &m, &face.state)
                                    || remote_linkstatepeer_tokens(tables, &m))
                        })
                    }) {
                        if let Some(id) = face_hat_mut!(&mut face.state).local_tokens.remove(&res) {
                            send_declare(
                                face,
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
                                Some(res.clone()),
                            );
                        }
                    }
                }
            }
        }
    }
}

fn forget_simple_token(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    id: TokenId,
    send_declare: &mut SendDeclare,
) -> Option<Arc<Resource>> {
    if let Some(mut res) = face_hat_mut!(face).remote_tokens.remove(&id) {
        undeclare_simple_token(tables, face, &mut res, send_declare);
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
        .linkstatepeer_tokens
        .iter()
        .filter(|res| res_hat!(res).linkstatepeer_tokens.contains(node))
        .cloned()
        .collect::<Vec<Arc<Resource>>>()
    {
        unregister_linkstatepeer_token(tables, &mut res, node, send_declare);
        Resource::clean(&mut res)
    }
}

pub(super) fn token_tree_change(tables: &mut Tables, new_clildren: &[Vec<NodeIndex>]) {
    let net = match hat!(tables).linkstatepeers_net.as_ref() {
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

                let tokens_res = &hat!(tables).linkstatepeer_tokens;

                for res in tokens_res {
                    let tokens = &res_hat!(res).linkstatepeer_tokens;
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

#[inline]
fn make_token_id(res: &Arc<Resource>, face: &mut Arc<FaceState>, mode: InterestMode) -> u32 {
    if mode.future() {
        if let Some(id) = face_hat!(face).local_tokens.get(res) {
            *id
        } else {
            let id = face_hat!(face).next_id.fetch_add(1, Ordering::SeqCst);
            face_hat_mut!(face).local_tokens.insert(res.clone(), id);
            id
        }
    } else {
        0
    }
}

pub(crate) fn declare_token_interest(
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
                if hat!(tables).linkstatepeer_tokens.iter().any(|token| {
                    token.context.is_some()
                        && token.matches(res)
                        && (remote_simple_tokens(tables, token, &face.state)
                            || remote_linkstatepeer_tokens(tables, token))
                }) {
                    let id = make_token_id(res, &mut face.state.clone(), mode);
                    let wire_expr =
                        Resource::decl_key(res, face, push_declaration_profile(&face.state));
                    send_declare(
                        face,
                        Declare {
                            interest_id,
                            ext_qos: ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType::DEFAULT,
                            body: DeclareBody::DeclareToken(DeclareToken { id, wire_expr }),
                        },
                        Some(res.deref().clone()),
                    );
                }
            } else {
                for token in &hat!(tables).linkstatepeer_tokens {
                    if token.context.is_some()
                        && token.matches(res)
                        && (remote_simple_tokens(tables, token, &face.state)
                            || remote_linkstatepeer_tokens(tables, token))
                    {
                        let id = make_token_id(token, &mut face.state.clone(), mode);
                        let wire_expr =
                            Resource::decl_key(token, face, push_declaration_profile(&face.state));
                        send_declare(
                            face,
                            Declare {
                                interest_id,
                                ext_qos: ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType::DEFAULT,
                                body: DeclareBody::DeclareToken(DeclareToken { id, wire_expr }),
                            },
                            Some(token.clone()),
                        );
                    }
                }
            }
        } else {
            for token in &hat!(tables).linkstatepeer_tokens {
                if token.context.is_some()
                    && (remote_simple_tokens(tables, token, &face.state)
                        || remote_linkstatepeer_tokens(tables, token))
                {
                    let id = make_token_id(token, &mut face.state.clone(), mode);
                    let wire_expr =
                        Resource::decl_key(token, face, push_declaration_profile(&face.state));
                    send_declare(
                        face,
                        Declare {
                            interest_id,
                            ext_qos: ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType::DEFAULT,
                            body: DeclareBody::DeclareToken(DeclareToken { id, wire_expr }),
                        },
                        Some(token.clone()),
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
        face: &Face,
        id: TokenId,
        res: &mut Arc<Resource>,
        node_id: NodeId,
        _interest_id: Option<InterestId>,
        send_declare: &mut SendDeclare,
    ) {
        if face.state.whatami != WhatAmI::Client {
            if let Some(peer) = get_peer(tables, &face.state, node_id) {
                declare_linkstatepeer_token(
                    tables,
                    &mut face.state.clone(),
                    res,
                    peer,
                    send_declare,
                )
            }
        } else {
            declare_simple_token(tables, face, id, res, send_declare)
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
                    forget_linkstatepeer_token(tables, face, &mut res, &peer, send_declare);
                    Some(res)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            forget_simple_token(tables, face, id, send_declare)
        }
    }
}
