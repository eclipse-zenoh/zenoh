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
use super::{face_hat, face_hat_mut};
use super::{HatCode, HatFace};
use crate::net::routing::dispatcher::face::FaceState;
use crate::net::routing::dispatcher::queries::*;
use crate::net::routing::dispatcher::resource::{NodeId, Resource, SessionContext};
use crate::net::routing::dispatcher::tables::{
    QueryRoutes, QueryTargetQabl, QueryTargetQablSet, RoutingExpr,
};
use crate::net::routing::dispatcher::tables::{Tables, TablesLock};
use crate::net::routing::hat::HatQueriesTrait;
use crate::net::routing::{RoutingContext, PREFIX_LIVELINESS};
use ordered_float::OrderedFloat;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, RwLockReadGuard};
use zenoh_buffers::ZBuf;
use zenoh_protocol::core::key_expr::include::{Includer, DEFAULT_INCLUDER};
use zenoh_protocol::core::key_expr::OwnedKeyExpr;
use zenoh_protocol::{
    core::{key_expr::keyexpr, WhatAmI, WireExpr},
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
                        id: 0, // TODO
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
        log::debug!("Register queryable {} (face: {})", res.expr(), face,);
        get_mut_unchecked(res.session_ctxs.entry(face.id).or_insert_with(|| {
            Arc::new(SessionContext {
                face: face.clone(),
                local_expr_id: None,
                remote_expr_id: None,
                subs: None,
                qabl: None,
                last_values: HashMap::new(),
            })
        }))
        .qabl = Some(*qabl_info);
    }
    face_hat_mut!(face).remote_qabls.insert(res.clone());
}

