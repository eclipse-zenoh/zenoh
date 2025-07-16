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
        interest::{InterestId, InterestMode},
        Declare, DeclareBody, DeclareToken, UndeclareToken,
    },
};
use zenoh_sync::get_mut_unchecked;

use super::{face_hat, face_hat_mut, res_hat, res_hat_mut, Hat};
use crate::net::{
    protocol::network::Network,
    routing::{
        dispatcher::{face::FaceState, interests::RemoteInterest, tables::TablesData},
        hat::{CurrentFutureTrait, HatTokenTrait, SendDeclare},
        router::{NodeId, Resource, SessionContext},
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
    ) {
        for child in clildren {
            if net.graph.contains_node(*child) {
                match tables.get_face(&net.graph[*child].zid).cloned() {
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
                                    body: DeclareBody::DeclareToken(DeclareToken {
                                        id: 0, // Sourced tokens do not use ids
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
    fn propagate_simple_token_to(
        &self,
        tables: &mut TablesData,
        dst_face: &mut Arc<FaceState>,
        res: &Arc<Resource>,
        src_face: &mut Arc<FaceState>,
        send_declare: &mut SendDeclare,
    ) {
        if (src_face.id != dst_face.id || dst_face.zid == tables.zid)
            && !face_hat!(dst_face).local_tokens.contains_key(res)
            && dst_face.whatami != WhatAmI::Router
            && (src_face.whatami != WhatAmI::Peer || dst_face.whatami != WhatAmI::Peer)
        {
            let matching_interests = face_hat!(dst_face)
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
                if !face_hat!(dst_face).local_tokens.contains_key(res) {
                    let id = face_hat!(dst_face).next_id.fetch_add(1, Ordering::SeqCst);
                    face_hat_mut!(dst_face).local_tokens.insert(res.clone(), id);
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
        for mut dst_face in tables
            .faces
            .values()
            .cloned()
            .collect::<Vec<Arc<FaceState>>>()
        {
            self.propagate_simple_token_to(tables, &mut dst_face, res, src_face, send_declare);
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

    fn register_router_token(
        &mut self,
        tables: &mut TablesData,
        face: &mut Arc<FaceState>,
        res: &mut Arc<Resource>,
        router: ZenohIdProto,
        send_declare: &mut SendDeclare,
    ) {
        if !res_hat!(res).router_tokens.contains(&router) {
            // Register router liveliness
            {
                res_hat_mut!(res).router_tokens.insert(router);
                self.router_tokens.insert(res.clone());
            }

            // Propagate liveliness to routers
            self.propagate_sourced_token(tables, res, Some(face), &router);
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
        send_declare: &mut SendDeclare,
    ) {
        self.register_router_token(tables, face, res, router, send_declare);
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
            match res.session_ctxs.get_mut(&face.id) {
                Some(ctx) => {
                    if !ctx.token {
                        get_mut_unchecked(ctx).token = true;
                    }
                }
                None => {
                    let ctx = res
                        .session_ctxs
                        .entry(face.id)
                        .or_insert_with(|| Arc::new(SessionContext::new(face.clone())));
                    get_mut_unchecked(ctx).token = true;
                }
            }
        }
        face_hat_mut!(face).remote_tokens.insert(id, res.clone());
    }

    fn declare_simple_token(
        &mut self,
        tables: &mut TablesData,
        face: &mut Arc<FaceState>,
        id: TokenId,
        res: &mut Arc<Resource>,
        send_declare: &mut SendDeclare,
    ) {
        self.register_simple_token(tables, face, id, res);
        let zid = tables.zid;
        self.register_router_token(tables, face, res, zid, send_declare);
    }

    #[inline]
    fn remote_router_tokens(&self, tables: &TablesData, res: &Arc<Resource>) -> bool {
        res.context.is_some()
            && res_hat!(res)
                .router_tokens
                .iter()
                .any(|peer| peer != &tables.zid)
    }

    #[inline]
    fn simple_tokens(&self, res: &Arc<Resource>) -> Vec<Arc<FaceState>> {
        res.session_ctxs
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
        res.session_ctxs
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
                match tables.get_face(&net.graph[*child].zid).cloned() {
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
        for mut face in tables.faces.values().cloned() {
            if let Some(id) = face_hat_mut!(&mut face).local_tokens.remove(res) {
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
            }) && face_hat!(face)
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
                                id: face_hat!(face).next_id.fetch_add(1, Ordering::SeqCst),
                                ext_wire_expr: WireExprType {
                                    wire_expr: Resource::get_best_key(res, "", face.id),
                                },
                            }),
                        },
                        res.expr().to_string(),
                    ),
                );
            }
            for res in face_hat!(&mut face)
                .local_tokens
                .keys()
                .cloned()
                .collect::<Vec<Arc<Resource>>>()
            {
                if !res.context().matches.iter().any(|m| {
                    m.upgrade().is_some_and(|m| {
                        m.context.is_some()
                            && (self.remote_simple_tokens(tables, &m, &face)
                                || self.remote_router_tokens(tables, &m))
                    })
                }) {
                    if let Some(id) = face_hat_mut!(&mut face).local_tokens.remove(&res) {
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
                    } else if face_hat!(face)
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
                                        id: face_hat!(face).next_id.fetch_add(1, Ordering::SeqCst),
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
        if res_hat!(res).router_tokens.len() == 1
            && res_hat!(res).router_tokens.contains(&tables.zid)
        {
            for mut face in tables
                .faces
                .values()
                .cloned()
                .collect::<Vec<Arc<FaceState>>>()
            {
                if face.whatami == WhatAmI::Peer
                    && face_hat!(face).local_tokens.contains_key(res)
                    && !res.session_ctxs.values().any(|s| {
                        face.zid != s.face.zid && s.token && s.face.whatami == WhatAmI::Client
                    })
                {
                    if let Some(id) = face_hat_mut!(&mut face).local_tokens.remove(res) {
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
        res_hat_mut!(res)
            .router_tokens
            .retain(|token| token != router);

        if res_hat!(res).router_tokens.is_empty() {
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
    ) {
        if res_hat!(res).router_tokens.contains(router) {
            self.unregister_router_token(tables, face, res, router, send_declare);
            self.propagate_forget_sourced_token(tables, res, face, router);
        }
    }

    fn forget_router_token(
        &mut self,
        tables: &mut TablesData,
        face: &mut Arc<FaceState>,
        res: &mut Arc<Resource>,
        router: &ZenohIdProto,
        send_declare: &mut SendDeclare,
    ) {
        self.undeclare_router_token(tables, Some(face), res, router, send_declare);
    }

    pub(super) fn undeclare_simple_token(
        &mut self,
        tables: &mut TablesData,
        face: &mut Arc<FaceState>,
        res: &mut Arc<Resource>,
        send_declare: &mut SendDeclare,
    ) {
        if !face_hat_mut!(face)
            .remote_tokens
            .values()
            .any(|s| *s == *res)
        {
            if let Some(ctx) = get_mut_unchecked(res).session_ctxs.get_mut(&face.id) {
                get_mut_unchecked(ctx).token = false;
            }

            let mut simple_tokens = self.simple_tokens(res);
            let router_tokens = self.remote_router_tokens(tables, res);
            if simple_tokens.is_empty() {
                self.undeclare_router_token(
                    tables,
                    Some(face),
                    res,
                    &tables.zid.clone(),
                    send_declare,
                );
            } else {
                self.propagate_forget_simple_token_to_peers(tables, res, send_declare);
            }

            if simple_tokens.len() == 1 && !router_tokens {
                let mut face = &mut simple_tokens[0];
                if face.whatami != WhatAmI::Client {
                    if let Some(id) = face_hat_mut!(face).local_tokens.remove(res) {
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
                    for res in face_hat!(face)
                        .local_tokens
                        .keys()
                        .cloned()
                        .collect::<Vec<Arc<Resource>>>()
                    {
                        if !res.context().matches.iter().any(|m| {
                            m.upgrade().is_some_and(|m| {
                                m.context.is_some()
                                    && (self.remote_simple_tokens(tables, &m, face)
                                        || self.remote_router_tokens(tables, &m))
                            })
                        }) {
                            if let Some(id) = face_hat_mut!(&mut face).local_tokens.remove(&res) {
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
        }
    }

    fn forget_simple_token(
        &mut self,
        tables: &mut TablesData,
        face: &mut Arc<FaceState>,
        id: TokenId,
        send_declare: &mut SendDeclare,
    ) -> Option<Arc<Resource>> {
        if let Some(mut res) = face_hat_mut!(face).remote_tokens.remove(&id) {
            self.undeclare_simple_token(tables, face, &mut res, send_declare);
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
            .filter(|res| res_hat!(res).router_tokens.contains(node))
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
                        let tokens = &res_hat!(res).router_tokens;
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

    #[inline]
    fn make_token_id(
        &self,
        res: &Arc<Resource>,
        face: &mut Arc<FaceState>,
        mode: InterestMode,
    ) -> u32 {
        if mode.future() {
            if let Some(id) = face_hat!(face).local_tokens.get(res) {
                *id
            } else {
                let id = face_hat!(face).next_id.fetch_add(1, Ordering::SeqCst);
                face_hat_mut!(face).local_tokens.insert(res.clone(), id);
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
        if mode.current() && (face.whatami == WhatAmI::Client || face.whatami == WhatAmI::Peer) {
            let interest_id = Some(id);
            if let Some(res) = res.as_ref() {
                if aggregate {
                    if self.router_tokens.iter().any(|token| {
                        token.context.is_some()
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
                                    ext_nodeid: ext::NodeIdType::DEFAULT,
                                    body: DeclareBody::DeclareToken(DeclareToken { id, wire_expr }),
                                },
                                res.expr().to_string(),
                            ),
                        );
                    }
                } else {
                    for token in &self.router_tokens {
                        if token.context.is_some()
                            && token.matches(res)
                            && (res_hat!(token)
                                .router_tokens
                                .iter()
                                .any(|r| *r != tables.zid)
                                || token.session_ctxs.values().any(|s| {
                                    s.face.id != face.id
                                        && s.token
                                        && (s.face.whatami == WhatAmI::Client
                                            || face.whatami == WhatAmI::Client)
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
                                        ext_nodeid: ext::NodeIdType::DEFAULT,
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
                    if token.context.is_some()
                        && (res_hat!(token)
                            .router_tokens
                            .iter()
                            .any(|r| *r != tables.zid)
                            || token.session_ctxs.values().any(|s| {
                                s.token
                                    && (s.face.whatami != WhatAmI::Peer
                                        || face.whatami != WhatAmI::Peer)
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
                                    ext_nodeid: ext::NodeIdType::DEFAULT,
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
    fn declare_token(
        &mut self,
        tables: &mut TablesData,
        face: &mut Arc<FaceState>,
        id: TokenId,
        res: &mut Arc<Resource>,
        node_id: NodeId,
        _interest_id: Option<InterestId>,
        send_declare: &mut SendDeclare,
    ) {
        match face.whatami {
            WhatAmI::Router => {
                if let Some(router) = self.get_router(face, node_id) {
                    self.declare_router_token(tables, face, res, router, send_declare)
                }
            }
            _ => self.declare_simple_token(tables, face, id, res, send_declare),
        }
    }

    fn undeclare_token(
        &mut self,
        tables: &mut TablesData,
        face: &mut Arc<FaceState>,
        id: TokenId,
        res: Option<Arc<Resource>>,
        node_id: NodeId,
        send_declare: &mut SendDeclare,
    ) -> Option<Arc<Resource>> {
        match face.whatami {
            WhatAmI::Router => {
                if let Some(mut res) = res {
                    if let Some(router) = self.get_router(face, node_id) {
                        self.forget_router_token(tables, face, &mut res, &router, send_declare);
                        Some(res)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => self.forget_simple_token(tables, face, id, send_declare),
        }
    }
}
