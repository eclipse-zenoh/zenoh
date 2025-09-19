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

use std::sync::{atomic::Ordering, Arc};

use petgraph::graph::NodeIndex;
use zenoh_protocol::{
    core::{WhatAmI, ZenohIdProto},
    network::{
        declare::{common::ext::WireExprType, TokenId},
        ext,
        interest::{self, InterestId, InterestMode},
        Declare, DeclareBody, DeclareToken, UndeclareToken,
    },
};
use zenoh_sync::get_mut_unchecked;

use super::Hat;
use crate::net::{
    protocol::network::Network,
    routing::{
        dispatcher::{
            face::FaceState,
            gateway::BoundMap,
            interests::{CurrentInterest, RemoteInterest},
            tables::TablesData,
        },
        hat::{
            BaseContext, CurrentFutureTrait, HatBaseTrait, HatTokenTrait, HatTrait,
            InterestProfile, SendDeclare,
        },
        router::{FaceContext, NodeId, Resource, DEFAULT_NODE_ID},
        RoutingContext,
    },
};

impl Hat {
    #[inline]
    fn send_sourced_token_to_net_clildren(
        &self,
        tables: &TablesData,
        net: &Network,
        clildren: &[NodeIndex],
        res: &Arc<Resource>,
        src_face: Option<&Arc<FaceState>>,
        routing_context: NodeId,
        interest_id: Option<InterestId>,
    ) {
        for child in clildren {
            if net.graph.contains_node(*child) {
                match self.face(tables, &net.graph[*child].zid).cloned() {
                    Some(mut someface) => {
                        if src_face
                            .map(|src_face| someface.id != src_face.id)
                            .unwrap_or(true)
                        {
                            let push_declaration = self.push_declaration_profile(&someface);
                            let key_expr = Resource::decl_key(res, &mut someface, push_declaration);

                            tracing::trace!(
                                interest_id,
                                keyexpr = %res.expr(),
                                dst = %net.graph[*child].zid.short(),
                                "Propagating token to tree child"
                            );
                            someface.primitives.send_declare(RoutingContext::with_expr(
                                &mut Declare {
                                    interest_id,
                                    ext_qos: ext::QoSType::DECLARE,
                                    ext_tstamp: None,
                                    ext_nodeid: ext::NodeIdType {
                                        node_id: routing_context,
                                    },
                                    body: DeclareBody::DeclareToken(DeclareToken {
                                        id: 0, // Sourced tokens do not use ids
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

    #[inline]
    fn propagate_simple_token_to(
        &self,
        tables: &mut TablesData,
        dst_face: &mut Arc<FaceState>,
        res: &Arc<Resource>,
        src_face: &mut Arc<FaceState>,
        send_declare: &mut SendDeclare,
    ) {
        if (src_face.id != dst_face.id || dst_face.zid == tables.zid)
            && !self.face_hat(dst_face).local_tokens.contains_key(res)
            && dst_face.whatami != WhatAmI::Router
            && (src_face.whatami != WhatAmI::Peer || dst_face.whatami != WhatAmI::Peer)
        {
            let matching_interests = self
                .face_hat(dst_face)
                .remote_interests
                .values()
                .filter(|i| i.options.tokens() && i.matches(res))
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
                if !self.face_hat(dst_face).local_tokens.contains_key(res) {
                    let id = self
                        .face_hat(dst_face)
                        .next_id
                        .fetch_add(1, Ordering::SeqCst);
                    self.face_hat_mut(dst_face)
                        .local_tokens
                        .insert(res.clone(), id);
                    let key_expr =
                        Resource::decl_key(res, dst_face, self.push_declaration_profile(dst_face));
                    send_declare(
                        &dst_face.primitives,
                        RoutingContext::with_expr(
                            Declare {
                                interest_id: None,
                                ext_qos: ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType::DEFAULT,
                                body: DeclareBody::DeclareToken(DeclareToken {
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

    fn propagate_simple_token(
        &self,
        tables: &mut TablesData,
        res: &Arc<Resource>,
        src_face: &mut Arc<FaceState>,
        send_declare: &mut SendDeclare,
    ) {
        for mut dst_face in self.owned_faces(tables).cloned().collect::<Vec<_>>() {
            self.propagate_simple_token_to(tables, &mut dst_face, res, src_face, send_declare);
        }
    }

    fn propagate_sourced_token(
        &self,
        tables: &TablesData,
        res: &Arc<Resource>,
        src_face: Option<&Arc<FaceState>>,
        source: &ZenohIdProto,
        interest_id: Option<InterestId>,
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
                        interest_id,
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

    fn register_router_token(
        &mut self,
        tables: &mut TablesData,
        face: &mut Arc<FaceState>,
        res: &mut Arc<Resource>,
        router: ZenohIdProto,
        interest_id: Option<InterestId>,
        send_declare: &mut SendDeclare,
        profile: InterestProfile,
    ) {
        if !self.res_hat(res).router_tokens.contains(&router) {
            // Register router liveliness
            {
                self.res_hat_mut(res).router_tokens.insert(router);
                self.router_tokens.insert(res.clone());
            }

            if profile.is_push()
                || self.router_remote_interests.values().any(|interest| {
                    interest.options.tokens()
                        && interest
                            .res
                            .as_ref()
                            .map(|r| r.matches(res))
                            .unwrap_or(true)
                })
                || router != tables.zid
            {
                // Propagate liveliness to routers
                self.propagate_sourced_token(tables, res, Some(face), &router, None);
            }
        }

        // Propagate liveliness to clients
        self.propagate_simple_token(tables, res, face, send_declare);
    }

    fn declare_router_token(
        &mut self,
        tables: &mut TablesData,
        face: &mut Arc<FaceState>,
        res: &mut Arc<Resource>,
        router: ZenohIdProto,
        interest_id: Option<InterestId>,
        send_declare: &mut SendDeclare,
        profile: InterestProfile,
    ) {
        self.register_router_token(
            tables,
            face,
            res,
            router,
            interest_id,
            send_declare,
            profile,
        );
    }

    fn register_simple_token(
        &mut self,
        _tables: &mut TablesData,
        face: &mut Arc<FaceState>,
        id: TokenId,
        res: &mut Arc<Resource>,
    ) {
        // Register liveliness
        {
            let res = get_mut_unchecked(res);
            match res.face_ctxs.get_mut(&face.id) {
                Some(ctx) => {
                    if !ctx.token {
                        get_mut_unchecked(ctx).token = true;
                    }
                }
                None => {
                    let ctx = res
                        .face_ctxs
                        .entry(face.id)
                        .or_insert_with(|| Arc::new(FaceContext::new(face.clone())));
                    get_mut_unchecked(ctx).token = true;
                }
            }
        }
        self.face_hat_mut(face)
            .remote_tokens
            .insert(id, res.clone());
    }

    fn declare_simple_token(
        &mut self,
        tables: &mut TablesData,
        face: &mut Arc<FaceState>,
        id: TokenId,
        res: &mut Arc<Resource>,
        interest_id: Option<InterestId>,
        send_declare: &mut SendDeclare,
        profile: InterestProfile,
    ) {
        self.register_simple_token(tables, face, id, res);
        let zid = tables.zid;
        self.register_router_token(tables, face, res, zid, interest_id, send_declare, profile);
    }

    #[inline]
    fn remote_router_tokens(&self, tables: &TablesData, res: &Arc<Resource>) -> bool {
        res.ctx.is_some()
            && self
                .res_hat(res)
                .router_tokens
                .iter()
                .any(|peer| peer != &tables.zid)
    }

    #[inline]
    fn simple_tokens(&self, res: &Arc<Resource>) -> Vec<Arc<FaceState>> {
        res.face_ctxs
            .values()
            .filter_map(|ctx| {
                if ctx.token {
                    Some(ctx.face.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    #[inline]
    fn remote_simple_tokens(
        &self,
        tables: &TablesData,
        res: &Arc<Resource>,
        face: &Arc<FaceState>,
    ) -> bool {
        res.face_ctxs
            .values()
            .any(|ctx| (ctx.face.id != face.id || face.zid == tables.zid) && ctx.token)
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
                                    body: DeclareBody::UndeclareToken(UndeclareToken {
                                        id: 0, // Sourced tokens do not use ids
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

    fn propagate_forget_simple_token(
        &self,
        tables: &mut TablesData,
        res: &Arc<Resource>,
        src_face: Option<&Arc<FaceState>>,
        send_declare: &mut SendDeclare,
    ) {
        for mut face in self.faces(tables).values().cloned() {
            if let Some(id) = self.face_hat_mut(&mut face).local_tokens.remove(res) {
                send_declare(
                    &face.primitives,
                    RoutingContext::with_expr(
                        Declare {
                            interest_id: None,
                            ext_qos: ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType::DEFAULT,
                            body: DeclareBody::UndeclareToken(UndeclareToken {
                                id,
                                ext_wire_expr: WireExprType::null(),
                            }),
                        },
                        res.expr().to_string(),
                    ),
                );
            // NOTE(fuzzypixelz): We need to check that `face` is not the source Face of the token
            // undeclaration, otherwise the undeclaration would be duplicated at the source Face. In
            // cases where we don't have access to a Face as we didnt't receive an undeclaration and we
            // default to true.
            } else if src_face.map_or(true, |src_face| {
                src_face.id != face.id
                    && (src_face.whatami != WhatAmI::Peer || face.whatami != WhatAmI::Peer)
            }) && self
                .face_hat(&face)
                .remote_interests
                .values()
                .any(|i| i.options.tokens() && i.matches(res) && !i.options.aggregate())
            {
                // Token has never been declared on this face.
                // Send an Undeclare with a one shot generated id and a WireExpr ext.
                send_declare(
                    &face.primitives,
                    RoutingContext::with_expr(
                        Declare {
                            interest_id: None,
                            ext_qos: ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType::DEFAULT,
                            body: DeclareBody::UndeclareToken(UndeclareToken {
                                id: self.face_hat(&face).next_id.fetch_add(1, Ordering::SeqCst),
                                ext_wire_expr: WireExprType {
                                    wire_expr: Resource::get_best_key(res, "", face.id),
                                },
                            }),
                        },
                        res.expr().to_string(),
                    ),
                );
            }
            for res in self
                .face_hat(&face)
                .local_tokens
                .keys()
                .cloned()
                .collect::<Vec<Arc<Resource>>>()
            {
                if !res.context().matches.iter().any(|m| {
                    m.upgrade().is_some_and(|m| {
                        m.ctx.is_some()
                            && (self.remote_simple_tokens(tables, &m, &face)
                                || self.remote_router_tokens(tables, &m))
                    })
                }) {
                    if let Some(id) = self.face_hat_mut(&mut face).local_tokens.remove(&res) {
                        send_declare(
                            &face.primitives,
                            RoutingContext::with_expr(
                                Declare {
                                    interest_id: None,
                                    ext_qos: ext::QoSType::DECLARE,
                                    ext_tstamp: None,
                                    ext_nodeid: ext::NodeIdType::DEFAULT,
                                    body: DeclareBody::UndeclareToken(UndeclareToken {
                                        id,
                                        ext_wire_expr: WireExprType::null(),
                                    }),
                                },
                                res.expr().to_string(),
                            ),
                        );
                    } else if self
                        .face_hat(&face)
                        .remote_interests
                        .values()
                        .any(|i| i.options.tokens() && i.matches(&res) && !i.options.aggregate())
                        && src_face.map_or(true, |src_face| {
                            src_face.whatami != WhatAmI::Peer || face.whatami != WhatAmI::Peer
                        })
                    {
                        // Token has never been declared on this face.
                        // Send an Undeclare with a one shot generated id and a WireExpr ext.
                        send_declare(
                            &face.primitives,
                            RoutingContext::with_expr(
                                Declare {
                                    interest_id: None,
                                    ext_qos: ext::QoSType::DECLARE,
                                    ext_tstamp: None,
                                    ext_nodeid: ext::NodeIdType::DEFAULT,
                                    body: DeclareBody::UndeclareToken(UndeclareToken {
                                        id: self
                                            .face_hat(&face)
                                            .next_id
                                            .fetch_add(1, Ordering::SeqCst),
                                        ext_wire_expr: WireExprType {
                                            wire_expr: Resource::get_best_key(&res, "", face.id),
                                        },
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

    fn propagate_forget_simple_token_to_peers(
        &self,
        tables: &mut TablesData,
        res: &Arc<Resource>,
        send_declare: &mut SendDeclare,
    ) {
        if self.res_hat(res).router_tokens.len() == 1
            && self.res_hat(res).router_tokens.contains(&tables.zid)
        {
            for mut face in self.faces(tables).values().cloned().collect::<Vec<_>>() {
                if face.whatami == WhatAmI::Peer
                    && self.face_hat(&face).local_tokens.contains_key(res)
                    && !res.face_ctxs.values().any(|ctx| {
                        face.zid != ctx.face.zid && ctx.token && ctx.face.whatami == WhatAmI::Client
                    })
                {
                    if let Some(id) = self.face_hat_mut(&mut face).local_tokens.remove(res) {
                        send_declare(
                            &face.primitives,
                            RoutingContext::with_expr(
                                Declare {
                                    interest_id: None,
                                    ext_qos: ext::QoSType::DECLARE,
                                    ext_tstamp: None,
                                    ext_nodeid: ext::NodeIdType::DEFAULT,
                                    body: DeclareBody::UndeclareToken(UndeclareToken {
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

    fn unregister_router_token(
        &mut self,
        tables: &mut TablesData,
        face: Option<&Arc<FaceState>>,
        res: &mut Arc<Resource>,
        router: &ZenohIdProto,
        send_declare: &mut SendDeclare,
    ) {
        self.res_hat_mut(res)
            .router_tokens
            .retain(|token| token != router);

        if self.res_hat(res).router_tokens.is_empty() {
            self.router_tokens.retain(|token| !Arc::ptr_eq(token, res));

            self.propagate_forget_simple_token(tables, res, face, send_declare);
        }

        self.propagate_forget_simple_token_to_peers(tables, res, send_declare);
    }

    fn undeclare_router_token(
        &mut self,
        tables: &mut TablesData,
        face: Option<&Arc<FaceState>>,
        res: &mut Arc<Resource>,
        router: &ZenohIdProto,
        send_declare: &mut SendDeclare,
        profile: InterestProfile,
    ) {
        if self.res_hat(res).router_tokens.contains(router) {
            self.unregister_router_token(tables, face, res, router, send_declare);

            if profile.is_push()
                || self.router_remote_interests.values().any(|interest| {
                    interest.options.tokens()
                        && interest
                            .res
                            .as_ref()
                            .map(|r| r.matches(&res))
                            .unwrap_or(true)
                })
                || router != &tables.zid
            {
                self.propagate_forget_sourced_token(tables, res, face, router);
            }
        }
    }

    fn forget_router_token(
        &mut self,
        tables: &mut TablesData,
        face: &mut Arc<FaceState>,
        res: &mut Arc<Resource>,
        router: &ZenohIdProto,
        send_declare: &mut SendDeclare,
        profile: InterestProfile,
    ) {
        self.undeclare_router_token(tables, Some(face), res, router, send_declare, profile);
    }

    pub(super) fn undeclare_simple_token(
        &mut self,
        ctx: BaseContext,
        res: &mut Arc<Resource>,
        profile: InterestProfile,
    ) {
        if !self
            .face_hat_mut(ctx.src_face)
            .remote_tokens
            .values()
            .any(|s| *s == *res)
        {
            if let Some(ctx) = get_mut_unchecked(res).face_ctxs.get_mut(&ctx.src_face.id) {
                get_mut_unchecked(ctx).token = false;
            }

            let mut simple_tokens = self.simple_tokens(res);
            let router_tokens = self.remote_router_tokens(ctx.tables, res);
            if simple_tokens.is_empty() {
                self.undeclare_router_token(
                    ctx.tables,
                    Some(ctx.src_face),
                    res,
                    &ctx.tables.zid.clone(),
                    ctx.send_declare,
                    profile,
                );
            } else {
                self.propagate_forget_simple_token_to_peers(ctx.tables, res, ctx.send_declare);
            }

            if simple_tokens.len() == 1 && !router_tokens {
                let face = &mut simple_tokens[0];
                if face.whatami != WhatAmI::Client {
                    if let Some(id) = self.face_hat_mut(face).local_tokens.remove(res) {
                        (ctx.send_declare)(
                            &face.primitives,
                            RoutingContext::with_expr(
                                Declare {
                                    interest_id: None,
                                    ext_qos: ext::QoSType::DECLARE,
                                    ext_tstamp: None,
                                    ext_nodeid: ext::NodeIdType::DEFAULT,
                                    body: DeclareBody::UndeclareToken(UndeclareToken {
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
                        .local_tokens
                        .keys()
                        .cloned()
                        .collect::<Vec<Arc<Resource>>>()
                    {
                        if !res.context().matches.iter().any(|m| {
                            m.upgrade().is_some_and(|m| {
                                m.ctx.is_some()
                                    && (self.remote_simple_tokens(ctx.tables, &m, face)
                                        || self.remote_router_tokens(ctx.tables, &m))
                            })
                        }) {
                            if let Some(id) = self.face_hat_mut(face).local_tokens.remove(&res) {
                                (ctx.send_declare)(
                                    &face.primitives,
                                    RoutingContext::with_expr(
                                        Declare {
                                            interest_id: None,
                                            ext_qos: ext::QoSType::DECLARE,
                                            ext_tstamp: None,
                                            ext_nodeid: ext::NodeIdType::DEFAULT,
                                            body: DeclareBody::UndeclareToken(UndeclareToken {
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
        }
    }

    fn forget_simple_token(
        &mut self,
        ctx: BaseContext,
        id: TokenId,
        profile: InterestProfile,
    ) -> Option<Arc<Resource>> {
        if let Some(mut res) = self.face_hat_mut(ctx.src_face).remote_tokens.remove(&id) {
            self.undeclare_simple_token(ctx, &mut res, profile);
            Some(res)
        } else {
            None
        }
    }

    pub(super) fn token_remove_node(
        &mut self,
        tables: &mut TablesData,
        node: &ZenohIdProto,
        send_declare: &mut SendDeclare,
    ) {
        for mut res in self
            .router_tokens
            .iter()
            .filter(|res| self.res_hat(res).router_tokens.contains(node))
            .cloned()
            .collect::<Vec<Arc<Resource>>>()
        {
            self.unregister_router_token(tables, None, &mut res, node, send_declare);
            Resource::clean(&mut res)
        }
    }

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
                                    None,
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[inline]
    fn make_token_id(
        &self,
        res: &Arc<Resource>,
        face: &mut Arc<FaceState>,
        mode: InterestMode,
    ) -> u32 {
        if mode.future() {
            if let Some(id) = self.face_hat(face).local_tokens.get(res) {
                *id
            } else {
                let id = self.face_hat(face).next_id.fetch_add(1, Ordering::SeqCst);
                self.face_hat_mut(face).local_tokens.insert(res.clone(), id);
                id
            }
        } else {
            0
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn declare_token_interest(
        &self,
        tables: &mut TablesData,
        face: &mut Arc<FaceState>,
        id: InterestId,
        res: Option<&mut Arc<Resource>>,
        mode: InterestMode,
        aggregate: bool,
        send_declare: &mut SendDeclare,
    ) {
        self.declare_token_interest_with_source(
            tables,
            face,
            id,
            res,
            mode,
            aggregate,
            DEFAULT_NODE_ID,
            send_declare,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn declare_token_interest_with_source(
        &self,
        tables: &mut TablesData,
        face: &mut Arc<FaceState>,
        id: InterestId,
        res: Option<&mut Arc<Resource>>,
        mode: InterestMode,
        aggregate: bool,
        source: NodeId,
        send_declare: &mut SendDeclare,
    ) {
        if mode.current()
            && (face.whatami == WhatAmI::Client
                || face.whatami == WhatAmI::Peer
                || !face.bound.is_north())
        {
            let interest_id = Some(id);
            if let Some(res) = res.as_ref() {
                if aggregate {
                    if self.router_tokens.iter().any(|token| {
                        token.ctx.is_some()
                            && token.matches(res)
                            && (self.remote_simple_tokens(tables, token, face)
                                || self.remote_router_tokens(tables, token))
                    }) {
                        let id = self.make_token_id(res, face, mode);
                        let wire_expr =
                            Resource::decl_key(res, face, self.push_declaration_profile(face));
                        send_declare(
                            &face.primitives,
                            RoutingContext::with_expr(
                                Declare {
                                    interest_id,
                                    ext_qos: ext::QoSType::DECLARE,
                                    ext_tstamp: None,
                                    ext_nodeid: interest::ext::NodeIdType { node_id: source },
                                    body: DeclareBody::DeclareToken(DeclareToken { id, wire_expr }),
                                },
                                res.expr().to_string(),
                            ),
                        );
                    }
                } else {
                    for token in &self.router_tokens {
                        if token.ctx.is_some()
                            && token.matches(res)
                            && (self
                                .res_hat(token)
                                .router_tokens
                                .iter()
                                .any(|r| *r != tables.zid)
                                || token.face_ctxs.values().any(|ctx| {
                                    ctx.face.id != face.id
                                        && ctx.token
                                        && (ctx.face.whatami == WhatAmI::Client
                                            || face.whatami == WhatAmI::Client
                                            || !face.bound.is_north())
                                }))
                        {
                            let id = self.make_token_id(token, face, mode);
                            let wire_expr = Resource::decl_key(
                                token,
                                face,
                                self.push_declaration_profile(face),
                            );
                            send_declare(
                                &face.primitives,
                                RoutingContext::with_expr(
                                    Declare {
                                        interest_id,
                                        ext_qos: ext::QoSType::DECLARE,
                                        ext_tstamp: None,
                                        ext_nodeid: interest::ext::NodeIdType { node_id: source },
                                        body: DeclareBody::DeclareToken(DeclareToken {
                                            id,
                                            wire_expr,
                                        }),
                                    },
                                    token.expr().to_string(),
                                ),
                            );
                        }
                    }
                }
            } else {
                for token in &self.router_tokens {
                    if token.ctx.is_some()
                        && (self
                            .res_hat(token)
                            .router_tokens
                            .iter()
                            .any(|r| *r != tables.zid)
                            || token.face_ctxs.values().any(|ctx| {
                                ctx.token
                                    && (ctx.face.whatami != WhatAmI::Peer
                                        || face.whatami != WhatAmI::Peer
                                        || !face.bound.is_north())
                            }))
                    {
                        let id = self.make_token_id(token, face, mode);
                        let wire_expr =
                            Resource::decl_key(token, face, self.push_declaration_profile(face));
                        send_declare(
                            &face.primitives,
                            RoutingContext::with_expr(
                                Declare {
                                    interest_id,
                                    ext_qos: ext::QoSType::DECLARE,
                                    ext_tstamp: None,
                                    ext_nodeid: interest::ext::NodeIdType { node_id: source },
                                    body: DeclareBody::DeclareToken(DeclareToken { id, wire_expr }),
                                },
                                token.expr().to_string(),
                            ),
                        );
                    }
                }
            }
        }
    }
}

impl HatTokenTrait for Hat {
    #[tracing::instrument(level = "trace", skip_all, fields(wai = %self.whatami().short(), bnd = %self.bound, profile = %profile))]
    fn declare_token(
        &mut self,
        ctx: BaseContext,
        id: TokenId,
        res: &mut Arc<Resource>,
        node_id: NodeId,
        interest_id: Option<InterestId>,
        profile: InterestProfile,
    ) {
        let router = if self.owns_router(ctx.src_face) {
            let Some(router) = self.get_router(ctx.src_face, node_id) else {
                tracing::error!(%node_id, "Token from unknown router");
                return;
            };

            router
        } else {
            ctx.tables.zid
        };

        match ctx.src_face.whatami {
            WhatAmI::Router => {
                self.declare_router_token(
                    ctx.tables,
                    ctx.src_face,
                    res,
                    router,
                    interest_id,
                    ctx.send_declare,
                    profile,
                );
            }
            WhatAmI::Peer | WhatAmI::Client => {
                // TODO(regions2): clients and peers of this
                // router are handled as if they were bound to future broker/peer-to-peer south hats resp.
                self.declare_simple_token(
                    ctx.tables,
                    ctx.src_face,
                    id,
                    res,
                    interest_id,
                    ctx.send_declare,
                    profile,
                );
            }
        }
    }

    #[tracing::instrument(level = "trace", skip_all, fields(wai = %self.whatami().short(), bnd = %self.bound, profile = %profile))]
    fn undeclare_token(
        &mut self,
        ctx: BaseContext,
        id: TokenId,
        res: Option<Arc<Resource>>,
        node_id: NodeId,
        profile: InterestProfile,
    ) -> Option<Arc<Resource>> {
        let router = if self.owns_router(ctx.src_face) {
            let Some(router) = self.get_router(ctx.src_face, node_id) else {
                tracing::error!(%node_id, "Token from unknown router");
                return None;
            };

            router
        } else {
            ctx.tables.zid
        };

        let mut res = res?;

        match ctx.src_face.whatami {
            WhatAmI::Router => {
                self.forget_router_token(
                    ctx.tables,
                    ctx.src_face,
                    &mut res,
                    &router,
                    ctx.send_declare,
                    profile,
                );
                Some(res)
            }
            WhatAmI::Peer | WhatAmI::Client => self.forget_simple_token(ctx, id, profile),
        }
    }

    #[tracing::instrument(level = "trace", skip_all, fields(wai = %self.whatami().short(), bnd = %self.bound, interest_id))]
    fn declare_current_token(
        &mut self,
        ctx: BaseContext,
        id: TokenId,
        res: &mut Arc<Resource>,
        node_id: NodeId,
        interest_id: InterestId,
        mut downstream_hats: BoundMap<&mut dyn HatTrait>,
    ) {
        debug_assert!(self.bound.is_north());

        if let Some(interest) = self
            .gateway_pending_current_interests
            .get(&interest_id)
            .map(|p| &p.interest)
        {
            let hat = &mut downstream_hats[interest.src_face.bound];
            hat.propagate_current_token(ctx, res, interest);
        } else {
            tracing::error!(
                id = interest_id,
                keyexpr = res.expr(),
                src = %ctx.src_face,
                "Received current token with unknown interest id"
            );
        }
    }

    #[tracing::instrument(level = "trace", skip_all, fields(wai = %self.whatami().short(), bnd = %self.bound, interest_id = interest.src_interest_id))]
    fn propagate_current_token(
        &mut self,
        ctx: BaseContext,
        res: &mut Arc<Resource>,
        interest: &CurrentInterest,
    ) {
        debug_assert!(!self.bound.is_north());

        if interest.mode == InterestMode::CurrentFuture {
            unimplemented!()
        }

        // REVIEW(regions): router entities are broadcast
        self.propagate_sourced_token(
            ctx.tables,
            res,
            None,
            &ctx.tables.zid,
            Some(interest.src_interest_id),
        );
    }
}
