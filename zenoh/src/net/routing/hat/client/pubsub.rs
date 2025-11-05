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
    core::WhatAmI,
    network::declare::{
        self, common::ext::WireExprType, Declare, DeclareBody, DeclareSubscriber, SubscriberId,
        UndeclareSubscriber,
    },
};
use zenoh_sync::get_mut_unchecked;

use super::Hat;
use crate::{
    key_expr::KeyExpr,
    net::routing::{
        dispatcher::{
            face::FaceState,
            pubsub::SubscriberInfo,
            resource::{FaceContext, NodeId, Resource},
            tables::{Route, RoutingExpr, TablesData},
        },
        hat::{BaseContext, HatBaseTrait, HatPubSubTrait, SendDeclare, Sources},
        router::{Direction, RouteBuilder, DEFAULT_NODE_ID},
        RoutingContext,
    },
};

impl Hat {
    #[inline]
    fn propagate_simple_subscription_to(
        &self,
        _tables: &mut TablesData,
        dst_face: &mut Arc<FaceState>,
        res: &Arc<Resource>,
        _sub_info: &SubscriberInfo,
        src_face: &mut Arc<FaceState>,
        send_declare: &mut SendDeclare,
    ) {
        if src_face.id != dst_face.id
            && !self.face_hat(dst_face).local_subs.contains_key(res)
            && self.should_route_between(src_face, dst_face)
        {
            let id = self
                .face_hat(dst_face)
                .next_id
                .fetch_add(1, Ordering::SeqCst);
            self.face_hat_mut(dst_face)
                .local_subs
                .insert(res.clone(), id);
            let key_expr = Resource::decl_key(res, dst_face, true);
            send_declare(
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

    fn propagate_simple_subscription(
        &self,
        tables: &mut TablesData,
        res: &Arc<Resource>,
        sub_info: &SubscriberInfo,
        src_face: &mut Arc<FaceState>,
        send_declare: &mut SendDeclare,
    ) {
        for mut dst_face in self
            .faces(tables)
            .values()
            .cloned()
            .collect::<Vec<Arc<FaceState>>>()
        {
            self.propagate_simple_subscription_to(
                tables,
                &mut dst_face,
                res,
                sub_info,
                src_face,
                send_declare,
            );
        }
    }

    fn register_simple_subscription(
        &self,
        _tables: &mut TablesData,
        face: &mut Arc<FaceState>,
        id: SubscriberId,
        res: &mut Arc<Resource>,
        sub_info: &SubscriberInfo,
    ) {
        // Register subscription
        {
            let res = get_mut_unchecked(res);
            match res.face_ctxs.get_mut(&face.id) {
                Some(ctx) => {
                    if ctx.subs.is_none() {
                        get_mut_unchecked(ctx).subs = Some(*sub_info);
                    }
                }
                None => {
                    let ctx = res
                        .face_ctxs
                        .entry(face.id)
                        .or_insert_with(|| Arc::new(FaceContext::new(face.clone())));
                    get_mut_unchecked(ctx).subs = Some(*sub_info);
                }
            }
        }
        self.face_hat_mut(face).remote_subs.insert(id, res.clone());
    }

    fn declare_simple_subscription(
        &self,
        tables: &mut TablesData,
        face: &mut Arc<FaceState>,
        id: SubscriberId,
        res: &mut Arc<Resource>,
        sub_info: &SubscriberInfo,
        send_declare: &mut SendDeclare,
    ) {
        self.register_simple_subscription(tables, face, id, res, sub_info);

        self.propagate_simple_subscription(tables, res, sub_info, face, send_declare);
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
        for face in self.faces_mut(tables).values_mut() {
            if let Some(id) = self.face_hat_mut(face).local_subs.remove(res) {
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

    pub(super) fn pubsub_new_face(
        &self,
        tables: &mut TablesData,
        face: &mut Arc<FaceState>,
        send_declare: &mut SendDeclare,
    ) {
        let sub_info = SubscriberInfo;
        for src_face in self
            .faces(tables)
            .values()
            .cloned()
            .collect::<Vec<Arc<FaceState>>>()
        {
            for sub in self.face_hat(&src_face).remote_subs.values() {
                self.propagate_simple_subscription_to(
                    tables,
                    face,
                    sub,
                    &sub_info,
                    &mut src_face.clone(),
                    send_declare,
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
        self.declare_simple_subscription(
            ctx.tables,
            ctx.src_face,
            id,
            res,
            sub_info,
            ctx.send_declare,
        );
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
                // Append src_face as a subscription source in the proper list
                match src_face.whatami {
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
                        match face.whatami {
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

    #[tracing::instrument(level = "trace", skip_all, fields(zid = %tables.zid.short(), bnd = %self.region), ret)]
    fn compute_data_route(
        &self,
        tables: &TablesData,
        src_face: &FaceState,
        expr: &RoutingExpr,
        node_id: NodeId,
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

            for (sid, ctx) in self.owned_face_contexts(&mres) {
                if ctx.subs.is_some() && self.should_route_between(src_face, &ctx.face) {
                    route.insert(*sid, || {
                        tracing::trace!(dst = %ctx.face, reason = "resource match");
                        let wire_expr = expr.get_best_key(*sid);
                        Direction {
                            dst_face: ctx.face.clone(),
                            wire_expr: wire_expr.to_owned(),
                            node_id: DEFAULT_NODE_ID,
                        }
                    });
                }
            }
        }

        if src_face.region.bound().is_south() {
            // REVIEW(regions): there should only be one such face?
            for face in self
                .faces(tables)
                .values()
                .filter(|f| f.region.bound().is_north())
            {
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
                        }
                    })
                });
            }
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

            for (sid, ctx) in &mres.face_ctxs {
                if ctx.subs.is_some() {
                    matching_subscriptions
                        .entry(*sid)
                        .or_insert_with(|| ctx.face.clone());
                }
            }
        }
        matching_subscriptions
    }
}
