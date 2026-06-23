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
#[allow(unused_imports)]
use zenoh_core::polyfill::*;
use zenoh_protocol::network::{
    declare::{self, queryable::ext::QueryableInfoType, QueryableId, TokenId},
    interest::{self, InterestId, InterestMode},
    Declare, DeclareBody, DeclareFinal, DeclareQueryable, DeclareSubscriber, DeclareToken,
    Interest,
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
    #[allow(clippy::incompatible_msrv)]
    #[tracing::instrument(level = "debug", skip(ctx, msg), fields(%src), ret)]
    fn route_interest(
        &mut self,
        ctx: DispatcherContext,
        msg: &Interest,
        res: Option<Arc<Resource>>,
        src: &Remote,
    ) -> RouteInterestResult {
        use RouteInterestResult::*;

        debug_assert_ne!(msg.mode, InterestMode::Final);
        debug_assert!(self.region().bound().is_north());
        debug_assert_implies!(
            ctx.src_face.region.bound().is_north(),
            ctx.src_face.whatami.is_peer()
        );
        debug_assert_implies!(msg.mode == InterestMode::Current, msg.options.tokens());

        if ctx.src_face.region.bound().is_north() {
            return ResolvedCurrentInterest;
        }

        // NOTE(regions): `Current` (and not `CurrentFuture`) interests are stateless, i.e. gateways
        // don't register inbound/outbound token propagation. For this reason, we pick at most one
        // destination for `Current` interests; otherwise we would wind up with duplicate tokens.
        let dsts = if msg.mode == InterestMode::Current {
            // NOTE(regions): we don't send current-future interest to peers with unfinalized
            // initial interests. After finalization, we will be notified with potential tokens
            // and propagate them as future tokens.
            self.owned_faces(ctx.tables)
                .filter(|p| {
                    p.id != ctx.src_face.id
                        && p.remote_bound.is_north()
                        && !initial_interest(p).is_none_or(|i| i.finalized)
                })
                .chain(
                    self.owned_faces(ctx.tables)
                        .find(|f| f.remote_bound.is_south()),
                )
                .cloned()
                .collect_vec()
            // FIXME(regions): we may get duplicate tokens when sending to one gateway _and_ (at
            // least) one peer with unfinalized initial interest.
        } else {
            self.owned_faces(ctx.tables)
                .filter(|f| f.remote_bound.is_south())
                .cloned()
                .collect_vec()
        };

        let interest = Arc::new(CurrentInterest {
            src: src.clone(),
            src_region: ctx.src_face.region,
            src_interest_id: msg.id,
            mode: msg.mode,
        });

        for mut dst in dsts {
            let id = self.face_hat(&dst).next_id.fetch_add(1, Ordering::SeqCst);

            if msg.mode.is_future() {
                let dst_id = dst.id;
                let is_finalized = msg.mode == InterestMode::Future;
                get_mut_unchecked(&mut dst).local_interests.insert(
                    id,
                    InterestState::new(dst_id, msg.options, res.clone(), is_finalized),
                );
            }

            if msg.mode.is_current() {
                let dst_face_mut = get_mut_unchecked(&mut dst);
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
                    &dst,
                    ctx.tables_lock,
                    id,
                    ctx.tables.interests_timeout,
                );
            }
            let wire_expr = res.as_ref().map(|res| Resource::decl_key(res, &mut dst));
            dst.primitives.send_interest(RoutingContext::with_expr(
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

    #[tracing::instrument(level = "debug", skip(ctx), ret)]
    fn route_declare_final(
        &mut self,
        ctx: DispatcherContext,
        interest_id: InterestId,
    ) -> RouteCurrentDeclareResult {
        use RouteCurrentDeclareResult::*;

        debug_assert!(self.owns(ctx.src_face));
        debug_assert!(self.region().bound().is_north());
        debug_assert!(ctx.src_face.region.bound().is_north());
        // NOTE(regions): the reverse implication doesn't hold: peer regions may exchange interests
        // while initial interests are unfinalized.
        debug_assert_implies!(
            interest_id == INITIAL_INTEREST_ID,
            ctx.src_face.remote_bound.is_north()
        );

        let has_local_interest = get_mut_unchecked(ctx.src_face)
            .local_interests
            .get_mut(&interest_id)
            .map(|i| i.set_finalized())
            .is_some();

        if interest_id == INITIAL_INTEREST_ID {
            if !has_local_interest {
                bug!("Unknown initial interest");
                return Noop;
            }

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

            Noop
        } else {
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
                        // NOTE(regions): this event occurs as a result of current interests that,
                        // in addition to hitting the gateway, also hit peers with non-finalized
                        // initial interests.
                        tracing::debug!("Pending current interest not yet resolved");
                        return Noop;
                    };

                    breadcrumb.cancellation_token.cancel();
                    Breadcrumb { interest }
                }
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

    #[tracing::instrument(level = "debug", skip(ctx, msg), ret)]
    fn send_current_subscribers(
        &self,
        ctx: DispatcherContext,
        msg: &Interest,
        res: Option<Arc<Resource>>,
        other_matches: HashMap<Arc<Resource>, SubscriberInfo>,
    ) {
        debug_assert!(self.owns(ctx.src_face));
        debug_assert!(ctx.src_face.region.bound().is_south());
        debug_assert_ne!(msg.mode, InterestMode::Final);

        let mut matches = other_matches.into_keys();

        if msg.options.aggregate() {
            if let Some(aggregated_res) = &res {
                let (sub_id, sub_info) = if msg.mode.is_future() {
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

                if msg.mode.is_current() && sub_info.is_some() {
                    // send declare only if there is at least one resource matching the aggregate
                    let wire_expr = Resource::decl_key(aggregated_res, ctx.src_face);
                    tracing::debug!(dst = %ctx.src_face);
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
        } else if !msg.options.aggregate() && msg.mode.is_current() {
            for sub in matches {
                let sub_id = if msg.mode.is_future() {
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
                tracing::debug!(dst = %ctx.src_face);
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

    #[tracing::instrument(level = "debug", skip(ctx, msg), ret)]
    fn send_current_queryables(
        &self,
        ctx: DispatcherContext,
        msg: &Interest,
        res: Option<Arc<Resource>>,
        other_matches: HashMap<Arc<Resource>, QueryableInfoType>,
    ) {
        debug_assert!(self.owns(ctx.src_face));
        debug_assert!(ctx.src_face.region.bound().is_south());
        debug_assert_ne!(msg.mode, InterestMode::Final);

        // NOTE(regions): we don't propagate the regions's entities in peer-to-peer mode
        let matches = other_matches;

        if msg.options.aggregate() {
            if let Some(aggregated_res) = &res {
                let (resource_id, qabl_info) = if msg.mode.is_future() {
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
                if let Some(ext_info) = msg.mode.is_current().then_some(qabl_info).flatten() {
                    // send declare only if there is at least one resource matching the aggregate
                    let wire_expr = Resource::decl_key(aggregated_res, ctx.src_face);
                    tracing::debug!(dst = %ctx.src_face);
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
        } else if !msg.options.aggregate() && msg.mode.is_current() {
            for (qabl, qabl_info) in matches {
                let resource_id = if msg.mode.is_future() {
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
                tracing::debug!(dst = %ctx.src_face);
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

    #[tracing::instrument(level = "debug", skip(ctx, msg), ret)]
    fn send_current_tokens(
        &self,
        ctx: DispatcherContext,
        msg: &Interest,
        res: Option<Arc<Resource>>,
        other_matches: HashSet<Arc<Resource>>,
    ) {
        debug_assert!(self.owns(ctx.src_face));
        debug_assert_implies!(
            ctx.src_face.region.bound().is_north(),
            ctx.src_face.whatami.is_peer()
        );

        let matches = other_matches.into_iter();

        for token in matches {
            let id = if msg.mode.is_future() {
                let face_hat = self.face_hat_mut(ctx.src_face);

                *face_hat
                    .local_tokens
                    .entry(token.clone())
                    .or_insert_with(|| face_hat.next_id.fetch_add(1, Ordering::SeqCst))
            } else {
                TokenId::default()
            };

            let wire_expr = Resource::decl_key(&token, ctx.src_face);
            tracing::debug!(dst = %ctx.src_face);
            (ctx.send_declare)(
                &ctx.src_face.primitives,
                RoutingContext::with_expr(
                    Declare {
                        interest_id: Some(msg.id),
                        ext_qos: declare::ext::QoSType::DECLARE,
                        ext_tstamp: None,
                        ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                        body: DeclareBody::DeclareToken(DeclareToken { id, wire_expr }),
                    },
                    token.expr().to_string(),
                ),
            );
        }
    }

    #[tracing::instrument(level = "debug", skip(ctx), ret)]
    fn propagate_current_token(
        &self,
        ctx: DispatcherContext,
        res: Arc<Resource>,
        interest: CurrentInterest,
    ) {
        debug_assert!(self.region().bound().is_south());

        let mut dst = self.hat_remote(&interest.src).clone();

        debug_assert!(dst.region.bound().is_south());

        if self.face_hat(&dst).local_tokens.contains_key(&res) {
            tracing::debug!("Already propagated");
            return;
        }

        let id = if interest.mode.is_future() {
            let id = self.face_hat(&dst).next_id.fetch_add(1, Ordering::SeqCst);
            self.face_hat_mut(&mut dst)
                .local_tokens
                .insert(res.clone(), id);
            id
        } else {
            TokenId::default()
        };

        let wire_expr = Resource::decl_key(&res, &mut dst.clone());
        tracing::debug!(dst = %dst);
        (ctx.send_declare)(
            &dst.primitives,
            RoutingContext::with_expr(
                Declare {
                    interest_id: Some(interest.src_interest_id),
                    ext_qos: declare::ext::QoSType::DECLARE,
                    ext_tstamp: None,
                    ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                    body: DeclareBody::DeclareToken(DeclareToken { id, wire_expr }),
                },
                res.expr().to_string(),
            ),
        );
    }

    #[tracing::instrument(level = "debug", skip(ctx, dst), ret)]
    fn send_declare_final(&mut self, ctx: DispatcherContext, id: InterestId, dst: &Remote) {
        // I should send a DeclareFinal to the source of the current interest identified by the given IID and FID

        let dst_face = self.hat_remote(dst);

        tracing::debug!(dst = %dst_face);
        (ctx.send_declare)(
            &dst_face.primitives,
            RoutingContext::new(Declare {
                interest_id: Some(id),
                ext_qos: declare::ext::QoSType::INTEREST,
                ext_tstamp: None,
                ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                body: DeclareBody::DeclareFinal(DeclareFinal),
            }),
        );
    }

    #[tracing::instrument(level = "debug", skip(ctx, msg), ret)]
    fn register_interest(
        &mut self,
        ctx: DispatcherContext,
        msg: &Interest,
        res: Option<Arc<Resource>>,
    ) {
        if self
            .face_hat_mut(ctx.src_face)
            .remote_interests
            .contains_key(&msg.id)
        {
            tracing::error!("Interest ids cannot be re-used");
            return;
        }

        self.face_hat_mut(ctx.src_face).remote_interests.insert(
            msg.id,
            RemoteInterest {
                res,
                options: msg.options,
                mode: msg.mode,
            },
        );
    }

    #[tracing::instrument(level = "debug", skip(ctx, msg), fields(id = msg.id), ret)]
    fn unregister_interest(
        &mut self,
        ctx: DispatcherContext,
        msg: &Interest,
    ) -> Option<RemoteInterest> {
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

    #[tracing::instrument(level = "trace", skip(tables), ret)]
    fn remote_interests(&self, tables: &TablesData) -> HashSet<RemoteInterest> {
        if self.region().bound().is_north() {
            return HashSet::default();
        }

        self.owned_faces(tables)
            .flat_map(|face| self.face_hat(face).remote_interests.values().cloned())
            .collect()
    }
}
