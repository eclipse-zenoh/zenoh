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
    borrow::Cow,
    collections::{HashMap, HashSet},
    sync::{atomic::Ordering, Arc},
};

use itertools::Itertools;
#[allow(unused_imports)]
use zenoh_core::compat::*;
use zenoh_protocol::network::declare::{
    self, common::ext::WireExprType, Declare, DeclareBody, DeclareSubscriber, SubscriberId,
    UndeclareSubscriber,
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

        let (should_notify, simple_interests) = self
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
            );

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

    #[inline]
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
    fn declare_subscription(
        &mut self,
        _ctx: BaseContext,
        _id: SubscriberId,
        _res: &mut Arc<Resource>,
        _node_id: NodeId,
        _sub_info: &SubscriberInfo,
    ) {
        unimplemented!()
    }

    fn undeclare_subscription(
        &mut self,
        _ctx: BaseContext,
        _id: SubscriberId,
        _res: Option<Arc<Resource>>,
        _node_id: NodeId,
    ) -> Option<Arc<Resource>> {
        unimplemented!()
    }

    fn get_subscriptions(&self, _tables: &TablesData) -> Vec<(Arc<Resource>, Sources)> {
        unimplemented!()
    }

    fn get_publications(&self, _tables: &TablesData) -> Vec<(Arc<Resource>, Sources)> {
        unimplemented!()
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

            for (sid, ctx) in self.owned_face_contexts(&mres) {
                if ctx.subs.is_some() && src_face.id != ctx.face.id {
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

        Arc::new(route.build())
    }

    fn get_matching_subscriptions(
        &self,
        _tables: &TablesData,
        _key_expr: &KeyExpr<'_>,
    ) -> HashMap<usize, Arc<FaceState>> {
        unimplemented!()
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
        for dst_face in self.owned_faces_mut(ctx.tables) {
            if !self.owns(ctx.src_face) || ctx.src_face.id != dst_face.id {
                if let Some(info) = self
                    .owned_face_contexts(&res)
                    .filter(|(_, face_ctx)| face_ctx.face.id != dst_face.id)
                    .flat_map(|(_, face_ctx)| face_ctx.subs)
                    .chain(other_info.into_iter())
                    .reduce(|_, _| SubscriberInfo)
                {
                    self.maybe_propagate_subscription(&res, &info, dst_face, ctx.send_declare);
                }
            }
        }
    }

    #[tracing::instrument(level = "trace", skip_all)]
    fn unpropagate_subscription(&mut self, ctx: BaseContext, res: Arc<Resource>) {
        for mut face in self.owned_faces(ctx.tables).cloned() {
            self.maybe_unpropagate_subscription(&mut face, &res, ctx.send_declare);
        }
    }

    #[tracing::instrument(level = "trace", skip_all)]
    fn unpropagate_last_non_owned_subscription(&mut self, ctx: BaseContext, res: Arc<Resource>) {
        // FIXME(regions): remove this
        debug_assert!(self.remote_subscriptions_of(&res).is_some());

        if let Ok(face) = self
            .owned_face_contexts(&res)
            .filter_map(|(_, ctx)| ctx.subs.map(|_| ctx.face.clone()))
            .exactly_one()
            .as_mut()
        {
            self.maybe_unpropagate_subscription(face, &res, ctx.send_declare)
        }
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region), ret)]
    fn remote_subscriptions_of(&self, res: &Resource) -> Option<SubscriberInfo> {
        self.owned_face_contexts(res)
            .filter_map(|(_, ctx)| ctx.subs)
            .reduce(|_, _| SubscriberInfo)
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region), ret)]
    fn remote_subscriptions(&self, tables: &TablesData) -> HashMap<Arc<Resource>, SubscriberInfo> {
        self.remote_subscriptions_matching(tables, None)
    }

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
