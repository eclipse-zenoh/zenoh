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
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    sync::{atomic::Ordering, Arc},
};

use zenoh_protocol::{
    core::WhatAmI,
    network::{
        declare::{
            self, common::ext::WireExprType, Declare, DeclareBody, DeclareSubscriber, SubscriberId,
            UndeclareSubscriber,
        },
        interest::{InterestId, InterestMode},
    },
};
use zenoh_sync::get_mut_unchecked;

use crate::{
    key_expr::KeyExpr,
    net::routing::{
        dispatcher::{
            face::FaceState,
            pubsub::SubscriberInfo,
            resource::{FaceContext, NodeId, Resource},
            tables::{Route, RoutingExpr, TablesData},
        },
        hat::{
            peer::{initial_interest, Hat, INITIAL_INTEREST_ID},
            BaseContext, CurrentFutureTrait, HatBaseTrait, HatPubSubTrait, SendDeclare, Sources,
        },
        router::{Direction, RouteBuilder, DEFAULT_NODE_ID},
        RoutingContext,
    },
};

impl Hat {
    #[inline]
    fn maybe_register_local_subscriber(
        &self,
        dst_face: &mut Arc<FaceState>,
        res: &Arc<Resource>,
        initial_interest: Option<InterestId>,
        send_declare: &mut SendDeclare,
    ) {
        if self
            .face_hat(dst_face)
            .local_subs
            .contains_simple_resource(res)
        {
            return;
        }
        let (should_notify, simple_interests) = match initial_interest {
            Some(interest) => (true, HashSet::from([interest])),
            None => self
                .face_hat(dst_face)
                .remote_interests
                .iter()
                .filter(|(_, i)| i.options.subscribers() && i.matches(res))
                .fold(
                    (false, HashSet::new()),
                    |(_, mut simple_interests), (id, i)| {
                        if !i.options.aggregate() {
                            simple_interests.insert(*id);
                        }
                        (true, simple_interests)
                    },
                ),
        };

        if !should_notify {
            return;
        }
        let face_hat_mut = self.face_hat_mut(dst_face);
        let (_, subs_to_notify) = face_hat_mut.local_subs.insert_simple_resource(
            res.clone(),
            SubscriberInfo,
            || face_hat_mut.next_id.fetch_add(1, Ordering::SeqCst),
            simple_interests,
        );

        for update in subs_to_notify {
            let key_expr = Resource::decl_key(
                &update.resource,
                dst_face,
                super::push_declaration_profile(dst_face),
            );
            send_declare(
                &dst_face.primitives,
                RoutingContext::with_expr(
                    Declare {
                        interest_id: None,
                        ext_qos: declare::ext::QoSType::DECLARE,
                        ext_tstamp: None,
                        ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                        body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                            id: update.id,
                            wire_expr: key_expr.clone(),
                        }),
                    },
                    update.resource.expr().to_string(),
                ),
            );
        }
    }

    #[inline]
    fn maybe_unregister_local_subscriber(
        &self,
        face: &mut Arc<FaceState>,
        res: &Arc<Resource>,
        send_declare: &mut SendDeclare,
    ) {
        for update in self
            .face_hat_mut(face)
            .local_subs
            .remove_simple_resource(res)
        {
            send_declare(
                &face.primitives,
                RoutingContext::with_expr(
                    Declare {
                        interest_id: None,
                        ext_qos: declare::ext::QoSType::DECLARE,
                        ext_tstamp: None,
                        ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                        body: DeclareBody::UndeclareSubscriber(UndeclareSubscriber {
                            id: update.id,
                            ext_wire_expr: WireExprType::null(),
                        }),
                    },
                    update.resource.expr().to_string(),
                ),
            );
        }
    }

    fn get_subscribers_matching_resource<'a>(
        &self,
        tables: &'a TablesData,
        face: &Arc<FaceState>,
        res: Option<&'a Arc<Resource>>,
    ) -> Vec<&'a Arc<Resource>> {
        let face_id = face.id;

        tables
            .faces
            .values()
            .filter(move |f| f.id != face_id)
            .flat_map(|f| self.face_hat(f).remote_subs.values())
            .filter(move |sub| sub.ctx.is_some() && res.map(|r| sub.matches(r)).unwrap_or(true))
            .collect()
    }

    #[inline]
    fn propagate_simple_subscription_to(
        &self,
        ctx: BaseContext,
        src_face: &mut Arc<FaceState>,
        dst_face: &mut Arc<FaceState>,
        res: &Arc<Resource>,
        _sub_info: &SubscriberInfo,
    ) {
        if (src_face.id != dst_face.id)
            && !self
                .face_hat(dst_face)
                .local_subs
                .contains_simple_resource(res)
            && self.should_route_between(src_face, dst_face)
        {
            if dst_face.whatami != WhatAmI::Client {
                self.maybe_register_local_subscriber(
                    dst_face,
                    res,
                    Some(INITIAL_INTEREST_ID),
                    ctx.send_declare,
                );
            } else {
                self.maybe_register_local_subscriber(dst_face, res, None, ctx.send_declare);
            }
        }
    }

    fn propagate_simple_subscription(
        &self,
        mut ctx: BaseContext,
        res: &Arc<Resource>,
        sub_info: &SubscriberInfo,
    ) {
        for mut dst_face in self
            .faces(ctx.tables)
            .values()
            .cloned()
            .collect::<Vec<Arc<FaceState>>>()
        {
            let src_face = &mut ctx.src_face.clone();
            self.propagate_simple_subscription_to(
                ctx.reborrow(),
                src_face,
                &mut dst_face,
                res,
                sub_info,
            );
        }
    }

    fn register_simple_subscription(
        &self,
        ctx: BaseContext,
        id: SubscriberId,
        res: &mut Arc<Resource>,
        sub_info: &SubscriberInfo,
    ) {
        // Register subscription
        {
            let res = get_mut_unchecked(res);
            match res.face_ctxs.get_mut(&ctx.src_face.id) {
                Some(ctx) => {
                    if ctx.subs.is_none() {
                        get_mut_unchecked(ctx).subs = Some(*sub_info);
                    }
                }
                None => {
                    let ctx = res
                        .face_ctxs
                        .entry(ctx.src_face.id)
                        .or_insert_with(|| Arc::new(FaceContext::new(ctx.src_face.clone())));
                    get_mut_unchecked(ctx).subs = Some(*sub_info);
                }
            }
        }
        self.face_hat_mut(ctx.src_face)
            .remote_subs
            .insert(id, res.clone());
    }

    fn declare_simple_subscription(
        &self,
        mut ctx: BaseContext,
        id: SubscriberId,
        res: &mut Arc<Resource>,
        sub_info: &SubscriberInfo,
    ) {
        self.register_simple_subscription(ctx.reborrow(), id, res, sub_info);

        self.propagate_simple_subscription(ctx.reborrow(), res, sub_info);
        // This introduced a buffer overflow on windows
        // TODO: Let's deactivate this on windows until Fixed
        #[cfg(not(windows))]
        if ctx.src_face.whatami == WhatAmI::Client {
            for mcast_group in self.mcast_groups(ctx.tables) {
                if mcast_group.mcast_group != ctx.src_face.mcast_group {
                    mcast_group
                        .primitives
                        .send_declare(RoutingContext::with_expr(
                            &mut Declare {
                                interest_id: None,
                                ext_qos: declare::ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                                body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                    id: 0, // @TODO use proper SubscriberId
                                    wire_expr: res.expr().to_string().into(),
                                }),
                            },
                            res.expr().to_string(),
                        ))
                }
            }
        }
    }

    #[inline]
    fn simple_subs(&self, res: &Arc<Resource>) -> Vec<Arc<FaceState>> {
        res.face_ctxs
            .values()
            .filter_map(|ctx| {
                if ctx.subs.is_some() {
                    Some(ctx.face.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    fn propagate_forget_simple_subscription(
        &self,
        tables: &mut TablesData,
        res: &Arc<Resource>,
        send_declare: &mut SendDeclare,
    ) {
        for mut face in self.faces(tables).values().cloned() {
            self.maybe_unregister_local_subscriber(&mut face, res, send_declare);
        }
    }

    pub(super) fn undeclare_simple_subscription(&self, ctx: BaseContext, res: &mut Arc<Resource>) {
        if !self
            .face_hat_mut(ctx.src_face)
            .remote_subs
            .values()
            .any(|s| *s == *res)
        {
            if let Some(ctx) = get_mut_unchecked(res).face_ctxs.get_mut(&ctx.src_face.id) {
                get_mut_unchecked(ctx).subs = None;
            }

            let mut simple_subs = self.simple_subs(res);
            if simple_subs.is_empty() {
                self.propagate_forget_simple_subscription(ctx.tables, res, ctx.send_declare);
            }

            if simple_subs.len() == 1 {
                let face = &mut simple_subs[0];
                self.maybe_unregister_local_subscriber(face, res, ctx.send_declare);
            }
        }
    }

    fn forget_simple_subscription(
        &self,
        ctx: BaseContext,
        id: SubscriberId,
    ) -> Option<Arc<Resource>> {
        if let Some(mut res) = self.face_hat_mut(ctx.src_face).remote_subs.remove(&id) {
            self.undeclare_simple_subscription(ctx, &mut res);
            Some(res)
        } else {
            None
        }
    }

    pub(super) fn pubsub_new_face(&self, mut ctx: BaseContext) {
        let sub_info = SubscriberInfo;
        for mut face in self
            .faces(ctx.tables)
            .values()
            .cloned()
            .collect::<Vec<Arc<FaceState>>>()
        {
            for sub in self.face_hat(&face.clone()).remote_subs.values() {
                let dst_face = &mut ctx.src_face.clone();
                self.propagate_simple_subscription_to(
                    ctx.reborrow(),
                    &mut face, // src
                    dst_face,  // dst
                    sub,
                    &sub_info,
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn declare_sub_interest(
        &self,
        tables: &mut TablesData,
        face: &mut Arc<FaceState>,
        interest_id: InterestId,
        res: Option<&mut Arc<Resource>>,
        mode: InterestMode,
        aggregate: bool,
        send_declare: &mut SendDeclare,
    ) {
        if face.whatami != WhatAmI::Client {
            return;
        }

        let res = res.map(|r| r.clone());
        let mut matching_subs = self
            .get_subscribers_matching_resource(tables, face, res.as_ref())
            .into_iter();

        if aggregate && (mode.current() || mode.future()) {
            if let Some(aggregated_res) = &res {
                let (resource_id, sub_info) = if mode.future() {
                    let face_hat_mut = self.face_hat_mut(face);
                    for sub in matching_subs {
                        face_hat_mut.local_subs.insert_simple_resource(
                            sub.clone(),
                            SubscriberInfo,
                            || face_hat_mut.next_id.fetch_add(1, Ordering::SeqCst),
                            HashSet::new(),
                        );
                    }
                    let face_hat_mut = self.face_hat_mut(face);
                    face_hat_mut.local_subs.insert_aggregated_resource(
                        aggregated_res.clone(),
                        || face_hat_mut.next_id.fetch_add(1, Ordering::SeqCst),
                        HashSet::from_iter([interest_id]),
                    )
                } else {
                    (0, matching_subs.next().map(|_| SubscriberInfo))
                };
                if mode.current() && sub_info.is_some() {
                    // send declare only if there is at least one resource matching the aggregate
                    let wire_expr = Resource::decl_key(
                        aggregated_res,
                        face,
                        super::push_declaration_profile(face),
                    );
                    send_declare(
                        &face.primitives,
                        RoutingContext::with_expr(
                            Declare {
                                interest_id: Some(interest_id),
                                ext_qos: declare::ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                                body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                    id: resource_id,
                                    wire_expr,
                                }),
                            },
                            aggregated_res.expr().to_string(),
                        ),
                    );
                }
            }
        } else if !aggregate && mode.current() {
            for sub in matching_subs {
                let resource_id = if mode.future() {
                    let face_hat_mut = self.face_hat_mut(face);
                    face_hat_mut
                        .local_subs
                        .insert_simple_resource(
                            sub.clone(),
                            SubscriberInfo,
                            || face_hat_mut.next_id.fetch_add(1, Ordering::SeqCst),
                            HashSet::from([interest_id]),
                        )
                        .0
                } else {
                    0
                };
                let wire_expr =
                    Resource::decl_key(sub, face, super::push_declaration_profile(face));
                send_declare(
                    &face.primitives,
                    RoutingContext::with_expr(
                        Declare {
                            interest_id: Some(interest_id),
                            ext_qos: declare::ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                            body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                id: resource_id,
                                wire_expr,
                            }),
                        },
                        sub.expr().to_string(),
                    ),
                );
            }
        }
    }
}

impl HatPubSubTrait for Hat {
    fn declare_subscription(
        &mut self,
        ctx: BaseContext,
        id: SubscriberId,
        res: &mut Arc<Resource>,
        _node_id: NodeId,
        sub_info: &SubscriberInfo,
    ) {
        // TODO(regions2): clients of this peer are handled as if they were bound to a future broker south hat
        self.declare_simple_subscription(ctx, id, res, sub_info);
    }

    fn undeclare_subscription(
        &mut self,
        ctx: BaseContext,
        id: SubscriberId,
        _res: Option<Arc<Resource>>,
        _node_id: NodeId,
    ) -> Option<Arc<Resource>> {
        self.forget_simple_subscription(ctx, id)
    }

    fn get_subscriptions(&self, tables: &TablesData) -> Vec<(Arc<Resource>, Sources)> {
        // Compute the list of known subscriptions (keys)
        let mut subs = HashMap::new();
        for src_face in self.faces(tables).values() {
            for sub in self.face_hat(src_face).remote_subs.values() {
                // Insert the key in the list of known subscriptions
                let srcs = subs.entry(sub.clone()).or_insert_with(Sources::empty);
                // Append src_face as a subscriptions source in the proper list
                let whatami = if src_face.is_local {
                    tables.hats.north().whatami // REVIEW(fuzzypixelz)
                } else {
                    src_face.whatami
                };
                match whatami {
                    WhatAmI::Router => srcs.routers.push(src_face.zid),
                    WhatAmI::Peer => srcs.peers.push(src_face.zid),
                    WhatAmI::Client => srcs.clients.push(src_face.zid),
                }
            }
        }
        Vec::from_iter(subs)
    }

    fn get_publications(&self, tables: &TablesData) -> Vec<(Arc<Resource>, Sources)> {
        let mut result = HashMap::new();
        for face in self.faces(tables).values() {
            for interest in self.face_hat(face).remote_interests.values() {
                if interest.options.subscribers() {
                    if let Some(res) = interest.res.as_ref() {
                        let sources = result.entry(res.clone()).or_insert_with(Sources::default);
                        let whatami = if face.is_local {
                            tables.hats.north().whatami // REVIEW(fuzzypixelz)
                        } else {
                            face.whatami
                        };
                        match whatami {
                            WhatAmI::Router => sources.routers.push(face.zid),
                            WhatAmI::Peer => sources.peers.push(face.zid),
                            WhatAmI::Client => sources.clients.push(face.zid),
                        }
                    }
                }
            }
        }
        result.into_iter().collect()
    }

    #[tracing::instrument(level = "trace", skip_all, fields(zid = %tables.zid.short(), bnd = %self.bound), ret)]
    fn compute_data_route(
        &self,
        tables: &TablesData,
        src_face: &FaceState,
        expr: &RoutingExpr,
        node_id: NodeId,
        _dst_node_id: NodeId,
    ) -> Arc<Route> {
        let mut route = RouteBuilder::<Direction>::new();
        let Some(key_expr) = expr.key_expr() else {
            return Arc::new(route.build());
        };
        let source_type = src_face.whatami;
        tracing::trace!(
            "compute_data_route({}, {:?}, {:?})",
            key_expr,
            node_id,
            source_type
        );

        let matches = expr
            .resource()
            .as_ref()
            .and_then(|res| res.ctx.as_ref())
            .map(|ctx| Cow::from(&ctx.matches))
            .unwrap_or_else(|| Cow::from(Resource::get_matches(tables, key_expr)));

        for mres in matches.iter() {
            let mres = mres.upgrade().unwrap();

            for (fid, ctx) in self.owned_face_contexts(&mres) {
                if ctx.subs.is_some() && self.should_route_between(src_face, &ctx.face) {
                    route.insert(*fid, || {
                        tracing::trace!(dst = %ctx.face, reason = "resource match");
                        let wire_expr = expr.get_best_key(*fid);
                        Direction {
                            dst_face: ctx.face.clone(),
                            wire_expr: wire_expr.to_owned(),
                            node_id: DEFAULT_NODE_ID,
                            dst_node_id: DEFAULT_NODE_ID,
                        }
                    });
                }
            }
        }

        if !src_face.local_bound.is_north() {
            for face in self.faces(tables).values() {
                if !face.remote_bound.is_north() {
                    route.try_insert(face.id, || {
                        let has_interest_finalized = expr
                            .resource()
                            .and_then(|res| res.face_ctxs.get(&face.id))
                            .is_some_and(|ctx| ctx.subscriber_interest_finalized);
                        (!has_interest_finalized).then(|| {
                            tracing::trace!(dst = %face, reason = "unfinalized subscriber interest");
                            let wire_expr = expr.get_best_key(face.id);
                            Direction {
                                dst_face: face.clone(),
                                wire_expr: wire_expr.to_owned(),
                                node_id: DEFAULT_NODE_ID,
                                dst_node_id: DEFAULT_NODE_ID,
                            }
                        })
                    });
                } else if face.whatami == WhatAmI::Peer
                    && face.local_bound.is_north() // REVIEW(regions): not sure
                    && initial_interest(face).is_some_and(|i| !i.finalized)
                {
                    tracing::trace!(dst = %face, reason = "unfinalized initial interest");
                    route.insert(face.id, || {
                        let wire_expr = expr.get_best_key(face.id);
                        Direction {
                            dst_face: face.clone(),
                            wire_expr: wire_expr.to_owned(),
                            node_id: DEFAULT_NODE_ID,
                            dst_node_id: DEFAULT_NODE_ID,
                        }
                    });
                }
            }
        }

        for mcast_group in self.mcast_groups(tables) {
            route.insert(mcast_group.id, || Direction {
                dst_face: mcast_group.clone(),
                wire_expr: key_expr.to_string().into(),
                node_id: DEFAULT_NODE_ID,
                dst_node_id: DEFAULT_NODE_ID,
            });
        }
        Arc::new(route.build())
    }

    fn get_matching_subscriptions(
        &self,
        tables: &TablesData,
        key_expr: &KeyExpr<'_>,
    ) -> HashMap<usize, Arc<FaceState>> {
        let mut matching_subscriptions = HashMap::new();
        if key_expr.ends_with('/') {
            return matching_subscriptions;
        }
        tracing::trace!("get_matching_subscriptions({})", key_expr,);
        let res = Resource::get_resource(&tables.root_res, key_expr);
        let matches = res
            .as_ref()
            .and_then(|res| res.ctx.as_ref())
            .map(|ctx| Cow::from(&ctx.matches))
            .unwrap_or_else(|| Cow::from(Resource::get_matches(tables, key_expr)));

        for mres in matches.iter() {
            let mres = mres.upgrade().unwrap();

            for (fid, ctx) in &mres.face_ctxs {
                if ctx.subs.is_some() {
                    matching_subscriptions
                        .entry(*fid)
                        .or_insert_with(|| ctx.face.clone());
                }
            }
        }
        matching_subscriptions
    }
}
