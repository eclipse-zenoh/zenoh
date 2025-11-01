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

use zenoh_config::WhatAmI;
use zenoh_protocol::network::{
    declare::{common::ext::WireExprType, TokenId},
    ext,
    interest::{InterestId, InterestMode},
    Declare, DeclareBody, DeclareToken, UndeclareToken,
};
use zenoh_sync::get_mut_unchecked;

use super::{Hat, INITIAL_INTEREST_ID};
use crate::net::routing::{
    dispatcher::{
        face::FaceState, gateway::BoundMap, interests::CurrentInterest, tables::TablesData,
    },
    hat::{BaseContext, CurrentFutureTrait, HatBaseTrait, HatTokenTrait, HatTrait, SendDeclare},
    router::{FaceContext, NodeId, Resource},
    RoutingContext,
};

impl Hat {
    fn new_token(
        &self,
        tables: &TablesData,
        res: &Arc<Resource>,
        src_face: &Arc<FaceState>,
        dst_face: &mut Arc<FaceState>,
    ) -> bool {
        // Is there any face that
        !res.face_ctxs.values().any(|ctx| {
            ctx.token // declared the token
            && (ctx.face.id != src_face.id) // is not the face that just registered it
            && (ctx.face.id != dst_face.id || dst_face.zid == tables.zid) // is not the face we are propagating to (except for local)
            && (ctx.face.whatami == WhatAmI::Client || dst_face.whatami == WhatAmI::Client)
            // don't forward from/to router/peers
        })
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    fn propagate_simple_token_to(
        &self,
        ctx: BaseContext,
        src_face: &mut Arc<FaceState>,
        dst_face: &mut Arc<FaceState>,
        res: &Arc<Resource>,
        src_interest_id: Option<InterestId>,
        dst_interest_id: Option<InterestId>,
    ) {
        if (src_face.id != dst_face.id || dst_face.zid == ctx.tables.zid)
            && !self.face_hat(dst_face).local_tokens.contains_key(res)
            && (src_face.whatami == WhatAmI::Client || dst_face.whatami == WhatAmI::Client)
            && self.new_token(ctx.tables, res, src_face, dst_face)
            && (dst_face.whatami != WhatAmI::Client
                || dst_face.is_local
                || self.face_hat(dst_face).remote_interests.values().any(|i| {
                    i.options.tokens()
                        && (i.mode.current() || src_interest_id.is_none())
                        && i.matches(res)
                }))
        {
            let id = self
                .face_hat(dst_face)
                .next_id
                .fetch_add(1, Ordering::SeqCst);
            self.face_hat_mut(dst_face)
                .local_tokens
                .insert(res.clone(), id);
            let key_expr =
                Resource::decl_key(res, dst_face, super::push_declaration_profile(dst_face));
            (ctx.send_declare)(
                &dst_face.primitives,
                RoutingContext::with_expr(
                    Declare {
                        interest_id: dst_interest_id,
                        ext_qos: ext::QoSType::DECLARE,
                        ext_tstamp: None,
                        ext_nodeid: ext::NodeIdType::DEFAULT,
                        body: DeclareBody::DeclareToken(DeclareToken {
                            id,
                            wire_expr: key_expr,
                        }),
                    },
                    res.expr().to_string(),
                ),
            );
        }
    }

    fn propagate_simple_token(
        &self,
        mut ctx: BaseContext,
        res: &Arc<Resource>,
        interest_id: Option<InterestId>,
    ) {
        for mut dst_face in self.owned_faces(ctx.tables).cloned().collect::<Vec<_>>() {
            let src_face = &mut ctx.src_face.clone();
            self.propagate_simple_token_to(
                ctx.reborrow(),
                src_face,
                &mut dst_face,
                res,
                interest_id,
                interest_id.is_some().then_some(INITIAL_INTEREST_ID),
            );
        }
    }

    fn register_simple_token(&self, ctx: BaseContext, id: TokenId, res: &mut Arc<Resource>) {
        // Register liveliness
        {
            let res = get_mut_unchecked(res);
            match res.face_ctxs.get_mut(&ctx.src_face.id) {
                Some(ctx) => {
                    if !ctx.token {
                        get_mut_unchecked(ctx).token = true;
                    }
                }
                None => {
                    let ctx = res
                        .face_ctxs
                        .entry(ctx.src_face.id)
                        .or_insert_with(|| Arc::new(FaceContext::new(ctx.src_face.clone())));
                    get_mut_unchecked(ctx).token = true;
                }
            }
        }
        self.face_hat_mut(ctx.src_face)
            .remote_tokens
            .insert(id, res.clone());
    }

    fn declare_simple_token(
        &self,
        mut ctx: BaseContext,
        id: TokenId,
        res: &mut Arc<Resource>,
        interest_id: Option<InterestId>,
    ) {
        if let Some(interest_id) = interest_id {
            if let Some(interest) = ctx
                .src_face
                .clone()
                .pending_current_interests
                .get(&interest_id)
                .map(|p| &p.interest)
            {
                if interest.mode == InterestMode::CurrentFuture {
                    self.register_simple_token(ctx.reborrow(), id, res);
                }

                // FIXME(regions): router subregions don't support the interest protocol
                let src_face = interest.src.downcast_ref::<Arc<FaceState>>().unwrap();

                let id = self.make_token_id(res, &mut src_face.clone(), interest.mode);
                let wire_expr = Resource::get_best_key(res, "", src_face.id);
                (ctx.send_declare)(
                    &src_face.primitives,
                    RoutingContext::with_expr(
                        Declare {
                            interest_id: Some(interest.src_interest_id),
                            ext_qos: ext::QoSType::default(),
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType::default(),
                            body: DeclareBody::DeclareToken(DeclareToken { id, wire_expr }),
                        },
                        res.expr().to_string(),
                    ),
                );
                return;
            } else if !ctx.src_face.local_interests.contains_key(&interest_id) {
                tracing::error!(
                    "Received DeclareToken for {} from {} with unknown interest_id {}. Ignore.",
                    res.expr(),
                    ctx.src_face,
                    interest_id,
                );
                return;
            }
        }
        self.register_simple_token(ctx.reborrow(), id, res);
        self.propagate_simple_token(ctx, res, interest_id);
    }

    #[inline]
    fn simple_tokens(&self, res: &Arc<Resource>) -> Vec<Arc<FaceState>> {
        res.face_ctxs
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

    fn propagate_forget_simple_token(
        &self,
        tables: &mut TablesData,
        res: &Arc<Resource>,
        src_face: &Arc<FaceState>,
        send_declare: &mut SendDeclare,
    ) {
        for mut face in self.faces(tables).values().cloned() {
            if let Some(id) = self.face_hat_mut(&mut face).local_tokens.remove(res) {
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
                        res.expr().to_string(),
                    ),
                );
            } else if src_face.id != face.id
                && (src_face.whatami == WhatAmI::Client || face.whatami == WhatAmI::Client)
                && (face.whatami != WhatAmI::Client
                    || face.is_local
                    || self
                        .face_hat(&face)
                        .remote_interests
                        .values()
                        .any(|i| i.options.tokens() && i.matches(res)))
            {
                // Token has never been declared on this face.
                // Send an Undeclare with a one shot generated id and a WireExpr ext.
                send_declare(
                    &face.primitives,
                    RoutingContext::with_expr(
                        Declare {
                            interest_id: None,
                            ext_qos: ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType::DEFAULT,
                            body: DeclareBody::UndeclareToken(UndeclareToken {
                                id: self.face_hat(&face).next_id.fetch_add(1, Ordering::SeqCst),
                                ext_wire_expr: WireExprType {
                                    wire_expr: Resource::get_best_key(res, "", face.id),
                                },
                            }),
                        },
                        res.expr().to_string(),
                    ),
                );
            }
        }
    }

