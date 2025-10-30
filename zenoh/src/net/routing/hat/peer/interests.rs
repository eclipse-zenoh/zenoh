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

use itertools::Itertools;
use zenoh_protocol::{
    core::WhatAmI,
    network::{
        interest::{self, InterestId, InterestMode},
        Declare, DeclareBody, DeclareFinal, Interest,
    },
};
use zenoh_sync::get_mut_unchecked;

use super::{initial_interest, Hat, INITIAL_INTEREST_ID};
use crate::net::routing::{
    dispatcher::{
        face::InterestState,
        gateway::BoundMap,
        interests::{
            CurrentInterest, CurrentInterestCleanup, PendingCurrentInterest, RemoteInterest,
        },
        resource::Resource,
    },
    hat::{BaseContext, CurrentFutureTrait, HatBaseTrait, HatInterestTrait, HatTrait, Remote},
    RoutingContext,
};

impl Hat {
    pub(super) fn interests_new_face(&self, ctx: BaseContext) {
        if ctx.src_face.whatami != WhatAmI::Client {
            for mut face in self.faces(ctx.tables).values().cloned().collect::<Vec<_>>() {
                if !face.remote_bound.is_north() {
                    for RemoteInterest { res, options, .. } in
                        self.face_hat_mut(&mut face).remote_interests.values()
                    {
                        let id = self
                            .face_hat(ctx.src_face)
                            .next_id
                            .fetch_add(1, Ordering::SeqCst);
                        let face_id = ctx.src_face.id;
                        get_mut_unchecked(ctx.src_face).local_interests.insert(
                            id,
                            InterestState::new(face_id, *options, res.clone(), false),
                        );
                        let wire_expr = res.as_ref().map(|res| {
                            Resource::decl_key(
                                res,
                                ctx.src_face,
                                super::push_declaration_profile(ctx.src_face),
                            )
                        });
                        ctx.src_face
                            .primitives
                            .send_interest(RoutingContext::with_expr(
                                &mut Interest {
                                    id,
                                    mode: InterestMode::CurrentFuture,
                                    options: *options,
                                    wire_expr,
                                    ext_qos: interest::ext::QoSType::DECLARE,
                                    ext_tstamp: None,
                                    ext_nodeid: interest::ext::NodeIdType::DEFAULT,
                                },
                                res.as_ref()
                                    .map(|res| res.expr().to_string())
                                    .unwrap_or_default(),
                            ));
                    }
                }
            }
        }
    }
}

impl HatInterestTrait for Hat {
    #[tracing::instrument(level = "trace", skip_all, fields(wai = %self.whatami().short(), bnd = %self.bound))]
    fn route_interest(
        &mut self,
        mut ctx: BaseContext,
        msg: &Interest,
        res: Option<&mut Arc<Resource>>,
        mut south_hats: BoundMap<&mut dyn HatTrait>,
    ) {
        // I have received an interest with mode != FINAL.
        // I should be the north hat.
        // The face cannot be bound to me, so it must be south-bound. In which case the msg originates in a subregion (for which I am the gateway):
        //   1. If I have a gateway, I should re-propagate the interest to it.
        //   2. If the interest is current, I need to send all current declarations in the (south) owner hat.
        //   3. If the interest is future, I need to register it as a remote interest in the (south) owner hat.

        assert!(self.bound().is_north());
        assert!(!ctx.src_face.local_bound.is_north());

        let owner_hat = &mut *south_hats[ctx.src_face.local_bound];

        if msg.mode.current() {
            owner_hat.send_declarations(ctx.reborrow(), msg, res.as_deref().cloned().as_mut());
        }

        if msg.mode.future() {
            owner_hat.register_interest(ctx.reborrow(), msg, res.as_deref().cloned().as_mut());
        }

        let Some(src) = owner_hat.new_remote(ctx.src_face, msg.ext_nodeid.node_id) else {
            return;
        };

        let interest = Arc::new(CurrentInterest {
            src,
            src_bound: ctx.src_face.local_bound,
            src_interest_id: msg.id,
            mode: msg.mode,
        });

        let propagated_mode = if msg.mode.future() {
            InterestMode::CurrentFuture
        } else {
            msg.mode
        };

        let interests_timeout = ctx.tables.interests_timeout;

        for mut dst_face in self
            .faces_mut(ctx.tables)
            .values()
            .filter(|f| {
                (!f.remote_bound.is_north())
                    || (f.whatami == WhatAmI::Peer
                        && msg.options.tokens()
                        && msg.mode == InterestMode::Current
                        && !initial_interest(f).map(|i| i.finalized).unwrap_or(true))
            })
            .cloned()
            .collect::<Vec<_>>()
        {
            let id = self
                .face_hat(&dst_face)
                .next_id
                .fetch_add(1, Ordering::SeqCst);
            let dst_face_id = dst_face.id;
            get_mut_unchecked(&mut dst_face).local_interests.insert(
                id,
                InterestState::new(
                    dst_face_id,
                    msg.options,
                    res.as_deref().cloned(),
                    propagated_mode == InterestMode::Future,
                ),
            );
            if msg.mode.current() {
                let dst_face_mut = get_mut_unchecked(&mut dst_face);
                let cancellation_token = dst_face_mut.task_controller.get_cancellation_token();
                let rejection_token = dst_face_mut.task_controller.get_cancellation_token();
                dst_face_mut.pending_current_interests.insert(
                    id,
                    PendingCurrentInterest {
                        interest: interest.clone(),
                        cancellation_token,
                        rejection_token,
                    },
                );
                CurrentInterestCleanup::spawn_interest_clean_up_task(
                    &dst_face,
                    ctx.tables_lock,
                    id,
                    interests_timeout,
                );
            }
            let wire_expr = res.as_ref().map(|res| {
                let push = super::push_declaration_profile(&dst_face);
                Resource::decl_key(res, &mut dst_face, push)
            });
            dst_face.primitives.send_interest(RoutingContext::with_expr(
                &mut Interest {
                    id,
                    mode: propagated_mode,
                    options: msg.options,
                    wire_expr,
                    ext_qos: interest::ext::QoSType::INTEREST,
                    ext_tstamp: None,
                    ext_nodeid: interest::ext::NodeIdType::DEFAULT,
                },
                res.as_ref()
                    .map(|res| res.expr().to_string())
                    .unwrap_or_default(),
            ));
        }

        if msg.mode.current() {
            if let Some(interest) = Arc::into_inner(interest) {
                tracing::debug!(
                    id = msg.id,
                    src = %ctx.src_face,
                    "Finalizing current interest. It was not propagated upstream"
                );

                owner_hat.send_final_declaration(ctx, interest.src_interest_id, &interest.src);
            }
        }
    }

