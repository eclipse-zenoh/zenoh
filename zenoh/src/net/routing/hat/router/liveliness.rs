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
    ext, Declare, DeclareBody, DeclareToken, UndeclareToken,
};
use zenoh_sync::get_mut_unchecked;

use crate::net::routing::{
    dispatcher::{face::FaceState, tables::Tables},
    hat::HatLivelinessTrait,
    router::{NodeId, Resource, SessionContext},
    RoutingContext, PREFIX_LIVELINESS,
};

use super::{
    face_hat, face_hat_mut, get_peer, get_router, hat, hat_mut, network::Network, res_hat,
    res_hat_mut, HatCode, HatContext, HatFace, HatTables,
};

#[inline]
fn send_sourced_liveliness_to_net_childs(
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
fn propagate_simple_liveliness_to(
    tables: &mut Tables,
    dst_face: &mut Arc<FaceState>,
    res: &Arc<Resource>,
    src_face: &mut Arc<FaceState>,
    full_peer_net: bool,
) {
    if (src_face.id != dst_face.id
        || (dst_face.whatami == WhatAmI::Client && res.expr().starts_with(PREFIX_LIVELINESS)))
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
                        },
                        res.expr(),
                    ));
                }
            }
        }
    }
}

fn propagate_simple_liveliness(
    tables: &mut Tables,
    res: &Arc<Resource>,
    src_face: &mut Arc<FaceState>,
) {
    let full_peer_net = hat!(tables).full_net(WhatAmI::Peer);
    for mut dst_face in tables
        .faces
        .values()
        .cloned()
        .collect::<Vec<Arc<FaceState>>>()
    {
        propagate_simple_liveliness_to(tables, &mut dst_face, res, src_face, full_peer_net);
    }
}

fn propagate_sourced_liveliness(
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
                send_sourced_liveliness_to_net_childs(
                    tables,
                    net,
                    &net.trees[tree_sid.index()].childs,
                    res,
                    src_face,
                    tree_sid.index() as NodeId,
                );
            } else {
                log::trace!(
                    "Propagating liveliness {}: tree for node {} sid:{} not yet ready",
                    res.expr(),
                    tree_sid.index(),
                    source
                );
            }
        }
        None => log::error!(
            "Error propagating token {}: cannot get index of {}!",
            res.expr(),
            source
        ),
    }
}

fn register_router_liveliness(
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
        propagate_sourced_liveliness(tables, res, Some(face), &router, WhatAmI::Router);
    }
    // Propagate liveliness to peers
    if hat!(tables).full_net(WhatAmI::Peer) && face.whatami != WhatAmI::Peer {
        register_peer_liveliness(tables, face, res, tables.zid)
    }

    // Propagate liveliness to clients
    propagate_simple_liveliness(tables, res, face);
}

fn declare_router_liveliness(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    router: ZenohId,
) {
    register_router_liveliness(tables, face, res, router);
}

fn register_peer_liveliness(
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
        propagate_sourced_liveliness(tables, res, Some(face), &peer, WhatAmI::Peer);
    }
}

fn declare_peer_liveliness(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    peer: ZenohId,
) {
    register_peer_liveliness(tables, face, res, peer);
    let zid = tables.zid;
    register_router_liveliness(tables, face, res, zid);
}

fn register_client_liveliness(
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

fn declare_client_liveliness(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    id: TokenId,
    res: &mut Arc<Resource>,
) {
    register_client_liveliness(tables, face, id, res);
    let zid = tables.zid;
    register_router_liveliness(tables, face, res, zid);
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
fn send_forget_sourced_liveliness_to_net_childs(
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

fn propagate_forget_simple_liveliness(tables: &mut Tables, res: &Arc<Resource>) {
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
                        },
                        res.expr(),
                    ));
                }
            }
        }
    }
}

fn propagate_forget_simple_liveliness_to_peers(tables: &mut Tables, res: &Arc<Resource>) {
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
                        },
                        res.expr(),
                    ));
                }
            }
        }
    }
}

fn propagate_forget_sourced_liveliness(
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
                send_forget_sourced_liveliness_to_net_childs(
                    tables,
                    net,
                    &net.trees[tree_sid.index()].childs,
                    res,
                    src_face,
                    Some(tree_sid.index() as NodeId),
                );
            } else {
                log::trace!(
                    "Propagating forget liveliness {}: tree for node {} sid:{} not yet ready",
                    res.expr(),
                    tree_sid.index(),
                    source
                );
            }
        }
        None => log::error!(
            "Error propagating forget liveliness {}: cannot get index of {}!",
            res.expr(),
            source
        ),
    }
}