    pub(super) fn undeclare_simple_token(
        &self,
        tables: &mut TablesData,
        face: &mut Arc<FaceState>,
        res: &mut Arc<Resource>,
        send_declare: &mut SendDeclare,
    ) {
        if !self
            .face_hat_mut(face)
            .remote_tokens
            .values()
            .any(|s| *s == *res)
        {
            if let Some(ctx) = get_mut_unchecked(res).face_ctxs.get_mut(&face.id) {
                get_mut_unchecked(ctx).token = false;
            }

            let mut simple_tokens = self.simple_tokens(res);
            if simple_tokens.is_empty() {
                self.propagate_forget_simple_token(tables, res, face, send_declare);
            }

            if simple_tokens.len() == 1 {
                let face = &mut simple_tokens[0];
                if face.whatami != WhatAmI::Client {
                    if let Some(id) = self.face_hat_mut(face).local_tokens.remove(res) {
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
                                res.expr().to_string(),
                            ),
                        );
                    }
                }
            }
        }
    }

    fn forget_simple_token(
        &self,
        tables: &mut TablesData,
        face: &mut Arc<FaceState>,
        id: TokenId,
        res: Option<Arc<Resource>>,
        send_declare: &mut SendDeclare,
    ) -> Option<Arc<Resource>> {
        if let Some(mut res) = self.face_hat_mut(face).remote_tokens.remove(&id) {
            self.undeclare_simple_token(tables, face, &mut res, send_declare);
            Some(res)
        } else if let Some(mut res) = res {
            self.undeclare_simple_token(tables, face, &mut res, send_declare);
            Some(res)
        } else {
            None
        }
    }

    pub(super) fn token_new_face(&self, mut ctx: BaseContext) {
        if ctx.src_face.whatami != WhatAmI::Client {
            for mut face in self
                .faces(ctx.tables)
                .values()
                .cloned()
                .collect::<Vec<Arc<FaceState>>>()
            {
                for token in self.face_hat(&face.clone()).remote_tokens.values() {
                    let dst_face = &mut ctx.src_face.clone();
                    self.propagate_simple_token_to(
                        ctx.reborrow(),
                        &mut face,
                        dst_face,
                        token,
                        None,
                        Some(INITIAL_INTEREST_ID),
                    );
                }
            }
        }
    }

    #[inline]
    fn make_token_id(
        &self,
        res: &Arc<Resource>,
        face: &mut Arc<FaceState>,
        mode: InterestMode,
    ) -> u32 {
        if mode.future() {
            if let Some(id) = self.face_hat(face).local_tokens.get(res) {
                *id
            } else {
                let id = self.face_hat(face).next_id.fetch_add(1, Ordering::SeqCst);
                self.face_hat_mut(face).local_tokens.insert(res.clone(), id);
                id
            }
        } else {
            0
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn declare_token_interest(
        &self,
        tables: &mut TablesData,
        face: &mut Arc<FaceState>,
        id: InterestId,
        res: Option<&mut Arc<Resource>>,
        mode: InterestMode,
        send_declare: &mut SendDeclare,
    ) {
        if mode.current() {
            let interest_id = Some(id);
            if let Some(res) = res.as_ref() {
                for src_face in tables
                    .faces
                    .values()
                    .filter(|f| f.whatami != WhatAmI::Router)
                    .cloned()
                    .collect::<Vec<Arc<FaceState>>>()
                {
                    for token in self.face_hat(&src_face).remote_tokens.values() {
                        if token.ctx.is_some() && token.matches(res) {
                            let id = self.make_token_id(token, face, mode);
                            let wire_expr = Resource::decl_key(
                                token,
                                face,
                                super::push_declaration_profile(face),
                            );
                            send_declare(
                                &face.primitives,
                                RoutingContext::with_expr(
                                    Declare {
                                        interest_id,
                                        ext_qos: ext::QoSType::DECLARE,
                                        ext_tstamp: None,
                                        ext_nodeid: ext::NodeIdType::DEFAULT,
                                        body: DeclareBody::DeclareToken(DeclareToken {
                                            id,
                                            wire_expr,
                                        }),
                                    },
                                    token.expr().to_string(),
                                ),
                            );
                        }
                    }
                }
            } else {
                for src_face in tables
                    .faces
                    .values()
                    .filter(|f| f.whatami != WhatAmI::Router)
                    .cloned()
                    .collect::<Vec<Arc<FaceState>>>()
                {
                    for token in self.face_hat(&src_face).remote_tokens.values() {
                        let id = self.make_token_id(token, face, mode);
                        let wire_expr =
                            Resource::decl_key(token, face, super::push_declaration_profile(face));
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
                                token.expr().to_string(),
                            ),
                        );
                    }
                }
            }
        }
    }
}

