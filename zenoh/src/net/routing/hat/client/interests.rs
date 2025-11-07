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
    collections::{HashMap, HashSet},
    sync::{atomic::Ordering, Arc},
};

use zenoh_protocol::network::{
    declare::queryable::ext::QueryableInfoType,
    interest::{self, InterestId, InterestMode},
    Interest,
};
use zenoh_sync::get_mut_unchecked;

use super::Hat;
use crate::net::routing::{
    dispatcher::{
        face::InterestState,
        interests::{
            CurrentInterest, CurrentInterestCleanup, PendingCurrentInterest, RemoteInterest,
        },
        region::RegionMap,
        resource::Resource,
        tables::TablesData,
    },
    hat::{BaseContext, CurrentFutureTrait, HatBaseTrait, HatInterestTrait, HatTrait, Remote},
    router::SubscriberInfo,
    RoutingContext,
};
impl Hat {
    pub(super) fn interests_new_face(
        &self,
        ctx: BaseContext,
        other_hats: &RegionMap<&dyn HatTrait>,
    ) {
        for RemoteInterest { res, options, .. } in other_hats
            .values()
            .filter(|hat| hat.region().bound().is_south())
            .flat_map(|hat| hat.remote_interests(&ctx.tables))
        {
            let id = self
                .face_hat(ctx.src_face)
                .next_id
                .fetch_add(1, Ordering::SeqCst);
            let face_id = ctx.src_face.id;
            get_mut_unchecked(ctx.src_face)
                .local_interests
                .insert(id, InterestState::new(face_id, options, res.clone(), false));
            let wire_expr = res
                .as_ref()
                .map(|res| Resource::decl_key(res, ctx.src_face));
            ctx.src_face
                .primitives
                .send_interest(RoutingContext::with_expr(
                    &mut Interest {
                        id,
                        mode: InterestMode::CurrentFuture,
                        options,
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

impl HatInterestTrait for Hat {
    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region))]
    fn route_interest(
        &mut self,
        ctx: BaseContext,
        msg: &Interest,
        res: Option<Arc<Resource>>,
        remote: &Remote,
    ) -> Option<CurrentInterest> {
        // I have received an interest with mode != FINAL.
        // I should be the north hat.
        // The face cannot be bound to me, so it must be south-bound. In which case the msg originates in a subregion (for which I am the gateway):
        //   1. If I have a gateway, I should re-propagate the interest to it.
        //   2. If the interest is current, I need to send all current declarations in the (south) owner hat.
        //   3. If the interest is future, I need to register it as a remote interest in the (south) owner hat.

        debug_assert!(self.region().bound().is_north());
        debug_assert!(ctx.src_face.region.bound().is_south());

        let interest = Arc::new(CurrentInterest {
            src: remote.clone(),
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
                    res.clone(),
                    msg.mode == InterestMode::Future,
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

            let wire_expr = res
                .as_ref()
                .map(|res| Resource::decl_key(res, &mut dst_face));
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
                "Client region is empty. Will not propagate interest north"
            );
        }

        if msg.mode.current() {
            if let Some(interest) = Arc::into_inner(interest) {
                return Some(interest);
            }
        }

        None
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region))]
    fn route_interest_final(
        &mut self,
        ctx: BaseContext,
        _msg: &Interest,
        remote_interest: &RemoteInterest,
    ) {
        // I have received an interest with mode FINAL.
        // I should be the north hat.
        // The face cannot be bound to me, so it must be south-bound. In which case the msg originates in a subregion (for which I am the gateway):
        //   1. I need to unregister it as a remote interest in the owner (south) hat.
        //   2. If I have a gateway, I should re-propagate the FINAL interest to it iff no other subregion has the same remote interest.

        debug_assert!(self.region().bound().is_north());
        debug_assert!(ctx.src_face.region.bound().is_south());

        if let Some(dst_face) = self
            .owned_faces_mut(ctx.tables)
            .next()
            .map(get_mut_unchecked)
        {
            dst_face.local_interests.retain(|id, local_interest| {
                if local_interest == remote_interest {
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

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region))]
    fn route_declare_final(
        &mut self,
        ctx: BaseContext,
        interest_id: InterestId,
    ) -> Option<CurrentInterest> {
        // I have received a Declare Final.
        // The source face must be a gateway, in which case there should be a pending current interest and I should be the north hat.

        debug_assert!(self.region().bound().is_north());
        debug_assert!(ctx.src_face.region.bound().is_north());

        if let Some(interest) = get_mut_unchecked(ctx.src_face)
            .local_interests
            .get_mut(&interest_id)
        {
            interest.set_finalized();
            tracing::trace!(?interest, "Finalized interest");
        } else {
            tracing::error!(
                id = interest_id,
                src = %ctx.src_face,
                "Unknown local interest"
            );
            return None;
        };

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
            return None;
        };

        cancellation_token.cancel();
        Arc::into_inner(interest)
    }

    fn send_declarations(
        &mut self,
        _ctx: BaseContext,
        _msg: &Interest,
        _res: Option<&mut Arc<Resource>>,
    ) {
        unimplemented!()
    }

    fn propagate_current_subscriptions(
        &self,
        _ctx: BaseContext,
        _msg: &Interest,
        _res: Option<Arc<Resource>>,
        _other_matches: HashMap<Arc<Resource>, SubscriberInfo>,
    ) {
        // The client hat is always the north hat, thus it shouldn't receive interests
        unreachable!()
    }

    fn propagate_current_queryables(
        &self,
        _ctx: BaseContext,
        _msg: &Interest,
        _res: Option<Arc<Resource>>,
        _other_matches: HashMap<Arc<Resource>, QueryableInfoType>,
    ) {
        unreachable!()
    }

    fn send_declare_final(&mut self, _ctx: BaseContext, _id: InterestId, _src: &Remote) {
        // The client hat is always the north hat, thus it shouldn't receive interests
        unreachable!()
    }

    fn register_interest(
        &mut self,
        _ctx: BaseContext,
        _msg: &Interest,
        _res: Option<Arc<Resource>>,
    ) {
        // The client hat is always the north hat, thus it shouldn't receive interests
        unreachable!()
    }

    fn unregister_interest(
        &mut self,
        _ctx: BaseContext,
        _msg: &Interest,
    ) -> Option<RemoteInterest> {
        // The client hat is always the north hat, thus it shouldn't receive interests
        unreachable!()
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region))]
    fn remote_interests(&self, _tables: &TablesData) -> HashSet<RemoteInterest> {
        // The client hat is always the north hat, thus it shouldn't receive interests
        HashSet::default()
    }
}