fn unregister_router_liveliness(tables: &mut Tables, res: &mut Arc<Resource>, router: &ZenohId) {
    res_hat_mut!(res)
        .router_tokens
        .retain(|token| token != router);

    if res_hat!(res).router_tokens.is_empty() {
        hat_mut!(tables)
            .router_tokens
            .retain(|token| !Arc::ptr_eq(token, res));

        if hat_mut!(tables).full_net(WhatAmI::Peer) {
            undeclare_peer_liveliness(tables, None, res, &tables.zid.clone());
        }
        propagate_forget_simple_liveliness(tables, res);
    }

    propagate_forget_simple_liveliness_to_peers(tables, res);
}

fn undeclare_router_liveliness(
    tables: &mut Tables,
    face: Option<&Arc<FaceState>>,
    res: &mut Arc<Resource>,
    router: &ZenohId,
) {
    if res_hat!(res).router_tokens.contains(router) {
        unregister_router_liveliness(tables, res, router);
        propagate_forget_sourced_liveliness(tables, res, face, router, WhatAmI::Router);
    }
}

fn forget_router_liveliness(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    router: &ZenohId,
) {
    undeclare_router_liveliness(tables, Some(face), res, router);
}

fn unregister_peer_liveliness(tables: &mut Tables, res: &mut Arc<Resource>, peer: &ZenohId) {
    res_hat_mut!(res).peer_tokens.retain(|token| token != peer);

    if res_hat!(res).peer_tokens.is_empty() {
        hat_mut!(tables)
            .peer_tokens
            .retain(|token| !Arc::ptr_eq(token, res));
    }
}

fn undeclare_peer_liveliness(
    tables: &mut Tables,
    face: Option<&Arc<FaceState>>,
    res: &mut Arc<Resource>,
    peer: &ZenohId,
) {
    if res_hat!(res).peer_tokens.contains(peer) {
        unregister_peer_liveliness(tables, res, peer);
        propagate_forget_sourced_liveliness(tables, res, face, peer, WhatAmI::Peer);
    }
}

fn forget_peer_liveliness(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    peer: &ZenohId,
) {
    undeclare_peer_liveliness(tables, Some(face), res, peer);
    let client_tokens = res.session_ctxs.values().any(|ctx| ctx.token);
    let peer_tokens = remote_peer_tokens(tables, res);
    let zid = tables.zid;
    if !client_tokens && !peer_tokens {
        undeclare_router_liveliness(tables, None, res, &zid);
    }
}

pub(super) fn undeclare_client_liveliness(
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
            undeclare_router_liveliness(tables, None, res, &tables.zid.clone());
        } else {
            propagate_forget_simple_liveliness_to_peers(tables, res);
        }

        if client_tokens.len() == 1 && !router_tokens && !peer_tokens {
            let mut face = &mut client_tokens[0];
            if !(face.whatami == WhatAmI::Client && res.expr().starts_with(PREFIX_LIVELINESS)) {
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

fn forget_client_liveliness(tables: &mut Tables, face: &mut Arc<FaceState>, id: TokenId) {
    if let Some(mut res) = face_hat_mut!(face).remote_tokens.remove(&id) {
        undeclare_client_liveliness(tables, face, &mut res);
    }
}

impl HatLivelinessTrait for HatCode {
    fn declare_liveliness(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: TokenId,
        res: &mut Arc<Resource>,
        node_id: NodeId,
    ) {
        match face.whatami {
            WhatAmI::Router => {
                if let Some(router) = get_router(tables, face, node_id) {
                    declare_router_liveliness(tables, face, res, router)
                }
            }
            WhatAmI::Peer => {
                if hat!(tables).full_net(WhatAmI::Peer) {
                    if let Some(peer) = get_peer(tables, face, node_id) {
                        declare_peer_liveliness(tables, face, res, peer)
                    }
                } else {
                    declare_client_liveliness(tables, face, id, res)
                }
            }
            _ => declare_client_liveliness(tables, face, id, res),
        }
    }

    fn undeclare_liveliness(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: TokenId,
        res: Option<Arc<Resource>>,
        node_id: NodeId,
    ) {
        match face.whatami {
            WhatAmI::Router => {
                if let Some(mut res) = res {
                    if let Some(router) = get_router(tables, face, node_id) {
                        forget_router_liveliness(tables, face, &mut res, &router);
                    }
                }
            }
            WhatAmI::Peer => {
                if hat!(tables).full_net(WhatAmI::Peer) {
                    if let Some(mut res) = res {
                        if let Some(peer) = get_peer(tables, face, node_id) {
                            forget_peer_liveliness(tables, face, &mut res, &peer);
                        }
                    }
                } else {
                    forget_client_liveliness(tables, face, id);
                }
            }
            _ => forget_client_liveliness(tables, face, id),
        }
    }
}
