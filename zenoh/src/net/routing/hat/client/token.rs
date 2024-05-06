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
    Declare, DeclareBody, DeclareToken, Interest, UndeclareToken,
};
use zenoh_sync::get_mut_unchecked;

use crate::net::routing::{
    dispatcher::{
        face::{FaceState, InterestState},
        tables::Tables,
    },
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
                interest_id: todo!(),
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

fn propagate_forget_simple_token(tables: &mut Tables, res: &Arc<Resource>) {
    for face in tables.faces.values_mut() {
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
                    interest_id: todo!(),
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
                            interest_id: todo!(),
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

impl HatTokenTrait for HatCode {
    fn declare_token(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: TokenId,
        res: &mut Arc<Resource>,
        _node_id: NodeId,
    ) {
        declare_client_token(tables, face, id, res);
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
        id: InterestId,
        res: Option<&mut Arc<Resource>>,
        mode: InterestMode,
        _aggregate: bool,
    ) {
        face_hat_mut!(face)
            .remote_token_interests
            .insert(id, res.as_ref().map(|res| (*res).clone()));
        for dst_face in tables
            .faces
            .values_mut()
            .filter(|f| f.whatami != WhatAmI::Client)
        {
            let id = face_hat!(dst_face).next_id.fetch_add(1, Ordering::SeqCst);
            let options = InterestOptions::KEYEXPRS + InterestOptions::TOKENS;
            get_mut_unchecked(dst_face).local_interests.insert(
                id,
                InterestState {
                    options,
                    res: res.as_ref().map(|res| (*res).clone()),
                    finalized: mode == InterestMode::Future,
                },
            );
            let wire_expr = res.as_ref().map(|res| Resource::decl_key(res, dst_face));
            dst_face.primitives.send_interest(RoutingContext::with_expr(
                Interest {
                    id,
                    mode,
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
                        let InterestState { options, res, .. } =
                            dst_face.local_interests.get(&id).unwrap();
                        if options.tokens() && (*res == interest) {
                            dst_face.primitives.send_interest(RoutingContext::with_expr(
                                Interest {
                                    id,
                                    mode: InterestMode::Final,
                                    options: InterestOptions::KEYEXPRS + InterestOptions::TOKENS,
                                    wire_expr: None,
                                    ext_qos: ext::QoSType::DECLARE,
                                    ext_tstamp: None,
                                    ext_nodeid: ext::NodeIdType::DEFAULT,
                                },
                                res.as_ref().map(|res| res.expr()).unwrap_or_default(),
                            ));

                            get_mut_unchecked(dst_face).local_interests.remove(&id);
                        }
                    }
                }
            }
        }
    }
}
