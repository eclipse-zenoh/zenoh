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
use super::{face_hat, face_hat_mut, get_routes_entries};
use super::{HatCode, HatFace};
use crate::net::routing::dispatcher::face::FaceState;
use crate::net::routing::dispatcher::resource::{NodeId, Resource, SessionContext};
use crate::net::routing::dispatcher::tables::Tables;
use crate::net::routing::dispatcher::tables::{QueryTargetQabl, QueryTargetQablSet, RoutingExpr};
use crate::net::routing::hat::{HatQueriesTrait, Sources};
use crate::net::routing::router::RoutesIndexes;
use crate::net::routing::{RoutingContext, PREFIX_LIVELINESS};
use ordered_float::OrderedFloat;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use zenoh_buffers::ZBuf;
use zenoh_protocol::core::key_expr::include::{Includer, DEFAULT_INCLUDER};
use zenoh_protocol::core::key_expr::OwnedKeyExpr;
use zenoh_protocol::{
    core::{WhatAmI, WireExpr},
    network::declare::{
        common::ext::WireExprType, ext, queryable::ext::QueryableInfo, Declare, DeclareBody,
        DeclareQueryable, UndeclareQueryable,
    },
};
use zenoh_sync::get_mut_unchecked;

#[cfg(feature = "complete_n")]
#[inline]
fn merge_qabl_infos(mut this: QueryableInfo, info: &QueryableInfo) -> QueryableInfo {
    this.complete += info.complete;
    this.distance = std::cmp::min(this.distance, info.distance);
    this
}

#[cfg(not(feature = "complete_n"))]
#[inline]
fn merge_qabl_infos(mut this: QueryableInfo, info: &QueryableInfo) -> QueryableInfo {
    this.complete = u8::from(this.complete != 0 || info.complete != 0);
    this.distance = std::cmp::min(this.distance, info.distance);
    this
}

fn local_qabl_info(_tables: &Tables, res: &Arc<Resource>, face: &Arc<FaceState>) -> QueryableInfo {
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
        .unwrap_or(QueryableInfo {
            complete: 0,
            distance: 0,
        })
}

fn propagate_simple_queryable(
    tables: &mut Tables,
    res: &Arc<Resource>,
    src_face: Option<&mut Arc<FaceState>>,
) {
    let faces = tables.faces.values().cloned();
    for mut dst_face in faces {
        let info = local_qabl_info(tables, res, &dst_face);
        let current_info = face_hat!(dst_face).local_qabls.get(res);
        if (src_face.is_none() || src_face.as_ref().unwrap().id != dst_face.id)
            && (current_info.is_none() || *current_info.unwrap() != info)
            && (src_face.is_none()
                || src_face.as_ref().unwrap().whatami == WhatAmI::Client
                || dst_face.whatami == WhatAmI::Client)
        {
            face_hat_mut!(&mut dst_face)
                .local_qabls
                .insert(res.clone(), info);
            let key_expr = Resource::decl_key(res, &mut dst_face);
            dst_face.primitives.send_declare(RoutingContext::with_expr(
                Declare {
                    ext_qos: ext::QoSType::declare_default(),
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::default(),
                    body: DeclareBody::DeclareQueryable(DeclareQueryable {
                        id: 0, // @TODO use proper QueryableId (#703)
                        wire_expr: key_expr,
                        ext_info: info,
                    }),
                },
                res.expr(),
            ));
        }
    }
}

fn register_client_queryable(
    _tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    qabl_info: &QueryableInfo,
) {
    // Register queryable
    {
        let res = get_mut_unchecked(res);
        tracing::debug!("Register queryable {} (face: {})", res.expr(), face,);
        get_mut_unchecked(res.session_ctxs.entry(face.id).or_insert_with(|| {
            Arc::new(SessionContext {
                face: face.clone(),
                local_expr_id: None,
                remote_expr_id: None,
                subs: None,
                qabl: None,
                last_values: HashMap::new(),
                in_interceptor_cache: None,
                e_interceptor_cache: None,
            })
        }))
        .qabl = Some(*qabl_info);
    }
    face_hat_mut!(face).remote_qabls.insert(res.clone());
}

fn declare_client_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    qabl_info: &QueryableInfo,
) {
    register_client_queryable(tables, face, res, qabl_info);
    propagate_simple_queryable(tables, res, Some(face));
}

