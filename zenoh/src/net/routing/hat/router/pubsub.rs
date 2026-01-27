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
    sync::Arc,
};

use itertools::Itertools;
use petgraph::graph::NodeIndex;
use zenoh_protocol::{
    core::{WhatAmI, ZenohIdProto},
    network::declare::{
        common::ext::WireExprType, ext, Declare, DeclareBody, DeclareSubscriber, SubscriberId,
        UndeclareSubscriber,
    },
};

use super::Hat;
use crate::net::{
    protocol::network::Network,
    routing::{
        dispatcher::{
            face::FaceState,
            pubsub::SubscriberInfo,
            resource::{NodeId, Resource},
            tables::{Route, RoutingExpr, TablesData},
        },
        hat::{BaseContext, HatBaseTrait, HatPubSubTrait, Sources},
        router::{Direction, RouteBuilder},
        RoutingContext,
    },
};
#[allow(unused_imports)]
use crate::zenoh_core::polyfill::*;

impl Hat {
    pub(super) fn pubsub_tree_change(
        &mut self,
        tables: &mut TablesData,
        new_children: &[Vec<NodeIndex>],
    ) {
        let net = self.net();
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
    fn send_sourced_subscription_to_net_children(
        &self,
        tables: &TablesData,
        net: &Network, // TODO(regions): remove this
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
                            let key_expr = Resource::decl_key(res, &mut someface);

                            someface.primitives.send_declare(RoutingContext::with_expr(
                                &mut Declare {
                                    interest_id: None,
                                    ext_qos: ext::QoSType::DECLARE,
                                    ext_tstamp: None,
                                    ext_nodeid: ext::NodeIdType {
                                        node_id: routing_context,
                                    },
                                    body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                        id: SubscriberId::default(), // Sourced subscriptions do not use ids
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
                            let wire_expr = Resource::decl_key(res, &mut someface);

                            someface.primitives.send_declare(RoutingContext::with_expr(
                                &mut Declare {
                                    interest_id: None,
                                    ext_qos: ext::QoSType::DECLARE,
                                    ext_tstamp: None,
                                    ext_nodeid: ext::NodeIdType {
                                        node_id: routing_context.unwrap_or(0),
                                    },
                                    body: DeclareBody::UndeclareSubscriber(UndeclareSubscriber {
                                        id: SubscriberId::default(), // Sourced subscriptions do not use ids
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

    pub(super) fn unregister_node_subscriptions(
        &mut self,
        zid: &ZenohIdProto,
    ) -> HashSet<Arc<Resource>> {
        let removed_routers = self
            .net_mut()
            .remove_link(zid)
            .into_iter()
            .map(|(_, node)| node)
            .collect::<HashSet<_>>();

        let mut resources = HashSet::new();

        for mut res in self.router_subs.iter().cloned().collect_vec() {
            self.res_hat_mut(&mut res)
                .router_subs
                .retain(|router| !removed_routers.contains(router));

            if self.res_hat(&res).router_subs.is_empty() {
                self.router_subs.retain(|r| !Arc::ptr_eq(r, &res));
                resources.insert(res);
            }
        }

        resources
    }
}

impl HatPubSubTrait for Hat {
    #[tracing::instrument(level = "debug", skip(_tables), ret)]
    fn sourced_subscribers(&self, _tables: &TablesData) -> Vec<(Arc<Resource>, Sources)> {
        // Compute the list of known subscriptions (keys)
        self.router_subs
            .iter()
            .map(|sub| {
                (
                    sub.clone(),
                    Sources {
                        routers: Vec::from_iter(self.res_hat(sub).router_subs.iter().cloned()),
                        peers: Vec::default(),
                        clients: Vec::default(),
                    },
                )
            })
            .collect()
    }

    #[tracing::instrument(level = "debug", skip(_tables), ret)]
    fn sourced_publishers(&self, _tables: &TablesData) -> Vec<(Arc<Resource>, Sources)> {
        Vec::default()
    }

    #[tracing::instrument(level = "trace", skip_all, fields(zid = %tables.zid.short(), rgn = %self.region), ret)]
    fn compute_data_route(
        &self,
        tables: &TablesData,
        src_face: &FaceState,
        expr: &RoutingExpr,
        node_id: NodeId,
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
        }

        Arc::new(route.build())
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region))]
    fn register_subscription(
        &mut self,
        ctx: BaseContext,
        _id: SubscriberId,
        mut res: Arc<Resource>,
        nid: NodeId,
        info: &SubscriberInfo,
    ) {
        debug_assert!(self.owns(ctx.src_face));

        let Some(router) = self.get_router(ctx.src_face, nid) else {
            tracing::error!(%nid, "Subscriber from unknown router");
            return;
        };

        debug_assert_ne!(router, ctx.tables.zid);

        self.res_hat_mut(&mut res).router_subs.insert(router);
        self.router_subs.insert(res.clone());

        self.propagate_sourced_subscription(ctx.tables, &res, info, Some(ctx.src_face), &router);
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region), ret)]
    fn unregister_subscription(
        &mut self,
        ctx: BaseContext,
        _id: SubscriberId,
        res: Option<Arc<Resource>>,
        nid: NodeId,
    ) -> Option<Arc<Resource>> {
        debug_assert!(self.owns(ctx.src_face));

        let Some(router) = self.get_router(ctx.src_face, nid) else {
            tracing::error!(%nid, "Subscriber from unknown router");
            return None;
        };

        debug_assert_ne!(router, ctx.tables.zid);

        let Some(mut res) = res else {
            tracing::error!("Subscriber undeclaration in router region with no resource");
            return None;
        };

        self.res_hat_mut(&mut res).router_subs.remove(&router);
        if self.res_hat(&res).router_subs.is_empty() {
            self.router_subs.retain(|r| !Arc::ptr_eq(r, &res));
        }

        self.propagate_forget_sourced_subscription(ctx.tables, &res, Some(ctx.src_face), &router);

        Some(res)
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region), ret)]
    fn unregister_face_subscriptions(&mut self, ctx: BaseContext) -> HashSet<Arc<Resource>> {
        self.unregister_node_subscriptions(&ctx.src_face.zid)
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region))]
    fn propagate_subscription(
        &mut self,
        ctx: BaseContext,
        mut res: Arc<Resource>,
        other_info: Option<SubscriberInfo>,
    ) {
        let Some(other_info) = other_info else {
            // NOTE(regions): see Hat::register_subscription
            debug_assert!(self.owns(ctx.src_face));
            return;
        };

        if !self.res_hat(&res).router_subs.contains(&ctx.tables.zid) {
            self.res_hat_mut(&mut res)
                .router_subs
                .insert(ctx.tables.zid);
            self.router_subs.insert(res.clone());

            self.propagate_sourced_subscription(
                ctx.tables,
                &res,
                &other_info,
                None,
                &ctx.tables.zid,
            );
        }
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region))]
    fn unpropagate_subscription(&mut self, ctx: BaseContext, mut res: Arc<Resource>) {
        if self.owns(ctx.src_face) {
            // NOTE(regions): see Hat::unregister_subscription
            return;
        }

        let was_propagated = self.res_hat(&res).router_subs.contains(&ctx.tables.zid);

        debug_assert!(was_propagated);

        if was_propagated {
            self.res_hat_mut(&mut res)
                .router_subs
                .remove(&ctx.tables.zid);
            if self.res_hat(&res).router_subs.is_empty() {
                self.router_subs.retain(|r| !Arc::ptr_eq(r, &res));
            }

            self.propagate_forget_sourced_subscription(ctx.tables, &res, None, &ctx.tables.zid);
        }
    }

    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region), ret)]
    fn remote_subscriptions_of(&self, res: &Resource) -> Option<SubscriberInfo> {
        // FIXME(regions): use TablesData::zid?
        let net = self.net();
        let this_router = &net.graph[net.idx].zid;

        self.res_hat(res)
            .router_subs
            .iter()
            .any(|router| router != this_router)
            .then_some(SubscriberInfo)
    }

    #[allow(clippy::incompatible_msrv)]
    #[tracing::instrument(level = "trace", skip_all, fields(rgn = %self.region), ret)]
    fn remote_subscriptions_matching(
        &self,
        tables: &TablesData,
        res: Option<&Resource>,
    ) -> HashMap<Arc<Resource>, SubscriberInfo> {
        self.router_subs
            .iter()
            .filter_map(|sub| {
                if self
                    .res_hat(sub)
                    .router_subs
                    .iter()
                    .any(|router| router != &tables.zid)
                    && res.is_none_or(|res| res.matches(sub))
                {
                    Some((sub.clone(), SubscriberInfo))
                } else {
                    None
                }
            })
            .collect()
    }
}
