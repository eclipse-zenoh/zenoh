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
    ops::Not,
    sync::{Arc, Weak},
    time::Duration,
};

use async_trait::async_trait;
use itertools::Itertools;
use tokio_util::sync::CancellationToken;
use zenoh_buffers::ZBuf;
#[allow(unused_imports)]
use zenoh_core::polyfill::*;
use zenoh_protocol::{
    core::{Encoding, Region, WireExpr},
    network::{
        declare::{queryable::ext::QueryableInfoType, QueryableId},
        request::{self, ext::QueryTarget, Request, RequestId},
        response::{self, Response, ResponseFinal},
    },
    zenoh::{self, ResponseBody},
};
use zenoh_sync::get_mut_unchecked;
use zenoh_util::Timed;

use super::{
    face::FaceState,
    resource::{QueryTargetQablSet, Resource},
    tables::{NodeId, RoutingExpr, TablesLock},
};
use crate::net::routing::{
    dispatcher::{
        face::Face,
        local_resources::{LocalResourceInfoTrait, LocalResources},
        tables::{InterRegionFilter, Tables},
    },
    gateway::{get_or_set_route, node_id_as_source, QueryDirection, QueryTargetQabl, RouteBuilder},
    hat::{DispatcherContext, SendDeclare, UnregisterEntityResult},
};

#[derive(Clone)]
pub(crate) struct Query {
    src_face: Arc<FaceState>,
    src_qid: RequestId,
    src_qos: response::ext::QoSType,
}