#[inline]
fn client_qabls(res: &Arc<Resource>) -> Vec<Arc<FaceState>> {
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

fn propagate_forget_simple_queryable(tables: &mut Tables, res: &mut Arc<Resource>) {
    for face in tables.faces.values_mut() {
        if face_hat!(face).local_qabls.contains_key(res) {
            let wire_expr = Resource::get_best_key(res, "", face.id);
            face.primitives.send_declare(RoutingContext::with_expr(
                Declare {
                    ext_qos: ext::QoSType::declare_default(),
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::default(),
                    body: DeclareBody::UndeclareQueryable(UndeclareQueryable {
                        id: 0, // @TODO use proper QueryableId (#703)
                        ext_wire_expr: WireExprType { wire_expr },
                    }),
                },
                res.expr(),
            ));

            face_hat_mut!(face).local_qabls.remove(res);
        }
    }
}

pub(super) fn undeclare_client_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
) {
    tracing::debug!("Unregister client queryable {} for {}", res.expr(), face);
    if let Some(ctx) = get_mut_unchecked(res).session_ctxs.get_mut(&face.id) {
        get_mut_unchecked(ctx).qabl = None;
        if ctx.qabl.is_none() {
            face_hat_mut!(face).remote_qabls.remove(res);
        }
    }

    let mut client_qabls = client_qabls(res);
    if client_qabls.is_empty() {
        propagate_forget_simple_queryable(tables, res);
    } else {
        propagate_simple_queryable(tables, res, None);
    }
    if client_qabls.len() == 1 {
        let face = &mut client_qabls[0];
        if face_hat!(face).local_qabls.contains_key(res) {
            let wire_expr = Resource::get_best_key(res, "", face.id);
            face.primitives.send_declare(RoutingContext::with_expr(
                Declare {
                    ext_qos: ext::QoSType::declare_default(),
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::default(),
                    body: DeclareBody::UndeclareQueryable(UndeclareQueryable {
                        id: 0, // @TODO use proper QueryableId (#703)
                        ext_wire_expr: WireExprType { wire_expr },
                    }),
                },
                res.expr(),
            ));

            face_hat_mut!(face).local_qabls.remove(res);
        }
    }
}

fn forget_client_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
) {
    undeclare_client_queryable(tables, face, res);
}

pub(super) fn queries_new_face(tables: &mut Tables, _face: &mut Arc<FaceState>) {
    for face in tables
        .faces
        .values()
        .cloned()
        .collect::<Vec<Arc<FaceState>>>()
    {
        for qabl in face_hat!(face).remote_qabls.iter() {
            propagate_simple_queryable(tables, qabl, Some(&mut face.clone()));
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
        face: &mut Arc<FaceState>,
        res: &mut Arc<Resource>,
        qabl_info: &QueryableInfo,
        _node_id: NodeId,
    ) {
        declare_client_queryable(tables, face, res, qabl_info);
    }

    fn undeclare_queryable(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        res: &mut Arc<Resource>,
        _node_id: NodeId,
    ) {
        forget_client_queryable(tables, face, res);
    }

    fn get_queryables(&self, tables: &Tables) -> Vec<(Arc<Resource>, Sources)> {
        let mut qabls = HashMap::new();
        for src_face in tables.faces.values() {
            for qabl in &face_hat!(src_face).remote_qabls {
                let srcs = qabls.entry(qabl.clone()).or_insert_with(Sources::empty);
                match src_face.whatami {
                    WhatAmI::Router => srcs.routers.push(src_face.zid),
                    WhatAmI::Peer => srcs.peers.push(src_face.zid),
                    WhatAmI::Client => srcs.clients.push(src_face.zid),
                }
            }
        }
        Vec::from_iter(qabls)
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
                        complete: if complete {
                            qabl_info.complete as u64
                        } else {
                            0
                        },
                        distance: 0.5,
                    });
                }
            }
        }
        route.sort_by_key(|qabl| OrderedFloat(qabl.distance));
        Arc::new(route)
    }

    #[inline]
    fn compute_local_replies(
        &self,
        tables: &Tables,
        prefix: &Arc<Resource>,
        suffix: &str,
        face: &Arc<FaceState>,
    ) -> Vec<(WireExpr<'static>, ZBuf)> {
        let mut result = vec![];
        // Only the first routing point in the query route
        // should return the liveliness tokens
        if face.whatami == WhatAmI::Client {
            let key_expr = prefix.expr() + suffix;
            let key_expr = match OwnedKeyExpr::try_from(key_expr) {
                Ok(ke) => ke,
                Err(e) => {
                    tracing::warn!("Invalid KE reached the system: {}", e);
                    return result;
                }
            };
            if key_expr.starts_with(PREFIX_LIVELINESS) {
                let res = Resource::get_resource(prefix, suffix);
                let matches = res
                    .as_ref()
                    .and_then(|res| res.context.as_ref())
                    .map(|ctx| Cow::from(&ctx.matches))
                    .unwrap_or_else(|| Cow::from(Resource::get_matches(tables, &key_expr)));
                for mres in matches.iter() {
                    let mres = mres.upgrade().unwrap();
                    if mres.session_ctxs.values().any(|ctx| ctx.subs.is_some()) {
                        result.push((Resource::get_best_key(&mres, "", face.id), ZBuf::default()));
                    }
                }
            }
        }
        result
    }

    fn get_query_routes_entries(&self, _tables: &Tables) -> RoutesIndexes {
        get_routes_entries()
    }
}
