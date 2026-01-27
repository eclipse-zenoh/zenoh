//
// Copyright (c) 2025 ZettaScale Technology
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
    declare::{self, queryable::ext::QueryableInfoType, QueryableId, SubscriberId, TokenId},
    interest::InterestId,
    Declare, DeclareBody, DeclareFinal, DeclareQueryable, DeclareSubscriber, DeclareToken,
    Interest,
};

use super::Hat;
use crate::net::routing::{
    dispatcher::{
        interests::{CurrentInterest, RemoteInterest},
        local_resources::LocalResourceInfoTrait,
        queries::merge_qabl_infos,
        resource::Resource,
        tables::TablesData,
    },
    hat::{BaseContext, HatBaseTrait, HatInterestTrait, Remote},
    router::SubscriberInfo,
    RoutingContext,
};

impl HatInterestTrait for Hat {
    fn route_interest(
        &mut self,
        _ctx: BaseContext,
        _msg: &Interest,
        _res: Option<Arc<Resource>>,
        _remote: &Remote,
    ) -> Option<CurrentInterest> {
        // The broker hat is never the north hat, thus it doesn't route interests
        unreachable!()
    }

    fn route_interest_final(
        &mut self,
        _ctx: BaseContext,
        _msg: &Interest,
        _remote_interest: &RemoteInterest,
    ) {
        // The broker hat is never the north hat, thus it doesn't route interests
        unreachable!()
    }

    fn route_declare_final(
        &mut self,
        _ctx: BaseContext,
        _interest_id: InterestId,
    ) -> Option<CurrentInterest> {
        // The broker hat is never the north hat, thus it doesn't route current declarations
        unreachable!()
    }

    fn route_current_token(
        &mut self,
        _ctx: BaseContext,
        _interest_id: InterestId,
        _res: Arc<Resource>,
    ) -> Option<CurrentInterest> {
        // The broker hat is never the north hat, thus it doesn't route current declarations
        unreachable!()
    }

