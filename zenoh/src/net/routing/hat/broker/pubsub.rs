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
use zenoh_core::polyfill::*;
use zenoh_keyexpr::keyexpr;
use zenoh_protocol::network::declare::{
    self, common::ext::WireExprType, Declare, DeclareBody, DeclareSubscriber, SubscriberId,
    UndeclareSubscriber,
};
use zenoh_sync::get_mut_unchecked;

use super::Hat;
use crate::{
    net::routing::{
        dispatcher::{
            face::FaceState,
            pubsub::SubscriberInfo,
            region::RegionMap,
            resource::{FaceContext, NodeId, Resource},
            tables::{Route, RoutingExpr, TablesData},
        },
        hat::{DispatcherContext, HatBaseTrait, HatPubSubTrait, HatTrait, SendDeclare, Sources},
        router::{Direction, RouteBuilder, DEFAULT_NODE_ID},
        RoutingContext,
    },
    sample::Locality,
};

impl Hat {
    fn maybe_propagate_subscriber(
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
    fn maybe_unpropagate_subscriber(
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

    #[tracing::instrument(level = "debug", skip(tables, other_hats), ret)]
    pub(crate) fn remote_subscriber_matching_status(
        &self,
        tables: &TablesData,
        src_face: &FaceState,
        other_hats: RegionMap<&dyn HatTrait>,
        locality: Locality,
        key_expr: &keyexpr,
    ) -> bool {
        debug_assert!(self.owns(src_face));

        let Some(res) = Resource::get_resource(&tables.root_res, key_expr) else {
            tracing::error!(keyexpr = %key_expr, "Unknown matching status resource");
            return false;
        };

        let compute_other_matches = || {
            other_hats.values().any(|hat| {
                !hat.remote_subscribers_matching(tables, Some(&res))
                    .is_empty()
            })
        };

        match locality {
            Locality::SessionLocal => self
                .face_hat(src_face)
                .remote_subs
                .values()
                .any(|sub| res.matches(sub)),
            Locality::Remote => {
                self.owned_faces(tables)
                    .filter(|f| f.id != src_face.id)
                    .flat_map(|f| self.face_hat(f).remote_subs.values())
                    .any(|sub| res.matches(sub))
                    || compute_other_matches()
            }
            Locality::Any => {
                self.owned_faces(tables)
                    .flat_map(|f| self.face_hat(f).remote_subs.values())
                    .any(|sub| res.matches(sub))
                    || compute_other_matches()
            }
        }
    }
}

impl HatPubSubTrait for Hat {
    #[tracing::instrument(level = "debug", skip(tables), ret)]
    fn sourced_subscribers(&self, tables: &TablesData) -> Vec<(Arc<Resource>, Sources)> {
        let mut subs = HashMap::new();
        for face in self.owned_faces(tables) {
            for (sub, _) in self.face_hat(face).remote_qabls.values() {
                let srcs = subs.entry(sub.clone()).or_insert_with(Sources::empty);
                srcs.clients.push(face.zid);
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
                    .clients
                    .push(face.zid);
            }
        }
        result.into_iter().collect()
    }

    #[tracing::instrument(level = "debug", skip(tables, src_face, _node_id), ret)]
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

        let matches = expr
            .resource()
            .as_ref()
            .and_then(|res| res.ctx.as_ref())
            .map(|ctx| Cow::from(&ctx.matches))
            .unwrap_or_else(|| Cow::from(Resource::get_matches(tables, key_expr)));

        for mres in matches.iter() {
            let mres = mres.upgrade().unwrap();

            for ctx in self.owned_face_contexts(&mres) {
                if ctx.subs.is_some() && src_face.id != ctx.face.id {
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

        Arc::new(route.build())
    }

    #[tracing::instrument(level = "debug", skip(ctx, id, _node_id, info), ret)]
    fn register_subscriber(
        &mut self,
        ctx: DispatcherContext,
        id: SubscriberId,
        mut res: Arc<Resource>,
        _node_id: NodeId,
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

    #[tracing::instrument(level = "debug", skip(ctx, id, _res, _node_id), ret)]
    fn unregister_subscriber(
        &mut self,
        ctx: DispatcherContext,
        id: SubscriberId,
        _res: Option<Arc<Resource>>,
        _node_id: NodeId,
    ) -> Option<Arc<Resource>> {
        let Some(mut res) = self.face_hat_mut(ctx.src_face).remote_subs.remove(&id) else {
            tracing::error!(id, "Unknown subscriber");
            return None;
        };

        if self
            .face_hat(ctx.src_face)
            .remote_subs
            .values()
            .contains(&res)
        {
            tracing::debug!(id, ?res, "Duplicated subscriber");
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
    fn unregister_face_subscriber(&mut self, ctx: DispatcherContext) -> HashSet<Arc<Resource>> {
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

    #[tracing::instrument(level = "debug", skip(ctx), ret)]
    fn propagate_subscriber(
        &mut self,
        ctx: DispatcherContext,
        res: Arc<Resource>,
        other_info: Option<SubscriberInfo>,
    ) {
        for dst_face in self
            .owned_faces_mut(ctx.tables)
            .filter(|f| f.id != ctx.src_face.id)
        {
            if let Some(info) = self
                .owned_face_contexts(&res)
                .filter(|face_ctx| face_ctx.face.id != dst_face.id)
                .flat_map(|face_ctx| face_ctx.subs)
                .chain(other_info.into_iter())
                .reduce(|_, _| SubscriberInfo)
            {
                self.maybe_propagate_subscriber(&res, &info, dst_face, ctx.send_declare);
            }
        }
    }

    #[tracing::instrument(level = "debug", skip(ctx), ret)]
    fn unpropagate_subscriber(&mut self, ctx: DispatcherContext, res: Arc<Resource>) {
        for mut face in self
            .owned_faces(ctx.tables)
            .filter(|f| f.id != ctx.src_face.id)
            .cloned()
        {
            self.maybe_unpropagate_subscriber(&mut face, &res, ctx.send_declare);
        }
    }

    #[tracing::instrument(level = "debug", skip(ctx), ret)]
    fn unpropagate_last_non_owned_subscriber(
        &mut self,
        ctx: DispatcherContext,
        res: Arc<Resource>,
    ) {
        debug_assert!(self.remote_subscribers_of(&res).is_some());

        if let Ok(face) = self
            .owned_face_contexts(&res)
            .filter_map(|ctx| ctx.subs.map(|_| &ctx.face))
            .exactly_one()
            .cloned()
            .as_mut()
        {
            self.maybe_unpropagate_subscriber(face, &res, ctx.send_declare)
        }
    }

    #[tracing::instrument(level = "trace", ret)]
    fn remote_subscribers_of(&self, res: &Resource) -> Option<SubscriberInfo> {
        self.owned_face_contexts(res)
            .filter_map(|ctx| ctx.subs)
            .reduce(|_, _| SubscriberInfo)
    }

    #[allow(clippy::incompatible_msrv)]
    #[tracing::instrument(level = "trace", skip(tables), ret)]
    fn remote_subscribers_matching(
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
