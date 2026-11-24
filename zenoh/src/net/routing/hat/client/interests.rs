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
    gateway::SubscriberInfo,
    hat::{
        DispatcherContext, HatBaseTrait, HatInterestTrait, HatTrait, Remote,
        RouteCurrentDeclareResult, RouteInterestResult,
    },
    RoutingContext,
};
impl Hat {
    #[tracing::instrument(level = "debug", skip_all, ret)]
    pub(super) fn repropagate_interests(
        &self,
        ctx: DispatcherContext,
        other_hats: &RegionMap<&dyn HatTrait>,
    ) {
        for RemoteInterest { res, options, .. } in other_hats
            .values()
            .flat_map(|hat| hat.remote_interests(ctx.tables))
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
                        ext_qos: interest::ext::QoSType::INTEREST,
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
    #[tracing::instrument(level = "debug", skip(ctx, msg), fields(%src), ret)]
    fn route_interest(
        &mut self,
        ctx: DispatcherContext,
        msg: &Interest,
        res: Option<Arc<Resource>>,
        src: &Remote,
    ) -> RouteInterestResult {
        use RouteInterestResult::*;

        debug_assert!(self.region().bound().is_north());
        debug_assert!(ctx.src_face.region.bound().is_south());

        let interest = Arc::new(CurrentInterest {
            src: src.clone(),
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

            if msg.mode.is_future() {
                let dst_face_id = dst_face.id;
                let is_finalized = msg.mode == InterestMode::Future;
                get_mut_unchecked(&mut dst_face).local_interests.insert(
                    id,
                    InterestState::new(dst_face_id, msg.options, res.clone(), is_finalized),
                );
            }

            if msg.mode.is_current() {
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
            tracing::debug!("Client region is empty");
        }

        if msg.mode.is_current() && Arc::into_inner(interest).is_some() {
            ResolvedCurrentInterest
        } else {
            Noop
        }
    }

    #[tracing::instrument(level = "debug", skip(ctx, _msg), ret)]
    fn route_interest_final(
        &mut self,
        ctx: DispatcherContext,
        _msg: &Interest,
        remote_interest: &RemoteInterest,
    ) {
        debug_assert!(self.region().bound().is_north());
        debug_assert!(ctx.src_face.region.bound().is_south());

        // NOTE(regions): clients have at most one north-bound remote, this invariant is enforced in
        // the orchestrator.
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

    #[tracing::instrument(level = "debug", skip(ctx), ret)]
    fn route_declare_final(
        &mut self,
        ctx: DispatcherContext,
        interest_id: InterestId,
    ) -> RouteCurrentDeclareResult {
        use RouteCurrentDeclareResult::*;

        debug_assert!(self.region().bound().is_north());
        debug_assert!(ctx.src_face.region.bound().is_north());

        let has_local_interest = get_mut_unchecked(ctx.src_face)
            .local_interests
            .get_mut(&interest_id)
            .map(|i| i.set_finalized())
            .is_some();

        let breadcrumb = get_mut_unchecked(ctx.src_face)
            .pending_current_interests
            .remove(&interest_id);

        match (has_local_interest, breadcrumb) {
            (false, None) => {
                tracing::error!("Unknown interest");
                Noop
            }
            (true, None) => NoBreadcrumb,
            (_, Some(breadcrumb)) => {
                let Some(interest) = Arc::into_inner(breadcrumb.interest) else {
                    bug!("Pending current interest with strong count > 1");
                    return Noop;
                };

                breadcrumb.cancellation_token.cancel();
                Breadcrumb { interest }
            }
        }
    }

    #[tracing::instrument(level = "debug", skip(ctx), ret)]
    fn route_current_token(
        &mut self,
        ctx: DispatcherContext,
        interest_id: InterestId,
        _res: Arc<Resource>,
    ) -> RouteCurrentDeclareResult {
        use RouteCurrentDeclareResult::*;

        debug_assert!(self.region().bound().is_north());
        debug_assert!(ctx.src_face.region.bound().is_north());

        let has_local_interest = ctx.src_face.local_interests.contains_key(&interest_id);

        let breadcrumb = ctx
            .src_face
            .pending_current_interests
            .get(&interest_id)
            .map(|i| i.interest.as_ref().clone());

        match (has_local_interest, breadcrumb) {
            (false, None) => {
                tracing::error!("Unknown interest");
                Noop
            }
            (true, None) => NoBreadcrumb,
            (_, Some(interest)) => Breadcrumb { interest },
        }
    }

    fn send_current_subscribers(
        &self,
        _ctx: DispatcherContext,
        _msg: &Interest,
        _res: Option<Arc<Resource>>,
        _other_matches: HashMap<Arc<Resource>, SubscriberInfo>,
    ) {
        unreachable!("south-bound client hat")
    }

    fn send_current_queryables(
        &self,
        _ctx: DispatcherContext,
        _msg: &Interest,
        _res: Option<Arc<Resource>>,
        _other_matches: HashMap<Arc<Resource>, QueryableInfoType>,
    ) {
        unreachable!("south-bound client hat")
    }

    fn send_current_tokens(
        &self,
        _ctx: DispatcherContext,
        _msg: &Interest,
        _res: Option<Arc<Resource>>,
        _other_matches: HashSet<Arc<Resource>>,
    ) {
        unreachable!("south-bound client hat")
    }

    fn propagate_current_token(
        &self,
        _ctx: DispatcherContext,
        _res: Arc<Resource>,
        _interest: CurrentInterest,
    ) {
        unreachable!("south-bound client hat")
    }

    fn send_declare_final(&mut self, _ctx: DispatcherContext, _id: InterestId, _src: &Remote) {
        unreachable!("south-bound client hat")
    }

    fn register_interest(
        &mut self,
        _ctx: DispatcherContext,
        _msg: &Interest,
        _res: Option<Arc<Resource>>,
    ) {
        unreachable!("south-bound client hat")
    }

    fn unregister_interest(
        &mut self,
        _ctx: DispatcherContext,
        _msg: &Interest,
    ) -> Option<RemoteInterest> {
        unreachable!("south-bound client hat");
    }

    #[tracing::instrument(level = "trace", skip(_tables), ret)]
    fn remote_interests(&self, _tables: &TablesData) -> HashSet<RemoteInterest> {
        HashSet::default()
    }
}