    #[tracing::instrument(level = "trace", skip_all, fields(wai = %self.whatami().short(), bnd = %self.bound))]
    fn route_interest_final(
        &mut self,
        mut ctx: BaseContext,
        msg: &Interest,
        mut south_hats: BoundMap<&mut dyn HatTrait>,
    ) {
        // I have received an interest with mode FINAL.
        // I should be the north hat.
        // The face cannot be bound to me, so it must be south-bound. In which case the msg originates in a subregion (for which I am the gateway):
        //   1. I need to unregister it as a remote interest in the owner (south) hat.
        //   2. If I have a gateway, I should re-propagate the FINAL interest to it iff no other subregion has the same remote interest.

        assert!(self.bound().is_north());
        assert!(!ctx.src_face.local_bound.is_north());

        // FIXME(regions): check if any subregion has the same remote interest before propagating the interest final

        let owner_hat = &mut *south_hats[ctx.src_face.local_bound];

        let Some(remote_interest) = owner_hat.unregister_interest(ctx.reborrow(), msg) else {
            tracing::error!(id = msg.id, "Unknown remote interest");
            return;
        };

        if let Some(dst_face) = self
            .owned_faces_mut(ctx.tables)
            .find(|f| !f.remote_bound.is_north())
            .map(get_mut_unchecked)
        {
            dst_face.local_interests.retain(|id, local_interest| {
                if *local_interest == remote_interest {
                    dst_face.primitives.send_interest(RoutingContext::with_expr(
                        &mut Interest {
                            id: *id,
                            mode: InterestMode::Final,
                            // NOTE: InterestMode::Final options are undefined in the current protocol specification,
                            // they are initialized here for internal use by local egress interceptors.
                            options: remote_interest.options,
                            wire_expr: None,
                            ext_qos: interest::ext::QoSType::INTEREST,
                            ext_tstamp: None,
                            ext_nodeid: interest::ext::NodeIdType::DEFAULT,
                        },
                        local_interest
                            .res
                            .as_ref()
                            .map(|res| res.expr().to_string())
                            .unwrap_or_default(),
                    ));
                    return false;
                }
                true
            });
        }
    }

    #[tracing::instrument(level = "trace", skip_all, fields(wai = %self.whatami().short(), bnd = %self.bound))]
    fn route_final_declaration(
        &mut self,
        ctx: BaseContext,
        interest_id: InterestId,
        mut south_hats: BoundMap<&mut dyn HatTrait>,
    ) {
        // I have received a Declare Final.
        // Either this is a peer initial interest finalizer, in which case I should own the (peer) face.
        // Or, the source face is a gateway, in which case there should be a pending current interest and I should be the north hat.

        if interest_id == INITIAL_INTEREST_ID {
            self.assert_proper_ownership(&ctx);

            zenoh_runtime::ZRuntime::Net.block_in_place(async move {
                if let Some(runtime) = &ctx.tables.runtime {
                    if let Some(runtime) = runtime.upgrade() {
                        runtime
                            .start_conditions()
                            .terminate_peer_connector_zid(ctx.src_face.zid)
                            .await
                    }
                }
            });
        } else {
            assert!(self.bound().is_north());
            assert!(ctx.src_face.local_bound.is_north());

            if ctx.src_face.remote_bound.is_north() {
                tracing::error!(
                    id = interest_id,
                    src = %ctx.src_face,
                    "Received current interest finalization from non-gateway face"
                );
                return;
            }

            let Some(PendingCurrentInterest {
                interest,
                cancellation_token,
                ..
            }) = get_mut_unchecked(ctx.src_face)
                .pending_current_interests
                .remove(&interest_id)
            else {
                tracing::error!(
                    id = interest_id,
                    src = %ctx.src_face,
                    "Unknown current interest"
                );
                return;
            };

            cancellation_token.cancel();
            if let Some(interest) = Arc::into_inner(interest) {
                let owner_hat = &mut *south_hats[interest.src_bound];
                owner_hat.send_final_declaration(ctx, interest.src_interest_id, &interest.src);
            }
        }
    }

