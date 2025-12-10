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

use itertools::Itertools;
#[allow(unused_imports)]
use zenoh_core::polyfill::*;
use zenoh_protocol::{
    core::WhatAmI,
    network::declare::{
        self, common::ext::WireExprType, Declare, DeclareBody, DeclareSubscriber, SubscriberId,
        UndeclareSubscriber,
    },
};
use zenoh_sync::get_mut_unchecked;

use super::Hat;
use crate::net::routing::{
    dispatcher::{
        face::FaceState,
        pubsub::SubscriberInfo,
        region::RegionMap,
        resource::{FaceContext, NodeId, Resource},
        tables::{Route, RoutingExpr, TablesData},
    },
    hat::{BaseContext, HatBaseTrait, HatPubSubTrait, HatTrait, Sources},
    router::{Direction, RouteBuilder, DEFAULT_NODE_ID},
    RoutingContext,
};

impl Hat {
    pub(super) fn pubsub_new_face(&self, ctx: BaseContext, other_hats: &RegionMap<&dyn HatTrait>) {
        for res in other_hats
            .values()
            .flat_map(|hat| hat.remote_subscriptions(ctx.tables).into_keys())
        {
            if self.face_hat(ctx.src_face).local_subs.contains_key(&res) {
                continue;
            }

            let id = self
                .face_hat(ctx.src_face)
                .next_id
                .fetch_add(1, Ordering::SeqCst);
            self.face_hat_mut(ctx.src_face)
                .local_subs
                .insert(res.clone(), id);
            let key_expr = Resource::decl_key(&res, ctx.src_face);
            (ctx.send_declare)(
                &ctx.src_face.primitives,
                RoutingContext::with_expr(
                    Declare {
                        interest_id: None,
                        ext_qos: declare::ext::QoSType::DECLARE,
                        ext_tstamp: None,
                        ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                        body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                            id,
                            wire_expr: key_expr.clone(),
                        }),
                    },
                    res.expr().to_string(),
                ),
            );
        }
    }
}

impl HatPubSubTrait for Hat {
    #[tracing::instrument(level = "trace", skip(tables), ret)]
    fn sourced_subscribers(&self, tables: &TablesData) -> Vec<(Arc<Resource>, Sources)> {
        // Compute the list of known subscribers (keys)
        let mut subs = HashMap::new();
        for face in self.owned_faces(tables) {
            for sub in self.face_hat(face).remote_subs.values() {
                // Insert the key in the list of known subscribers
                let srcs = subs.entry(sub.clone()).or_insert_with(Sources::empty);
                // Append face as a subscriber source in the proper list
                match face.whatami {
                    WhatAmI::Router => srcs.routers.push(face.zid),
                    WhatAmI::Peer => srcs.peers.push(face.zid),
                    WhatAmI::Client => srcs.clients.push(face.zid),
                }
            }
        }
        Vec::from_iter(subs)
    }

    #[tracing::instrument(level = "trace", skip(_tables), ret)]
    fn sourced_publishers(&self, _tables: &TablesData) -> Vec<(Arc<Resource>, Sources)> {
        Vec::default()
    }

