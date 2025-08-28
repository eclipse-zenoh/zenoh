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
    collections::HashMap,
    sync::{atomic::Ordering, Arc},
};

use zenoh_protocol::{
    core::{key_expr::OwnedKeyExpr, WhatAmI},
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
            interests::RemoteInterest,
            pubsub::SubscriberInfo,
            resource::{FaceContext, NodeId, Resource},
            tables::{Route, RoutingExpr, TablesData},
        },
        hat::{
            p2p_peer::{initial_interest, Hat},
            BaseContext, CurrentFutureTrait, HatPubSubTrait, InterestProfile, SendDeclare, Sources,
        },
        router::{Direction, RouteBuilder},
        RoutingContext,
    },
};

impl Hat {
    #[inline]
    fn propagate_simple_subscription_to(
        &self,
        ctx: BaseContext,
        src_face: &mut Arc<FaceState>,
        dst_face: &mut Arc<FaceState>,
        res: &Arc<Resource>,
        _sub_info: &SubscriberInfo,
        profile: InterestProfile,
    ) {
        if (src_face.id != dst_face.id)
            && !self.face_hat(dst_face).local_subs.contains_key(res)
            && (src_face.whatami == WhatAmI::Client || dst_face.whatami == WhatAmI::Client)
        {
            if dst_face.whatami != WhatAmI::Client && profile.is_push() {
                let id = self
                    .face_hat(dst_face)
                    .next_id
                    .fetch_add(1, Ordering::SeqCst);
                self.face_hat_mut(dst_face)
                    .local_subs
                    .insert(res.clone(), id);
                let key_expr =
                    Resource::decl_key(res, dst_face, super::push_declaration_profile(dst_face));
                (ctx.send_declare)(
                    &dst_face.primitives,
                    RoutingContext::with_expr(
                        Declare {
                            interest_id: None,
                            ext_qos: declare::ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                            body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                id,
                                wire_expr: key_expr,
                            }),
                        },
                        res.expr().to_string(),
                    ),
                );
            } else {
                let matching_interests = self
                    .face_hat(dst_face)
                    .remote_interests
                    .values()
                    .filter(|i| i.options.subscribers() && i.matches(res))
                    .cloned()
                    .collect::<Vec<_>>();

                for RemoteInterest {
                    res: int_res,
                    options,
                    ..
                } in matching_interests
                {
                    let res = if options.aggregate() {
                        int_res.as_ref().unwrap_or(res)
                    } else {
                        res
                    };
                    if !self.face_hat(dst_face).local_subs.contains_key(res) {
                        let id = self
                            .face_hat(dst_face)
                            .next_id
                            .fetch_add(1, Ordering::SeqCst);
                        self.face_hat_mut(dst_face)
                            .local_subs
                            .insert(res.clone(), id);
                        let key_expr = Resource::decl_key(
                            res,
                            dst_face,
                            super::push_declaration_profile(dst_face),
                        );
                        (ctx.send_declare)(
                            &dst_face.primitives,
                            RoutingContext::with_expr(
                                Declare {
                                    interest_id: None,
                                    ext_qos: declare::ext::QoSType::DECLARE,
                                    ext_tstamp: None,
                                    ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                                    body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                        id,
                                        wire_expr: key_expr,
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

    fn propagate_simple_subscription(
        &self,
        mut ctx: BaseContext,
        res: &Arc<Resource>,
        sub_info: &SubscriberInfo,
        profile: InterestProfile,
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
                profile,
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
        profile: InterestProfile,
    ) {
        self.register_simple_subscription(ctx.reborrow(), id, res, sub_info);

        self.propagate_simple_subscription(ctx.reborrow(), res, sub_info, profile);
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

    #[inline]
    fn remote_simple_subs(&self, res: &Arc<Resource>, face: &Arc<FaceState>) -> bool {
        res.face_ctxs
            .values()
            .any(|ctx| ctx.face.id != face.id && ctx.subs.is_some())
    }

    fn propagate_forget_simple_subscription(
        &self,
        tables: &mut TablesData,
        res: &Arc<Resource>,
        send_declare: &mut SendDeclare,
    ) {
        for mut face in self.faces(tables).values().cloned() {
            if let Some(id) = self.face_hat_mut(&mut face).local_subs.remove(res) {
                send_declare(
                    &face.primitives,
                    RoutingContext::with_expr(
                        Declare {
                            interest_id: None,
                            ext_qos: declare::ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                            body: DeclareBody::UndeclareSubscriber(UndeclareSubscriber {
                                id,
                                ext_wire_expr: WireExprType::null(),
                            }),
                        },
                        res.expr().to_string(),
                    ),
                );
            }
            for res in self
                .face_hat(&face)
                .local_subs
                .keys()
                .cloned()
                .collect::<Vec<Arc<Resource>>>()
            {
                if !res.context().matches.iter().any(|m| {
                    m.upgrade()
                        .is_some_and(|m| m.ctx.is_some() && self.remote_simple_subs(&m, &face))
                }) {
                    if let Some(id) = self.face_hat_mut(&mut face).local_subs.remove(&res) {
                        send_declare(
                            &face.primitives,
                            RoutingContext::with_expr(
                                Declare {
                                    interest_id: None,
                                    ext_qos: declare::ext::QoSType::DECLARE,
                                    ext_tstamp: None,
                                    ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                                    body: DeclareBody::UndeclareSubscriber(UndeclareSubscriber {
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
                if let Some(id) = self.face_hat_mut(face).local_subs.remove(res) {
                    (ctx.send_declare)(
                        &face.primitives,
                        RoutingContext::with_expr(
                            Declare {
                                interest_id: None,
                                ext_qos: declare::ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                                body: DeclareBody::UndeclareSubscriber(UndeclareSubscriber {
                                    id,
                                    ext_wire_expr: WireExprType::null(),
                                }),
                            },
                            res.expr().to_string(),
                        ),
                    );
                }
                for res in self
                    .face_hat(face)
                    .local_subs
                    .keys()
                    .cloned()
                    .collect::<Vec<Arc<Resource>>>()
                {
                    if !res.context().matches.iter().any(|m| {
                        m.upgrade()
                            .is_some_and(|m| m.ctx.is_some() && self.remote_simple_subs(&m, face))
                    }) {
                        if let Some(id) = self.face_hat_mut(face).local_subs.remove(&res) {
                            (ctx.send_declare)(
                                &face.primitives,
                                RoutingContext::with_expr(
                                    Declare {
                                        interest_id: None,
                                        ext_qos: declare::ext::QoSType::DECLARE,
                                        ext_tstamp: None,
                                        ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                                        body: DeclareBody::UndeclareSubscriber(
                                            UndeclareSubscriber {
                                                id,
                                                ext_wire_expr: WireExprType::null(),
                                            },
                                        ),
                                    },
                                    res.expr().to_string(),
                                ),
                            );
                        }
                    }
                }
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

    pub(super) fn pubsub_new_face(&self, mut ctx: BaseContext, profile: InterestProfile) {
        if ctx.src_face.whatami != WhatAmI::Client {
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
                        profile,
                    );
                }
            }
        }
    }

    #[inline]
    fn make_sub_id(
        &self,
        res: &Arc<Resource>,
        face: &mut Arc<FaceState>,
        mode: InterestMode,
    ) -> u32 {
        if mode.future() {
            if let Some(id) = self.face_hat(face).local_subs.get(res) {
                *id
            } else {
                let id = self.face_hat(face).next_id.fetch_add(1, Ordering::SeqCst);
                self.face_hat_mut(face).local_subs.insert(res.clone(), id);
                id
            }
        } else {
            0
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn declare_sub_interest(
        &self,
        tables: &mut TablesData,
        face: &mut Arc<FaceState>,
        id: InterestId,
        res: Option<&mut Arc<Resource>>,
        mode: InterestMode,
        aggregate: bool,
        send_declare: &mut SendDeclare,
    ) {
        if mode.current() && face.whatami == WhatAmI::Client {
            let interest_id = Some(id);
            if let Some(res) = res.as_ref() {
                if aggregate {
                    if self.faces(tables).values().any(|src_face| {
                        src_face.id != face.id
                            && self
                                .face_hat(src_face)
                                .remote_subs
                                .values()
                                .any(|sub| sub.ctx.is_some() && sub.matches(res))
                    }) {
                        let id = self.make_sub_id(res, face, mode);
                        let wire_expr =
                            Resource::decl_key(res, face, super::push_declaration_profile(face));
                        send_declare(
                            &face.primitives,
                            RoutingContext::with_expr(
                                Declare {
                                    interest_id,
                                    ext_qos: declare::ext::QoSType::DECLARE,
                                    ext_tstamp: None,
                                    ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                                    body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                        id,
                                        wire_expr,
                                    }),
                                },
                                res.expr().to_string(),
                            ),
                        );
                    }
                } else {
                    for src_face in self
                        .faces(tables)
                        .values()
                        .cloned()
                        .collect::<Vec<Arc<FaceState>>>()
                    {
                        if src_face.id != face.id {
                            for sub in self.face_hat(&src_face).remote_subs.values() {
                                if sub.ctx.is_some() && sub.matches(res) {
                                    let id = self.make_sub_id(sub, face, mode);
                                    let wire_expr = Resource::decl_key(
                                        sub,
                                        face,
                                        super::push_declaration_profile(face),
                                    );
                                    send_declare(
                                        &face.primitives,
                                        RoutingContext::with_expr(
                                            Declare {
                                                interest_id,
                                                ext_qos: declare::ext::QoSType::DECLARE,
                                                ext_tstamp: None,
                                                ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                                                body: DeclareBody::DeclareSubscriber(
                                                    DeclareSubscriber { id, wire_expr },
                                                ),
                                            },
                                            sub.expr().to_string(),
                                        ),
                                    );
                                }
                            }
                        }
                    }
                }
            } else {
                for src_face in self
                    .faces(tables)
                    .values()
                    .cloned()
                    .collect::<Vec<Arc<FaceState>>>()
                {
                    if src_face.id != face.id {
                        for sub in self.face_hat(&src_face).remote_subs.values() {
                            let id = self.make_sub_id(sub, face, mode);
                            let wire_expr = Resource::decl_key(
                                sub,
                                face,
                                super::push_declaration_profile(face),
                            );
                            send_declare(
                                &face.primitives,
                                RoutingContext::with_expr(
                                    Declare {
                                        interest_id,
                                        ext_qos: declare::ext::QoSType::DECLARE,
                                        ext_tstamp: None,
                                        ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                                        body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                            id,
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
        profile: InterestProfile,
    ) {
        // TODO(regions2): clients of this peer are handled as if they were bound to a future broker south hat
        self.declare_simple_subscription(ctx, id, res, sub_info, profile);
    }

    fn undeclare_subscription(
        &mut self,
        ctx: BaseContext,
        id: SubscriberId,
        _res: Option<Arc<Resource>>,
        _node_id: NodeId,
        _profile: InterestProfile,
    ) -> Option<Arc<Resource>> {
        self.forget_simple_subscription(ctx, id)
    }

    fn get_subscriptions(&self, tables: &TablesData) -> Vec<(Arc<Resource>, Sources)> {
        // Compute the list of known suscriptions (keys)
        let mut subs = HashMap::new();
        for src_face in self.faces(tables).values() {
            for sub in self.face_hat(src_face).remote_subs.values() {
                // Insert the key in the list of known suscriptions
                let srcs = subs.entry(sub.clone()).or_insert_with(Sources::empty);
                // Append src_face as a suscription source in the proper list
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

    fn compute_data_route(
        &self,
        tables: &TablesData,
        expr: &mut RoutingExpr,
        source: NodeId,
        source_type: WhatAmI,
    ) -> Arc<Route> {
        let mut route = RouteBuilder::<Direction>::new();
        let key_expr = expr.full_expr();
        if key_expr.ends_with('/') {
            return Arc::new(route.build());
        }
        tracing::trace!(
            "compute_data_route({}, {:?}, {:?})",
            key_expr,
            source,
            source_type
        );
        let key_expr = match OwnedKeyExpr::try_from(key_expr) {
            Ok(ke) => ke,
            Err(e) => {
                tracing::warn!("Invalid KE reached the system: {}", e);
                return Arc::new(route.build());
            }
        };

        if source_type == WhatAmI::Client {
            for face in self
                .faces(tables)
                .values()
                .filter(|f| f.whatami == WhatAmI::Router)
            {
                if !face.local_interests.values().any(|interest| {
                    interest.finalized
                        && interest.options.subscribers()
                        && interest
                            .res
                            .as_ref()
                            .map(|res| KeyExpr::keyexpr_include(res.expr(), expr.full_expr()))
                            .unwrap_or(true)
                }) || self
                    .face_hat(face)
                    .remote_subs
                    .values()
                    .any(|sub| KeyExpr::keyexpr_intersect(sub.expr(), expr.full_expr()))
                {
                    let key_expr = Resource::get_best_key(expr.prefix, expr.suffix, face.id);
                    route.insert(face.id, || Direction {
                        dst_face: face.clone(),
                        wire_expr: key_expr.to_owned(),
                        node_id: NodeId::default(),
                    });
                }
            }

            for face in self.faces(tables).values().filter(|f| {
                f.whatami == WhatAmI::Peer
                    && !initial_interest(f).map(|i| i.finalized).unwrap_or(true)
            }) {
                route.insert(face.id, || {
                    let key_expr = Resource::get_best_key(expr.prefix, expr.suffix, face.id);
                    Direction {
                        dst_face: face.clone(),
                        wire_expr: key_expr.to_owned(),
                        node_id: NodeId::default(),
                    }
                });
            }
        }

        let res = Resource::get_resource(expr.prefix, expr.suffix);
        let matches = res
            .as_ref()
            .and_then(|res| res.ctx.as_ref())
            .map(|ctx| Cow::from(&ctx.matches))
            .unwrap_or_else(|| Cow::from(Resource::get_matches(tables, &key_expr)));

        for mres in matches.iter() {
            let mres = mres.upgrade().unwrap();

            for (fid, ctx) in &mres.face_ctxs {
                if ctx.subs.is_some()
                    && (source_type == WhatAmI::Client || ctx.face.whatami == WhatAmI::Client)
                {
                    route.insert(*fid, || {
                        let key_expr = Resource::get_best_key(expr.prefix, expr.suffix, *fid);
                        Direction {
                            dst_face: ctx.face.clone(),
                            wire_expr: key_expr.to_owned(),
                            node_id: NodeId::default(),
                        }
                    });
                }
            }
        }
        for mcast_group in self.mcast_groups(tables) {
            route.insert(mcast_group.id, || Direction {
                dst_face: mcast_group.clone(),
                wire_expr: expr.full_expr().to_string().into(),
                node_id: NodeId::default(),
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
