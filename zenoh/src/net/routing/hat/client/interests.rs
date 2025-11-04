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
        declare,
        interest::{self, InterestId, InterestMode},
        Declare, DeclareBody, DeclareFinal, Interest,
    },
};
use zenoh_sync::get_mut_unchecked;

use super::Hat;
use crate::net::routing::{
    dispatcher::{
        face::{FaceState, InterestState},
        interests::{
            CurrentInterest, CurrentInterestCleanup, PendingCurrentInterest, RemoteInterest,
        },
        region::RegionMap,
        resource::Resource,
        tables::TablesData,
    },
    hat::{BaseContext, CurrentFutureTrait, HatBaseTrait, HatInterestTrait, HatTrait, Remote},
    RoutingContext,
};
impl Hat {
    pub(super) fn interests_new_face(&self, tables: &mut TablesData, face: &mut Arc<FaceState>) {
        if face.whatami != WhatAmI::Client {
            for mut src_face in self
                .faces(tables)
                .values()
                .cloned()
                .collect::<Vec<Arc<FaceState>>>()
            {
                for RemoteInterest { res, options, .. } in
                    self.face_hat_mut(&mut src_face).remote_interests.values()
                {
                    let id = self.face_hat(face).next_id.fetch_add(1, Ordering::SeqCst);
                    let face_id = face.id;
                    get_mut_unchecked(face).local_interests.insert(
                        id,
                        InterestState::new(face_id, *options, res.clone(), false),
                    );

                    let wire_expr = res.as_ref().map(|res| Resource::decl_key(res, face, true));
                    face.primitives.send_interest(RoutingContext::with_expr(
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

impl HatInterestTrait for Hat {
    #[tracing::instrument(level = "trace", skip_all, fields(wai = %self.whatami().short(), bnd = %self.region))]
    fn route_interest(
        &mut self,
        mut ctx: BaseContext,
        msg: &Interest,
        res: Option<&mut Arc<Resource>>,
        mut south_hats: RegionMap<&mut dyn HatTrait>,
    ) {
        // I have received an interest with mode != FINAL.
        // I should be the north hat.
        // The face cannot be bound to me, so it must be south-bound. In which case the msg originates in a subregion (for which I am the gateway):
        //   1. If I have a gateway, I should re-propagate the interest to it.
        //   2. If the interest is current, I need to send all current declarations in the (south) owner hat.
        //   3. If the interest is future, I need to register it as a remote interest in the (south) owner hat.

        assert!(self.region().bound().is_north());
        assert!(ctx.src_face.region.bound().is_south());

        let owner_hat = &mut *south_hats[ctx.src_face.region];

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
            src_region: ctx.src_face.region,
            src_interest_id: msg.id,
            mode: msg.mode,
        });

        let interests_timeout = ctx.tables.interests_timeout;

        if let Some(mut dst_face) = self.owned_faces(ctx.tables).next().cloned() {
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
                    msg.mode == InterestMode::Future,
                ),
            );

            if msg.mode.current() && msg.options.tokens() {
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

            let wire_expr = res
                .as_ref()
                .map(|res| Resource::decl_key(res, &mut dst_face, true));
            dst_face.primitives.send_interest(RoutingContext::with_expr(
                &mut Interest {
                    id,
                    mode: msg.mode,
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
        } else {
            tracing::debug!(
                id = msg.id,
                src = %ctx.src_face,
                "No client gateway found. Will not propagate interest"
            );
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

    #[tracing::instrument(level = "trace", skip_all, fields(wai = %self.whatami().short(), bnd = %self.region))]
    fn route_interest_final(
        &mut self,
        mut ctx: BaseContext,
        msg: &Interest,
        mut south_hats: RegionMap<&mut dyn HatTrait>,
    ) {
        // I have received an interest with mode FINAL.
        // I should be the north hat.
        // The face cannot be bound to me, so it must be south-bound. In which case the msg originates in a subregion (for which I am the gateway):
        //   1. I need to unregister it as a remote interest in the owner (south) hat.
        //   2. If I have a gateway, I should re-propagate the FINAL interest to it iff no other subregion has the same remote interest.

        assert!(self.region().bound().is_north());
        assert!(ctx.src_face.region.bound().is_south());

        // FIXME(regions): check if any subregion has the same remote interest before propagating the interest final

        let owner_hat = &mut *south_hats[ctx.src_face.region];

        let Some(remote_interest) = owner_hat.unregister_interest(ctx.reborrow(), msg) else {
            tracing::error!(id = msg.id, "Unknown remote interest");
            return;
        };

        if let Some(dst_face) = self
            .owned_faces_mut(ctx.tables)
            .next()
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
                            ext_qos: interest::ext::QoSType::DECLARE,
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

    fn route_final_declaration(
        &mut self,
        ctx: BaseContext,
        interest_id: InterestId,
        mut south_hats: RegionMap<&mut dyn HatTrait>,
    ) {
        // I have received a Declare Final.
        // The source face must be a gateway, in which case there should be a pending current interest and I should be the north hat.

        assert!(self.region().bound().is_north());
        assert!(ctx.src_face.region.bound().is_north());

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
            let owner_hat = &mut *south_hats[interest.src_region];
            owner_hat.send_final_declaration(ctx, interest.src_interest_id, &interest.src);
        }
    }

    #[tracing::instrument(level = "trace", skip_all, fields(wai = %self.whatami().short(), bnd = %self.region))]
    fn send_declarations(
        &mut self,
        ctx: BaseContext,
        msg: &Interest,
        res: Option<&mut Arc<Resource>>,
    ) {
        // FIXME(regions): send subscribers and queryables as well

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

        if msg.mode.future() {
            self.face_hat_mut(ctx.src_face).remote_interests.insert(
                msg.id,
                RemoteInterest {
                    res: res.as_deref().cloned(),
                    options: msg.options,
                    mode: msg.mode,
                },
            );
        }
    }

    #[tracing::instrument(level = "trace", skip_all, fields(wai = %self.whatami().short(), bnd = %self.region))]
    fn send_final_declaration(&mut self, ctx: BaseContext, id: InterestId, src: &Remote) {
        // I should send a DeclareFinal to the source of the current interest identified by the given IID and FID

        let src_face = self.hat_remote(src);

        (ctx.send_declare)(
            &src_face.primitives,
            RoutingContext::new(Declare {
                interest_id: Some(id),
                ext_qos: declare::ext::QoSType::DECLARE,
                ext_tstamp: None,
                ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                body: DeclareBody::DeclareFinal(DeclareFinal),
            }),
        );
    }

    #[tracing::instrument(level = "trace", skip_all, fields(wai = %self.whatami().short(), bnd = %self.region))]
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

    #[tracing::instrument(level = "trace", skip_all, fields(wai = %self.whatami().short(), bnd = %self.region))]
    fn unregister_interest(&mut self, ctx: BaseContext, msg: &Interest) -> Option<RemoteInterest> {
        assert!(self.region().bound().is_south());
        assert!(self.owns(&ctx.src_face));

        let Some(remote_interest) = self
            .face_hat_mut(ctx.src_face)
            .remote_interests
            .remove(&msg.id)
        else {
            tracing::error!(id = msg.id, "Unknown remote interest");
            return None;
        };

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