impl Face {
    #[tracing::instrument(
        level = "debug",
        skip(self, send_declare, qabl_info),
        fields(
            expr = %expr,
            node_id = node_id_as_source(node_id),
            complete = qabl_info.complete,
            distance = qabl_info.distance,
        ),
        ret
    )]
    pub(crate) fn declare_queryable(
        &self,
        id: QueryableId,
        expr: &WireExpr,
        qabl_info: &QueryableInfoType,
        node_id: NodeId,
        send_declare: &mut SendDeclare,
    ) {
        self.with_mapped_expr(expr, |tables, mut res| {
            let region = self.state.region;

            let mut ctx = DispatcherContext {
                tables_lock: &self.tables,
                tables: &mut tables.data,
                src_face: &mut self.state.clone(),
                send_declare,
            };

            tables.hats[region].register_queryable(
                ctx.reborrow(),
                id,
                res.clone(),
                node_id,
                qabl_info,
            );

            tables.hats[region].disable_query_routes(&mut res);

            for dst in tables.hats.regions().collect_vec() {
                let other_info = tables
                    .hats
                    .values()
                    .filter(|hat| hat.region() != dst)
                    .flat_map(|hat| hat.remote_queryables_of(ctx.tables, &res))
                    .reduce(merge_qabl_infos);

                tables.hats[dst].propagate_queryable(ctx.reborrow(), res.clone(), other_info);
            }
        });
    }

    #[tracing::instrument(
        level = "debug",
        skip(self, send_declare),
        fields(expr = %expr, node_id = node_id_as_source(node_id)),
        ret
    )]
    pub(crate) fn undeclare_queryable(
        &self,
        id: QueryableId,
        expr: &WireExpr,
        node_id: NodeId,
        send_declare: &mut SendDeclare,
    ) {
        self.with_mapped_nullable_expr(expr, /* make_if_unknown */ false, |tables, res| {
            let region = self.state.region;

            let mut ctx = DispatcherContext {
                tables_lock: &self.tables,
                tables: &mut tables.data,
                src_face: &mut self.state.clone(),
                send_declare,
            };

            match tables.hats[region].unregister_queryable(ctx.reborrow(), id, res.clone(), node_id)
            {
                UnregisterEntityResult::Noop => {} // ¯\_(ツ)_/¯
                UnregisterEntityResult::InfoUpdate { mut res } => {
                    tables.hats[region].disable_query_routes(&mut res);

                    for dst in tables.hats.regions().collect_vec() {
                        let other_info = tables
                            .hats
                            .values()
                            .filter(|hat| hat.region() != dst)
                            .filter_map(|hat| hat.remote_queryables_of(ctx.tables, &res))
                            .reduce(merge_qabl_infos);

                        tables.hats[dst].propagate_queryable(
                            ctx.reborrow(),
                            res.clone(),
                            other_info,
                        );
                    }
                }
                UnregisterEntityResult::LastUnregistered { mut res } => {
                    tables.hats[region].disable_query_routes(&mut res);

                    let remainder = tables
                        .hats
                        .values()
                        .filter_map(|hat| {
                            (hat.region() != region)
                                .then(|| hat.remote_queryables_of(ctx.tables, &res))
                                .flatten()
                                .map(|info| (hat.region(), info))
                        })
                        .collect_vec();

                    match &*remainder {
                        [] => {
                            for hat in tables.hats.values_mut() {
                                hat.unpropagate_queryable(ctx.reborrow(), res.clone());
                            }
                            Resource::clean(&mut res);
                        }
                        [(last_owner, _)] => tables.hats[last_owner]
                            .unpropagate_last_non_owned_queryable(ctx, res.clone()),
                        _ => {
                            for hat in tables.hats.values_mut() {
                                let other_info = remainder
                                    .iter()
                                    .filter_map(|(region, info)| {
                                        (region != &hat.region()).then_some(*info)
                                    })
                                    .reduce(merge_qabl_infos);

                                hat.propagate_queryable(ctx.reborrow(), res.clone(), other_info);
                            }
                        }
                    }
                }
            }
        });
    }

    pub fn route_query(&self, msg: &mut Request) {
        let rtables = zread!(self.tables.tables);
        match rtables
            .data
            .get_mapping(&self.state, &msg.wire_expr.scope, msg.wire_expr.mapping)
        {
            Some(prefix) => {
                tracing::debug!(
                    "{}:{} Route query for res {}{}",
                    self.state,
                    msg.id,
                    prefix.expr(),
                    msg.wire_expr.suffix.as_ref(),
                );
                let prefix = prefix.clone();
                let expr = RoutingExpr::new(&prefix, msg.wire_expr.suffix.as_ref());

                #[cfg(feature = "stats")]
                let payload_observer =
                    super::stats::PayloadObserver::new(msg, Some(&expr), &rtables);
                #[cfg(feature = "stats")]
                payload_observer.observe_payload(zenoh_stats::Rx, &self.state, msg);

                let mut builder = RouteBuilder::<QueryDirection>::new();

                let queries_lock = zwrite!(self.tables.queries_lock);

                let query = Arc::new(Query {
                    src_face: self.state.clone(),
                    src_qid: msg.id,
                    src_qos: msg.ext_qos,
                });

                let src_face = &self.state;

                if !rtables.ingress_filter(src_face) {
                    return;
                }

                for dst in rtables.hats.regions() {
                    let qabls =
                        get_query_route(&rtables, src_face, &expr, msg.ext_nodeid.node_id, &dst);

                    let filter = {
                        let src_zid = rtables.hats[src_face.region]
                            .remote_node_id_to_zid(src_face, msg.ext_nodeid.node_id);
                        let tables = &rtables;

                        move |q: &QueryTargetQabl| {
                            InterRegionFilter {
                                src: &src_face.region,
                                dst: &q.region,
                                src_zid: src_zid.as_ref(),
                                fwd_zid: Some(&self.state.zid),
                                dst_zid: Some(&q.dir.dst_face.zid),
                            }
                            .resolve(tables)
                                && tables.egress_filter(src_face, &q.dir.dst_face)
                        }
                    };

                    self.compute_final_route(msg.ext_target, &mut builder, &query, &qabls, filter);
                }

                // NOTE: it's important to drop the `Arc<Query>` object immediately otherwise
                // a ResponseFinal from a local queryable won't finalize the query,
                // this is because `Arc::strong_count(&query)` would always be > 1.
                drop(query);

                let timeout = msg
                    .ext_timeout
                    .unwrap_or(rtables.data.queries_default_timeout);

                drop(queries_lock);
                drop(rtables);

                let dirs = builder.build();

                tracing::trace!(?dirs);

                if dirs.is_empty() {
                    tracing::debug!(
                        "{}:{} Send final reply (no matching queryables or not master)",
                        self.state,
                        msg.id
                    );
                    self.state
                        .primitives
                        .clone()
                        .send_response_final(&mut ResponseFinal {
                            rid: msg.id,
                            ext_qos: msg.ext_qos,
                            ext_tstamp: None,
                        });
                } else {
                    for QueryDirection { dir, rid } in dirs.into_iter() {
                        QueryCleanup::spawn_query_clean_up_task(
                            &dir.dst_face,
                            &self.tables,
                            rid,
                            msg.ext_qos,
                            timeout,
                        );

                        tracing::trace!(
                            "{}:{} Propagate query to {}:{}",
                            self.state,
                            msg.id,
                            dir.dst_face,
                            rid
                        );

                        let msg = &mut Request {
                            id: rid,
                            wire_expr: dir.wire_expr,
                            ext_qos: msg.ext_qos,
                            ext_tstamp: msg.ext_tstamp,
                            ext_nodeid: request::ext::NodeIdType {
                                node_id: dir.node_id,
                            },
                            ext_target: msg.ext_target,
                            ext_budget: msg.ext_budget,
                            ext_timeout: msg.ext_timeout,
                            payload: msg.payload.clone(),
                        };

                        if dir.dst_face.primitives.send_request(msg) {
                            #[cfg(feature = "stats")]
                            payload_observer.observe_payload(zenoh_stats::Tx, &dir.dst_face, msg);
                        }
                    }
                }
            }
            None => {
                tracing::error!(
                    "{}:{} Route query with unknown scope {}! Send final reply.",
                    self.state,
                    msg.id,
                    msg.wire_expr.scope,
                );
                drop(rtables);
                self.state
                    .primitives
                    .clone()
                    .send_response_final(&mut ResponseFinal {
                        rid: msg.id,
                        ext_qos: msg.ext_qos,
                        ext_tstamp: None,
                    });
            }
        }
    }

    #[allow(clippy::incompatible_msrv)]
    fn compute_final_route(
        &self,
        target: QueryTarget,
        route: &mut RouteBuilder<QueryDirection>,
        query: &Arc<Query>,
        qabls: &Arc<QueryTargetQablSet>,
        filter: impl Fn(&QueryTargetQabl) -> bool,
    ) {
        match target {
            QueryTarget::All => {
                for qabl in qabls.iter().filter(|q| filter(q)) {
                    route.insert(qabl.dir.dst_face.id, || {
                        let mut dir = qabl.dir.clone();
                        let rid = insert_pending_query(&mut dir.dst_face, query.clone());
                        tracing::debug!(dst = %dir.dst_face, dst.target = "all");
                        QueryDirection { dir, rid }
                    });
                }
            }
            QueryTarget::AllComplete => {
                for qabl in qabls
                    .iter()
                    .filter(|q| q.info.is_none_or(|info| info.complete) && filter(q))
                {
                    route.insert(qabl.dir.dst_face.id, || {
                        let mut dir = qabl.dir.clone();
                        let rid = insert_pending_query(&mut dir.dst_face, query.clone());
                        tracing::debug!(dst = %dir.dst_face, dst.target = "all-complete");
                        QueryDirection { dir, rid }
                    });
                }
            }
            QueryTarget::BestMatching => {
                if let Some(qabl) = qabls
                    .iter()
                    .find(|q| q.info.is_some_and(|info| info.complete) && filter(q))
                {
                    route.insert(qabl.dir.dst_face.id, || {
                        let mut dir = qabl.dir.clone();
                        let rid = insert_pending_query(&mut dir.dst_face, query.clone());
                        tracing::debug!(dst = %dir.dst_face, dst.target = "best-matching");
                        QueryDirection { dir, rid }
                    });
                } else {
                    self.compute_final_route(QueryTarget::All, route, query, qabls, filter)
                }
            }
        }
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

#[derive(Clone)]
struct QueryCleanup {
    tables: Arc<TablesLock>,
    face: Weak<FaceState>,
    qid: RequestId,
    qos: response::ext::QoSType,
    timeout: Duration,
}

impl QueryCleanup {
    pub fn spawn_query_clean_up_task(
        face: &Arc<FaceState>,
        tables_ref: &Arc<TablesLock>,
        qid: u32,
        qos: response::ext::QoSType,
        timeout: Duration,
    ) {
        let mut cleanup = QueryCleanup {
            tables: tables_ref.clone(),
            face: Arc::downgrade(face),
            qid,
            qos,
            timeout,
        };
        let queries_lock = zread!(tables_ref.queries_lock);
        if let Some((_, cancellation_token)) = face.pending_queries.get(&qid) {
            let c_cancellation_token = cancellation_token.clone();
            drop(queries_lock);
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
                &mut Response {
                    rid: self.qid,
                    wire_expr: WireExpr::empty(),
                    payload: ResponseBody::Err(zenoh::Err {
                        encoding: Encoding::default(),
                        ext_sinfo: None,
                        #[cfg(feature = "shared-memory")]
                        ext_shm: None,
                        ext_unknown: vec![],
                        payload: ZBuf::from("Timeout".as_bytes().to_vec()),
                    }),
                    ext_qos: self.qos,
                    ext_tstamp: None,
                    ext_respid,
                },
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

#[inline]
fn get_query_route(
    tables: &Tables,
    src_face: &FaceState,
    expr: &RoutingExpr,
    routing_context: NodeId,
    region: &Region,
) -> Arc<QueryTargetQablSet> {
    let node_id = tables.hats[region].map_routing_context(&tables.data, src_face, routing_context);
    let compute_route =
        || tables.hats[region].compute_query_route(&tables.data, &src_face.region, expr, node_id);
    if let Some(query_routes) = expr
        .resource()
        .as_ref()
        .and_then(|res| res.ctx.as_ref())
        .map(|ctx| &ctx.hats[region].query_routes)
    {
        return get_or_set_route(
            query_routes,
            tables.data.hats[region].routes_version,
            &src_face.region,
            node_id,
            compute_route,
        );
    }
    compute_route()
}

pub(crate) fn route_send_response(
    tables_ref: &Arc<TablesLock>,
    face: &mut Arc<FaceState>,
    msg: &mut Response,
) {
    let tables = zread!(tables_ref.tables);
    match tables
        .data
        .get_mapping(face, &msg.wire_expr.scope, msg.wire_expr.mapping)
    {
        Some(prefix) => {
            let expr = msg
                .wire_expr
                .is_empty() // account for empty wire expression in ReplyErr messages
                .not()
                .then(|| RoutingExpr::new(prefix, msg.wire_expr.suffix.as_ref()));
            #[cfg(feature = "stats")]
            let payload_observer = super::stats::PayloadObserver::new(msg, expr.as_ref(), &tables);
            #[cfg(feature = "stats")]
            payload_observer.observe_payload(zenoh_stats::Rx, face, msg);
            let queries_lock = zread!(tables_ref.queries_lock);
            match face
                .pending_queries
                .get(&msg.rid)
                .map(|(q, _)| q.as_ref().clone())
            {
                Some(query) => {
                    if let Some(expr) = expr {
                        // TODO: consider to optimize keyexpr for 2.0 ?
                        // Doing it now will break wire compatibility
                        // msg.wire_expr = expr.get_best_key(src_face.id).to_owned();
                        match expr.key_expr() {
                            Some(key_expr) => {
                                msg.wire_expr =
                                    WireExpr::empty().with_suffix(key_expr.as_str()).to_owned();
                            }
                            None => {
                                tracing::error!("{}:{} Route reply: wire expr {} does not map to a valid key expression!", face, msg.rid, msg.wire_expr);
                                return;
                            }
                        }
                    }
                    tracing::trace!(
                        "{}:{} Route reply for query {}:{} ({})",
                        face,
                        msg.rid,
                        query.src_face,
                        query.src_qid,
                        msg.wire_expr.suffix.as_ref()
                    );
                    drop(tables);
                    drop(queries_lock);

                    msg.rid = query.src_qid;
                    msg.ext_qos = query.src_qos;
                    if query.src_face.primitives.send_response(msg) {
                        #[cfg(feature = "stats")]
                        payload_observer.observe_payload(zenoh_stats::Tx, &query.src_face, msg);
                    }
                }
                None => tracing::warn!("{}:{} Route reply: Query not found!", face, msg.rid),
            }
        }
        None => {
            tracing::error!(
                "{} Routing reply {} for unknown scope {}",
                face,
                msg.rid,
                msg.wire_expr.scope
            )
        }
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
                "{}:{} Received final reply for query {}:{} strong_count={}",
                face,
                qid,
                query.0.src_face,
                query.0.src_qid,
                Arc::strong_count(&query.0)
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
            .send_response_final(&mut ResponseFinal {
                rid: query.src_qid,
                ext_qos: query.src_qos,
                ext_tstamp: None,
            });
    }
}

pub(crate) fn merge_qabl_infos(
    mut this: QueryableInfoType,
    info: QueryableInfoType,
) -> QueryableInfoType {
    this.distance = match (this.complete, info.complete) {
        (true, true) | (false, false) => std::cmp::min(this.distance, info.distance),
        (true, false) => this.distance,
        (false, true) => info.distance,
    };
    this.complete = this.complete || info.complete;
    this
}

impl LocalResourceInfoTrait<Arc<Resource>> for QueryableInfoType {
    fn aggregate(
        self_val: Option<Self>,
        self_res: &Arc<Resource>,
        other_val: &Self,
        other_res: &Arc<Resource>,
    ) -> Self {
        // shortcut to avoid checking inclusion of ke, since we only care about completeness in aggregates and can ignore distance
        if let Some(val) = self_val {
            if val.complete == other_val.complete {
                return val;
            }
        }

        let other_complete = if other_val.complete {
            if let (Some(self_ke), Some(other_ke)) = (self_res.keyexpr(), other_res.keyexpr()) {
                other_ke.includes(self_ke)
            } else {
                false
            }
        } else {
            false
        };
        let mut other_val = *other_val;
        other_val.complete = other_complete;

        if let Some(val) = self_val {
            merge_qabl_infos(val, other_val)
        } else {
            other_val
        }
    }
}

pub(crate) type LocalQueryables = LocalResources<QueryableId, Arc<Resource>, QueryableInfoType>;
