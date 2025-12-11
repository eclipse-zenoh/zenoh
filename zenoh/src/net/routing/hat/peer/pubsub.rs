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
use zenoh_protocol::network::declare::{
    self, common::ext::WireExprType, Declare, DeclareBody, DeclareSubscriber, SubscriberId,
    UndeclareSubscriber,
};
use zenoh_sync::get_mut_unchecked;

use crate::net::routing::{
    dispatcher::{
        face::FaceState,
        pubsub::SubscriberInfo,
        region::RegionMap,
        resource::{FaceContext, NodeId, Resource},
        tables::{Route, RoutingExpr, TablesData},
    },
    hat::{
        peer::{initial_interest, Hat, INITIAL_INTEREST_ID},
        BaseContext, HatBaseTrait, HatPubSubTrait, HatTrait, SendDeclare, Sources,
    },
    router::{Direction, RouteBuilder, DEFAULT_NODE_ID},
    RoutingContext,
};

impl Hat {
    pub(super) fn pubsub_new_face(&self, ctx: BaseContext, other_hats: &RegionMap<&dyn HatTrait>) {
        if ctx.src_face.region.bound().is_south() {
            tracing::trace!(face = %ctx.src_face, "New south-bound peer remote; not propagating subscriptions");
            return;
        }

        let info = SubscriberInfo;

        for res in other_hats
            .values()
            .flat_map(|hat| hat.remote_subscriptions(ctx.tables).into_keys())
        {
            // FIXME(regions): We always propagate entities in this codepath; the method name is misleading
            self.maybe_propagate_subscription(&res, &info, ctx.src_face, ctx.send_declare);
        }
    }

    fn maybe_propagate_subscription(
        &self,
        res: &Arc<Resource>,
        info: &SubscriberInfo,
        dst_face: &mut Arc<FaceState>,
        send_declare: &mut SendDeclare,
    ) {
        if self
            .face_hat(dst_face)
            .local_subs
            .contains_simple_resource(res)
        {
            return;
        };

        // FIXME(regions): it's not because of initial interest that we push subscribers to north-bound peers.
        // Initial interest only exists to track current declarations at startup. This code is misleading.
        // See 20a95fb.
        let initial_interest = dst_face
            .region
            .bound()
            .is_north()
            .then_some(INITIAL_INTEREST_ID);

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
            *info,
            || face_hat_mut.next_id.fetch_add(1, Ordering::SeqCst),
            simple_interests,
        );

