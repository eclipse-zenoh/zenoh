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
    ext,
    interest::{InterestId, InterestMode, InterestOptions},
    Declare, DeclareBody, DeclareToken, UndeclareToken,
};
use zenoh_sync::get_mut_unchecked;

use super::{face_hat, face_hat_mut, HatCode, HatFace};
use crate::net::routing::{
    dispatcher::{face::FaceState, tables::Tables},
    hat::{CurrentFutureTrait, HatTokenTrait},
    router::{NodeId, Resource, SessionContext},
    RoutingContext,
};

#[inline]
fn propagate_simple_token_to(
    _tables: &mut Tables,
    dst_face: &mut Arc<FaceState>,
    res: &Arc<Resource>,
    src_face: &mut Arc<FaceState>,
) {
    if (src_face.id != dst_face.id || dst_face.whatami == WhatAmI::Client)
        && !face_hat!(dst_face).local_tokens.contains_key(res)
        && (src_face.whatami == WhatAmI::Client || dst_face.whatami == WhatAmI::Client)
    {
        if dst_face.whatami != WhatAmI::Client {
            let id = face_hat!(dst_face).next_id.fetch_add(1, Ordering::SeqCst);
            face_hat_mut!(dst_face).local_tokens.insert(res.clone(), id);
            let key_expr = Resource::decl_key(res, dst_face);
            dst_face.primitives.send_declare(RoutingContext::with_expr(
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
            ));
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
                    dst_face.primitives.send_declare(RoutingContext::with_expr(
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
            face.primitives.send_declare(RoutingContext::with_expr(
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
            ));
        } else if face_hat!(face).remote_interests.values().any(|(r, o)| {
            o.tokens() && r.as_ref().map(|r| r.matches(res)).unwrap_or(true) && !o.aggregate()
        }) {
            // Token has never been declared on this face.
            // Send an Undeclare with a one shot generated id and a WireExpr ext.
            face.primitives.send_declare(RoutingContext::with_expr(
                Declare {
                    interest_id: None,
                    ext_qos: ext::QoSType::DECLARE,
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::DEFAULT,
                    body: DeclareBody::UndeclareToken(UndeclareToken {
                        id: face_hat!(face).next_id.fetch_add(1, Ordering::SeqCst),
                        ext_wire_expr: WireExprType {
                            wire_expr: Resource::get_best_key(res, "", face.id),
                        },
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
                    face.primitives.send_declare(RoutingContext::with_expr(
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
                    ));
                } else if face_hat!(face).remote_interests.values().any(|(r, o)| {
                    o.tokens()
                        && r.as_ref().map(|r| r.matches(&res)).unwrap_or(true)
                        && !o.aggregate()
                }) {
                    // Token has never been declared on this face.
                    // Send an Undeclare with a one shot generated id and a WireExpr ext.
                    face.primitives.send_declare(RoutingContext::with_expr(
                        Declare {
                            interest_id: None,
                            ext_qos: ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType::DEFAULT,
                            body: DeclareBody::UndeclareToken(UndeclareToken {
                                id: face_hat!(face).next_id.fetch_add(1, Ordering::SeqCst),
                                ext_wire_expr: WireExprType {
                                    wire_expr: Resource::get_best_key(&res, "", face.id),
                                },
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
            if face.whatami != WhatAmI::Client {
                if let Some(id) = face_hat_mut!(face).local_tokens.remove(res) {
                    face.primitives.send_declare(RoutingContext::with_expr(
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
                            face.primitives.send_declare(RoutingContext::with_expr(
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
    res: Option<Arc<Resource>>,
) -> Option<Arc<Resource>> {
    if let Some(mut res) = face_hat_mut!(face).remote_tokens.remove(&id) {
        undeclare_client_token(tables, face, &mut res);
        Some(res)
    } else if let Some(mut res) = res {
        undeclare_client_token(tables, face, &mut res);
        Some(res)
    } else {
        None
    }
}

pub(super) fn token_new_face(tables: &mut Tables, face: &mut Arc<FaceState>) {
    if face.whatami != WhatAmI::Client {
        for src_face in tables
            .faces
            .values()
            .cloned()
            .collect::<Vec<Arc<FaceState>>>()
        {
            for token in face_hat!(src_face).remote_tokens.values() {
                propagate_simple_token_to(tables, face, token, &mut src_face.clone());
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
) {
    if mode.current() && face.whatami == WhatAmI::Client {
        let interest_id = (!mode.future()).then_some(id);
        if let Some(res) = res.as_ref() {
            if aggregate {
                if tables.faces.values().any(|src_face| {
                    face_hat!(src_face)
                        .remote_tokens
                        .values()
                        .any(|token| token.context.is_some() && token.matches(res))
                }) {
                    let id = if mode.future() {
                        let id = face_hat!(face).next_id.fetch_add(1, Ordering::SeqCst);
                        face_hat_mut!(face).local_tokens.insert((*res).clone(), id);
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
                    for token in face_hat!(src_face).remote_tokens.values() {
                        if token.context.is_some() && token.matches(res) {
                            let id = if mode.future() {
                                let id = face_hat!(face).next_id.fetch_add(1, Ordering::SeqCst);
                                face_hat_mut!(face).local_tokens.insert(token.clone(), id);
                                id
                            } else {
                                0
                            };
                            let wire_expr = Resource::decl_key(token, face);
                            face.primitives.send_declare(RoutingContext::with_expr(
                                Declare {
                                    interest_id,
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
        } else {
            for src_face in tables
                .faces
                .values()
                .cloned()
                .collect::<Vec<Arc<FaceState>>>()
            {
                for token in face_hat!(src_face).remote_tokens.values() {
                    let id = if mode.future() {
                        let id = face_hat!(face).next_id.fetch_add(1, Ordering::SeqCst);
                        face_hat_mut!(face).local_tokens.insert(token.clone(), id);
                        id
                    } else {
                        0
                    };
                    let wire_expr = Resource::decl_key(token, face);
                    face.primitives.send_declare(RoutingContext::with_expr(
                        Declare {
                            interest_id,
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

impl HatTokenTrait for HatCode {
    fn declare_token(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: TokenId,
        res: &mut Arc<Resource>,
        _node_id: NodeId,
        _interest_id: Option<InterestId>,
    ) {
        declare_client_token(tables, face, id, res)
    }

    fn undeclare_token(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: TokenId,
        res: Option<Arc<Resource>>,
        _node_id: NodeId,
    ) -> Option<Arc<Resource>> {
        forget_client_token(tables, face, id, res)
    }
}
