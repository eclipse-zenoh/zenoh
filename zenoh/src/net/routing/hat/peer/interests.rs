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

use itertools::Itertools;
use zenoh_protocol::{
    core::WhatAmI,
    network::{
        declare::{self, queryable::ext::QueryableInfoType, QueryableId},
        interest::{self, InterestId, InterestMode},
        Declare, DeclareBody, DeclareFinal, DeclareQueryable, DeclareSubscriber, Interest,
    },
};
use zenoh_sync::get_mut_unchecked;

use super::{initial_interest, Hat, INITIAL_INTEREST_ID};
use crate::net::routing::{
    dispatcher::{
        face::InterestState,
        interests::{
            CurrentInterest, CurrentInterestCleanup, PendingCurrentInterest, RemoteInterest,
        },
        local_resources::LocalResourceInfoTrait,
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
        if ctx.src_face.remote_bound.is_north() {
            return;
        }

        for RemoteInterest { res, options, .. } in other_hats
            .values()
            .filter(|hat| hat.region().bound().is_south())
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
                f.remote_bound.is_south()
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
                    res.clone(),
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
            let wire_expr = res
                .as_ref()
                .map(|res| Resource::decl_key(res, &mut dst_face));
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
            .find(|f| f.remote_bound.is_south())
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

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region))]
    fn route_declare_final(
        &mut self,
        ctx: BaseContext,
        interest_id: InterestId,
    ) -> Option<CurrentInterest> {
        // I have received a Declare Final.
        // Either this is a peer initial interest finalizer, in which case I should own the (peer) face.
        // Or, the source face is a gateway, in which case there should be a pending current interest and I should be the north hat.

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

        if interest_id == INITIAL_INTEREST_ID {
            debug_assert!(self.owns(ctx.src_face));
            debug_assert!(ctx.src_face.remote_bound.is_north());

            // FIXME(regions): don't create start conditions for gateway peers
            zenoh_runtime::ZRuntime::Net.block_in_place(async move {
                if let Some(runtime) = &ctx.tables.runtime {
                    if let Some(runtime) = runtime.upgrade() {
                        tracing::debug!("Terminating peer connector");
                        runtime
                            .start_conditions()
                            .terminate_peer_connector_zid(ctx.src_face.zid)
                            .await
                    }
                }
            });
        } else {
            debug_assert!(self.owns(ctx.src_face));
            debug_assert!(self.region().bound().is_north());
            debug_assert!(ctx.src_face.region.bound().is_north());

            if ctx.src_face.remote_bound.is_north() {
                tracing::error!(
                    id = interest_id,
                    src = %ctx.src_face,
                    "Received current interest finalization from non-gateway face"
                );
                return None;
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
                return None;
            };

            cancellation_token.cancel();
            if let Some(interest) = Arc::into_inner(interest) {
                return Some(interest);
            }
        }

        None
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region))]
    fn send_declarations(
        &mut self,
        _ctx: BaseContext,
        _msg: &Interest,
        _res: Option<&mut Arc<Resource>>,
    ) {
        unimplemented!()
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region))]
    fn propagate_current_subscriptions(
        &self,
        ctx: BaseContext,
        msg: &Interest,
        res: Option<Arc<Resource>>,
        other_matches: HashMap<Arc<Resource>, SubscriberInfo>,
    ) {
        debug_assert!(self.owns(ctx.src_face));
        debug_assert!(ctx.src_face.region.bound().is_south());

        let mut matches = other_matches.into_keys();

        if msg.options.aggregate() && (msg.mode.current() || msg.mode.future()) {
            if let Some(aggregated_res) = &res {
                let (sub_id, sub_info) = if msg.mode.future() {
                    let face_hat_mut = self.face_hat_mut(ctx.src_face);

                    for sub in matches {
                        face_hat_mut.local_subs.insert_simple_resource(
                            sub.clone(),
                            SubscriberInfo,
                            || face_hat_mut.next_id.fetch_add(1, Ordering::SeqCst),
                            HashSet::new(),
                        );
                    }

                    face_hat_mut.local_subs.insert_aggregated_resource(
                        aggregated_res.clone(),
                        || face_hat_mut.next_id.fetch_add(1, Ordering::SeqCst),
                        HashSet::from_iter([msg.id]),
                    )
                } else {
                    (0, matches.next().map(|_| SubscriberInfo))
                };

                if msg.mode.current() && sub_info.is_some() {
                    // send declare only if there is at least one resource matching the aggregate
                    let wire_expr = Resource::decl_key(aggregated_res, ctx.src_face);
                    (ctx.send_declare)(
                        &ctx.src_face.primitives,
                        RoutingContext::with_expr(
                            Declare {
                                interest_id: Some(msg.id),
                                ext_qos: declare::ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                                body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                    id: sub_id,
                                    wire_expr,
                                }),
                            },
                            aggregated_res.expr().to_string(),
                        ),
                    );
                }
            }
        } else if !msg.options.aggregate() && msg.mode.current() {
            for sub in matches {
                let sub_id = if msg.mode.future() {
                    let face_hat_mut = self.face_hat_mut(ctx.src_face);
                    face_hat_mut
                        .local_subs
                        .insert_simple_resource(
                            sub.clone(),
                            SubscriberInfo,
                            || face_hat_mut.next_id.fetch_add(1, Ordering::SeqCst),
                            HashSet::from([msg.id]),
                        )
                        .0
                } else {
                    0
                };
                let wire_expr = Resource::decl_key(&sub, ctx.src_face);
                (ctx.send_declare)(
                    &ctx.src_face.primitives,
                    RoutingContext::with_expr(
                        Declare {
                            interest_id: Some(msg.id),
                            ext_qos: declare::ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                            body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                id: sub_id,
                                wire_expr,
                            }),
                        },
                        sub.expr().to_string(),
                    ),
                );
            }
        }
    }

    #[tracing::instrument(level = "trace", skip_all)]
    fn propagate_current_queryables(
        &self,
        ctx: BaseContext,
        msg: &Interest,
        res: Option<Arc<Resource>>,
        other_matches: HashMap<Arc<Resource>, QueryableInfoType>,
    ) {
        debug_assert!(self.owns(ctx.src_face));
        debug_assert!(ctx.src_face.region.bound().is_south());

        // NOTE(regions): we don't propagate the regions's entities in peer-to-peer mode
        let matches = other_matches;

        if msg.options.aggregate() && (msg.mode.current() || msg.mode.future()) {
            if let Some(aggregated_res) = &res {
                let (resource_id, qabl_info) = if msg.mode.future() {
                    for (qabl, qabl_info) in matches {
                        let face_hat_mut = self.face_hat_mut(ctx.src_face);
                        face_hat_mut.local_qabls.insert_simple_resource(
                            qabl.clone(),
                            qabl_info,
                            || face_hat_mut.next_id.fetch_add(1, Ordering::SeqCst),
                            HashSet::new(),
                        );
                    }
                    let face_hat_mut = self.face_hat_mut(ctx.src_face);
                    face_hat_mut.local_qabls.insert_aggregated_resource(
                        aggregated_res.clone(),
                        || face_hat_mut.next_id.fetch_add(1, Ordering::SeqCst),
                        HashSet::from_iter([msg.id]),
                    )
                } else {
                    (
                        QueryableId::default(),
                        QueryableInfoType::aggregate_many(
                            aggregated_res,
                            matches.iter().map(|(res, info)| (res, *info)),
                        ),
                    )
                };
                if let Some(ext_info) = msg.mode.current().then_some(qabl_info).flatten() {
                    // send declare only if there is at least one resource matching the aggregate
                    let wire_expr = Resource::decl_key(aggregated_res, ctx.src_face);
                    (ctx.send_declare)(
                        &ctx.src_face.primitives,
                        RoutingContext::with_expr(
                            Declare {
                                interest_id: Some(msg.id),
                                ext_qos: declare::ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                                body: DeclareBody::DeclareQueryable(DeclareQueryable {
                                    id: resource_id,
                                    wire_expr,
                                    ext_info,
                                }),
                            },
                            aggregated_res.expr().to_string(),
                        ),
                    );
                }
            }
        } else if !msg.options.aggregate() && msg.mode.current() {
            for (qabl, qabl_info) in matches {
                let resource_id = if msg.mode.future() {
                    let face_hat_mut = self.face_hat_mut(ctx.src_face);
                    face_hat_mut
                        .local_qabls
                        .insert_simple_resource(
                            qabl.clone(),
                            qabl_info,
                            || face_hat_mut.next_id.fetch_add(1, Ordering::SeqCst),
                            HashSet::from([msg.id]),
                        )
                        .0
                } else {
                    QueryableId::default()
                };
                let wire_expr = Resource::decl_key(&qabl, ctx.src_face);
                (ctx.send_declare)(
                    &ctx.src_face.primitives,
                    RoutingContext::with_expr(
                        Declare {
                            interest_id: Some(msg.id),
                            ext_qos: declare::ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                            body: DeclareBody::DeclareQueryable(DeclareQueryable {
                                id: resource_id,
                                wire_expr,
                                ext_info: qabl_info,
                            }),
                        },
                        qabl.expr().to_string(),
                    ),
                );
            }
        }
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region))]
    fn send_declare_final(&mut self, ctx: BaseContext, id: InterestId, src: &Remote) {
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

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region))]
    fn register_interest(&mut self, ctx: BaseContext, msg: &Interest, res: Option<Arc<Resource>>) {
        self.face_hat_mut(ctx.src_face).remote_interests.insert(
            msg.id,
            RemoteInterest {
                res,
                options: msg.options,
                mode: msg.mode,
            },
        );
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region))]
    fn unregister_interest(&mut self, ctx: BaseContext, msg: &Interest) -> Option<RemoteInterest> {
        debug_assert!(!self.region().bound().is_north());
        debug_assert!(self.owns(ctx.src_face));

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

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region))]
    fn remote_interests(&self, tables: &TablesData) -> HashSet<RemoteInterest> {
        if self.region().bound().is_north() {
            return HashSet::default();
        }

        self.owned_faces(tables)
            .flat_map(|face| self.face_hat(face).remote_interests.values().cloned())
            .collect()
    }
}
