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
    core::{
        key_expr::{
            include::{Includer, DEFAULT_INCLUDER},
            OwnedKeyExpr,
        },
        WhatAmI,
    },
    network::{
        declare::{
            common::ext::WireExprType, ext, queryable::ext::QueryableInfoType, Declare,
            DeclareBody, DeclareQueryable, QueryableId, UndeclareQueryable,
        },
        interest::{InterestId, InterestMode},
    },
};
use zenoh_sync::get_mut_unchecked;

use super::{face_hat, face_hat_mut, HatCode, HatFace};
use crate::{
    key_expr::KeyExpr,
    net::routing::{
        dispatcher::{
            face::FaceState,
            resource::{NodeId, Resource, SessionContext},
            tables::{QueryTargetQabl, QueryTargetQablSet, RoutingExpr, Tables},
        },
        hat::{
            p2p_peer::initial_interest, CurrentFutureTrait, HatQueriesTrait, SendDeclare, Sources,
        },
        RoutingContext,
    },
};

#[inline]
fn merge_qabl_infos(mut this: QueryableInfoType, info: &QueryableInfoType) -> QueryableInfoType {
    this.complete = this.complete || info.complete;
    this.distance = std::cmp::min(this.distance, info.distance);
    this
}

fn local_qabl_info(
    _tables: &Tables,
    res: &Arc<Resource>,
    face: &Arc<FaceState>,
) -> QueryableInfoType {
    res.session_ctxs
        .values()
        .fold(None, |accu, ctx| {
            if ctx.face.id != face.id {
                if let Some(info) = ctx.qabl.as_ref() {
                    Some(match accu {
                        Some(accu) => merge_qabl_infos(accu, info),
                        None => *info,
                    })
                } else {
                    accu
                }
            } else {
                accu
            }
        })
        .unwrap_or(QueryableInfoType::DEFAULT)
}

#[inline]
fn propagate_simple_queryable_to(
    tables: &mut Tables,
    dst_face: &mut Arc<FaceState>,
    res: &Arc<Resource>,
    src_face: &Option<&mut Arc<FaceState>>,
    send_declare: &mut SendDeclare,
) {
    let info = local_qabl_info(tables, res, dst_face);
    let current = face_hat!(dst_face).local_qabls.get(res);
    if src_face
        .as_ref()
        .map(|src_face| dst_face.id != src_face.id)
        .unwrap_or(true)
        && (current.is_none() || current.unwrap().1 != info)
        && (dst_face.whatami != WhatAmI::Client
            || face_hat!(dst_face)
                .remote_interests
                .values()
                .any(|i| i.options.queryables() && i.matches(res)))
        && src_face
            .as_ref()
            .map(|src_face| {
                src_face.whatami == WhatAmI::Client || dst_face.whatami == WhatAmI::Client
            })
            .unwrap_or(true)
    {
        let id = current
            .map(|c| c.0)
            .unwrap_or(face_hat!(dst_face).next_id.fetch_add(1, Ordering::SeqCst));
        face_hat_mut!(dst_face)
            .local_qabls
            .insert(res.clone(), (id, info));
        let key_expr = Resource::decl_key(res, dst_face, super::push_declaration_profile(dst_face));
        send_declare(
            &dst_face.primitives,
            RoutingContext::with_expr(
                Declare {
                    interest_id: None,
                    ext_qos: ext::QoSType::DECLARE,
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::DEFAULT,
                    body: DeclareBody::DeclareQueryable(DeclareQueryable {
                        id,
                        wire_expr: key_expr,
                        ext_info: info,
                    }),
                },
                res.expr().to_string(),
            ),
        );
    }
}

fn propagate_simple_queryable(
    tables: &mut Tables,
    res: &Arc<Resource>,
    src_face: Option<&mut Arc<FaceState>>,
    send_declare: &mut SendDeclare,
) {
    let faces = tables
        .faces
        .values()
        .cloned()
        .collect::<Vec<Arc<FaceState>>>();
    for mut dst_face in faces {
        propagate_simple_queryable_to(tables, &mut dst_face, res, &src_face, send_declare);
    }
}