    #[tracing::instrument(level = "trace", skip_all, fields(wai = %self.whatami().short(), bnd = %self.bound))]
    fn send_declarations(
        &mut self,
        ctx: BaseContext,
        msg: &Interest,
        res: Option<&mut Arc<Resource>>,
    ) {
        if msg.options.subscribers() {
            self.declare_sub_interest(
                ctx.tables,
                ctx.src_face,
                msg.id,
                res.as_deref().cloned().as_mut(),
                msg.mode,
                msg.options.aggregate(),
                ctx.send_declare,
            )
        }
        if msg.options.queryables() {
            self.declare_qabl_interest(
                ctx.tables,
                ctx.src_face,
                msg.id,
                res.as_deref().cloned().as_mut(),
                msg.mode,
                msg.options.aggregate(),
                ctx.send_declare,
            )
        }
        if msg.options.tokens() {
            // Note: aggregation is forbidden for tokens. The flag is ignored.
            self.declare_token_interest(
                ctx.tables,
                ctx.src_face,
                msg.id,
                res.as_deref().cloned().as_mut(),
                msg.mode,
                ctx.send_declare,
            )
        }
    }

    #[tracing::instrument(level = "trace", skip_all, fields(wai = %self.whatami().short(), bnd = %self.bound))]
    fn send_final_declaration(&mut self, ctx: BaseContext, id: InterestId, src: &Remote) {
        // I should send a DeclareFinal to the source of the current interest identified by the given IID and FID

        let src_face = self.hat_remote(src);

        (ctx.send_declare)(
            &src_face.primitives,
            RoutingContext::new(Declare {
                interest_id: Some(id),
                ext_qos: interest::ext::QoSType::INTEREST,
                ext_tstamp: None,
                ext_nodeid: interest::ext::NodeIdType::DEFAULT,
                body: DeclareBody::DeclareFinal(DeclareFinal),
            }),
        );
    }

    #[tracing::instrument(level = "trace", skip_all, fields(wai = %self.whatami().short(), bnd = %self.bound))]
    fn register_interest(
        &mut self,
        ctx: BaseContext,
        msg: &Interest,
        res: Option<&mut Arc<Resource>>,
    ) {
        self.face_hat_mut(ctx.src_face).remote_interests.insert(
            msg.id,
            RemoteInterest {
                res: res.as_deref().cloned(),
                options: msg.options,
                mode: msg.mode,
            },
        );
    }

    #[tracing::instrument(level = "trace", skip_all, fields(wai = %self.whatami().short(), bnd = %self.bound))]
    fn unregister_interest(&mut self, ctx: BaseContext, msg: &Interest) -> Option<RemoteInterest> {
        assert!(!self.bound().is_north());
        self.assert_proper_ownership(&ctx);

        let Some(remote_interest) = self
            .face_hat_mut(ctx.src_face)
            .remote_interests
            .remove(&msg.id)
        else {
            tracing::error!(id = msg.id, "Unknown remote interest");
            return None;
        };

        if remote_interest.options.subscribers() {
            if remote_interest.options.aggregate() {
                if let Some(ires) = &remote_interest.res {
                    self.face_hat_mut(ctx.src_face)
                        .local_subs
                        .remove_aggregated_resource_interest(ires, msg.id);
                }
            } else {
                self.face_hat_mut(ctx.src_face)
                    .local_subs
                    .remove_simple_resource_interest(msg.id);
            }
        }
        if remote_interest.options.queryables() {
            if remote_interest.options.aggregate() {
                if let Some(ires) = &remote_interest.res {
                    self.face_hat_mut(ctx.src_face)
                        .local_qabls
                        .remove_aggregated_resource_interest(ires, msg.id);
                }
            } else {
                self.face_hat_mut(ctx.src_face)
                    .local_qabls
                    .remove_simple_resource_interest(msg.id);
            }
        }

        self.owned_faces(ctx.tables)
            .all(|face| {
                !self
                    .face_hat(face)
                    .remote_interests
                    .values()
                    .contains(&remote_interest)
            })
            .then_some(remote_interest)
    }
}