impl HatTokenTrait for Hat {
    #[tracing::instrument(level = "trace", skip_all, fields(wai = %self.whatami().short(), bnd = %self.bound))]
    fn declare_token(
        &mut self,
        ctx: BaseContext,
        id: TokenId,
        res: &mut Arc<Resource>,
        _node_id: NodeId,
        interest_id: Option<InterestId>,
    ) {
        // TODO(regions2): clients of this peer are handled as if they were bound to a future broker south hat
        self.declare_simple_token(ctx, id, res, interest_id);
    }

    #[tracing::instrument(level = "trace", skip_all, fields(wai = %self.whatami().short(), bnd = %self.bound))]
    fn undeclare_token(
        &mut self,
        ctx: BaseContext,
        id: TokenId,
        res: Option<Arc<Resource>>,
        _node_id: NodeId,
    ) -> Option<Arc<Resource>> {
        self.forget_simple_token(ctx.tables, ctx.src_face, id, res, ctx.send_declare)
    }

    #[tracing::instrument(level = "trace", skip_all, fields(wai = %self.whatami().short(), bnd = %self.bound, interest_id))]
    fn declare_current_token(
        &mut self,
        ctx: BaseContext,
        res: &mut Arc<Resource>,
        interest_id: InterestId,
        mut downstream_hats: BoundMap<&mut dyn HatTrait>,
    ) {
        debug_assert!(self.bound.is_north());

        if let Some(interest) = ctx
            .src_face
            .clone()
            .pending_current_interests
            .get(&interest_id)
            .map(|p| &p.interest)
        {
            let hat = &mut downstream_hats[interest.src_bound];
            hat.propagate_current_token(ctx, res, interest);
        } else {
            tracing::error!(
                id = interest_id,
                keyexpr = res.expr(),
                src = %ctx.src_face,
                "Received current token with unknown interest id"
            );
        }
    }

    #[tracing::instrument(level = "trace", skip_all, fields(wai = %self.whatami().short(), bnd = %self.bound, interest_id = interest.src_interest_id))]
    fn propagate_current_token(
        &mut self,
        mut ctx: BaseContext,
        res: &mut Arc<Resource>,
        interest: &CurrentInterest,
    ) {
        debug_assert!(!self.bound.is_north());

        if interest.mode == InterestMode::CurrentFuture {
            self.register_simple_token(ctx.reborrow(), interest.src_interest_id, res);
        }

        // FIXME(regions): router subregions don't support the interest protocol
        let src_face = interest.src.downcast_ref::<Arc<FaceState>>().unwrap();

        let id = self.make_token_id(res, &mut src_face.clone(), interest.mode);
        let wire_expr = Resource::get_best_key(res, "", src_face.id);
        (ctx.send_declare)(
            &src_face.primitives,
            RoutingContext::with_expr(
                Declare {
                    interest_id: Some(interest.src_interest_id),
                    ext_qos: ext::QoSType::default(),
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::default(),
                    body: DeclareBody::DeclareToken(DeclareToken { id, wire_expr }),
                },
                res.expr().to_string(),
            ),
        );
    }
}