fn register_simple_queryable(
    _tables: &mut Tables,
    face: &mut Arc<FaceState>,
    id: QueryableId,
    res: &mut Arc<Resource>,
    qabl_info: &QueryableInfoType,
) {
    // Register queryable
    {
        let res = get_mut_unchecked(res);
        get_mut_unchecked(
            res.session_ctxs
                .entry(face.id)
                .or_insert_with(|| Arc::new(SessionContext::new(face.clone()))),
        )
        .qabl = Some(*qabl_info);
    }
    face_hat_mut!(face).remote_qabls.insert(id, res.clone());
}

fn declare_simple_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    id: QueryableId,
    res: &mut Arc<Resource>,
    qabl_info: &QueryableInfoType,
    send_declare: &mut SendDeclare,
) {
    register_simple_queryable(tables, face, id, res, qabl_info);
    propagate_simple_queryable(tables, res, Some(face), send_declare);
}

#[inline]
fn simple_qabls(res: &Arc<Resource>) -> Vec<Arc<FaceState>> {
    res.session_ctxs
        .values()
        .filter_map(|ctx| {
            if ctx.qabl.is_some() {
                Some(ctx.face.clone())
            } else {
                None
            }
        })
        .collect()
}

#[inline]
fn remote_simple_qabls(res: &Arc<Resource>, face: &Arc<FaceState>) -> bool {
    res.session_ctxs
        .values()
        .any(|ctx| ctx.face.id != face.id && ctx.qabl.is_some())
}