    #[allow(clippy::incompatible_msrv)]
    #[tracing::instrument(level = "trace", skip(ctx, msg))]
    fn send_current_subscriptions(
        &self,
        ctx: BaseContext,
        msg: &Interest,
        res: Option<Arc<Resource>>,
        mut other_matches: HashMap<Arc<Resource>, SubscriberInfo>,
    ) {
        debug_assert!(self.owns(ctx.src_face));
        debug_assert!(ctx.src_face.region.bound().is_south());

        let src_fid = ctx.src_face.id;

        let matches = {
            other_matches.extend(
                self.owned_faces(ctx.tables)
                    .filter(|face| face.id != src_fid)
                    .flat_map(|face| self.face_hat(face).remote_subs.values())
                    .filter(|sub| res.as_ref().is_none_or(|res| res.matches(sub)))
                    .cloned()
                    .zip(std::iter::repeat(SubscriberInfo)),
            );

            other_matches
        };
        let mut matches = matches.into_keys();

        if msg.options.aggregate() && (msg.mode.is_current() || msg.mode.is_future()) {
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
                    (
                        SubscriberId::default(),
                        matches.next().map(|_| SubscriberInfo),
                    )
                };

                if msg.mode.is_current() && sub_info.is_some() {
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
                    SubscriberId::default()
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

    #[allow(clippy::incompatible_msrv)]
    #[tracing::instrument(level = "trace", skip(ctx, msg))]
    fn send_current_queryables(
        &self,
        ctx: BaseContext,
        msg: &Interest,
        res: Option<Arc<Resource>>,
        other_matches: HashMap<Arc<Resource>, QueryableInfoType>,
    ) {
        debug_assert!(self.owns(ctx.src_face));
        debug_assert!(ctx.src_face.region.bound().is_south());

        let matches: HashMap<Arc<Resource>, QueryableInfoType> = other_matches
            .into_iter()
            .chain(
                self.owned_faces(ctx.tables)
                    .filter(|f| f.id != ctx.src_face.id)
                    .flat_map(|f| self.face_hat(f).remote_qabls.values().cloned())
                    .filter(|(r, _)| {
                        r.ctx.is_some() && res.as_ref().is_none_or(|res| res.matches(r))
                    }),
            )
            .fold(HashMap::new(), |mut acc, (res, info)| {
                acc.entry(res)
                    .and_modify(|i| {
                        *i = merge_qabl_infos(*i, info);
                    })
                    .or_insert(info);
                acc
            });

        if msg.options.aggregate() && (msg.mode.is_current() || msg.mode.is_future()) {
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

    #[allow(clippy::incompatible_msrv)]
    #[tracing::instrument(level = "trace", skip(ctx, msg), ret)]
    fn send_current_tokens(
        &self,
        ctx: BaseContext,
        msg: &Interest,
        res: Option<Arc<Resource>>,
        mut other_matches: HashSet<Arc<Resource>>,
    ) {
        debug_assert!(self.owns(ctx.src_face));
        debug_assert!(ctx.src_face.region.bound().is_south());
        debug_assert!(!msg.options.aggregate());

        let matches = {
            // NOTE(regions): we don't exclude inbound tokens from the src face as the API includes
            // session-local tokens in liveliness queries/subscribers.
            other_matches.extend(
                self.owned_faces(ctx.tables)
                    .flat_map(|face| self.face_hat(face).remote_tokens.values())
                    .filter(|token| res.as_ref().is_none_or(|res| res.matches(token)))
                    .cloned(),
            );

            other_matches.into_iter()
        };

        for token in matches {
            // TODO(regions*): apply this everywhere else
            if self
                .face_hat(ctx.src_face)
                .local_tokens
                .contains_key(&token)
            {
                continue;
            }

            let token_id = if msg.mode.is_future() {
                let id = self
                    .face_hat(ctx.src_face)
                    .next_id
                    .fetch_add(1, Ordering::SeqCst);
                self.face_hat_mut(ctx.src_face)
                    .local_tokens
                    .insert(token.clone(), id);
                id
            } else {
                SubscriberId::default()
            };

            let wire_expr = Resource::decl_key(&token, ctx.src_face);
            tracing::trace!(res = ?token);
            (ctx.send_declare)(
                &ctx.src_face.primitives,
                RoutingContext::with_expr(
                    Declare {
                        interest_id: Some(msg.id),
                        ext_qos: declare::ext::QoSType::DECLARE,
                        ext_tstamp: None,
                        ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                        body: DeclareBody::DeclareToken(DeclareToken {
                            id: token_id,
                            wire_expr,
                        }),
                    },
                    token.expr().to_string(),
                ),
            );
        }
    }

    #[tracing::instrument(level = "debug", skip(ctx), ret)]
    fn propagate_current_token(
        &self,
        ctx: BaseContext,
        res: Arc<Resource>,
        interest: CurrentInterest,
    ) {
        debug_assert!(self.region().bound().is_south());

        let mut dst = self.hat_remote(&interest.src).clone();

        debug_assert!(dst.region.bound().is_south());

        // TODO(regions*): apply this everywhere else
        if self.face_hat(&dst).local_tokens.contains_key(&res) {
            return;
        }

        // TODO(regions*): apply this everywhere else (?)
        let id = if interest.mode.is_future() {
            let id = self
                .face_hat(ctx.src_face)
                .next_id
                .fetch_add(1, Ordering::SeqCst);
            self.face_hat_mut(&mut dst)
                .local_tokens
                .insert(res.clone(), id);
            id
        } else {
            TokenId::default()
        };

        let wire_expr = Resource::decl_key(&res, &mut dst);
        tracing::trace!(?dst);
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

    #[tracing::instrument(level = "trace", skip(ctx, dst))]
    fn send_declare_final(&mut self, ctx: BaseContext, id: InterestId, dst: &Remote) {
        let dst_face = self.hat_remote(dst);

        (ctx.send_declare)(
            &dst_face.primitives,
            RoutingContext::new(Declare {
                interest_id: Some(id),
                ext_qos: declare::ext::QoSType::DECLARE,
                ext_tstamp: None,
                ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                body: DeclareBody::DeclareFinal(DeclareFinal),
            }),
        );
    }

    #[tracing::instrument(level = "debug", skip(ctx, msg), ret)]
    fn register_interest(&mut self, ctx: BaseContext, msg: &Interest, res: Option<Arc<Resource>>) {
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

    #[tracing::instrument(level = "trace", skip(ctx, msg), ret)]
    fn unregister_interest(&mut self, ctx: BaseContext, msg: &Interest) -> Option<RemoteInterest> {
        debug_assert!(self.region().bound().is_south());
        debug_assert!(self.owns(ctx.src_face));

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

    #[tracing::instrument(level = "trace", skip(tables), ret)]
    fn remote_interests(&self, tables: &TablesData) -> HashSet<RemoteInterest> {
        self.owned_faces(tables)
            .flat_map(|face| self.face_hat(face).remote_interests.values().cloned())
            .collect()
    }
}
