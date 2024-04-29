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

use zenoh_config::WhatAmI;
use zenoh_protocol::network::{
    declare::{common::ext::WireExprType, TokenId},
    ext, Declare, DeclareBody, DeclareToken, UndeclareToken,
};
use zenoh_sync::get_mut_unchecked;

use crate::net::routing::{
    dispatcher::{face::FaceState, tables::Tables},
    hat::HatTokenTrait,
    router::{NodeId, Resource, SessionContext},
    RoutingContext, PREFIX_LIVELINESS,
};

use super::{face_hat, face_hat_mut, HatCode, HatFace};

#[inline]
fn propagate_simple_token_to(
    _tables: &mut Tables,
    dst_face: &mut Arc<FaceState>,
    res: &Arc<Resource>,
    src_face: &mut Arc<FaceState>,
) {
    if (src_face.id != dst_face.id
        || (dst_face.whatami == WhatAmI::Client && res.expr().starts_with(PREFIX_LIVELINESS)))
        && !face_hat!(dst_face).local_tokens.contains_key(res)
        && (src_face.whatami == WhatAmI::Client || dst_face.whatami == WhatAmI::Client)
    {
        if dst_face.whatami != WhatAmI::Client {
            let id = face_hat!(dst_face).next_id.fetch_add(1, Ordering::SeqCst);
            face_hat_mut!(dst_face).local_tokens.insert(res.clone(), id);
            let key_expr = Resource::decl_key(res, dst_face);
            dst_face
                .primitives
                .egress_declare(RoutingContext::with_expr(
                    Declare {
                        ext_qos: ext::QoSType::DECLARE,
                        ext_tstamp: None,
                        ext_nodeid: ext::NodeIdType::DEFAULT,
                        body: DeclareBody::DeclareToken(DeclareToken {
                            // TODO(fuzzypixelz): In the subscription-based
                            // implementation of liveliness, there was a comment
                            // here that stated "use proper SubscriberId"
                            id: 0,
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
                    dst_face
                        .primitives
                        .egress_declare(RoutingContext::with_expr(
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

fn propagate_simple_token(tables: &mut Tables, res: &Arc<Resource>, src_face: &mut Arc<FaceState>) {
    for mut dst_face in tables
        .faces
        .values()
        .cloned()
        .collect::<Vec<Arc<FaceState>>>()
    {
        propagate_simple_token_to(tables, &mut dst_face, res, src_face);
    }
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

    propagate_simple_token(tables, res, face);
    // This introduced a buffer overflow on windows
    // TODO: Let's deactivate this on windows until Fixed
    #[cfg(not(windows))]
    for mcast_group in &tables.mcast_groups {
        mcast_group
            .primitives
            .egress_declare(RoutingContext::with_expr(
                Declare {
                    ext_qos: ext::QoSType::DECLARE,
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::DEFAULT,
                    body: DeclareBody::DeclareToken(DeclareToken {
                        id,
                        wire_expr: res.expr().into(),
                    }),
                },
                res.expr(),
            ))
    }
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

fn propagate_forget_simple_token(tables: &mut Tables, res: &Arc<Resource>) {
    for mut face in tables.faces.values().cloned() {
        if let Some(id) = face_hat_mut!(&mut face).local_tokens.remove(res) {
            face.primitives.egress_declare(RoutingContext::with_expr(
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
                m.upgrade()
                    .is_some_and(|m| m.context.is_some() && remote_client_tokens(&m, &face))
            }) {
                if let Some(id) = face_hat_mut!(&mut face).local_tokens.remove(&res) {
                    face.primitives.egress_declare(RoutingContext::with_expr(
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
        if client_tokens.is_empty() {
            propagate_forget_simple_token(tables, res);
        }

        if client_tokens.len() == 1 {
            let mut face = &mut client_tokens[0];
            if !(face.whatami == WhatAmI::Client && res.expr().starts_with(PREFIX_LIVELINESS)) {
                if let Some(id) = face_hat_mut!(face).local_tokens.remove(res) {
                    face.primitives.egress_declare(RoutingContext::with_expr(
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
                        m.upgrade()
                            .is_some_and(|m| m.context.is_some() && remote_client_tokens(&m, face))
                    }) {
                        if let Some(id) = face_hat_mut!(&mut face).local_tokens.remove(&res) {
                            face.primitives.egress_declare(RoutingContext::with_expr(
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
        _node_id: NodeId,
    ) {
        declare_client_token(tables, face, id, res)
    }

    fn undeclare_token(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: TokenId,
        _res: Option<Arc<Resource>>,
        _node_id: NodeId,
    ) -> Option<Arc<Resource>> {
        forget_client_token(tables, face, id)
    }

    fn declare_token_interest(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: zenoh_protocol::network::declare::InterestId,
        res: Option<&mut Arc<Resource>>,
        current: bool,
        future: bool,
        aggregate: bool,
    ) {
        if current && face.whatami == WhatAmI::Client {
            if let Some(res) = res.as_ref() {
                if aggregate {
                    if tables.faces.values().any(|src_face| {
                        src_face.id != face.id
                            && face_hat!(src_face)
                                .remote_tokens
                                .values()
                                .any(|token| token.context.is_some() && token.matches(res))
                    }) {
                        let id = face_hat!(face).next_id.fetch_add(1, Ordering::SeqCst);
                        face_hat_mut!(face).local_tokens.insert((*res).clone(), id);
                        let wire_expr = Resource::decl_key(res, face);
                        face.primitives.egress_declare(RoutingContext::with_expr(
                            Declare {
                                ext_qos: ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType::DEFAULT,
                                body: DeclareBody::DeclareToken(DeclareToken { id, wire_expr }),
                            },
                            res.expr(),
                        ));
                    }
                } else {
                    for src_face in tables
                        .faces
                        .values()
                        .cloned()
                        .collect::<Vec<Arc<FaceState>>>()
                    {
                        if src_face.id != face.id {
                            for token in face_hat!(src_face).remote_tokens.values() {
                                if token.context.is_some() && token.matches(res) {
                                    let id = face_hat!(face).next_id.fetch_add(1, Ordering::SeqCst);
                                    face_hat_mut!(face).local_tokens.insert(token.clone(), id);
                                    let wire_expr = Resource::decl_key(token, face);
                                    face.primitives.egress_declare(RoutingContext::with_expr(
                                        Declare {
                                            ext_qos: ext::QoSType::DECLARE,
                                            ext_tstamp: None,
                                            ext_nodeid: ext::NodeIdType::DEFAULT,
                                            body: DeclareBody::DeclareToken(DeclareToken {
                                                id,
                                                wire_expr,
                                            }),
                                        },
                                        token.expr(),
                                    ));
                                }
                            }
                        }
                    }
                }
            } else {
                for src_face in tables
                    .faces
                    .values()
                    .cloned()
                    .collect::<Vec<Arc<FaceState>>>()
                {
                    if src_face.id != face.id {
                        for token in face_hat!(src_face).remote_tokens.values() {
                            let id = face_hat!(face).next_id.fetch_add(1, Ordering::SeqCst);
                            face_hat_mut!(face).local_tokens.insert(token.clone(), id);
                            let wire_expr = Resource::decl_key(token, face);
                            face.primitives.egress_declare(RoutingContext::with_expr(
                                Declare {
                                    ext_qos: ext::QoSType::DECLARE,
                                    ext_tstamp: None,
                                    ext_nodeid: ext::NodeIdType::DEFAULT,
                                    body: DeclareBody::DeclareToken(DeclareToken { id, wire_expr }),
                                },
                                token.expr(),
                            ));
                        }
                    }
                }
            }
        }
        if future {
            face_hat_mut!(face)
                .remote_token_interests
                .insert(id, (res.cloned(), aggregate));
        }
    }

    fn undeclare_token_interest(
        &self,
        _tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: zenoh_protocol::network::declare::InterestId,
    ) {
        face_hat_mut!(face).remote_token_interests.remove(&id);
    }
}