        for update in subs_to_notify {
            tracing::trace!(%dst_face, res = update.resource.expr());
            let key_expr = Resource::decl_key(&update.resource, dst_face);
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

    fn maybe_unpropagate_subscription(
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
}

impl HatPubSubTrait for Hat {
    #[tracing::instrument(level = "debug", skip(tables), ret)]
    fn sourced_subscribers(&self, tables: &TablesData) -> Vec<(Arc<Resource>, Sources)> {
        // Compute the list of known subscriptions (keys)
        let mut subs = HashMap::new();
        for face in self.owned_faces(tables) {
            for (sub, _) in self.face_hat(face).remote_qabls.values() {
                // Insert the key in the list of known subscriptions
                let srcs = subs.entry(sub.clone()).or_insert_with(Sources::empty);
                // Append src_face as a subscriptions source in the proper list
                srcs.peers.push(face.zid);
            }
        }
        Vec::from_iter(subs)
    }

    #[tracing::instrument(level = "debug", skip(tables), ret)]
    fn sourced_publishers(&self, tables: &TablesData) -> Vec<(Arc<Resource>, Sources)> {
        let mut result = HashMap::new();
        for face in self.owned_faces(tables) {
            for res in self
                .face_hat(face)
                .remote_interests
                .values()
                .filter_map(|i| {
                    if i.options.subscribers() {
                        i.res.as_ref()
                    } else {
                        None
                    }
                })
            {
                result
                    .entry(res.clone())
                    .or_insert_with(Sources::default)
                    .peers
                    .push(face.zid);
            }
        }
        result.into_iter().collect()
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region), ret)]
    fn compute_data_route(
        &self,
        tables: &TablesData,
        src_face: &FaceState,
        expr: &RoutingExpr,
        _node_id: NodeId,
    ) -> Arc<Route> {
        let mut route = RouteBuilder::<Direction>::new();
        let Some(key_expr) = expr.key_expr() else {
            return Arc::new(route.build());
        };

        tracing::trace!(%key_expr);

        let matches = expr
            .resource()
            .as_ref()
            .and_then(|res| res.ctx.as_ref())
            .map(|ctx| Cow::from(&ctx.matches))
            .unwrap_or_else(|| Cow::from(Resource::get_matches(tables, key_expr)));

        for mres in matches.iter() {
            let mres = mres.upgrade().unwrap();

            for (fid, ctx) in self.owned_face_contexts(&mres) {
                if ctx.subs.is_some() && !self.owns(src_face) {
                    route.insert(*fid, || {
                        tracing::trace!(dst = %ctx.face, reason = "resource match");
                        let wire_expr = expr.get_best_key(*fid);
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
            for face in self.owned_faces(tables) {
                if face.remote_bound.is_south() {
                    route.try_insert(face.id, || {
                        let has_interest_finalized = expr
                            .resource()
                            .and_then(|res| res.face_ctxs.get(&face.id))
                            .is_some_and(|ctx| ctx.subscriber_interest_finalized);
                        (!has_interest_finalized).then(|| {
                            tracing::trace!(dst = %face, res = ?expr.resource(), reason = "unfinalized subscriber interest");
                            let wire_expr = expr.get_best_key(face.id);
                            Direction {
                                dst_face: face.clone(),
                                wire_expr: wire_expr.to_owned(),
                                node_id: DEFAULT_NODE_ID,
                            }
                        })
                    });
                } else if initial_interest(face).is_some_and(|i| !i.finalized) {
                    tracing::trace!(dst = %face, reason = "unfinalized initial interest");
                    route.insert(face.id, || {
                        let wire_expr = expr.get_best_key(face.id);
                        Direction {
                            dst_face: face.clone(),
                            wire_expr: wire_expr.to_owned(),
                            node_id: DEFAULT_NODE_ID,
                        }
                    });
                }
            }
        }

        Arc::new(route.build())
    }

    #[tracing::instrument(level = "trace", skip_all)]
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

    #[tracing::instrument(level = "trace", skip_all)]
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

    #[tracing::instrument(level = "trace", skip_all)]
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

    #[tracing::instrument(level = "trace", skip_all)]
    fn propagate_subscription(
        &mut self,
        ctx: BaseContext,
        res: Arc<Resource>,
        other_info: Option<SubscriberInfo>,
    ) {
        let Some(other_info) = other_info else {
            debug_assert!(self.owns(ctx.src_face));
            return;
        };

        for dst_face in self.owned_faces_mut(ctx.tables) {
            self.maybe_propagate_subscription(&res, &other_info, dst_face, ctx.send_declare);
        }
    }

    #[tracing::instrument(level = "trace", skip_all)]
    fn unpropagate_subscription(&mut self, ctx: BaseContext, res: Arc<Resource>) {
        if self.owns(ctx.src_face) {
            return;
        }

        for mut face in self.owned_faces(ctx.tables).cloned() {
            self.maybe_unpropagate_subscription(&mut face, &res, ctx.send_declare);
        }
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region), ret)]
    fn remote_subscriptions_of(&self, res: &Resource) -> Option<SubscriberInfo> {
        self.owned_face_contexts(res)
            .filter_map(|(_, ctx)| ctx.subs)
            .reduce(|_, _| SubscriberInfo)
    }

    #[allow(clippy::incompatible_msrv)]
    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region), ret)]
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
