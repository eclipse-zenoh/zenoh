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
    collections::HashSet,
    sync::{atomic::Ordering, Arc},
};

use itertools::Itertools;
use zenoh_protocol::network::{
    declare::{self, common::ext::WireExprType, TokenId},
    Declare, DeclareBody, DeclareToken, UndeclareToken,
};
use zenoh_sync::get_mut_unchecked;

use super::Hat;
use crate::net::routing::{
    dispatcher::{face::FaceState, region::RegionMap, tables::TablesData},
    hat::{BaseContext, HatBaseTrait, HatTokenTrait, HatTrait, SendDeclare},
    router::{FaceContext, NodeId, Resource},
    RoutingContext,
};
#[allow(unused_imports)]
use crate::zenoh_core::polyfill::*;

impl Hat {
    pub(super) fn tokens_new_face(&self, ctx: BaseContext, other_hats: &RegionMap<&dyn HatTrait>) {
        if ctx.src_face.region.bound().is_south() {
            tracing::trace!(face = %ctx.src_face, "New south-bound peer remote; not propagating tokens");
            return;
        }

        for res in other_hats
            .values()
            .flat_map(|hat| hat.remote_tokens(ctx.tables).into_iter())
        {
            // FIXME(regions): We always propagate entities in this codepath; the method name is misleading
            self.maybe_propagate_token(&res, ctx.src_face, ctx.send_declare);
        }
    }

    fn maybe_propagate_token(
        &self,
        res: &Arc<Resource>,
        dst_face: &mut Arc<FaceState>,
        send_declare: &mut SendDeclare,
    ) {
        if self.face_hat(dst_face).local_tokens.contains_key(res) {
            return;
        };

        if dst_face.region.bound().is_north()
            || self
                .face_hat(dst_face)
                .remote_interests
                .values()
                .any(|i| i.options.tokens() && i.matches(res))
        {
            let id = self
                .face_hat(dst_face)
                .next_id
                .fetch_add(1, Ordering::SeqCst);
            self.face_hat_mut(dst_face)
                .local_tokens
                .insert(res.clone(), id);
            let key_expr = Resource::decl_key(res, dst_face);
            send_declare(
                &dst_face.primitives,
                RoutingContext::with_expr(
                    Declare {
                        interest_id: None,
                        ext_qos: declare::ext::QoSType::DECLARE,
                        ext_tstamp: None,
                        ext_nodeid: declare::ext::NodeIdType::DEFAULT,
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

    fn maybe_unpropagate_token(
        &self,
        dst_face: &mut Arc<FaceState>,
        res: &Arc<Resource>,
        send_declare: &mut SendDeclare,
    ) {
        if let Some(id) = self.face_hat_mut(dst_face).local_tokens.remove(res) {
            send_declare(
                &dst_face.primitives,
                RoutingContext::with_expr(
                    Declare {
                        interest_id: None,
                        ext_qos: declare::ext::QoSType::DECLARE,
                        ext_tstamp: None,
                        ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                        body: DeclareBody::UndeclareToken(UndeclareToken {
                            id,
                            ext_wire_expr: WireExprType::null(),
                        }),
                    },
                    res.expr().to_string(),
                ),
            );
        } else if self
            .face_hat(dst_face)
            .remote_interests
            .values()
            .any(|i| i.options.tokens() && i.matches(res))
        {
            // Token has never been declared on this face.
            // Send an Undeclare with a one shot generated id and a WireExpr ext.
            send_declare(
                &dst_face.primitives,
                RoutingContext::with_expr(
                    Declare {
                        interest_id: None,
                        ext_qos: declare::ext::QoSType::DECLARE,
                        ext_tstamp: None,
                        ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                        body: DeclareBody::UndeclareToken(UndeclareToken {
                            id: self
                                .face_hat(dst_face)
                                .next_id
                                .fetch_add(1, Ordering::SeqCst),
                            ext_wire_expr: WireExprType {
                                wire_expr: Resource::get_best_key(res, "", dst_face.id),
                            },
                        }),
                    },
                    res.expr().to_string(),
                ),
            );
        }
    }
}

impl HatTokenTrait for Hat {
    #[tracing::instrument(level = "trace", skip(ctx))]
    fn register_token(
        &mut self,
        ctx: BaseContext,
        id: TokenId,
        mut res: Arc<Resource>,
        _nid: NodeId,
    ) {
        debug_assert!(self.owns(ctx.src_face));

        {
            let res = get_mut_unchecked(&mut res);
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

    #[tracing::instrument(level = "trace", skip(ctx), ret)]
    fn unregister_token(
        &mut self,
        ctx: BaseContext,
        id: TokenId,
        _res: Option<Arc<Resource>>,
        _nid: NodeId,
    ) -> Option<Arc<Resource>> {
        let Some(mut res) = self.face_hat_mut(ctx.src_face).remote_tokens.remove(&id) else {
            tracing::error!(id, "Unknown token");
            return None;
        };

        if self
            .face_hat(ctx.src_face)
            .remote_tokens
            .values()
            .contains(&res)
        {
            tracing::debug!(id, ?res, "Duplicated token");
            return None;
        };

        if let Some(ctx) = get_mut_unchecked(&mut res)
            .face_ctxs
            .get_mut(&ctx.src_face.id)
        {
            get_mut_unchecked(ctx).token = false;
        }

        Some(res)
    }

    #[tracing::instrument(level = "trace", skip(ctx), ret)]
    fn unregister_face_tokens(&mut self, ctx: BaseContext) -> HashSet<Arc<Resource>> {
        debug_assert!(self.owns(ctx.src_face));

        let fid = ctx.src_face.id;

        self.face_hat_mut(ctx.src_face)
            .remote_tokens
            .drain()
            .map(|(_, mut res)| {
                if let Some(ctx) = get_mut_unchecked(&mut res).face_ctxs.get_mut(&fid) {
                    get_mut_unchecked(ctx).token = false;
                }

                res
            })
            .collect()
    }

    #[tracing::instrument(level = "trace", skip(ctx))]
    fn propagate_token(&mut self, ctx: BaseContext, res: Arc<Resource>, other_tokens: bool) {
        if !other_tokens {
            debug_assert!(self.owns(ctx.src_face));
            return;
        };

        for dst_face in self.owned_faces_mut(ctx.tables) {
            self.maybe_propagate_token(&res, dst_face, ctx.send_declare);
        }
    }

    #[tracing::instrument(level = "trace", skip(ctx))]
    fn unpropagate_token(&mut self, ctx: BaseContext, res: Arc<Resource>) {
        if self.owns(ctx.src_face) {
            return;
        }

        for mut face in self.owned_faces(ctx.tables).cloned() {
            self.maybe_unpropagate_token(&mut face, &res, ctx.send_declare);
        }
    }

    #[tracing::instrument(level = "trace", ret)]
    fn remote_tokens_of(&self, res: &Resource) -> bool {
        self.owned_face_contexts(res).any(|(_, ctx)| ctx.token)
    }

    #[tracing::instrument(level = "trace", skip(tables), ret)]
    fn remote_tokens_matching(
        &self,
        tables: &TablesData,
        res: Option<&Resource>,
    ) -> HashSet<Arc<Resource>> {
        self.owned_faces(tables)
            .flat_map(|f| self.face_hat(f).remote_tokens.values())
            .filter(|token| res.is_none_or(|res| res.matches(token)))
            .cloned()
            .collect()
    }
}