    #[tracing::instrument(level = "debug", skip(tables, _nid), fields(%src_face) ret)]
    fn compute_data_route(
        &self,
        tables: &TablesData,
        src_face: &FaceState,
        expr: &RoutingExpr,
        _nid: NodeId,
    ) -> Arc<Route> {
        let mut route = RouteBuilder::<Direction>::new();
        let Some(key_expr) = expr.key_expr() else {
            return Arc::new(route.build());
        };

        let matches = expr
            .resource()
            .as_ref()
            .and_then(|res| res.ctx.as_ref())
            .map(|ctx| Cow::from(&ctx.matches))
            .unwrap_or_else(|| Cow::from(Resource::get_matches(tables, key_expr)));

        for mres in matches.iter() {
            let mres = mres.upgrade().unwrap();

            for ctx in self.owned_face_contexts(&mres) {
                if ctx.subs.is_some() && !self.owns(src_face) {
                    route.insert(ctx.face.id, || {
                        tracing::trace!(dst = %ctx.face, reason = "resource match");
                        let wire_expr = expr.get_best_key(ctx.face.id);
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
                .owned_faces(tables)
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

    #[tracing::instrument(level = "debug", skip(ctx, _nid))]
    fn register_subscription(
        &mut self,
        ctx: BaseContext,
        id: SubscriberId,
        mut res: Arc<Resource>,
        _nid: NodeId,
        info: &SubscriberInfo,
    ) {
        debug_assert!(self.owns(ctx.src_face));

        {
            let res = get_mut_unchecked(&mut res);
            match res.face_ctxs.get_mut(&ctx.src_face.id) {
                Some(ctx) => {
                    if ctx.subs.is_none() {
                        get_mut_unchecked(ctx).subs = Some(*info);
                    }
                }
                None => {
                    let ctx = res
                        .face_ctxs
                        .entry(ctx.src_face.id)
                        .or_insert_with(|| Arc::new(FaceContext::new(ctx.src_face.clone())));
                    get_mut_unchecked(ctx).subs = Some(*info);
                }
            }
        }

        self.face_hat_mut(ctx.src_face)
            .remote_subs
            .insert(id, res.clone());
    }

    #[tracing::instrument(level = "debug", skip(ctx, _nid), ret)]
    fn unregister_subscription(
        &mut self,
        ctx: BaseContext,
        id: SubscriberId,
        _res: Option<Arc<Resource>>,
        _nid: NodeId,
    ) -> Option<Arc<Resource>> {
        let Some(mut res) = self.face_hat_mut(ctx.src_face).remote_subs.remove(&id) else {
            tracing::error!(id, "Unknown subscription");
            return None;
        };

        if self
            .face_hat(ctx.src_face)
            .remote_subs
            .values()
            .contains(&res)
        {
            tracing::debug!(id, ?res, "Duplicated subscription");
            return None;
        };

        if let Some(ctx) = get_mut_unchecked(&mut res)
            .face_ctxs
            .get_mut(&ctx.src_face.id)
        {
            get_mut_unchecked(ctx).subs = None;
        }

        Some(res)
    }

    #[tracing::instrument(level = "debug", skip(ctx), ret)]
    fn unregister_face_subscriptions(&mut self, ctx: BaseContext) -> HashSet<Arc<Resource>> {
        debug_assert!(self.owns(ctx.src_face));

        let fid = ctx.src_face.id;

        self.face_hat_mut(ctx.src_face)
            .remote_subs
            .drain()
            .map(|(_, mut res)| {
                if let Some(ctx) = get_mut_unchecked(&mut res).face_ctxs.get_mut(&fid) {
                    get_mut_unchecked(ctx).subs = None;
                }

                res
            })
            .collect()
    }

    #[tracing::instrument(level = "trace", skip(ctx))]
    fn propagate_subscription(
        &mut self,
        ctx: BaseContext,
        res: Arc<Resource>,
        other_info: Option<SubscriberInfo>,
    ) {
        let Some(_) = other_info else {
            debug_assert!(self.owns(ctx.src_face));
            return;
        };

        let Some(mut dst_face) = self.owned_faces(ctx.tables).next().cloned() else {
            tracing::debug!("Client region is empty; won't unpropagate subscription upstream");
            return;
        };

        if self.face_hat(&dst_face).local_subs.contains_key(&res) {
            return;
        }

        let id = self
            .face_hat(&dst_face)
            .next_id
            .fetch_add(1, Ordering::SeqCst);
        self.face_hat_mut(&mut dst_face)
            .local_subs
            .insert(res.clone(), id);
        let key_expr = Resource::decl_key(&res, &mut dst_face);
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
                        wire_expr: key_expr.clone(),
                    }),
                },
                res.expr().to_string(),
            ),
        );
    }

    #[tracing::instrument(level = "trace", skip(ctx))]
    fn unpropagate_subscription(&mut self, ctx: BaseContext, res: Arc<Resource>) {
        let Some(mut dst_face) = self.owned_faces(ctx.tables).next().cloned() else {
            tracing::debug!("Client region is empty; won't unpropagate subscription upstream");
            return;
        };

        if let Some(id) = self.face_hat_mut(&mut dst_face).local_subs.remove(&res) {
            (ctx.send_declare)(
                &dst_face.primitives,
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

    #[tracing::instrument(level = "trace", ret)]
    fn remote_subscriptions_of(&self, res: &Resource) -> Option<SubscriberInfo> {
        self.owned_face_contexts(res)
            .filter_map(|ctx| ctx.subs)
            .reduce(|_, _| SubscriberInfo)
    }

    #[allow(clippy::incompatible_msrv)]
    #[tracing::instrument(level = "trace", skip(tables), ret)]
    fn remote_subscriptions_matching(
        &self,
        tables: &TablesData,
        res: Option<&Resource>,
    ) -> HashMap<Arc<Resource>, SubscriberInfo> {
        self.owned_faces(tables)
            .flat_map(|f| self.face_hat(f).remote_subs.values())
            .filter(|&sub| res.is_none_or(|res| res.matches(sub)))
            .map(|sub| (sub.clone(), SubscriberInfo))
            .collect()
    }
}
