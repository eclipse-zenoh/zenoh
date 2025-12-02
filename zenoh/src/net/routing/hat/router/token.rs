//
// Copyright (c) 2024 ZettaScale Technology
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

use std::{collections::HashSet, sync::Arc};

use itertools::Itertools;
use petgraph::graph::NodeIndex;
use zenoh_protocol::{
    core::ZenohIdProto,
    network::{
        declare::{common::ext::WireExprType, TokenId},
        ext, Declare, DeclareBody, DeclareToken, UndeclareToken,
    },
};

use super::Hat;
use crate::net::{
    protocol::network::Network,
    routing::{
        dispatcher::{face::FaceState, tables::TablesData},
        hat::{BaseContext, HatBaseTrait, HatTokenTrait},
        router::{NodeId, Resource},
        RoutingContext,
    },
};

impl Hat {
    pub(super) fn token_tree_change(
        &self,
        tables: &mut TablesData,
        new_clildren: &[Vec<NodeIndex>],
    ) {
        let net = self.routers_net.as_ref().unwrap();
        // propagate tokens to new clildren
        for (tree_sid, tree_clildren) in new_clildren.iter().enumerate() {
            if !tree_clildren.is_empty() {
                let tree_idx = NodeIndex::new(tree_sid);
                if net.graph.contains_node(tree_idx) {
                    let tree_id = net.graph[tree_idx].zid;

                    let tokens_res = &self.router_tokens;

                    for res in tokens_res {
                        let tokens = &self.res_hat(res).router_tokens;
                        for token in tokens {
                            if *token == tree_id {
                                self.send_sourced_token_to_net_clildren(
                                    tables,
                                    net,
                                    tree_clildren,
                                    res,
                                    None,
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
    fn send_sourced_token_to_net_clildren(
        &self,
        tables: &TablesData,
        net: &Network,
        clildren: &[NodeIndex],
        res: &Arc<Resource>,
        src_face: Option<&Arc<FaceState>>,
        routing_context: NodeId,
    ) {
        for child in clildren {
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
                                    body: DeclareBody::DeclareToken(DeclareToken {
                                        id: TokenId::default(), // Sourced tokens do not use ids
                                        wire_expr: key_expr,
                                    }),
                                },
                                res.expr().to_string(),
                            ));
                        } else {
                            tracing::trace!(
                                face = %someface,
                                "Will not propagate token to child face"
                            );
                        }
                    }
                    None => {
                        tracing::trace!("Unable to find face for zid {}", net.graph[*child].zid)
                    }
                }
            } else {
                tracing::error!(
                    node_id = child.index() as NodeId,
                    "Network graph doesn't contain destination node"
                );
            }
        }
    }

    fn propagate_sourced_token(
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
                    tracing::trace!(
                        dst = ?&net.trees[tree_sid.index()].children,
                        "Propagating token to tree children"
                    );
                    self.send_sourced_token_to_net_clildren(
                        tables,
                        net,
                        &net.trees[tree_sid.index()].children,
                        res,
                        src_face,
                        tree_sid.index() as NodeId,
                    );
                } else {
                    tracing::trace!(
                        "Propagating liveliness {}: tree for node {} sid:{} not yet ready",
                        res.expr(),
                        tree_sid.index(),
                        source
                    );
                }
            }
            None => tracing::error!(
                "Error propagating token {}: cannot get index of {}!",
                res.expr(),
                source
            ),
        }
    }

    #[inline]
    fn send_forget_sourced_token_to_net_clildren(
        &self,
        tables: &TablesData,
        net: &Network,
        clildren: &[NodeIndex],
        res: &Arc<Resource>,
        src_face: Option<&Arc<FaceState>>,
        routing_context: Option<NodeId>,
    ) {
        for child in clildren {
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
                                    body: DeclareBody::UndeclareToken(UndeclareToken {
                                        id: TokenId::default(), // Sourced tokens do not use ids
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

    fn propagate_forget_sourced_token(
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
                    self.send_forget_sourced_token_to_net_clildren(
                        tables,
                        net,
                        &net.trees[tree_sid.index()].children,
                        res,
                        src_face,
                        Some(tree_sid.index() as NodeId),
                    );
                } else {
                    tracing::trace!(
                        "Propagating forget token {}: tree for node {} sid:{} not yet ready",
                        res.expr(),
                        tree_sid.index(),
                        source
                    );
                }
            }
            None => tracing::error!(
                "Error propagating forget token {}: cannot get index of {}!",
                res.expr(),
                source
            ),
        }
    }

    pub(super) fn unregister_node_tokens(&mut self, zid: &ZenohIdProto) -> HashSet<Arc<Resource>> {
        let removed_routers = self
            .net_mut()
            .remove_link(zid)
            .into_iter()
            .map(|(_, node)| node.zid)
            .collect::<HashSet<_>>();

        let mut resources = HashSet::new();

        for mut res in self.router_tokens.iter().cloned().collect_vec() {
            self.res_hat_mut(&mut res)
                .router_tokens
                .retain(|router| !removed_routers.contains(router));

            if self.res_hat(&res).router_tokens.is_empty() {
                self.router_tokens.retain(|r| !Arc::ptr_eq(r, &res));
                resources.insert(res);
            }
        }

        resources
    }
}

