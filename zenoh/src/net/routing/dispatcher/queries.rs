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
    collections::HashMap,
    sync::{Arc, Weak},
    time::Duration,
};

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use zenoh_buffers::ZBuf;
#[cfg(feature = "stats")]
use zenoh_protocol::zenoh::reply::ReplyBody;
use zenoh_protocol::{
    core::{key_expr::keyexpr, Encoding, WireExpr},
    network::{
        declare::{ext, queryable::ext::QueryableInfoType, QueryableId},
        request::{
            ext::{BudgetType, QueryTarget, TimeoutType},
            Request, RequestId,
        },
        response::{self, ext::ResponderIdType, Response, ResponseFinal},
    },
    zenoh::{self, RequestBody, ResponseBody},
};
use zenoh_sync::get_mut_unchecked;
use zenoh_util::Timed;

use super::{
    face::FaceState,
    resource::{QueryRoute, QueryTargetQablSet, Resource},
    tables::{NodeId, RoutingExpr, Tables, TablesLock},
};
#[cfg(feature = "unstable")]
use crate::key_expr::KeyExpr;
use crate::net::routing::{
    hat::{HatTrait, SendDeclare},
    router::get_or_set_route,
};

pub(crate) struct Query {
    src_face: Arc<FaceState>,
    src_qid: RequestId,
}

