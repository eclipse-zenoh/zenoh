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
use super::face::FaceState;
use super::resource::{QueryRoute, QueryRoutes, QueryTargetQablSet, Resource};
use super::tables::NodeId;
use super::tables::{RoutingExpr, Tables, TablesLock};
use crate::net::routing::hat::HatTrait;
use crate::net::routing::RoutingContext;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Weak};
use zenoh_config::WhatAmI;
use zenoh_protocol::network::declare::InterestId;
use zenoh_protocol::{
    core::{key_expr::keyexpr, Encoding, WireExpr},
    network::{
        declare::{ext, queryable::ext::QueryableInfoType, QueryableId},
        request::{ext::TargetType, Request, RequestId},
        response::{self, ext::ResponderIdType, Response, ResponseFinal},
    },
    zenoh::{query::Consolidation, reply::ReplyBody, Put, Reply, RequestBody, ResponseBody},
};
use zenoh_sync::get_mut_unchecked;
use zenoh_util::Timed;

#[allow(clippy::too_many_arguments)] // TODO refactor
pub(crate) fn declare_qabl_interest(
    hat_code: &(dyn HatTrait + Send + Sync),
    tables: &TablesLock,
    face: &mut Arc<FaceState>,
    id: InterestId,
    expr: Option<&WireExpr>,
    future: bool,
    aggregate: bool,
) {
    if let Some(expr) = expr {
        let rtables = zread!(tables.tables);
        match rtables
            .get_mapping(face, &expr.scope, expr.mapping)
            .cloned()
        {
            Some(mut prefix) => {
                log::debug!(
                    "{} Declare qabl interest {} ({}{})",
                    face,
                    id,
                    prefix.expr(),
                    expr.suffix
                );
                let res = Resource::get_resource(&prefix, &expr.suffix);
                let (mut res, mut wtables) = if res
                    .as_ref()
                    .map(|r| r.context.is_some())
                    .unwrap_or(false)
                {
                    drop(rtables);
                    let wtables = zwrite!(tables.tables);
                    (res.unwrap(), wtables)
                } else {
                    let mut fullexpr = prefix.expr();
                    fullexpr.push_str(expr.suffix.as_ref());
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

                hat_code.declare_qabl_interest(
                    &mut wtables,
                    face,
                    id,
                    Some(&mut res),
                    future,
                    aggregate,
                );
            }
            None => log::error!(
                "{} Declare qabl interest {} for unknown scope {}!",
                face,
                id,
                expr.scope
            ),
        }
    } else {
        let mut wtables = zwrite!(tables.tables);
        hat_code.declare_qabl_interest(&mut wtables, face, id, None, future, aggregate);
    }
}

pub(crate) fn undeclare_qabl_interest(
    hat_code: &(dyn HatTrait + Send + Sync),
    tables: &TablesLock,
    face: &mut Arc<FaceState>,
    id: InterestId,
) {
    log::debug!("{} Undeclare qabl interest {}", face, id,);
    let mut wtables = zwrite!(tables.tables);
    hat_code.undeclare_qabl_interest(&mut wtables, face, id);
}

pub(crate) struct Query {
    src_face: Arc<FaceState>,
    src_qid: RequestId,
}

pub(crate) fn declare_queryable(
    hat_code: &(dyn HatTrait + Send + Sync),
    tables: &TablesLock,
    face: &mut Arc<FaceState>,
    id: QueryableId,
    expr: &WireExpr,
    qabl_info: &QueryableInfoType,
    node_id: NodeId,
) {
    let rtables = zread!(tables.tables);
    match rtables
        .get_mapping(face, &expr.scope, expr.mapping)
        .cloned()
    {
        Some(mut prefix) => {
            log::debug!(
                "{} Declare queryable {} ({}{})",
                face,
                id,
                prefix.expr(),
                expr.suffix
            );
            let res = Resource::get_resource(&prefix, &expr.suffix);
            let (mut res, mut wtables) =
                if res.as_ref().map(|r| r.context.is_some()).unwrap_or(false) {
                    drop(rtables);
                    let wtables = zwrite!(tables.tables);
                    (res.unwrap(), wtables)
                } else {
                    let mut fullexpr = prefix.expr();
                    fullexpr.push_str(expr.suffix.as_ref());
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

            hat_code.declare_queryable(&mut wtables, face, id, &mut res, qabl_info, node_id);

            disable_matches_query_routes(&mut wtables, &mut res);
            drop(wtables);

            let rtables = zread!(tables.tables);
            let matches_query_routes = compute_matches_query_routes(&rtables, &res);
            drop(rtables);

            let wtables = zwrite!(tables.tables);
            for (mut res, query_routes) in matches_query_routes {
                get_mut_unchecked(&mut res)
                    .context_mut()
                    .update_query_routes(query_routes);
            }
            drop(wtables);
        }
        None => log::error!(
            "{} Declare queryable {} for unknown scope {}!",
            face,
            id,
            expr.scope
        ),
    }
}

pub(crate) fn undeclare_queryable(
    hat_code: &(dyn HatTrait + Send + Sync),
    tables: &TablesLock,
    face: &mut Arc<FaceState>,
    id: QueryableId,
    expr: &WireExpr,
    node_id: NodeId,
) {
    let res = if expr.is_empty() {
        None
    } else {
        let rtables = zread!(tables.tables);
        match rtables.get_mapping(face, &expr.scope, expr.mapping) {
            Some(prefix) => match Resource::get_resource(prefix, expr.suffix.as_ref()) {
                Some(res) => Some(res),
                None => {
                    log::error!(
                        "{} Undeclare unknown queryable {}{}!",
                        face,
                        prefix.expr(),
                        expr.suffix
                    );
                    return;
                }
            },
            None => {
                log::error!(
                    "{} Undeclare queryable with unknown scope {}",
                    face,
                    expr.scope
                );
                return;
            }
        }
    };
    let mut wtables = zwrite!(tables.tables);
    if let Some(mut res) = hat_code.undeclare_queryable(&mut wtables, face, id, res, node_id) {
        log::debug!("{} Undeclare queryable {} ({})", face, id, res.expr());
        disable_matches_query_routes(&mut wtables, &mut res);
        drop(wtables);

        let rtables = zread!(tables.tables);
        let matches_query_routes = compute_matches_query_routes(&rtables, &res);
        drop(rtables);

        let wtables = zwrite!(tables.tables);
        for (mut res, query_routes) in matches_query_routes {
            get_mut_unchecked(&mut res)
                .context_mut()
                .update_query_routes(query_routes);
        }
        Resource::clean(&mut res);
        drop(wtables);
    } else {
        log::error!("{} Undeclare unknown queryable {}", face, id);
    }
}

fn compute_query_routes_(tables: &Tables, routes: &mut QueryRoutes, expr: &mut RoutingExpr) {
    let indexes = tables.hat_code.get_query_routes_entries(tables);

    let max_idx = indexes.routers.iter().max().unwrap();
    routes.routers.resize_with((*max_idx as usize) + 1, || {
        Arc::new(QueryTargetQablSet::new())
    });

    for idx in indexes.routers {
        routes.routers[idx as usize] =
            tables
                .hat_code
                .compute_query_route(tables, expr, idx, WhatAmI::Router);
    }

    let max_idx = indexes.peers.iter().max().unwrap();
    routes.peers.resize_with((*max_idx as usize) + 1, || {
        Arc::new(QueryTargetQablSet::new())
    });

    for idx in indexes.peers {
        routes.peers[idx as usize] =
            tables
                .hat_code
                .compute_query_route(tables, expr, idx, WhatAmI::Peer);
    }

    let max_idx = indexes.clients.iter().max().unwrap();
    routes.clients.resize_with((*max_idx as usize) + 1, || {
        Arc::new(QueryTargetQablSet::new())
    });

    for idx in indexes.clients {
        routes.clients[idx as usize] =
            tables
                .hat_code
                .compute_query_route(tables, expr, idx, WhatAmI::Client);
    }
}

pub(crate) fn compute_query_routes(tables: &Tables, res: &Arc<Resource>) -> QueryRoutes {
    let mut routes = QueryRoutes::default();
    compute_query_routes_(tables, &mut routes, &mut RoutingExpr::new(res, ""));
    routes
}

pub(crate) fn update_query_routes(tables: &Tables, res: &Arc<Resource>) {
    if res.context.is_some() {
        let mut res_mut = res.clone();
        let res_mut = get_mut_unchecked(&mut res_mut);
        compute_query_routes_(
            tables,
            &mut res_mut.context_mut().query_routes,
            &mut RoutingExpr::new(res, ""),
        );
    }
}

pub(crate) fn update_query_routes_from(tables: &mut Tables, res: &mut Arc<Resource>) {
    update_query_routes(tables, res);
    let res = get_mut_unchecked(res);
    for child in res.childs.values_mut() {
        update_query_routes_from(tables, child);
    }
}

pub(crate) fn compute_matches_query_routes(
    tables: &Tables,
    res: &Arc<Resource>,
) -> Vec<(Arc<Resource>, QueryRoutes)> {
    let mut routes = vec![];
    if res.context.is_some() {
        routes.push((res.clone(), compute_query_routes(tables, res)));
        for match_ in &res.context().matches {
            let match_ = match_.upgrade().unwrap();
            if !Arc::ptr_eq(&match_, res) {
                let match_routes = compute_query_routes(tables, &match_);
                routes.push((match_, match_routes));
            }
        }
    }
    routes
}

pub(crate) fn update_matches_query_routes(tables: &Tables, res: &Arc<Resource>) {
    if res.context.is_some() {
        update_query_routes(tables, res);
        for match_ in &res.context().matches {
            let match_ = match_.upgrade().unwrap();
            if !Arc::ptr_eq(&match_, res) {
                update_query_routes(tables, &match_);
            }
        }
    }
}

#[inline]
fn insert_pending_query(outface: &mut Arc<FaceState>, query: Arc<Query>) -> RequestId {
    let outface_mut = get_mut_unchecked(outface);
    outface_mut.next_qid += 1;
    let qid = outface_mut.next_qid;
    outface_mut.pending_queries.insert(qid, query);
    qid
}

#[inline]
fn compute_final_route(
    tables: &Tables,
    qabls: &Arc<QueryTargetQablSet>,
    src_face: &Arc<FaceState>,
    expr: &mut RoutingExpr,
    target: &TargetType,
    query: Arc<Query>,
) -> QueryRoute {
    match target {
        TargetType::All => {
            let mut route = HashMap::new();
            for qabl in qabls.iter() {
                if tables
                    .hat_code
                    .egress_filter(tables, src_face, &qabl.direction.0, expr)
                {
                    route.entry(qabl.direction.0.id).or_insert_with(|| {
                        let mut direction = qabl.direction.clone();
                        let qid = insert_pending_query(&mut direction.0, query.clone());
                        (direction, qid)
                    });
                }
            }
            route
        }
        TargetType::AllComplete => {
            let mut route = HashMap::new();
            for qabl in qabls.iter() {
                if qabl.complete > 0
                    && tables
                        .hat_code
                        .egress_filter(tables, src_face, &qabl.direction.0, expr)
                {
                    route.entry(qabl.direction.0.id).or_insert_with(|| {
                        let mut direction = qabl.direction.clone();
                        let qid = insert_pending_query(&mut direction.0, query.clone());
                        (direction, qid)
                    });
                }
            }
            route
        }
        TargetType::BestMatching => {
            if let Some(qabl) = qabls
                .iter()
                .find(|qabl| qabl.direction.0.id != src_face.id && qabl.complete > 0)
            {
                let mut route = HashMap::new();

                let mut direction = qabl.direction.clone();
                let qid = insert_pending_query(&mut direction.0, query);
                route.insert(direction.0.id, (direction, qid));

                route
            } else {
                compute_final_route(tables, qabls, src_face, expr, &TargetType::All, query)
            }
        }
    }
}

#[derive(Clone)]
struct QueryCleanup {
    tables: Arc<TablesLock>,
    face: Weak<FaceState>,
    qid: RequestId,
}

#[async_trait]
impl Timed for QueryCleanup {
    async fn run(&mut self) {
        if let Some(mut face) = self.face.upgrade() {
            let tables_lock = zwrite!(self.tables.tables);
            if let Some(query) = get_mut_unchecked(&mut face)
                .pending_queries
                .remove(&self.qid)
            {
                drop(tables_lock);
                log::warn!(
                    "Didn't receive final reply {}:{} from {}: Timeout!",
                    query.src_face,
                    self.qid,
                    face
                );
                finalize_pending_query(query);
            }
        }
    }
}

pub(crate) fn disable_matches_query_routes(_tables: &mut Tables, res: &mut Arc<Resource>) {
    if res.context.is_some() {
        get_mut_unchecked(res).context_mut().disable_query_routes();
        for match_ in &res.context().matches {
            let mut match_ = match_.upgrade().unwrap();
            if !Arc::ptr_eq(&match_, res) {
                get_mut_unchecked(&mut match_)
                    .context_mut()
                    .disable_query_routes();
            }
        }
    }
}

#[inline]
fn get_query_route(
    tables: &Tables,
    face: &FaceState,
    res: &Option<Arc<Resource>>,
    expr: &mut RoutingExpr,
    routing_context: NodeId,
) -> Arc<QueryTargetQablSet> {
    let local_context = tables
        .hat_code
        .map_routing_context(tables, face, routing_context);
    res.as_ref()
        .and_then(|res| res.query_route(face.whatami, local_context))
        .unwrap_or_else(|| {
            tables
                .hat_code
                .compute_query_route(tables, expr, local_context, face.whatami)
        })
}

#[cfg(feature = "stats")]
macro_rules! inc_req_stats {
    (
        $face:expr,
        $txrx:ident,
        $space:ident,
        $body:expr
    ) => {
        paste::paste! {
            if let Some(stats) = $face.stats.as_ref() {
                use zenoh_buffers::buffer::Buffer;
                match &$body {
                    RequestBody::Query(q) => {
                        stats.[<$txrx _z_query_msgs>].[<inc_ $space>](1);
                        stats.[<$txrx _z_query_pl_bytes>].[<inc_ $space>](
                            q.ext_body.as_ref().map(|b| b.payload.len()).unwrap_or(0),
                        );
                    }
                }
            }
        }
    };
}

#[cfg(feature = "stats")]
macro_rules! inc_res_stats {
    (
        $face:expr,
        $txrx:ident,
        $space:ident,
        $body:expr
    ) => {
        paste::paste! {
            if let Some(stats) = $face.stats.as_ref() {
                use zenoh_buffers::buffer::Buffer;
                match &$body {
                    ResponseBody::Reply(r) => {
                        stats.[<$txrx _z_reply_msgs>].[<inc_ $space>](1);
                        let mut n = 0;
                        match &r.payload {
                            ReplyBody::Put(p) => {
                                if let Some(a) = p.ext_attachment.as_ref() {
                                   n += a.buffer.len();
                                }
                                n += p.payload.len();
                            }
                            ReplyBody::Del(d) => {
                                if let Some(a) = d.ext_attachment.as_ref() {
                                   n += a.buffer.len();
                                }
                            }
                        }
                        stats.[<$txrx _z_reply_pl_bytes>].[<inc_ $space>](n);
                    }
                    ResponseBody::Err(e) => {
                        stats.[<$txrx _z_reply_msgs>].[<inc_ $space>](1);
                        stats.[<$txrx _z_reply_pl_bytes>].[<inc_ $space>](
                            e.payload.len()
                        );
                    }
                }
            }
        }
    };
}

pub fn route_query(
    tables_ref: &Arc<TablesLock>,
    face: &Arc<FaceState>,
    expr: &WireExpr,
    qid: RequestId,
    target: TargetType,
    body: RequestBody,
    routing_context: NodeId,
) {
    let rtables = zread!(tables_ref.tables);
    match rtables.get_mapping(face, &expr.scope, expr.mapping) {
        Some(prefix) => {
            log::debug!(
                "Route query {}:{} for res {}{}",
                face,
                qid,
                prefix.expr(),
                expr.suffix.as_ref(),
            );
            let prefix = prefix.clone();
            let mut expr = RoutingExpr::new(&prefix, expr.suffix.as_ref());

            #[cfg(feature = "stats")]
            let admin = expr.full_expr().starts_with("@/");
            #[cfg(feature = "stats")]
            if !admin {
                inc_req_stats!(face, rx, user, body)
            } else {
                inc_req_stats!(face, rx, admin, body)
            }

            if rtables.hat_code.ingress_filter(&rtables, face, &mut expr) {
                let res = Resource::get_resource(&prefix, expr.suffix);

                let route = get_query_route(&rtables, face, &res, &mut expr, routing_context);

                let query = Arc::new(Query {
                    src_face: face.clone(),
                    src_qid: qid,
                });

                let queries_lock = zwrite!(tables_ref.queries_lock);
                let route = compute_final_route(&rtables, &route, face, &mut expr, &target, query);
                let local_replies =
                    rtables
                        .hat_code
                        .compute_local_replies(&rtables, &prefix, expr.suffix, face);
                let zid = rtables.zid;

                let timeout = rtables.queries_default_timeout;

                drop(queries_lock);
                drop(rtables);

                for (wexpr, payload) in local_replies {
                    let payload = ResponseBody::Reply(Reply {
                        consolidation: Consolidation::DEFAULT, // @TODO: handle Del case
                        ext_unknown: vec![],                   // @TODO: handle unknown extensions
                        payload: ReplyBody::Put(Put {
                            // @TODO: handle Del case
                            timestamp: None,             // @TODO: handle timestamp
                            encoding: Encoding::empty(), // @TODO: handle encoding
                            ext_sinfo: None,             // @TODO: handle source info
                            ext_attachment: None,        // @TODO: expose it in the API
                            #[cfg(feature = "shared-memory")]
                            ext_shm: None,
                            ext_unknown: vec![], // @TODO: handle unknown extensions
                            payload,
                        }),
                    });
                    #[cfg(feature = "stats")]
                    if !admin {
                        inc_res_stats!(face, tx, user, payload)
                    } else {
                        inc_res_stats!(face, tx, admin, payload)
                    }

                    face.primitives
                        .clone()
                        .send_response(RoutingContext::with_expr(
                            Response {
                                rid: qid,
                                wire_expr: wexpr,
                                payload,
                                ext_qos: response::ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_respid: Some(response::ext::ResponderIdType {
                                    zid,
                                    eid: 0, // 0 is reserved for routing core
                                }),
                            },
                            expr.full_expr().to_string(),
                        ));
                }

                if route.is_empty() {
                    log::debug!(
                        "Send final reply {}:{} (no matching queryables or not master)",
                        face,
                        qid
                    );
                    face.primitives
                        .clone()
                        .send_response_final(RoutingContext::with_expr(
                            ResponseFinal {
                                rid: qid,
                                ext_qos: response::ext::QoSType::RESPONSE_FINAL,
                                ext_tstamp: None,
                            },
                            expr.full_expr().to_string(),
                        ));
                } else {
                    for ((outface, key_expr, context), qid) in route.values() {
                        let mut cleanup = QueryCleanup {
                            tables: tables_ref.clone(),
                            face: Arc::downgrade(outface),
                            qid: *qid,
                        };
                        zenoh_runtime::ZRuntime::Net.spawn(async move {
                            tokio::time::sleep(timeout).await;
                            cleanup.run().await
                        });
                        #[cfg(feature = "stats")]
                        if !admin {
                            inc_req_stats!(outface, tx, user, body)
                        } else {
                            inc_req_stats!(outface, tx, admin, body)
                        }

                        log::trace!("Propagate query {}:{} to {}", face, qid, outface);
                        outface.primitives.send_request(RoutingContext::with_expr(
                            Request {
                                id: *qid,
                                wire_expr: key_expr.into(),
                                ext_qos: ext::QoSType::REQUEST,
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType { node_id: *context },
                                ext_target: target,
                                ext_budget: None,
                                ext_timeout: None,
                                payload: body.clone(),
                            },
                            expr.full_expr().to_string(),
                        ));
                    }
                }
            } else {
                log::debug!("Send final reply {}:{} (not master)", face, qid);
                drop(rtables);
                face.primitives
                    .clone()
                    .send_response_final(RoutingContext::with_expr(
                        ResponseFinal {
                            rid: qid,
                            ext_qos: response::ext::QoSType::RESPONSE_FINAL,
                            ext_tstamp: None,
                        },
                        expr.full_expr().to_string(),
                    ));
            }
        }
        None => {
            log::error!(
                "{} Route query with unknown scope {}! Send final reply.",
                face,
                expr.scope,
            );
            drop(rtables);
            face.primitives
                .clone()
                .send_response_final(RoutingContext::with_expr(
                    ResponseFinal {
                        rid: qid,
                        ext_qos: response::ext::QoSType::RESPONSE_FINAL,
                        ext_tstamp: None,
                    },
                    "".to_string(),
                ));
        }
    }
}

pub(crate) fn route_send_response(
    tables_ref: &Arc<TablesLock>,
    face: &mut Arc<FaceState>,
    qid: RequestId,
    ext_respid: Option<ResponderIdType>,
    key_expr: WireExpr,
    body: ResponseBody,
) {
    let queries_lock = zread!(tables_ref.queries_lock);
    #[cfg(feature = "stats")]
    let admin = key_expr.as_str().starts_with("@/");
    #[cfg(feature = "stats")]
    if !admin {
        inc_res_stats!(face, rx, user, body)
    } else {
        inc_res_stats!(face, rx, admin, body)
    }

    match face.pending_queries.get(&qid) {
        Some(query) => {
            drop(queries_lock);

            #[cfg(feature = "stats")]
            if !admin {
                inc_res_stats!(query.src_face, tx, user, body)
            } else {
                inc_res_stats!(query.src_face, tx, admin, body)
            }

            query
                .src_face
                .primitives
                .clone()
                .send_response(RoutingContext::with_expr(
                    Response {
                        rid: query.src_qid,
                        wire_expr: key_expr.to_owned(),
                        payload: body,
                        ext_qos: response::ext::QoSType::RESPONSE,
                        ext_tstamp: None,
                        ext_respid,
                    },
                    "".to_string(), // @TODO provide the proper key expression of the response for interceptors
                ));
        }
        None => log::warn!(
            "Route reply {}:{} from {}: Query nof found!",
            face,
            qid,
            face
        ),
    }
}

pub(crate) fn route_send_response_final(
    tables_ref: &Arc<TablesLock>,
    face: &mut Arc<FaceState>,
    qid: RequestId,
) {
    let queries_lock = zwrite!(tables_ref.queries_lock);
    match get_mut_unchecked(face).pending_queries.remove(&qid) {
        Some(query) => {
            drop(queries_lock);
            log::debug!(
                "Received final reply {}:{} from {}",
                query.src_face,
                qid,
                face
            );
            finalize_pending_query(query);
        }
        None => log::warn!(
            "Route final reply {}:{} from {}: Query nof found!",
            face,
            qid,
            face
        ),
    }
}

pub(crate) fn finalize_pending_queries(tables_ref: &TablesLock, face: &mut Arc<FaceState>) {
    let queries_lock = zwrite!(tables_ref.queries_lock);
    for (_, query) in get_mut_unchecked(face).pending_queries.drain() {
        finalize_pending_query(query);
    }
    drop(queries_lock);
}

pub(crate) fn finalize_pending_query(query: Arc<Query>) {
    if let Some(query) = Arc::into_inner(query) {
        log::debug!("Propagate final reply {}:{}", query.src_face, query.src_qid);
        query
            .src_face
            .primitives
            .clone()
            .send_response_final(RoutingContext::with_expr(
                ResponseFinal {
                    rid: query.src_qid,
                    ext_qos: response::ext::QoSType::RESPONSE_FINAL,
                    ext_tstamp: None,
                },
                "".to_string(),
            ));
    }
}