fn declare_client_queryable(
    tables: &TablesLock,
    rtables: RwLockReadGuard<Tables>,
    face: &mut Arc<FaceState>,
    expr: &WireExpr,
    qabl_info: &QueryableInfo,
) {
    match rtables
        .get_mapping(face, &expr.scope, expr.mapping)
        .cloned()
    {
        Some(mut prefix) => {
            let res = Resource::get_resource(&prefix, &expr.suffix);
            let (mut res, mut wtables) =
                if res.as_ref().map(|r| r.context.is_some()).unwrap_or(false) {
                    drop(rtables);
                    let wtables = zwrite!(tables.tables);
                    (res.unwrap(), wtables)
                } else {
                    let mut fullexpr = prefix.expr();
                    fullexpr.push_str(expr.suffix.as_ref());
                    log::debug!("Register client queryable {}", fullexpr);
                    let mut matches = keyexpr::new(fullexpr.as_str())
                        .map(|ke| Resource::get_matches(&rtables, ke))
                        .unwrap_or_default();
                    drop(rtables);
                    let mut wtables = zwrite!(tables.tables);
                    let mut res =
                        Resource::make_resource(&mut wtables, &mut prefix, expr.suffix.as_ref());
                    matches.push(Arc::downgrade(&res));
                    Resource::match_resource(&wtables, &mut res, matches);
                    (res, wtables)
                };

            register_client_queryable(&mut wtables, face, &mut res, qabl_info);
            propagate_simple_queryable(&mut wtables, &res, Some(face));
            disable_matches_query_routes(&mut wtables, &mut res);
            drop(wtables);

            let rtables = zread!(tables.tables);
            let matches_query_routes = compute_matches_query_routes_(&rtables, &res);
            drop(rtables);

            let wtables = zwrite!(tables.tables);
            for (mut res, query_routes) in matches_query_routes {
                get_mut_unchecked(&mut res)
                    .context_mut()
                    .update_query_routes(query_routes);
            }
            drop(wtables);
        }
        None => log::error!("Declare queryable for unknown scope {}!", expr.scope),
    }
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
                        id: 0, // TODO
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
    log::debug!("Unregister client queryable {} for {}", res.expr(), face);
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
                        id: 0, // TODO
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
    tables: &TablesLock,
    rtables: RwLockReadGuard<Tables>,
    face: &mut Arc<FaceState>,
    expr: &WireExpr,
) {
    match rtables.get_mapping(face, &expr.scope, expr.mapping) {
        Some(prefix) => match Resource::get_resource(prefix, expr.suffix.as_ref()) {
            Some(mut res) => {
                drop(rtables);
                let mut wtables = zwrite!(tables.tables);
                undeclare_client_queryable(&mut wtables, face, &mut res);
                disable_matches_query_routes(&mut wtables, &mut res);
                drop(wtables);

                let rtables = zread!(tables.tables);
                let matches_query_routes = compute_matches_query_routes_(&rtables, &res);
                drop(rtables);

                let wtables = zwrite!(tables.tables);
                for (mut res, query_routes) in matches_query_routes {
                    get_mut_unchecked(&mut res)
                        .context_mut()
                        .update_query_routes(query_routes);
                }
                Resource::clean(&mut res);
                drop(wtables);
            }
            None => log::error!("Undeclare unknown queryable!"),
        },
        None => log::error!("Undeclare queryable with unknown scope!"),
    }
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
        tables: &TablesLock,
        face: &mut Arc<FaceState>,
        expr: &WireExpr,
        qabl_info: &QueryableInfo,
        _node_id: NodeId,
    ) {
        let rtables = zread!(tables.tables);
        declare_client_queryable(tables, rtables, face, expr, qabl_info);
    }

    fn forget_queryable(
        &self,
        tables: &TablesLock,
        face: &mut Arc<FaceState>,
        expr: &WireExpr,
        _node_id: NodeId,
    ) {
        let rtables = zread!(tables.tables);
        forget_client_queryable(tables, rtables, face, expr);
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
        log::trace!(
            "compute_query_route({}, {:?}, {:?})",
            key_expr,
            source,
            source_type
        );
        let key_expr = match OwnedKeyExpr::try_from(key_expr) {
            Ok(ke) => ke,
            Err(e) => {
                log::warn!("Invalid KE reached the system: {}", e);
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
                    log::warn!("Invalid KE reached the system: {}", e);
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

    fn compute_query_routes_(&self, tables: &Tables, res: &Arc<Resource>) -> QueryRoutes {
        let mut routes = QueryRoutes::default();
        let mut expr = RoutingExpr::new(res, "");

        let route = self.compute_query_route(tables, &mut expr, NodeId::default(), WhatAmI::Client);

        routes
            .routers
            .resize_with(1, || Arc::new(QueryTargetQablSet::new()));
        routes.routers[0] = route.clone();

        routes
            .peers
            .resize_with(1, || Arc::new(QueryTargetQablSet::new()));
        routes.peers[0] = route.clone();

        routes
            .clients
            .resize_with(1, || Arc::new(QueryTargetQablSet::new()));
        routes.clients[0] = route;

        routes
    }

    fn compute_query_routes(&self, tables: &mut Tables, res: &mut Arc<Resource>) {
        if res.context.is_some() {
            let mut res_mut = res.clone();
            let res_mut = get_mut_unchecked(&mut res_mut);
            let mut expr = RoutingExpr::new(res, "");

            let route =
                self.compute_query_route(tables, &mut expr, NodeId::default(), WhatAmI::Client);

            let routers_query_routes = &mut res_mut.context_mut().query_routes.routers;
            routers_query_routes.clear();
            routers_query_routes.resize_with(1, || Arc::new(QueryTargetQablSet::new()));
            routers_query_routes[0] = route.clone();

            let peers_query_routes = &mut res_mut.context_mut().query_routes.peers;
            peers_query_routes.clear();
            peers_query_routes.resize_with(1, || Arc::new(QueryTargetQablSet::new()));
            peers_query_routes[0] = route.clone();

            let clients_query_routes = &mut res_mut.context_mut().query_routes.clients;
            clients_query_routes.clear();
            clients_query_routes.resize_with(1, || Arc::new(QueryTargetQablSet::new()));
            clients_query_routes[0] = route;
        }
    }
}