#[zenoh_macros::unstable]
#[inline]
pub(crate) fn get_matching_queryables(
    tables: &Tables,
    key_expr: &KeyExpr<'_>,
    complete: bool,
) -> HashMap<usize, Arc<FaceState>> {
    tables
        .hat_code
        .get_matching_queryables(tables, key_expr, complete)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn declare_queryable(
    hat_code: &(dyn HatTrait + Send + Sync),
    tables: &TablesLock,
    face: &mut Arc<FaceState>,
    id: QueryableId,
    expr: &WireExpr,
    qabl_info: &QueryableInfoType,
    node_id: NodeId,
    send_declare: &mut SendDeclare,
) {
    let rtables = zread!(tables.tables);
    match rtables
        .get_mapping(face, &expr.scope, expr.mapping)
        .cloned()
    {
        Some(mut prefix) => {
            tracing::debug!(
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
                    let mut fullexpr = prefix.expr().to_string();
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

            hat_code.declare_queryable(
                &mut wtables,
                face,
                id,
                &mut res,
                qabl_info,
                node_id,
                send_declare,
            );

            disable_matches_query_routes(&mut wtables, &mut res);
            drop(wtables);
        }
        None => tracing::error!(
            "{} Declare queryable {} for unknown scope {}",
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
    send_declare: &mut SendDeclare,
) {
    let res = if expr.is_empty() {
        None
    } else {
        let rtables = zread!(tables.tables);
        match rtables.get_mapping(face, &expr.scope, expr.mapping) {
            Some(prefix) => match Resource::get_resource(prefix, expr.suffix.as_ref()) {
                Some(res) => Some(res),
                None => {
                    tracing::error!(
                        "{} Undeclare unknown queryable {} ({}{})",
                        face,
                        id,
                        prefix.expr(),
                        expr.suffix
                    );
                    return;
                }
            },
            None => {
                tracing::error!(
                    "{} Undeclare queryable {} with unknown scope {}",
                    face,
                    id,
                    expr.scope
                );
                return;
            }
        }
    };
    let mut wtables = zwrite!(tables.tables);
    if let Some(mut res) =
        hat_code.undeclare_queryable(&mut wtables, face, id, res, node_id, send_declare)
    {
        tracing::debug!("{} Undeclare queryable {} ({})", face, id, res.expr());
        disable_matches_query_routes(&mut wtables, &mut res);
        Resource::clean(&mut res);
        drop(wtables);
    } else {
        // NOTE: This is expected behavior if queryable declarations are denied with ingress ACL interceptor.
        tracing::debug!("{} Undeclare unknown queryable {}", face, id);
    }
}

#[inline]
fn insert_pending_query(outface: &mut Arc<FaceState>, query: Arc<Query>) -> RequestId {
    let outface_mut = get_mut_unchecked(outface);
    // This `wrapping_add` is kind of "safe" because it would require an incredible amount
    // of parallel running queries to conflict a currently used id.
    // However, query ids are encoded with varint algorithm, so an incremental id isn't a
    // good match, and there is still room for optimization.
    outface_mut.next_qid = outface_mut.next_qid.wrapping_add(1);
    let qid = outface_mut.next_qid;
    outface_mut.pending_queries.insert(
        qid,
        (query, outface_mut.task_controller.get_cancellation_token()),
    );
    qid
}

#[inline]
fn compute_final_route(
    tables: &Tables,
    qabls: &Arc<QueryTargetQablSet>,
    src_face: &Arc<FaceState>,
    expr: &mut RoutingExpr,
    target: &QueryTarget,
    query: Arc<Query>,
) -> QueryRoute {
    match target {
        QueryTarget::All => {
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
        QueryTarget::AllComplete => {
            let mut route = HashMap::new();
            for qabl in qabls.iter() {
                if qabl.info.map(|info| info.complete).unwrap_or(true)
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
        QueryTarget::BestMatching => {
            if let Some(qabl) = qabls.iter().find(|qabl| {
                qabl.direction.0.id != src_face.id && qabl.info.is_some_and(|info| info.complete)
            }) {
                let mut route = HashMap::new();

                let mut direction = qabl.direction.clone();
                let qid = insert_pending_query(&mut direction.0, query);
                route.insert(direction.0.id, (direction, qid));

                route
            } else {
                compute_final_route(tables, qabls, src_face, expr, &QueryTarget::All, query)
            }
        }
    }
}

#[derive(Clone)]
struct QueryCleanup {
    tables: Arc<TablesLock>,
    face: Weak<FaceState>,
    qid: RequestId,
    timeout: Duration,
}

impl QueryCleanup {
    pub fn spawn_query_clean_up_task(
        face: &Arc<FaceState>,
        tables_ref: &Arc<TablesLock>,
        qid: u32,
        timeout: Duration,
    ) {
        let mut cleanup = QueryCleanup {
            tables: tables_ref.clone(),
            face: Arc::downgrade(face),
            qid,
            timeout,
        };
        if let Some((_, cancellation_token)) = face.pending_queries.get(&qid) {
            let c_cancellation_token = cancellation_token.clone();
            face.task_controller
                .spawn_with_rt(zenoh_runtime::ZRuntime::Net, async move {
                    tokio::select! {
                        _ = tokio::time::sleep(timeout) => { cleanup.run().await }
                        _ = c_cancellation_token.cancelled() => {}
                    }
                });
        }
    }
}

#[async_trait]
impl Timed for QueryCleanup {
    async fn run(&mut self) {
        if let Some(mut face) = self.face.upgrade() {
            let ext_respid = Some(response::ext::ResponderIdType {
                zid: face.zid,
                eid: 0,
            });
            route_send_response(
                &self.tables,
                &mut face,
                self.qid,
                response::ext::QoSType::RESPONSE,
                None,
                ext_respid,
                WireExpr::empty(),
                ResponseBody::Err(zenoh::Err {
                    encoding: Encoding::default(),
                    ext_sinfo: None,
                    #[cfg(feature = "shared-memory")]
                    ext_shm: None,
                    ext_unknown: vec![],
                    payload: ZBuf::from("Timeout".as_bytes().to_vec()),
                }),
            );
            let queries_lock = zwrite!(self.tables.queries_lock);
            if let Some(query) = get_mut_unchecked(&mut face)
                .pending_queries
                .remove(&self.qid)
            {
                drop(queries_lock);
                tracing::warn!(
                    "{}:{} Didn't receive final reply for query {}:{}: Timeout({:#?})!",
                    face,
                    self.qid,
                    query.0.src_face,
                    query.0.src_qid,
                    self.timeout,
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
    let hat = &tables.hat_code;
    let local_context = hat.map_routing_context(tables, face, routing_context);
    let mut compute_route = || hat.compute_query_route(tables, expr, local_context, face.whatami);
    if let Some(query_routes) = res
        .as_ref()
        .and_then(|res| res.context.as_ref())
        .map(|ctx| &ctx.query_routes)
    {
        return get_or_set_route(
            query_routes,
            tables.routes_version,
            face.whatami,
            local_context,
            compute_route,
        );
    }
    compute_route()
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

#[allow(clippy::too_many_arguments)]
pub fn route_query(
    tables_ref: &Arc<TablesLock>,
    face: &Arc<FaceState>,
    expr: &WireExpr,
    qid: RequestId,
    ext_qos: ext::QoSType,
    ext_tstamp: Option<ext::TimestampType>,
    ext_target: QueryTarget,
    ext_budget: Option<BudgetType>,
    ext_timeout: Option<TimeoutType>,
    body: RequestBody,
    routing_context: NodeId,
) {
    let rtables = zread!(tables_ref.tables);
    match rtables.get_mapping(face, &expr.scope, expr.mapping) {
        Some(prefix) => {
            tracing::debug!(
                "{}:{} Route query for res {}{}",
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
                let route =
                    compute_final_route(&rtables, &route, face, &mut expr, &ext_target, query);
                let timeout = ext_timeout.unwrap_or(rtables.queries_default_timeout);
                drop(queries_lock);
                drop(rtables);

                if route.is_empty() {
                    tracing::debug!(
                        "{}:{} Send final reply (no matching queryables or not master)",
                        face,
                        qid
                    );
                    face.primitives.clone().send_response_final(ResponseFinal {
                        rid: qid,
                        ext_qos: response::ext::QoSType::RESPONSE_FINAL,
                        ext_tstamp: None,
                    });
                } else {
                    for ((outface, key_expr, context), outqid) in route.values() {
                        QueryCleanup::spawn_query_clean_up_task(
                            outface, tables_ref, *outqid, timeout,
                        );
                        #[cfg(feature = "stats")]
                        if !admin {
                            inc_req_stats!(outface, tx, user, body)
                        } else {
                            inc_req_stats!(outface, tx, admin, body)
                        }

                        tracing::trace!(
                            "{}:{} Propagate query to {}:{}",
                            face,
                            qid,
                            outface,
                            outqid
                        );
                        outface.primitives.send_request(Request {
                            id: *outqid,
                            wire_expr: key_expr.into(),
                            ext_qos,
                            ext_tstamp,
                            ext_nodeid: ext::NodeIdType { node_id: *context },
                            ext_target,
                            ext_budget,
                            ext_timeout,
                            payload: body.clone(),
                        });
                    }
                }
            } else {
                tracing::debug!("{}:{} Send final reply (not master)", face, qid);
                drop(rtables);
                face.primitives.clone().send_response_final(ResponseFinal {
                    rid: qid,
                    ext_qos: response::ext::QoSType::RESPONSE_FINAL,
                    ext_tstamp: None,
                });
            }
        }
        None => {
            tracing::error!(
                "{}:{} Route query with unknown scope {}! Send final reply.",
                face,
                qid,
                expr.scope,
            );
            drop(rtables);
            face.primitives.clone().send_response_final(ResponseFinal {
                rid: qid,
                ext_qos: response::ext::QoSType::RESPONSE_FINAL,
                ext_tstamp: None,
            });
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn route_send_response(
    tables_ref: &Arc<TablesLock>,
    face: &mut Arc<FaceState>,
    qid: RequestId,
    ext_qos: ext::QoSType,
    ext_tstamp: Option<ext::TimestampType>,
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
        Some((query, _)) => {
            tracing::trace!(
                "{}:{} Route reply for query {}:{} ({})",
                face,
                qid,
                query.src_face,
                query.src_qid,
                key_expr.suffix.as_ref()
            );

            drop(queries_lock);

            #[cfg(feature = "stats")]
            if !admin {
                inc_res_stats!(query.src_face, tx, user, body)
            } else {
                inc_res_stats!(query.src_face, tx, admin, body)
            }

            query.src_face.primitives.send_response(Response {
                rid: query.src_qid,
                wire_expr: key_expr.to_owned(),
                payload: body,
                ext_qos,
                ext_tstamp,
                ext_respid,
            });
        }
        None => tracing::warn!("{}:{} Route reply: Query not found!", face, qid),
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
            tracing::debug!(
                "{}:{} Received final reply for query {}:{}",
                face,
                qid,
                query.0.src_face,
                query.0.src_qid,
            );
            finalize_pending_query(query);
        }
        None => tracing::warn!("{}:{} Route final reply: Query not found!", face, qid),
    }
}

pub(crate) fn finalize_pending_queries(tables_ref: &TablesLock, face: &mut Arc<FaceState>) {
    let queries_lock = zwrite!(tables_ref.queries_lock);
    for (_, query) in get_mut_unchecked(face).pending_queries.drain() {
        finalize_pending_query(query);
    }
    drop(queries_lock);
}

pub(crate) fn finalize_pending_query(query: (Arc<Query>, CancellationToken)) {
    let (query, cancellation_token) = query;
    cancellation_token.cancel();
    if let Some(query) = Arc::into_inner(query) {
        tracing::debug!("{}:{} Propagate final reply", query.src_face, query.src_qid);
        query
            .src_face
            .primitives
            .clone()
            .send_response_final(ResponseFinal {
                rid: query.src_qid,
                ext_qos: response::ext::QoSType::RESPONSE_FINAL,
                ext_tstamp: None,
            });
    }
}