fn propagate_forget_simple_queryable(
    tables: &mut Tables,
    res: &mut Arc<Resource>,
    send_declare: &mut SendDeclare,
) {
    for face in tables.faces.values_mut() {
        if let Some((id, _)) = face_hat_mut!(face).local_qabls.remove(res) {
            send_declare(
                &face.primitives,
                RoutingContext::with_expr(
                    Declare {
                        interest_id: None,
                        ext_qos: ext::QoSType::DECLARE,
                        ext_tstamp: None,
                        ext_nodeid: ext::NodeIdType::DEFAULT,
                        body: DeclareBody::UndeclareQueryable(UndeclareQueryable {
                            id,
                            ext_wire_expr: WireExprType::null(),
                        }),
                    },
                    res.expr().to_string(),
                ),
            );
        }
        for res in face_hat!(face)
            .local_qabls
            .keys()
            .cloned()
            .collect::<Vec<Arc<Resource>>>()
        {
            if !res.context().matches.iter().any(|m| {
                m.upgrade()
                    .is_some_and(|m| m.context.is_some() && remote_simple_qabls(&m, face))
            }) {
                if let Some((id, _)) = face_hat_mut!(face).local_qabls.remove(&res) {
                    send_declare(
                        &face.primitives,
                        RoutingContext::with_expr(
                            Declare {
                                interest_id: None,
                                ext_qos: ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType::DEFAULT,
                                body: DeclareBody::UndeclareQueryable(UndeclareQueryable {
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

pub(super) fn undeclare_simple_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    send_declare: &mut SendDeclare,
) {
    if !face_hat_mut!(face)
        .remote_qabls
        .values()
        .any(|s| *s == *res)
    {
        if let Some(ctx) = get_mut_unchecked(res).session_ctxs.get_mut(&face.id) {
            get_mut_unchecked(ctx).qabl = None;
        }

        let mut simple_qabls = simple_qabls(res);
        if simple_qabls.is_empty() {
            propagate_forget_simple_queryable(tables, res, send_declare);
        } else {
            propagate_simple_queryable(tables, res, None, send_declare);
        }
        if simple_qabls.len() == 1 {
            let mut face = &mut simple_qabls[0];
            if let Some((id, _)) = face_hat_mut!(face).local_qabls.remove(res) {
                send_declare(
                    &face.primitives,
                    RoutingContext::with_expr(
                        Declare {
                            interest_id: None,
                            ext_qos: ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: ext::NodeIdType::DEFAULT,
                            body: DeclareBody::UndeclareQueryable(UndeclareQueryable {
                                id,
                                ext_wire_expr: WireExprType::null(),
                            }),
                        },
                        res.expr().to_string(),
                    ),
                );
            }
            for res in face_hat!(face)
                .local_qabls
                .keys()
                .cloned()
                .collect::<Vec<Arc<Resource>>>()
            {
                if !res.context().matches.iter().any(|m| {
                    m.upgrade()
                        .is_some_and(|m| m.context.is_some() && (remote_simple_qabls(&m, face)))
                }) {
                    if let Some((id, _)) = face_hat_mut!(&mut face).local_qabls.remove(&res) {
                        send_declare(
                            &face.primitives,
                            RoutingContext::with_expr(
                                Declare {
                                    interest_id: None,
                                    ext_qos: ext::QoSType::DECLARE,
                                    ext_tstamp: None,
                                    ext_nodeid: ext::NodeIdType::DEFAULT,
                                    body: DeclareBody::UndeclareQueryable(UndeclareQueryable {
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

fn forget_simple_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    id: QueryableId,
    send_declare: &mut SendDeclare,
) -> Option<Arc<Resource>> {
    if let Some(mut res) = face_hat_mut!(face).remote_qabls.remove(&id) {
        undeclare_simple_queryable(tables, face, &mut res, send_declare);
        Some(res)
    } else {
        None
    }
}

pub(super) fn queries_new_face(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    send_declare: &mut SendDeclare,
) {
    if face.whatami != WhatAmI::Client {
        for src_face in tables
            .faces
            .values()
            .cloned()
            .collect::<Vec<Arc<FaceState>>>()
        {
            for qabl in face_hat!(src_face).remote_qabls.values() {
                propagate_simple_queryable_to(
                    tables,
                    face,
                    qabl,
                    &Some(&mut src_face.clone()),
                    send_declare,
                );
            }
        }
    }
}

lazy_static::lazy_static! {
    static ref EMPTY_ROUTE: Arc<QueryTargetQablSet> = Arc::new(Vec::new());
}

#[inline]
fn make_qabl_id(
    res: &Arc<Resource>,
    face: &mut Arc<FaceState>,
    mode: InterestMode,
    info: QueryableInfoType,
) -> u32 {
    if mode.future() {
        if let Some((id, _)) = face_hat!(face).local_qabls.get(res) {
            *id
        } else {
            let id = face_hat!(face).next_id.fetch_add(1, Ordering::SeqCst);
            face_hat_mut!(face)
                .local_qabls
                .insert(res.clone(), (id, info));
            id
        }
    } else {
        0
    }
}

pub(super) fn declare_qabl_interest(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    id: InterestId,
    res: Option<&mut Arc<Resource>>,
    mode: InterestMode,
    aggregate: bool,
    send_declare: &mut SendDeclare,
) {
    if mode.current() && face.whatami == WhatAmI::Client {
        let interest_id = Some(id);
        if let Some(res) = res.as_ref() {
            if aggregate {
                if tables.faces.values().any(|src_face| {
                    src_face.id != face.id
                        && face_hat!(src_face)
                            .remote_qabls
                            .values()
                            .any(|qabl| qabl.context.is_some() && qabl.matches(res))
                }) {
                    let info = local_qabl_info(tables, res, face);
                    let id = make_qabl_id(res, face, mode, info);
                    let wire_expr =
                        Resource::decl_key(res, face, super::push_declaration_profile(face));
                    send_declare(
                        &face.primitives,
                        RoutingContext::with_expr(
                            Declare {
                                interest_id,
                                ext_qos: ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType::DEFAULT,
                                body: DeclareBody::DeclareQueryable(DeclareQueryable {
                                    id,
                                    wire_expr,
                                    ext_info: info,
                                }),
                            },
                            res.expr().to_string(),
                        ),
                    );
                }
            } else {
                for src_face in tables
                    .faces
                    .values()
                    .cloned()
                    .collect::<Vec<Arc<FaceState>>>()
                {
                    if src_face.id != face.id {
                        for qabl in face_hat!(src_face).remote_qabls.values() {
                            if qabl.context.is_some() && qabl.matches(res) {
                                let info = local_qabl_info(tables, qabl, face);
                                let id = make_qabl_id(qabl, face, mode, info);
                                let key_expr = Resource::decl_key(
                                    qabl,
                                    face,
                                    super::push_declaration_profile(face),
                                );
                                send_declare(
                                    &face.primitives,
                                    RoutingContext::with_expr(
                                        Declare {
                                            interest_id,
                                            ext_qos: ext::QoSType::DECLARE,
                                            ext_tstamp: None,
                                            ext_nodeid: ext::NodeIdType::DEFAULT,
                                            body: DeclareBody::DeclareQueryable(DeclareQueryable {
                                                id,
                                                wire_expr: key_expr,
                                                ext_info: info,
                                            }),
                                        },
                                        qabl.expr().to_string(),
                                    ),
                                );
                            }
                        }
                    }
                }
            }
        } else {
            for src_face in tables
                .faces
                .values()
                .cloned()
                .collect::<Vec<Arc<FaceState>>>()
            {
                if src_face.id != face.id {
                    for qabl in face_hat!(src_face).remote_qabls.values() {
                        if qabl.context.is_some() {
                            let info = local_qabl_info(tables, qabl, face);
                            let id = make_qabl_id(qabl, face, mode, info);
                            let key_expr = Resource::decl_key(
                                qabl,
                                face,
                                super::push_declaration_profile(face),
                            );
                            send_declare(
                                &face.primitives,
                                RoutingContext::with_expr(
                                    Declare {
                                        interest_id,
                                        ext_qos: ext::QoSType::DECLARE,
                                        ext_tstamp: None,
                                        ext_nodeid: ext::NodeIdType::DEFAULT,
                                        body: DeclareBody::DeclareQueryable(DeclareQueryable {
                                            id,
                                            wire_expr: key_expr,
                                            ext_info: info,
                                        }),
                                    },
                                    qabl.expr().to_string(),
                                ),
                            );
                        }
                    }
                }
            }
        }
    }
}

impl HatQueriesTrait for HatCode {
    fn declare_queryable(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: QueryableId,
        res: &mut Arc<Resource>,
        qabl_info: &QueryableInfoType,
        _node_id: NodeId,
        send_declare: &mut SendDeclare,
    ) {
        declare_simple_queryable(tables, face, id, res, qabl_info, send_declare);
    }

    fn undeclare_queryable(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: QueryableId,
        _res: Option<Arc<Resource>>,
        _node_id: NodeId,
        send_declare: &mut SendDeclare,
    ) -> Option<Arc<Resource>> {
        forget_simple_queryable(tables, face, id, send_declare)
    }

    fn get_queryables(&self, tables: &Tables) -> Vec<(Arc<Resource>, Sources)> {
        // Compute the list of known queryables (keys)
        let mut qabls = HashMap::new();
        for src_face in tables.faces.values() {
            for qabl in face_hat!(src_face).remote_qabls.values() {
                // Insert the key in the list of known queryables
                let srcs = qabls.entry(qabl.clone()).or_insert_with(Sources::empty);
                // Append src_face as a queryable source in the proper list
                match src_face.whatami {
                    WhatAmI::Router => srcs.routers.push(src_face.zid),
                    WhatAmI::Peer => srcs.peers.push(src_face.zid),
                    WhatAmI::Client => srcs.clients.push(src_face.zid),
                }
            }
        }
        Vec::from_iter(qabls)
    }

    fn get_queriers(&self, tables: &Tables) -> Vec<(Arc<Resource>, Sources)> {
        let mut result = HashMap::new();
        for face in tables.faces.values() {
            for interest in face_hat!(face).remote_interests.values() {
                if interest.options.queryables() {
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

    fn compute_query_route(
        &self,
        tables: &Tables,
        expr: &mut RoutingExpr,
        source: NodeId,
        source_type: WhatAmI,
    ) -> Arc<QueryTargetQablSet> {
        let mut route = QueryTargetQablSet::new();
        let key_expr = expr.full_expr();
        if key_expr.ends_with('/') {
            return EMPTY_ROUTE.clone();
        }
        tracing::trace!(
            "compute_query_route({}, {:?}, {:?})",
            key_expr,
            source,
            source_type
        );
        let key_expr = match OwnedKeyExpr::try_from(key_expr) {
            Ok(ke) => ke,
            Err(e) => {
                tracing::warn!("Invalid KE reached the system: {}", e);
                return EMPTY_ROUTE.clone();
            }
        };

        if source_type == WhatAmI::Client {
            // TODO: BestMatching: What if there is a local compete ?
            for face in tables
                .faces
                .values()
                .filter(|f| f.whatami == WhatAmI::Router)
            {
                if !face.local_interests.values().any(|interest| {
                    interest.finalized
                        && interest.options.queryables()
                        && interest
                            .res
                            .as_ref()
                            .map(|res| KeyExpr::keyexpr_include(res.expr(), expr.full_expr()))
                            .unwrap_or(true)
                }) || face_hat!(face)
                    .remote_qabls
                    .values()
                    .any(|sub| KeyExpr::keyexpr_intersect(sub.expr(), expr.full_expr()))
                {
                    let key_expr = Resource::get_best_key(expr.prefix, expr.suffix, face.id);
                    route.push(QueryTargetQabl {
                        direction: (face.clone(), key_expr.to_owned(), NodeId::default()),
                        info: None,
                    });
                }
            }

            for face in tables.faces.values().filter(|f| {
                f.whatami == WhatAmI::Peer
                    && !initial_interest(f).map(|i| i.finalized).unwrap_or(true)
            }) {
                let key_expr = Resource::get_best_key(expr.prefix, expr.suffix, face.id);
                route.push(QueryTargetQabl {
                    direction: (face.clone(), key_expr.to_owned(), NodeId::default()),
                    info: None,
                });
            }
        }

        let res = Resource::get_resource(expr.prefix, expr.suffix);
        let matches = res
            .as_ref()
            .and_then(|res| res.context.as_ref())
            .map(|ctx| Cow::from(&ctx.matches))
            .unwrap_or_else(|| Cow::from(Resource::get_matches(tables, &key_expr)));

        for mres in matches.iter() {
            let mres = mres.upgrade().unwrap();
            let complete = DEFAULT_INCLUDER.includes(mres.expr().as_bytes(), key_expr.as_bytes());
            for (sid, context) in &mres.session_ctxs {
                if source_type == WhatAmI::Client || context.face.whatami == WhatAmI::Client {
                    let key_expr = Resource::get_best_key(expr.prefix, expr.suffix, *sid);
                    if let Some(qabl_info) = context.qabl.as_ref() {
                        route.push(QueryTargetQabl {
                            direction: (
                                context.face.clone(),
                                key_expr.to_owned(),
                                NodeId::default(),
                            ),
                            info: Some(QueryableInfoType {
                                complete: complete && qabl_info.complete,
                                distance: 1,
                            }),
                        });
                    }
                }
            }
        }
        route.sort_by_key(|qabl| qabl.info.map_or(u16::MAX, |i| i.distance));
        Arc::new(route)
    }

    #[cfg(feature = "unstable")]
    fn get_matching_queryables(
        &self,
        tables: &Tables,
        key_expr: &KeyExpr<'_>,
        complete: bool,
    ) -> HashMap<usize, Arc<FaceState>> {
        let mut matching_queryables = HashMap::new();
        if key_expr.ends_with('/') {
            return matching_queryables;
        }
        tracing::trace!(
            "get_matching_queryables({}; complete: {})",
            key_expr,
            complete
        );
        let res = Resource::get_resource(&tables.root_res, key_expr);
        let matches = res
            .as_ref()
            .and_then(|res| res.context.as_ref())
            .map(|ctx| Cow::from(&ctx.matches))
            .unwrap_or_else(|| Cow::from(Resource::get_matches(tables, key_expr)));

        for mres in matches.iter() {
            let mres = mres.upgrade().unwrap();
            if complete && !KeyExpr::keyexpr_include(mres.expr(), key_expr) {
                continue;
            }
            for (sid, context) in &mres.session_ctxs {
                if match complete {
                    true => context.qabl.is_some_and(|q| q.complete),
                    false => context.qabl.is_some(),
                } {
                    matching_queryables
                        .entry(*sid)
                        .or_insert_with(|| context.face.clone());
                }
            }
        }
        matching_queryables
    }
}
