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
    declare::{common::ext::WireExprType, Interest, InterestId, TokenId},
    ext, Declare, DeclareBody, DeclareInterest, DeclareToken, UndeclareInterest, UndeclareToken,
};
use zenoh_sync::get_mut_unchecked;

use crate::net::routing::{
    dispatcher::{face::FaceState, tables::Tables},
    hat::HatLivelinessTrait,
    router::{NodeId, Resource, SessionContext},
    RoutingContext, PREFIX_LIVELINESS,
};

use super::{face_hat, face_hat_mut, HatCode, HatFace};

#[inline]
fn propagate_simple_liveliness_to(
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

fn propagate_simple_liveliness(
    tables: &mut Tables,
    res: &Arc<Resource>,
    src_face: &mut Arc<FaceState>,
) {
    for mut dst_face in tables
        .faces
        .values()
        .cloned()
        .collect::<Vec<Arc<FaceState>>>()
    {
        propagate_simple_liveliness_to(tables, &mut dst_face, res, src_face);
    }
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

    propagate_simple_liveliness(tables, res, face);

    // This introduced a buffer overflow on windows
    // @TODO: Let's deactivate this on windows until Fixed
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
                        // NOTE(fuzzypixelz): Here there was a TODO saying "use
                        // proper subscriber id" so I used the token id
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

fn propagate_forget_simple_liveliness(tables: &mut Tables, res: &Arc<Resource>) {
    for face in tables.faces.values_mut() {
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
        if client_tokens.is_empty() {
            propagate_forget_simple_liveliness(tables, res);
        }
        if client_tokens.len() == 1 {
            let face = &mut client_tokens[0];
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
        _node_id: NodeId,
    ) {
        declare_client_liveliness(tables, face, id, res);
    }

    fn undeclare_liveliness(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: TokenId,
        _res: Option<Arc<Resource>>,
        _node_id: NodeId,
    ) {
        forget_client_liveliness(tables, face, id)
    }

    fn declare_liveliness_interest(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: zenoh_protocol::network::declare::InterestId,
        res: Option<&mut Arc<Resource>>,
        current: bool,
        future: bool,
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
            let mut interest = Interest::KEYEXPRS + Interest::TOKENS;
            if current {
                interest += Interest::CURRENT;
            }
            if future {
                interest += Interest::FUTURE;
            }
            get_mut_unchecked(dst_face).local_interests.insert(
                id,
                (interest, res.as_ref().map(|res| (*res).clone()), !current),
            );
            let wire_expr = res.as_ref().map(|res| Resource::decl_key(res, dst_face));
            dst_face
                .primitives
                .egress_declare(RoutingContext::with_expr(
                    Declare {
                        ext_qos: ext::QoSType::DECLARE,
                        ext_tstamp: None,
                        ext_nodeid: ext::NodeIdType::DEFAULT,
                        body: DeclareBody::DeclareInterest(DeclareInterest {
                            id,
                            interest,
                            wire_expr,
                        }),
                    },
                    res.as_ref().map(|res| res.expr()).unwrap_or_default(),
                ));
        }
    }

    fn undeclare_liveliness_interest(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: zenoh_protocol::network::declare::InterestId,
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
                        let (int, res, _) = dst_face.local_interests.get(&id).unwrap();
                        if int.tokens() && (*res == interest) {
                            dst_face
                                .primitives
                                .egress_declare(RoutingContext::with_expr(
                                    Declare {
                                        ext_qos: ext::QoSType::DECLARE,
                                        ext_tstamp: None,
                                        ext_nodeid: ext::NodeIdType::DEFAULT,
                                        body: DeclareBody::UndeclareInterest(UndeclareInterest {
                                            id,
                                            ext_wire_expr: WireExprType::null(),
                                        }),
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
