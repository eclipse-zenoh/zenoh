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

use petgraph::graph::NodeIndex;
use zenoh_protocol::{
    core::{WhatAmI, ZenohIdProto},
    network::{
        declare::{
            common::ext::WireExprType, ext, Declare, DeclareBody, DeclareSubscriber, SubscriberId,
            UndeclareSubscriber,
        },
        interest::{InterestId, InterestMode},
    },
};
use zenoh_sync::get_mut_unchecked;

use super::Hat;
use crate::{
    key_expr::KeyExpr,
    net::{
        protocol::network::Network,
        routing::{
            dispatcher::{
                face::FaceState,
                pubsub::SubscriberInfo,
                resource::{FaceContext, NodeId, Resource},
                tables::{Route, RoutingExpr, TablesData},
            },
            hat::{
                BaseContext, CurrentFutureTrait, HatBaseTrait, HatPubSubTrait, InterestProfile,
                SendDeclare, Sources,
            },
            router::{disable_matches_data_routes, Direction, RouteBuilder, DEFAULT_NODE_ID},
            RoutingContext,
        },
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
                self.push_declaration_profile(dst_face),
            );
            send_declare(
                &dst_face.primitives,
                RoutingContext::with_expr(
                    Declare {
                        interest_id: None,
                        ext_qos: ext::QoSType::DECLARE,
                        ext_tstamp: None,
                        ext_nodeid: ext::NodeIdType::DEFAULT,
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
        send_ext_wire_expr: bool,
        send_declare: &mut SendDeclare,
    ) {
        for update in self
            .face_hat_mut(face)
            .local_subs
            .remove_simple_resource(res)
        {
            let ext_wire_expr = if send_ext_wire_expr {
                WireExprType {
                    wire_expr: Resource::get_best_key(&update.resource, "", face.id),
                }
            } else {
                WireExprType::null()
            };
            send_declare(
                &face.primitives,
                RoutingContext::with_expr(
                    Declare {
                        interest_id: None,
                        ext_qos: ext::QoSType::DECLARE,
                        ext_tstamp: None,
                        ext_nodeid: ext::NodeIdType::DEFAULT,
                        body: DeclareBody::UndeclareSubscriber(UndeclareSubscriber {
                            id: update.id,
                            ext_wire_expr,
                        }),
                    },
                    update.resource.expr().to_string(),
                ),
            );
        }
    }

    fn get_subscribers_matching_resource(
        &self,
        tables: &TablesData,
        face: &Arc<FaceState>,
        res: Option<&Arc<Resource>>,
    ) -> Vec<Arc<Resource>> {
        let face_id = face.id;
        let face_what_am_i = face.whatami;

        self.router_subs
            .iter()
            .filter(move |sub| {
                sub.ctx.is_some()
                    && res.as_ref().map(|r| sub.matches(r)).unwrap_or(true)
                    && (self
                        .res_hat(sub)
                        .router_subs
                        .iter()
                        .any(|r| *r != tables.zid)
                        || sub.face_ctxs.values().any(|s| {
                            s.face.id != face_id
                                && s.subs.is_some()
                                && (s.face.whatami == WhatAmI::Client
                                    || face_what_am_i == WhatAmI::Client)
                        }))
            })
            .cloned()
            .collect()
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    fn send_sourced_subscription_to_net_children(
        &self,
        tables: &TablesData,
        net: &Network,
        children: &[NodeIndex],
        res: &Arc<Resource>,
        src_face: Option<&Arc<FaceState>>,
        _sub_info: &SubscriberInfo,
        routing_context: NodeId,
    ) {
        for child in children {
            if net.graph.contains_node(*child) {
                match self.face(tables, &net.graph[*child].zid).cloned() {
                    Some(mut someface) => {
                        if src_face
                            .map(|src_face| someface.id != src_face.id)
                            .unwrap_or(true)
                        {
                            let push_declaration = self.push_declaration_profile(&someface);
                            let key_expr = Resource::decl_key(res, &mut someface, push_declaration);

                            someface.primitives.send_declare(RoutingContext::with_expr(
                                &mut Declare {
                                    interest_id: None,
                                    ext_qos: ext::QoSType::DECLARE,
                                    ext_tstamp: None,
                                    ext_nodeid: ext::NodeIdType {
                                        node_id: routing_context,
                                    },
                                    body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                        id: 0, // Sourced subscriptions do not use ids
                                        wire_expr: key_expr,
                                    }),
                                },
                                res.expr().to_string(),
                            ));
                        }
                    }
                    None => {
                        tracing::trace!("Unable to find face for zid {}", net.graph[*child].zid)
                    }
                }
            }
        }
    }

    #[inline]
    fn propagate_simple_subscription_to(
        &self,
        dst_face: &mut Arc<FaceState>,
        res: &Arc<Resource>,
        _sub_info: &SubscriberInfo,
        src_face: &mut Arc<FaceState>,
        send_declare: &mut SendDeclare,
    ) {
        if src_face.id != dst_face.id
            && !self
                .face_hat_mut(dst_face)
                .local_subs
                .contains_simple_resource(res)
            && dst_face.whatami != WhatAmI::Router
            && self.should_route_between(src_face, dst_face)
        {
            self.maybe_register_local_subscriber(dst_face, res, None, send_declare);
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
                &mut dst_face,
                res,
                sub_info,
                src_face,
                send_declare,
            );
        }
    }

    fn propagate_sourced_subscription(
        &self,
        tables: &TablesData,
        res: &Arc<Resource>,
        sub_info: &SubscriberInfo,
        src_face: Option<&Arc<FaceState>>,
        source: &ZenohIdProto,
    ) {
        let net = &self.routers_net.as_ref().unwrap();
        match net.get_idx(source) {
            Some(tree_sid) => {
                if net.trees.len() > tree_sid.index() {
                    self.send_sourced_subscription_to_net_children(
                        tables,
                        net,
                        &net.trees[tree_sid.index()].children,
                        res,
                        src_face,
                        sub_info,
                        tree_sid.index() as NodeId,
                    );
                } else {
                    tracing::trace!(
                        "Propagating sub {}: tree for node {} sid:{} not yet ready",
                        res.expr(),
                        tree_sid.index(),
                        source
                    );
                }
            }
            None => tracing::error!(
                "Error propagating sub {}: cannot get index of {}!",
                res.expr(),
                source
            ),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn register_router_subscription(
        &mut self,
        tables: &mut TablesData,
        face: &mut Arc<FaceState>,
        res: &mut Arc<Resource>,
        sub_info: &SubscriberInfo,
        router: ZenohIdProto,
        send_declare: &mut SendDeclare,
        profile: InterestProfile,
    ) {
        if !self.res_hat(res).router_subs.contains(&router) {
            // Register router subscription
            {
                self.res_hat_mut(res).router_subs.insert(router);
                self.router_subs.insert(res.clone());
            }

            // REVIEW(regions): not sure
            if profile.is_push()
                || self.router_remote_interests.values().any(|interest| {
                    interest.options.subscribers()
                        && interest
                            .res
                            .as_ref()
                            .map(|r| r.matches(res))
                            .unwrap_or(true)
                })
                || router != tables.zid
            {
                // Propagate subscription to routers
                self.propagate_sourced_subscription(tables, res, sub_info, Some(face), &router);
            }
        }

        // Propagate subscription to clients
        self.propagate_simple_subscription(tables, res, sub_info, face, send_declare);
    }

    #[allow(clippy::too_many_arguments)]
    fn declare_router_subscription(
        &mut self,
        tables: &mut TablesData,
        face: &mut Arc<FaceState>,
        res: &mut Arc<Resource>,
        sub_info: &SubscriberInfo,
        router: ZenohIdProto,
        send_declare: &mut SendDeclare,
        profile: InterestProfile,
    ) {
        self.register_router_subscription(
            tables,
            face,
            res,
            sub_info,
            router,
            send_declare,
            profile,
        );
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
        &mut self,
        tables: &mut TablesData,
        face: &mut Arc<FaceState>,
        id: SubscriberId,
        res: &mut Arc<Resource>,
        sub_info: &SubscriberInfo,
        send_declare: &mut SendDeclare,
    ) {
        self.register_simple_subscription(tables, face, id, res, sub_info);
        let zid = tables.zid;
        self.register_router_subscription(
            tables,
            face,
            res,
            sub_info,
            zid,
            send_declare,
            InterestProfile::Push,
        );
    }

    #[inline]
    fn remote_router_subs(&self, tables: &TablesData, res: &Arc<Resource>) -> bool {
        res.ctx.is_some()
            && self
                .res_hat(res)
                .router_subs
                .iter()
                .any(|peer| peer != &tables.zid)
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
    fn send_forget_sourced_subscription_to_net_children(
        &self,
        tables: &TablesData,
        net: &Network,
        children: &[NodeIndex],
        res: &Arc<Resource>,
        src_face: Option<&Arc<FaceState>>,
        routing_context: Option<NodeId>,
    ) {
        for child in children {
            if net.graph.contains_node(*child) {
                match self.face(tables, &net.graph[*child].zid).cloned() {
                    Some(mut someface) => {
                        if src_face
                            .map(|src_face| someface.id != src_face.id)
                            .unwrap_or(true)
                        {
                            let push_declaration = self.push_declaration_profile(&someface);
                            let wire_expr =
                                Resource::decl_key(res, &mut someface, push_declaration);

                            someface.primitives.send_declare(RoutingContext::with_expr(
                                &mut Declare {
                                    interest_id: None,
                                    ext_qos: ext::QoSType::DECLARE,
                                    ext_tstamp: None,
                                    ext_nodeid: ext::NodeIdType {
                                        node_id: routing_context.unwrap_or(0),
                                    },
                                    body: DeclareBody::UndeclareSubscriber(UndeclareSubscriber {
                                        id: 0, // Sourced subscriptions do not use ids
                                        ext_wire_expr: WireExprType { wire_expr },
                                    }),
                                },
                                res.expr().to_string(),
                            ));
                        }
                    }
                    None => {
                        tracing::trace!("Unable to find face for zid {}", net.graph[*child].zid)
                    }
                }
            }
        }
    }

    fn propagate_forget_simple_subscription(
        &self,
        tables: &mut TablesData,
        res: &Arc<Resource>,
        send_declare: &mut SendDeclare,
    ) {
        for mut face in self.faces(tables).values().cloned() {
            self.maybe_unregister_local_subscriber(&mut face, res, false, send_declare);
        }
    }

    fn propagate_forget_simple_subscription_to_peers(
        &self,
        tables: &mut TablesData,
        res: &Arc<Resource>,
        send_declare: &mut SendDeclare,
    ) {
        if self.res_hat(res).router_subs.len() == 1
            && self.res_hat(res).router_subs.contains(&tables.zid)
        {
            for mut face in self
                .faces(tables)
                .values()
                .cloned()
                .collect::<Vec<Arc<FaceState>>>()
            {
                if face.whatami == WhatAmI::Peer
                    && self
                        .face_hat(&face)
                        .local_subs
                        .contains_simple_resource(res)
                    && !res.face_ctxs.values().any(|s| {
                        face.zid != s.face.zid
                            && s.subs.is_some()
                            && s.face.whatami == WhatAmI::Client
                    })
                {
                    self.maybe_unregister_local_subscriber(&mut face, res, false, send_declare);
                }
            }
        }
    }

    fn propagate_forget_sourced_subscription(
        &self,
        tables: &TablesData,
        res: &Arc<Resource>,
        src_face: Option<&Arc<FaceState>>,
        source: &ZenohIdProto,
    ) {
        let net = self.routers_net.as_ref().unwrap();
        match net.get_idx(source) {
            Some(tree_sid) => {
                if net.trees.len() > tree_sid.index() {
                    self.send_forget_sourced_subscription_to_net_children(
                        tables,
                        net,
                        &net.trees[tree_sid.index()].children,
                        res,
                        src_face,
                        Some(tree_sid.index() as NodeId),
                    );
                } else {
                    tracing::trace!(
                        "Propagating forget sub {}: tree for node {} sid:{} not yet ready",
                        res.expr(),
                        tree_sid.index(),
                        source
                    );
                }
            }
            None => tracing::error!(
                "Error propagating forget sub {}: cannot get index of {}!",
                res.expr(),
                source
            ),
        }
    }

    fn unregister_router_subscription(
        &mut self,
        tables: &mut TablesData,
        res: &mut Arc<Resource>,
        router: &ZenohIdProto,
        send_declare: &mut SendDeclare,
    ) {
        self.res_hat_mut(res)
            .router_subs
            .retain(|sub| sub != router);

        if self.res_hat(res).router_subs.is_empty() {
            self.router_subs.retain(|sub| !Arc::ptr_eq(sub, res));

            self.propagate_forget_simple_subscription(tables, res, send_declare);
        }

        self.propagate_forget_simple_subscription_to_peers(tables, res, send_declare);
    }

    fn undeclare_router_subscription(
        &mut self,
        tables: &mut TablesData,
        face: Option<&Arc<FaceState>>,
        res: &mut Arc<Resource>,
        router: &ZenohIdProto,
        send_declare: &mut SendDeclare,
        profile: InterestProfile,
    ) {
        if self.res_hat(res).router_subs.contains(router) {
            self.unregister_router_subscription(tables, res, router, send_declare);
            // REVIEW(regions): not sure
            if profile.is_push()
                || self.router_remote_interests.values().any(|interest| {
                    interest.options.subscribers()
                        && interest
                            .res
                            .as_ref()
                            .map(|r| r.matches(res))
                            .unwrap_or(true)
                })
                || router == &tables.zid
            {
                self.propagate_forget_sourced_subscription(tables, res, face, router);
            }
        }
    }

    fn forget_router_subscription(
        &mut self,
        tables: &mut TablesData,
        face: &mut Arc<FaceState>,
        res: &mut Arc<Resource>,
        router: &ZenohIdProto,
        send_declare: &mut SendDeclare,
        profile: InterestProfile,
    ) {
        self.undeclare_router_subscription(tables, Some(face), res, router, send_declare, profile);
    }

    pub(super) fn undeclare_simple_subscription(
        &mut self,
        ctx: BaseContext,
        res: &mut Arc<Resource>,
        profile: InterestProfile,
    ) {
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
            let router_subs = self.remote_router_subs(ctx.tables, res);
            if simple_subs.is_empty() {
                self.undeclare_router_subscription(
                    ctx.tables,
                    None,
                    res,
                    &ctx.tables.zid.clone(),
                    ctx.send_declare,
                    profile,
                );
            } else {
                self.propagate_forget_simple_subscription_to_peers(
                    ctx.tables,
                    res,
                    ctx.send_declare,
                );
            }

            if simple_subs.len() == 1 && !router_subs {
                self.maybe_unregister_local_subscriber(
                    &mut simple_subs[0],
                    res,
                    false,
                    ctx.send_declare,
                );
            }
        }
    }

    fn forget_simple_subscription(
        &mut self,
        ctx: BaseContext,
        id: SubscriberId,
        profile: InterestProfile,
    ) -> Option<Arc<Resource>> {
        if let Some(mut res) = self.face_hat_mut(ctx.src_face).remote_subs.remove(&id) {
            self.undeclare_simple_subscription(ctx, &mut res, profile);
            Some(res)
        } else {
            None
        }
    }

    pub(super) fn pubsub_remove_node(
        &mut self,
        tables: &mut TablesData,
        node: &ZenohIdProto,
        send_declare: &mut SendDeclare,
    ) {
        for mut res in self
            .router_subs
            .iter()
            .filter(|res| self.res_hat(res).router_subs.contains(node))
            .cloned()
            .collect::<Vec<Arc<Resource>>>()
        {
            self.unregister_router_subscription(tables, &mut res, node, send_declare);

            disable_matches_data_routes(&mut res, &self.bound);
            Resource::clean(&mut res)
        }
    }

    pub(super) fn pubsub_tree_change(
        &mut self,
        tables: &mut TablesData,
        new_children: &[Vec<NodeIndex>],
    ) {
        let net = self.routers_net.as_ref().unwrap();
        // propagate subs to new children
        for (tree_sid, tree_children) in new_children.iter().enumerate() {
            if !tree_children.is_empty() {
                let tree_idx = NodeIndex::new(tree_sid);
                if net.graph.contains_node(tree_idx) {
                    let tree_id = net.graph[tree_idx].zid;

                    let subs_res = &self.router_subs;

                    for res in subs_res {
                        let subs = &self.res_hat(res).router_subs;
                        for sub in subs {
                            if *sub == tree_id {
                                let sub_info = SubscriberInfo;
                                self.send_sourced_subscription_to_net_children(
                                    tables,
                                    net,
                                    tree_children,
                                    res,
                                    None,
                                    &sub_info,
                                    tree_sid as NodeId,
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn declare_sub_interest(
        &self,
        tables: &mut TablesData,
        face: &mut Arc<FaceState>,
        interest_id: InterestId,
        res: Option<&mut Arc<Resource>>,
        mode: InterestMode,
        aggregate: bool,
        _source: NodeId,
        send_declare: &mut SendDeclare,
    ) {
        let res = res.map(|r| r.clone());
        let matching_subs = self.get_subscribers_matching_resource(tables, face, res.as_ref());
        let mut matching_subs = matching_subs.iter();

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
                        self.push_declaration_profile(face),
                    );
                    send_declare(
                        &face.primitives,
                        RoutingContext::with_expr(
                            Declare {
                                interest_id: Some(interest_id),
                                ext_qos: ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType::DEFAULT,
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
                let wire_expr = Resource::decl_key(sub, face, self.push_declaration_profile(face));
                send_declare(
                    &face.primitives,
                    RoutingContext::with_expr(
                        Declare {
                            interest_id: Some(interest_id),
                            ext_qos: ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType::DEFAULT,
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
        node_id: NodeId,
        sub_info: &SubscriberInfo,
        profile: InterestProfile,
    ) {
        let router = if self.owns_router(ctx.src_face) {
            let Some(router) = self.get_router(ctx.src_face, node_id) else {
                tracing::error!(%node_id, "Subscriber from unknown router");
                return;
            };

            router
        } else {
            ctx.tables.zid
        };

        match ctx.src_face.whatami {
            WhatAmI::Router => {
                self.declare_router_subscription(
                    ctx.tables,
                    ctx.src_face,
                    res,
                    sub_info,
                    router,
                    ctx.send_declare,
                    profile,
                );
            }
            WhatAmI::Peer | WhatAmI::Client => {
                // TODO(regions2): clients and peers of this
                // router are handled as if they were bound to future broker/peer-to-peer south hats resp.
                self.declare_simple_subscription(
                    ctx.tables,
                    ctx.src_face,
                    id,
                    res,
                    sub_info,
                    ctx.send_declare,
                );
            }
        }
    }

    fn undeclare_subscription(
        &mut self,
        ctx: BaseContext,
        id: SubscriberId,
        res: Option<Arc<Resource>>,
        node_id: NodeId,
        profile: InterestProfile,
    ) -> Option<Arc<Resource>> {
        let router = if self.owns_router(ctx.src_face) {
            let Some(router) = self.get_router(ctx.src_face, node_id) else {
                tracing::error!(%node_id, "Subscriber from unknown router");
                return None;
            };

            router
        } else {
            ctx.tables.zid
        };

        let mut res = res?;

        match ctx.src_face.whatami {
            WhatAmI::Router => {
                self.forget_router_subscription(
                    ctx.tables,
                    ctx.src_face,
                    &mut res,
                    &router,
                    ctx.send_declare,
                    profile,
                );
                Some(res)
            }
            WhatAmI::Peer | WhatAmI::Client => self.forget_simple_subscription(ctx, id, profile),
        }
    }

    fn get_subscriptions(&self, _tables: &TablesData) -> Vec<(Arc<Resource>, Sources)> {
        // Compute the list of known subscriptions (keys)
        self.router_subs
            .iter()
            .map(|s| {
                // Compute the list of routers, peers and clients that are known
                // sources of those subscriptions
                let routers = Vec::from_iter(self.res_hat(s).router_subs.iter().cloned());
                let mut peers = vec![];
                let mut clients = vec![];
                for ctx in s
                    .face_ctxs
                    .values()
                    .filter(|ctx| ctx.subs.is_some() && !ctx.face.is_local)
                {
                    match ctx.face.whatami {
                        WhatAmI::Router => (),
                        WhatAmI::Peer => peers.push(ctx.face.zid),
                        WhatAmI::Client => clients.push(ctx.face.zid),
                    }
                }
                (
                    s.clone(),
                    Sources {
                        routers,
                        peers,
                        clients,
                    },
                )
            })
            .collect()
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
        dst_node_id: NodeId,
    ) -> Arc<Route> {
        #[inline]
        fn insert_faces_for_subs(
            this: &Hat,
            route: &mut RouteBuilder<Direction>,
            expr: &RoutingExpr,
            tables: &TablesData,
            net: &Network,
            source: NodeId,
            subs: &HashSet<ZenohIdProto>,
        ) {
            if net.trees.len() > source as usize {
                for sub in subs {
                    if let Some(sub_idx) = net.get_idx(sub) {
                        if net.trees[source as usize].directions.len() > sub_idx.index() {
                            if let Some(direction) =
                                net.trees[source as usize].directions[sub_idx.index()]
                            {
                                if net.graph.contains_node(direction) {
                                    if let Some(face) = this.face(tables, &net.graph[direction].zid)
                                    {
                                        route.insert(face.id, || {
                                            let wire_expr = expr.get_best_key(face.id);
                                            Direction {
                                                dst_face: face.clone(),
                                                wire_expr: wire_expr.to_owned(),
                                                node_id: source,
                                                dst_node_id: DEFAULT_NODE_ID,
                                            }
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                tracing::trace!("Tree for node sid:{} not yet ready", source);
            }
        }

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

        if src_face.whatami == WhatAmI::Router
            && self.owns(src_face)
            && node_id == DEFAULT_NODE_ID
            && dst_node_id != DEFAULT_NODE_ID
        {
            if let Some(next_hop) = self.point_to_point_hop(tables, dst_node_id) {
                route.insert(next_hop.id, || {
                    let wire_expr = expr.get_best_key(next_hop.id);
                    Direction {
                        dst_face: next_hop,
                        wire_expr: wire_expr.to_owned(),
                        node_id,
                        dst_node_id,
                    }
                });
            } else {
                tracing::error!("Unable to find next hop face in point-to-point route");
            };

            return Arc::new(route.build());
        }

        let matches = expr
            .resource()
            .as_ref()
            .and_then(|res| res.ctx.as_ref())
            .map(|ctx| Cow::from(&ctx.matches))
            .unwrap_or_else(|| Cow::from(Resource::get_matches(tables, key_expr)));

        for mres in matches.iter() {
            let mres = mres.upgrade().unwrap();

            let net = self.routers_net.as_ref().unwrap();
            let router_source = match source_type {
                WhatAmI::Router => node_id,
                _ => net.idx.index() as NodeId,
            };
            insert_faces_for_subs(
                self,
                &mut route,
                expr,
                tables,
                net,
                router_source,
                &self.res_hat(&mres).router_subs,
            );

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

        // FIXME(regions): track gateway current interest finalization
        // FIXME(regions): add extension to request forwarding to the gateway
        if let Some(gwy_node_id) = self.subregion_gateway() {
            if let Some(next_hop) = self.point_to_point_hop(tables, gwy_node_id) {
                route.insert(next_hop.id, || {
                    tracing::trace!(dst = %next_hop, reason = "unfinalized subscriber interest");
                    let wire_expr = expr.get_best_key(next_hop.id);
                    Direction {
                        dst_face: next_hop,
                        wire_expr: wire_expr.to_owned(),
                        node_id: DEFAULT_NODE_ID,
                        dst_node_id: gwy_node_id,
                    }
                });
            } else {
                tracing::error!("Unable to find first hop face in point-to-point route");
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
        #[inline]
        fn insert_faces_for_subs(
            this: &Hat,
            route: &mut HashMap<usize, Arc<FaceState>>,
            tables: &TablesData,
            net: &Network,
            source: usize,
            subs: &HashSet<ZenohIdProto>,
        ) {
            if net.trees.len() > source {
                for sub in subs {
                    if let Some(sub_idx) = net.get_idx(sub) {
                        if net.trees[source].directions.len() > sub_idx.index() {
                            if let Some(direction) = net.trees[source].directions[sub_idx.index()] {
                                if net.graph.contains_node(direction) {
                                    if let Some(face) = this.face(tables, &net.graph[direction].zid)
                                    {
                                        route.entry(face.id).or_insert_with(|| face.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                tracing::trace!("Tree for node sid:{} not yet ready", source);
            }
        }

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

            let net = self.routers_net.as_ref().unwrap();
            insert_faces_for_subs(
                self,
                &mut matching_subscriptions,
                tables,
                net,
                net.idx.index(),
                &self.res_hat(&mres).router_subs,
            );

            for (sid, context) in &mres.face_ctxs {
                if context.subs.is_some() && context.face.whatami != WhatAmI::Router {
                    matching_subscriptions
                        .entry(*sid)
                        .or_insert_with(|| context.face.clone());
                }
            }
        }
        matching_subscriptions
    }
}
