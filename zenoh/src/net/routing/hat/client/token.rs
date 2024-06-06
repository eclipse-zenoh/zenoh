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
    Declare, DeclareBody, DeclareFinal, DeclareToken, Interest, UndeclareToken,
};
use zenoh_sync::get_mut_unchecked;

use super::{face_hat, face_hat_mut, HatCode, HatFace};
use crate::net::routing::{
    dispatcher::{
        face::{FaceState, InterestState},
        tables::Tables,
    },
    hat::{CurrentFutureTrait, HatTokenTrait},
    router::{NodeId, Resource, SessionContext, TokenQuery},
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
    interest_id: Option<InterestId>,
) {
    register_client_token(tables, face, id, res);

    propagate_simple_token(tables, res, face);

    let wire_expr = Resource::decl_key(res, face);
    if let Some(interest_id) = interest_id {
        if let Some((query, _)) = face.pending_token_queries.get(&interest_id) {
            query
                .src_face
                .primitives
                .send_declare(RoutingContext::with_expr(
                    Declare {
                        interest_id: Some(query.src_interest_id),
                        ext_qos: ext::QoSType::default(),
                        ext_tstamp: None,
                        ext_nodeid: ext::NodeIdType::default(),
                        body: DeclareBody::DeclareToken(DeclareToken { id, wire_expr }),
                    },
                    res.expr(),
                ))
        }
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

fn propagate_forget_simple_token(tables: &mut Tables, res: &Arc<Resource>) {
    for face in tables.faces.values_mut() {
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
        } else if face_hat!(face)
            .remote_token_interests
            .values()
            .any(|si| si.as_ref().map(|si| si.matches(res)).unwrap_or(true))
        {
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
            let face = &mut client_tokens[0];
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

pub(super) fn token_new_face(tables: &mut Tables, face: &mut Arc<FaceState>) {
    for mut src_face in tables
        .faces
        .values()
        .cloned()
        .collect::<Vec<Arc<FaceState>>>()
    {
        for token in face_hat!(src_face).remote_tokens.values() {
            propagate_simple_token_to(tables, face, token, &mut src_face.clone());
        }
        if face.whatami != WhatAmI::Client {
            for res in face_hat_mut!(&mut src_face).remote_token_interests.values() {
                let id = face_hat!(face).next_id.fetch_add(1, Ordering::SeqCst);
                let options = InterestOptions::KEYEXPRS + InterestOptions::TOKENS;
                get_mut_unchecked(face).local_interests.insert(
                    id,
                    InterestState {
                        options,
                        res: res.as_ref().map(|res| (*res).clone()),
                        finalized: false,
                    },
                );
                let wire_expr = res.as_ref().map(|res| Resource::decl_key(res, face));
                face.primitives.send_interest(RoutingContext::with_expr(
                    Interest {
                        id,
                        mode: InterestMode::CurrentFuture,
                        options,
                        wire_expr,
                        ext_qos: ext::QoSType::DECLARE,
                        ext_tstamp: None,
                        ext_nodeid: ext::NodeIdType::DEFAULT,
                    },
                    res.as_ref().map(|res| res.expr()).unwrap_or_default(),
                ));
            }
        }
    }
}

impl HatTokenTrait for HatCode {
    fn declare_token_interest(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: InterestId,
        res: Option<&mut Arc<Resource>>,
        mode: InterestMode,
        _aggregate: bool,
    ) {
        let src_id = id;
        face_hat_mut!(face)
            .remote_token_interests
            .insert(src_id, res.as_ref().map(|res| (*res).clone()));

        let query = Arc::new(TokenQuery {
            src_face: face.clone(),
            src_interest_id: src_id,
        });

        let mut should_send_declare_final = true;
        for dst_face in tables
            .faces
            .values_mut()
            .filter(|f| f.whatami != WhatAmI::Client)
        {
            let dst_id = face_hat!(dst_face).next_id.fetch_add(1, Ordering::SeqCst);

            if mode.current() {
                should_send_declare_final = false;
                let dst_face_mut = get_mut_unchecked(dst_face);
                let cancellation_token = dst_face_mut.task_controller.get_cancellation_token();

                dst_face_mut
                    .pending_token_queries
                    .insert(dst_id, (query.clone(), cancellation_token));
            }

            let options = InterestOptions::KEYEXPRS + InterestOptions::TOKENS;
            get_mut_unchecked(dst_face).local_interests.insert(
                dst_id,
                InterestState {
                    options,
                    res: res.as_ref().map(|res| (*res).clone()),
                    finalized: mode == InterestMode::Future,
                },
            );
            let wire_expr = res.as_ref().map(|res| Resource::decl_key(res, dst_face));
            dst_face.primitives.send_interest(RoutingContext::with_expr(
                Interest {
                    id: dst_id,
                    mode,
                    options,
                    wire_expr,
                    ext_qos: ext::QoSType::DEFAULT,
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::DEFAULT,
                },
                res.as_ref().map(|res| res.expr()).unwrap_or_default(),
            ));
        }

        if mode.current() {
            for dst_face in tables
                .faces
                .values_mut()
                .filter(|f| f.whatami == WhatAmI::Client)
            {
                let tokens = face_hat!(dst_face)
                    .remote_tokens
                    .values()
                    .filter(|res| res.matches(res))
                    .cloned()
                    .collect::<Vec<_>>();

                for res in tokens {
                    let id = if mode.future() {
                        let id = face_hat!(face).next_id.fetch_add(1, Ordering::SeqCst);
                        face_hat_mut!(face).local_tokens.insert(res.clone(), id);
                        id
                    } else {
                        0
                    };
                    let wire_expr = Resource::decl_key(&res, dst_face);
                    face.primitives.send_declare(RoutingContext::with_expr(
                        Declare {
                            interest_id: Some(src_id),
                            ext_qos: ext::QoSType::default(),
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType::default(),
                            body: DeclareBody::DeclareToken(DeclareToken { id, wire_expr }),
                        },
                        res.expr(),
                    ))
                }
            }
        }

        if should_send_declare_final {
            face.primitives.send_declare(RoutingContext::with_expr(
                Declare {
                    interest_id: Some(src_id),
                    ext_qos: ext::QoSType::default(),
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::default(),
                    body: DeclareBody::DeclareFinal(DeclareFinal),
                },
                res.as_ref().map(|res| res.expr()).unwrap_or_default(),
            ));
        }
    }

    fn undeclare_token_interest(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: InterestId,
    ) {
        if let Some(interest) = face_hat_mut!(face).remote_token_interests.remove(&id) {
            if !tables.faces.values().any(|f| {
                f.whatami == WhatAmI::Client
                    && face_hat!(f)
                        .remote_token_interests
                        .values()
                        .any(|i| *i == interest)
            }) {
                for dst_face in tables
                    .faces
                    .values_mut()
                    .filter(|f| f.whatami != WhatAmI::Client)
                {
                    for id in dst_face
                        .local_interests
                        .keys()
                        .cloned()
                        .collect::<Vec<InterestId>>()
                    {
                        let local_interest = dst_face.local_interests.get(&id).unwrap();
                        if local_interest.options.tokens() && (local_interest.res == interest) {
                            dst_face.primitives.send_interest(RoutingContext::with_expr(
                                Interest {
                                    id,
                                    mode: InterestMode::Final,
                                    options: InterestOptions::empty(),
                                    wire_expr: None,
                                    ext_qos: ext::QoSType::DECLARE,
                                    ext_tstamp: None,
                                    ext_nodeid: ext::NodeIdType::DEFAULT,
                                },
                                local_interest
                                    .res
                                    .as_ref()
                                    .map(|res| res.expr())
                                    .unwrap_or_default(),
                            ));
                            get_mut_unchecked(dst_face).local_interests.remove(&id);
                        }
                    }
                }
            }
        }
    }

    fn declare_token(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: TokenId,
        res: &mut Arc<Resource>,
        _node_id: NodeId,
        interest_id: Option<InterestId>,
    ) {
        declare_client_token(tables, face, id, res, interest_id);
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
}
