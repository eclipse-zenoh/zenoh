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
    network::declare::{
        common::ext::WireExprType, ext, queryable::ext::QueryableInfoType, Declare, DeclareBody,
        DeclareQueryable, QueryableId, UndeclareQueryable,
    },
};
use zenoh_sync::get_mut_unchecked;

use super::{face_hat, face_hat_mut, HatCode, HatFace};
use crate::{
    key_expr::KeyExpr,
    net::routing::{
        dispatcher::{
            face::{Face, FaceState},
            resource::{NodeId, Resource, SessionContext},
            tables::{QueryTargetQabl, QueryTargetQablSet, RoutingExpr, Tables},
        },
        hat::{HatQueriesTrait, SendDeclare, Sources},
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
            if ctx.face.state.id != face.id {
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

fn propagate_simple_queryable(
    tables: &mut Tables,
    res: &Arc<Resource>,
    src_face: Option<&Face>,
    send_declare: &mut SendDeclare,
) {
    let faces = tables.faces.values().cloned();
    for mut dst_face in faces {
        let info = local_qabl_info(tables, res, &dst_face.state);
        let current = face_hat!(dst_face.state).local_qabls.get(res);
        if src_face
            .as_ref()
            .map(|src_face| dst_face.state.id != src_face.state.id)
            .unwrap_or(true)
            && (current.is_none() || current.unwrap().1 != info)
            && src_face
                .as_ref()
                .map(|src_face| {
                    src_face.state.whatami == WhatAmI::Client
                        || dst_face.state.whatami == WhatAmI::Client
                })
                .unwrap_or(true)
        {
            let id = current.map(|c| c.0).unwrap_or(
                face_hat!(dst_face.state)
                    .next_id
                    .fetch_add(1, Ordering::SeqCst),
            );
            face_hat_mut!(&mut dst_face.state)
                .local_qabls
                .insert(res.clone(), (id, info));
            let key_expr = Resource::decl_key(res, &dst_face, true);
            send_declare(
                &dst_face.state,
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
                Some(res.clone()),
            );
        }
    }
}

fn register_simple_queryable(
    _tables: &mut Tables,
    face: &Face,
    id: QueryableId,
    res: &mut Arc<Resource>,
    qabl_info: &QueryableInfoType,
) {
    // Register queryable
    {
        let res = get_mut_unchecked(res);
        get_mut_unchecked(
            res.session_ctxs
                .entry(face.state.id)
                .or_insert_with(|| Arc::new(SessionContext::new(face.clone()))),
        )
        .qabl = Some(*qabl_info);
    }
    face_hat_mut!(&mut face.state.clone())
        .remote_qabls
        .insert(id, res.clone());
}

fn declare_simple_queryable(
    tables: &mut Tables,
    face: &Face,
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
                Some(ctx.face.state.clone())
            } else {
                None
            }
        })
        .collect()
}

