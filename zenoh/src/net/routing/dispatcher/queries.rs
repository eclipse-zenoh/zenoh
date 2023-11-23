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
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Weak};
use zenoh_protocol::{
    core::{Encoding, WireExpr},
    network::{
        declare::ext,
        request::{ext::TargetType, Request, RequestId},
        response::{self, ext::ResponderIdType, Response, ResponseFinal},
    },
    zenoh::{reply::ext::ConsolidationType, Reply, RequestBody, ResponseBody},
};
use zenoh_sync::get_mut_unchecked;
use zenoh_util::Timed;

pub(crate) struct Query {
    src_face: Arc<FaceState>,
    src_qid: RequestId,
}

pub(crate) fn compute_query_routes_from(tables: &mut Tables, res: &mut Arc<Resource>) {
    tables.hat_code.clone().compute_query_routes(tables, res);
    let res = get_mut_unchecked(res);
    for child in res.childs.values_mut() {
        compute_query_routes_from(tables, child);
    }
}

pub(crate) fn compute_matches_query_routes_(
    tables: &Tables,
    res: &Arc<Resource>,
) -> Vec<(Arc<Resource>, QueryRoutes)> {
    let mut routes = vec![];
    if res.context.is_some() {
        routes.push((
            res.clone(),
            tables.hat_code.compute_query_routes_(tables, res),
        ));
        for match_ in &res.context().matches {
            let match_ = match_.upgrade().unwrap();
            if !Arc::ptr_eq(&match_, res) {
                let match_routes = tables.hat_code.compute_query_routes_(tables, &match_);
                routes.push((match_, match_routes));
            }
        }
    }
    routes
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
                    #[cfg(feature = "complete_n")]
                    {
                        route.entry(qabl.direction.0.id).or_insert_with(|| {
                            let mut direction = qabl.direction.clone();
                            let qid = insert_pending_query(&mut direction.0, query.clone());
                            (direction, qid, *target)
                        });
                    }
                    #[cfg(not(feature = "complete_n"))]
                    {
                        route.entry(qabl.direction.0.id).or_insert_with(|| {
                            let mut direction = qabl.direction.clone();
                            let qid = insert_pending_query(&mut direction.0, query.clone());
                            (direction, qid)
                        });
                    }
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
                    #[cfg(feature = "complete_n")]
                    {
                        route.entry(qabl.direction.0.id).or_insert_with(|| {
                            let mut direction = qabl.direction.clone();
                            let qid = insert_pending_query(&mut direction.0, query.clone());
                            (direction, qid, *target)
                        });
                    }
                    #[cfg(not(feature = "complete_n"))]
                    {
                        route.entry(qabl.direction.0.id).or_insert_with(|| {
                            let mut direction = qabl.direction.clone();
                            let qid = insert_pending_query(&mut direction.0, query.clone());
                            (direction, qid)
                        });
                    }
                }
            }
            route
        }
        #[cfg(feature = "complete_n")]
        TargetType::Complete(n) => {
            let mut route = HashMap::new();
            let mut remaining = *n;
            if src_face.whatami == WhatAmI::Peer && !tables.full_net(WhatAmI::Peer) {
                let source_links = tables
                    .peers_net
                    .as_ref()
                    .map(|net| net.get_links(src_face.zid))
                    .unwrap_or_default();
                for qabl in qabls.iter() {
                    if qabl.direction.0.id != src_face.id
                        && qabl.complete > 0
                        && (qabl.direction.0.whatami != WhatAmI::Peer
                            || (tables.router_peers_failover_brokering
                                && Tables::failover_brokering_to(
                                    source_links,
                                    qabl.direction.0.zid,
                                )))
                    {
                        let nb = std::cmp::min(qabl.complete, remaining);
                        route.entry(qabl.direction.0.id).or_insert_with(|| {
                            let mut direction = qabl.direction.clone();
                            let qid = insert_pending_query(&mut direction.0, query.clone());
                            (direction, qid, TargetType::Complete(nb))
                        });
                        remaining -= nb;
                        if remaining == 0 {
                            break;
                        }
                    }
                }
            } else {
                for qabl in qabls.iter() {
                    if qabl.direction.0.id != src_face.id && qabl.complete > 0 {
                        let nb = std::cmp::min(qabl.complete, remaining);
                        route.entry(qabl.direction.0.id).or_insert_with(|| {
                            let mut direction = qabl.direction.clone();
                            let qid = insert_pending_query(&mut direction.0, query.clone());
                            (direction, qid, TargetType::Complete(nb))
                        });
                        remaining -= nb;
                        if remaining == 0 {
                            break;
                        }
                    }
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
                #[cfg(feature = "complete_n")]
                {
                    let mut direction = qabl.direction.clone();
                    let qid = insert_pending_query(&mut direction.0, query);
                    route.insert(direction.0.id, (direction, qid, *target));
                }
                #[cfg(not(feature = "complete_n"))]
                {
                    let mut direction = qabl.direction.clone();
                    let qid = insert_pending_query(&mut direction.0, query);
                    route.insert(direction.0.id, (direction, qid));
                }
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
        get_mut_unchecked(res).context_mut().valid_query_routes = false;
        for match_ in &res.context().matches {
            let mut match_ = match_.upgrade().unwrap();
            if !Arc::ptr_eq(&match_, res) {
                get_mut_unchecked(&mut match_)
                    .context_mut()
                    .valid_query_routes = false;
            }
        }
    }
}

// #[inline]
// fn get_query_route(
//     tables: &Tables,
//     face: &FaceState,
//     res: &Option<Arc<Resource>>,
//     expr: &mut RoutingExpr,
//     routing_context: NodeId,
// ) -> Arc<QueryTargetQablSet> {
//     match tables.whatami {
//         WhatAmI::Router => match face.whatami {
//             WhatAmI::Router => {
//                 let routers_net = tables.hat.routers_net.as_ref().unwrap();
//                 let local_context = routers_net.get_local_context(routing_context, face.link_id);
//                 res.as_ref()
//                     .and_then(|res| res.routers_query_route(local_context))
//                     .unwrap_or_else(|| {
//                         compute_query_route(tables, expr, local_context, face.whatami)
//                     })
//             }
//             WhatAmI::Peer => {
//                 if tables.hat.full_net(WhatAmI::Peer) {
//                     let peers_net = tables.hat.peers_net.as_ref().unwrap();
//                     let local_context = peers_net.get_local_context(routing_context, face.link_id);
//                     res.as_ref()
//                         .and_then(|res| res.peers_query_route(local_context))
//                         .unwrap_or_else(|| {
//                             compute_query_route(tables, expr, local_context, face.whatami)
//                         })
//                 } else {
//                     res.as_ref()
//                         .and_then(|res| res.peer_query_route())
//                         .unwrap_or_else(|| {
//                             compute_query_route(
//                                 tables,
//                                 expr,
//                                 NodeId::default(),
//                                 face.whatami,
//                             )
//                         })
//                 }
//             }
//             _ => res
//                 .as_ref()
//                 .and_then(|res| res.routers_query_route(NodeId::default()))
//                 .unwrap_or_else(|| {
//                     compute_query_route(tables, expr, NodeId::default(), face.whatami)
//                 }),
//         },
//         WhatAmI::Peer => {
//             if tables.hat.full_net(WhatAmI::Peer) {
//                 match face.whatami {
//                     WhatAmI::Router | WhatAmI::Peer => {
//                         let peers_net = tables.hat.peers_net.as_ref().unwrap();
//                         let local_context =
//                             peers_net.get_local_context(routing_context, face.link_id);
//                         res.as_ref()
//                             .and_then(|res| res.peers_query_route(local_context))
//                             .unwrap_or_else(|| {
//                                 compute_query_route(tables, expr, local_context, face.whatami)
//                             })
//                     }
//                     _ => res
//                         .as_ref()
//                         .and_then(|res| res.peers_query_route(NodeId::default()))
//                         .unwrap_or_else(|| {
//                             compute_query_route(
//                                 tables,
//                                 expr,
//                                 NodeId::default(),
//                                 face.whatami,
//                             )
//                         }),
//                 }
//             } else {
//                 res.as_ref()
//                     .and_then(|res| match face.whatami {
//                         WhatAmI::Client => res.client_query_route(),
//                         _ => res.peer_query_route(),
//                     })
//                     .unwrap_or_else(|| {
//                         compute_query_route(tables, expr, NodeId::default(), face.whatami)
//                     })
//             }
//         }
//         _ => res
//             .as_ref()
//             .and_then(|res| res.client_query_route())
//             .unwrap_or_else(|| {
//                 compute_query_route(tables, expr, NodeId::default(), face.whatami)
//             }),
//     }
// }

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
                use zenoh_buffers::SplitBuffer;
                match &$body {
                    RequestBody::Put(p) => {
                        stats.[<$txrx _z_put_msgs>].[<inc_ $space>](1);
                        stats.[<$txrx _z_put_pl_bytes>].[<inc_ $space>](p.payload.len());
                    }
                    RequestBody::Del(_) => {
                        stats.[<$txrx _z_del_msgs>].[<inc_ $space>](1);
                    }
                    RequestBody::Query(q) => {
                        stats.[<$txrx _z_query_msgs>].[<inc_ $space>](1);
                        stats.[<$txrx _z_query_pl_bytes>].[<inc_ $space>](
                            q.ext_body.as_ref().map(|b| b.payload.len()).unwrap_or(0),
                        );
                    }
                    RequestBody::Pull(_) => (),
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
                use zenoh_buffers::SplitBuffer;
                match &$body {
                    ResponseBody::Put(p) => {
                        stats.[<$txrx _z_put_msgs>].[<inc_ $space>](1);
                        stats.[<$txrx _z_put_pl_bytes>].[<inc_ $space>](p.payload.len());
                    }
                    ResponseBody::Reply(r) => {
                        stats.[<$txrx _z_reply_msgs>].[<inc_ $space>](1);
                        stats.[<$txrx _z_reply_pl_bytes>].[<inc_ $space>](r.payload.len());
                    }
                    ResponseBody::Err(e) => {
                        stats.[<$txrx _z_reply_msgs>].[<inc_ $space>](1);
                        stats.[<$txrx _z_reply_pl_bytes>].[<inc_ $space>](
                            e.ext_body.as_ref().map(|b| b.payload.len()).unwrap_or(0),
                        );
                    }
                    ResponseBody::Ack(_) => (),
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
                // let res = Resource::get_resource(&prefix, expr.suffix);
                let route = rtables.hat_code.compute_query_route(
                    &rtables,
                    &mut expr,
                    routing_context,
                    face.whatami,
                );

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

                drop(queries_lock);
                drop(rtables);

                for (expr, payload) in local_replies {
                    let payload = ResponseBody::Reply(Reply {
                        timestamp: None,
                        encoding: Encoding::default(),
                        ext_sinfo: None,
                        ext_consolidation: ConsolidationType::default(),
                        #[cfg(feature = "shared-memory")]
                        ext_shm: None,
                        ext_unknown: vec![],
                        payload,
                    });
                    #[cfg(feature = "stats")]
                    if !admin {
                        inc_res_stats!(face, tx, user, payload)
                    } else {
                        inc_res_stats!(face, tx, admin, payload)
                    }

                    face.primitives.clone().send_response(Response {
                        rid: qid,
                        wire_expr: expr,
                        payload,
                        ext_qos: response::ext::QoSType::declare_default(),
                        ext_tstamp: None,
                        ext_respid: Some(response::ext::ResponderIdType {
                            zid,
                            eid: 0, // TODO
                        }),
                    });
                }

                if route.is_empty() {
                    log::debug!(
                        "Send final reply {}:{} (no matching queryables or not master)",
                        face,
                        qid
                    );
                    face.primitives.clone().send_response_final(ResponseFinal {
                        rid: qid,
                        ext_qos: response::ext::QoSType::response_final_default(),
                        ext_tstamp: None,
                    });
                } else {
                    // let timer = tables.timer.clone();
                    // let timeout = tables.queries_default_timeout;
                    #[cfg(feature = "complete_n")]
                    {
                        for ((outface, key_expr, context), qid, t) in route.values() {
                            // timer.add(TimedEvent::once(
                            //     Instant::now() + timeout,
                            //     QueryCleanup {
                            //         tables: tables_ref.clone(),
                            //         face: Arc::downgrade(&outface),
                            //         *qid,
                            //     },
                            // ));
                            #[cfg(feature = "stats")]
                            if !admin {
                                inc_req_stats!(outface, tx, user, body)
                            } else {
                                inc_req_stats!(outface, tx, admin, body)
                            }

                            log::trace!("Propagate query {}:{} to {}", face, qid, outface);
                            outface.primitives.send_request(Request {
                                id: *qid,
                                wire_expr: key_expr.into(),
                                ext_qos: ext::QoSType::request_default(), // TODO
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType { node_id: *context },
                                ext_target: *t,
                                ext_budget: None,
                                ext_timeout: None,
                                payload: body.clone(),
                            });
                        }
                    }

                    #[cfg(not(feature = "complete_n"))]
                    {
                        for ((outface, key_expr, context), qid) in route.values() {
                            // timer.add(TimedEvent::once(
                            //     Instant::now() + timeout,
                            //     QueryCleanup {
                            //         tables: tables_ref.clone(),
                            //         face: Arc::downgrade(&outface),
                            //         *qid,
                            //     },
                            // ));
                            #[cfg(feature = "stats")]
                            if !admin {
                                inc_req_stats!(outface, tx, user, body)
                            } else {
                                inc_req_stats!(outface, tx, admin, body)
                            }

                            log::trace!("Propagate query {}:{} to {}", face, qid, outface);
                            outface.primitives.send_request(Request {
                                id: *qid,
                                wire_expr: key_expr.into(),
                                ext_qos: ext::QoSType::request_default(),
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType { node_id: *context },
                                ext_target: target,
                                ext_budget: None,
                                ext_timeout: None,
                                payload: body.clone(),
                            });
                        }
                    }
                }
            } else {
                log::debug!("Send final reply {}:{} (not master)", face, qid);
                drop(rtables);
                face.primitives.clone().send_response_final(ResponseFinal {
                    rid: qid,
                    ext_qos: response::ext::QoSType::response_final_default(),
                    ext_tstamp: None,
                });
            }
        }
        None => {
            log::error!(
                "Route query with unknown scope {}! Send final reply.",
                expr.scope
            );
            drop(rtables);
            face.primitives.clone().send_response_final(ResponseFinal {
                rid: qid,
                ext_qos: response::ext::QoSType::response_final_default(),
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

            query.src_face.primitives.clone().send_response(Response {
                rid: query.src_qid,
                wire_expr: key_expr.to_owned(),
                payload: body,
                ext_qos: response::ext::QoSType::response_default(),
                ext_tstamp: None,
                ext_respid,
            });
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
            .send_response_final(ResponseFinal {
                rid: query.src_qid,
                ext_qos: response::ext::QoSType::response_final_default(),
                ext_tstamp: None,
            });
    }
}