impl HatTokenTrait for Hat {
    #[tracing::instrument(level = "trace", skip(ctx))]
    fn register_token(
        &mut self,
        ctx: BaseContext,
        _id: TokenId,
        mut res: Arc<Resource>,
        nid: NodeId,
    ) {
        debug_assert!(self.owns(ctx.src_face));

        let Some(router) = self.get_router(ctx.src_face, nid) else {
            tracing::error!(%nid, "Token from unknown router");
            return;
        };

        debug_assert_ne!(router, ctx.tables.zid);

        self.res_hat_mut(&mut res).router_tokens.insert(router);
        self.router_tokens.insert(res.clone());

        self.propagate_sourced_token(ctx.tables, &res, Some(ctx.src_face), &router);
    }

    #[tracing::instrument(level = "trace", skip(ctx), ret)]
    fn unregister_token(
        &mut self,
        ctx: BaseContext,
        _id: TokenId,
        res: Option<Arc<Resource>>,
        nid: NodeId,
    ) -> Option<Arc<Resource>> {
        debug_assert!(self.owns(ctx.src_face));

        let Some(router) = self.get_router(ctx.src_face, nid) else {
            tracing::error!(%nid, "Token from unknown router");
            return None;
        };

        debug_assert_ne!(router, ctx.tables.zid);

        let Some(mut res) = res else {
            tracing::error!("Token undeclaration in router region with no resource");
            return None;
        };

        self.res_hat_mut(&mut res).router_tokens.remove(&router);

        if self.res_hat(&res).router_tokens.is_empty() {
            self.router_tokens.retain(|r| !Arc::ptr_eq(r, &res));
        }

        self.propagate_forget_sourced_token(ctx.tables, &res, Some(ctx.src_face), &router);

        Some(res)
    }

    #[tracing::instrument(level = "trace", skip(ctx), ret)]
    fn unregister_face_tokens(&mut self, ctx: BaseContext) -> HashSet<Arc<Resource>> {
        self.unregister_node_tokens(&ctx.src_face.zid)
    }

    #[tracing::instrument(level = "trace", skip(ctx))]
    fn propagate_token(&mut self, ctx: BaseContext, mut res: Arc<Resource>, other_tokens: bool) {
        if !other_tokens {
            // NOTE(regions): see Hat::register_token
            debug_assert!(self.owns(ctx.src_face));
            return;
        };

        if !self.res_hat(&res).router_tokens.contains(&ctx.tables.zid) {
            self.res_hat_mut(&mut res)
                .router_tokens
                .insert(ctx.tables.zid);
            self.router_tokens.insert(res.clone());

            self.propagate_sourced_token(ctx.tables, &res, None, &ctx.tables.zid);
        }
    }

    #[tracing::instrument(level = "trace", skip(ctx))]
    fn unpropagate_token(&mut self, ctx: BaseContext, res: Arc<Resource>) {
        if self.owns(ctx.src_face) {
            // NOTE(regions): see Hat::unregister_token
            return;
        }

        self.propagate_forget_sourced_token(ctx.tables, &res, None, &ctx.tables.zid);
    }

    #[tracing::instrument(level = "trace", ret)]
    fn remote_tokens_of(&self, res: &Resource) -> bool {
        // FIXME(regions): use TablesData::zid?
        let net = self.net();
        let this_router = &net.graph[net.idx].zid;

        self.res_hat(res)
            .router_tokens
            .iter()
            .any(|router| router != this_router)
    }

    #[tracing::instrument(level = "trace", skip(tables), ret)]
    fn remote_tokens_matching(
        &self,
        tables: &TablesData,
        res: Option<&Resource>,
    ) -> HashSet<Arc<Resource>> {
        self.router_tokens
            .iter()
            .filter(|token| {
                self.res_hat(token)
                    .router_tokens
                    .iter()
                    .any(|router| router != &tables.zid)
                    && res.is_none_or(|res| res.matches(token))
            })
            .cloned()
            .collect()
    }
}