fn propagate_forget_simple_queryable(
    tables: &mut Tables,
    res: &mut Arc<Resource>,
    send_declare: &mut SendDeclare,
) {
    for face in tables.faces.values_mut() {
        if let Some((id, _)) = face_hat_mut!(&mut face.state).local_qabls.remove(res) {
            send_declare(
                &face.state,
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
                Some(res.clone()),
            );
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
            let face = &mut simple_qabls[0];
            if let Some((id, _)) = face_hat_mut!(face).local_qabls.remove(res) {
                send_declare(
                    face,
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
                    Some(res.clone()),
                );
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
    _face: &mut Arc<FaceState>,
    send_declare: &mut SendDeclare,
) {
    for face in tables.faces.values().cloned().collect::<Vec<Face>>() {
        for qabl in face_hat!(face.state).remote_qabls.values() {
            propagate_simple_queryable(tables, qabl, Some(&face), send_declare);
        }
    }
}

lazy_static::lazy_static! {
    static ref EMPTY_ROUTE: Arc<QueryTargetQablSet> = Arc::new(Vec::new());
}

impl HatQueriesTrait for HatCode {
    fn declare_queryable(
        &self,
        tables: &mut Tables,
        face: &Face,
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
            for qabl in face_hat!(src_face.state).remote_qabls.values() {
                // Insert the key in the list of known queryables
                let srcs = qabls.entry(qabl.clone()).or_insert_with(Sources::empty);
                // Append src_face as a queryable source in the proper list
                match src_face.state.whatami {
                    WhatAmI::Router => srcs.routers.push(src_face.state.zid),
                    WhatAmI::Peer => srcs.peers.push(src_face.state.zid),
                    WhatAmI::Client => srcs.clients.push(src_face.state.zid),
                }
            }
        }
        Vec::from_iter(qabls)
    }

    fn get_queriers(&self, tables: &Tables) -> Vec<(Arc<Resource>, Sources)> {
        let mut result = HashMap::new();
        for face in tables.faces.values() {
            for interest in face_hat!(face.state).remote_interests.values() {
                if interest.options.queryables() {
                    if let Some(res) = interest.res.as_ref() {
                        let sources = result.entry(res.clone()).or_insert_with(Sources::default);
                        match face.state.whatami {
                            WhatAmI::Router => sources.routers.push(face.state.zid),
                            WhatAmI::Peer => sources.peers.push(face.state.zid),
                            WhatAmI::Client => sources.clients.push(face.state.zid),
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
            for face in tables
                .faces
                .values()
                .filter(|f| f.state.whatami != WhatAmI::Client)
            {
                if !face.state.local_interests.values().any(|interest| {
                    interest.finalized
                        && interest.options.queryables()
                        && interest
                            .res
                            .as_ref()
                            .map(|res| KeyExpr::keyexpr_include(res.expr(), expr.full_expr()))
                            .unwrap_or(true)
                }) || face_hat!(face.state)
                    .remote_qabls
                    .values()
                    .any(|qbl| KeyExpr::keyexpr_intersect(qbl.expr(), expr.full_expr()))
                {
                    let key_expr = Resource::get_best_key(expr.prefix, expr.suffix, face.state.id);
                    route.push(QueryTargetQabl {
                        direction: (face.clone(), key_expr.to_owned(), NodeId::default()),
                        info: None,
                    });
                }
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
                let key_expr = Resource::get_best_key(expr.prefix, expr.suffix, *sid);
                if let Some(qabl_info) = context.qabl.as_ref() {
                    route.push(QueryTargetQabl {
                        direction: (context.face.clone(), key_expr.to_owned(), NodeId::default()),
                        info: Some(QueryableInfoType {
                            complete: complete && qabl_info.complete,
                            distance: 1,
                        }),
                    });
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
        for face in tables
            .faces
            .values()
            .filter(|f| f.state.whatami != WhatAmI::Client)
        {
            if face.state.local_interests.values().any(|interest| {
                interest.finalized
                    && interest.options.queryables()
                    && interest
                        .res
                        .as_ref()
                        .map(|res| KeyExpr::keyexpr_include(res.expr(), key_expr))
                        .unwrap_or(true)
            }) && face_hat!(face.state)
                .remote_qabls
                .values()
                .any(|qbl| match complete {
                    true => {
                        qbl.session_ctxs
                            .get(&face.state.id)
                            .and_then(|sc| sc.qabl)
                            .is_some_and(|q| q.complete)
                            && KeyExpr::keyexpr_include(qbl.expr(), key_expr)
                    }
                    false => KeyExpr::keyexpr_intersect(qbl.expr(), key_expr),
                })
            {
                matching_queryables.insert(face.state.id, face.state.clone());
            }
        }

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
                if context.face.state.whatami == WhatAmI::Client
                    && match complete {
                        true => context.qabl.is_some_and(|q| q.complete),
                        false => context.qabl.is_some(),
                    }
                {
                    matching_queryables
                        .entry(*sid)
                        .or_insert_with(|| context.face.state.clone());
                }
            }
        }
        matching_queryables
    }
}
