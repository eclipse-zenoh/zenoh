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

use std::sync::Arc;

use itertools::Itertools;
use zenoh_core::zread;
use zenoh_protocol::{
    core::{Region, Reliability, WireExpr},
    network::{declare::SubscriberId, push::ext, Push},
};

use super::{
    face::FaceState,
    resource::Resource,
    tables::{NodeId, Route, RoutingExpr, Tables, TablesLock},
};
use crate::net::routing::{
    dispatcher::{
        face::Face,
        local_resources::{LocalResourceInfoTrait, LocalResources},
        tables::InterRegionFilter,
    },
    gateway::{get_or_set_route, node_id_as_source, Direction, RouteBuilder},
    hat::{DispatcherContext, SendDeclare},
};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) struct SubscriberInfo;

impl Face {
    #[tracing::instrument(
        level = "debug",
        skip(self, send_declare, sub_info),
        fields(expr = %expr, node_id = node_id_as_source(node_id)),
        ret
    )]
    pub(crate) fn declare_subscriber(
        &self,
        id: SubscriberId,
        expr: &WireExpr,
        sub_info: &SubscriberInfo,
        node_id: NodeId,
        send_declare: &mut SendDeclare,
    ) {
        self.with_mapped_expr(expr, |tables, mut res| {
            let hats = &mut tables.hats;
            let region = self.state.region;

            let mut ctx = DispatcherContext {
                tables_lock: &self.tables,
                tables: &mut tables.data,
                src_face: &mut self.state.clone(),
                send_declare,
            };

            hats[region].register_subscriber(ctx.reborrow(), id, res.clone(), node_id, sub_info);

            hats[region].disable_data_routes(&mut res);

            for dst in hats.regions().collect_vec() {
                let other_info = hats
                    .values()
                    .filter(|hat| hat.region() != dst)
                    .flat_map(|hat| hat.remote_subscribers_of(ctx.tables, &res))
                    .reduce(|_, _| SubscriberInfo);

                hats[dst].propagate_subscriber(ctx.reborrow(), res.clone(), other_info);
            }
        });
    }

    #[tracing::instrument(
        level = "debug",
        skip(self, send_declare),
        fields(expr = %expr, node_id = node_id_as_source(node_id)),
        ret
    )]
    pub(crate) fn undeclare_subscriber(
        &self,
        id: SubscriberId,
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

            if let Some(mut res) =
                tables.hats[region].unregister_subscriber(ctx.reborrow(), id, res.clone(), node_id)
            {
                tables.hats[region].disable_data_routes(&mut res);

                let mut remaining = tables
                    .hats
                    .values_mut()
                    .filter(|hat| hat.remote_subscribers_of(ctx.tables, &res).is_some())
                    .collect_vec();

                if (*remaining).is_empty() {
                    for hat in tables.hats.values_mut() {
                        hat.unpropagate_subscriber(ctx.reborrow(), res.clone());
                    }
                    Resource::clean(&mut res);
                } else if let [last_owner] = &mut *remaining {
                    last_owner.unpropagate_last_non_owned_subscriber(ctx, res.clone())
                }
            }
        });
    }
}

macro_rules! treat_timestamp {
    ($hlc:expr, $payload:expr, $drop:expr) => {
        // if an HLC was configured (via Config.add_timestamp),
        // check DataInfo and add a timestamp if there isn't
        if let Some(hlc) = $hlc {
            if let zenoh_protocol::zenoh::PushBody::Put(data) = &mut $payload {
                if let Some(ref ts) = data.timestamp {
                    // Timestamp is present; update HLC with it (possibly raising error if delta exceed)
                    match hlc.update_with_timestamp(ts) {
                        Ok(()) => (),
                        Err(e) => {
                            if $drop {
                                tracing::error!(
                                    "Error treating timestamp for received Data ({}). Drop it!",
                                    e
                                );
                                return;
                            } else {
                                data.timestamp = Some(hlc.new_timestamp());
                                tracing::error!(
                                    "Error treating timestamp for received Data ({}). Replace timestamp: {:?}",
                                    e,
                                    data.timestamp);
                            }
                        }
                    }
                } else {
                    // Timestamp not present; add one
                    data.timestamp = Some(hlc.new_timestamp());
                    tracing::trace!("Adding timestamp to DataInfo: {:?}", data.timestamp);
                }
            }
        }
    }
}

#[inline]
fn get_hat_data_route(
    tables: &Tables,
    src_face: &FaceState,
    expr: &RoutingExpr,
    node_id: NodeId,
    region: &Region,
) -> Arc<Route> {
    let node_id = tables.hats[region].map_routing_context(&tables.data, src_face, node_id);
    let compute_route =
        || tables.hats[region].compute_data_route(&tables.data, &src_face.region, expr, node_id);
    match expr
        .resource()
        .as_ref()
        .and_then(|res| res.ctx.as_ref())
        .map(|ctx| &ctx.hats[region].data_routes)
    {
        Some(data_routes) => get_or_set_route(
            data_routes,
            tables.data.hats[region].routes_version,
            &src_face.region,
            node_id,
            compute_route,
        ),
        None => compute_route(),
    }
}

#[inline]
fn get_data_route(
    tables: &Tables,
    src_face: &FaceState,
    expr: &RoutingExpr,
    node_id: NodeId,
) -> Arc<Route> {
    let compute_route = || {
        let mut builder = RouteBuilder::<Direction>::new();

        for (region, _) in tables.hats.iter() {
            let route = get_hat_data_route(tables, src_face, expr, node_id, &region);

            for dir in route.iter() {
                builder.insert(dir.dst_face.id, || dir.clone());
            }
        }
        Arc::new(builder.build())
    };
    let node_id = tables.hats[src_face.region].map_routing_context(&tables.data, src_face, node_id);
    match expr
        .resource()
        .as_ref()
        .and_then(|res| res.ctx.as_ref())
        .map(|ctx| &ctx.data_routes)
    {
        Some(data_routes) => get_or_set_route(
            data_routes,
            tables.data.routes_version,
            &src_face.region,
            node_id,
            compute_route,
        ),
        None => compute_route(),
    }
}

pub fn route_data(
    tables_ref: &Arc<TablesLock>,
    src_face: &FaceState,
    msg: &mut Push,
    reliability: Reliability,
    consume: bool,
) {
    let rtables = zread!(tables_ref.tables);
    let tables = &*rtables;
    let Some(prefix) =
        rtables
            .data
            .get_mapping(src_face, &msg.wire_expr.scope, msg.wire_expr.mapping)
    else {
        tracing::error!(
            "{} Route data with unknown scope {}!",
            src_face,
            msg.wire_expr.scope
        );
        return;
    };

    tracing::trace!(
        "{} Route data for res {}{}",
        src_face,
        prefix.expr(),
        msg.wire_expr.suffix.as_ref()
    );

    let expr = RoutingExpr::new(prefix, msg.wire_expr.suffix.as_ref());

    #[cfg(feature = "stats")]
    let payload_observer = super::stats::PayloadObserver::new(msg, Some(&expr), tables);
    #[cfg(feature = "stats")]
    payload_observer.observe_payload(zenoh_stats::Rx, src_face, msg);

    if !tables.ingress_filter(src_face) {
        return;
    }

    let send_push = |dst_face: &FaceState, msg: &mut Push, reliability: Reliability| {
        if dst_face.primitives.send_push(msg, reliability) {
            #[cfg(feature = "stats")]
            payload_observer.observe_payload(zenoh_stats::Tx, dst_face, msg);
        }
    };

    let route = get_data_route(&rtables, src_face, &expr, msg.ext_nodeid.node_id);

    tracing::trace!(?route);

    if !route.is_empty() {
        treat_timestamp!(
            &rtables.data.hlc,
            msg.payload,
            rtables.data.drop_future_timestamp
        );

        let inter_region_filter = {
            let src_zid = tables.hats[src_face.region]
                .remote_node_id_to_zid(src_face, msg.ext_nodeid.node_id);
            move |dir: &Direction| {
                InterRegionFilter {
                    src: &src_face.region,
                    dst: &dir.dst_face.region,
                    src_zid: src_zid.as_ref(),
                    fwd_zid: Some(&src_face.zid),
                    dst_zid: Some(&dir.dst_face.zid),
                }
                .resolve(tables)
            }
        };

        if route.len() == 1 {
            let dir = route.iter().next().unwrap();

            if inter_region_filter(dir) && rtables.egress_filter(src_face, &dir.dst_face) {
                drop(rtables);
                let mut msg_clone;
                let mut msg = &mut *msg;
                if !consume {
                    msg_clone = msg.clone();
                    msg = &mut msg_clone;
                }

                msg.wire_expr = dir.wire_expr.clone();
                msg.ext_nodeid = ext::NodeIdType {
                    node_id: dir.node_id,
                };
                send_push(&dir.dst_face, msg, reliability);
            }
        } else {
            // Concurrent multi-destination fan-out.
            //
            // The previous loop dispatched each destination's `send_push`
            // serially. For a `CongestionControl::Block` message the
            // per-destination `send_push` can wait up to `wait_before_close`
            // (default 5 s) when the destination's `TransmissionPipeline` is
            // back-pressured (peer congested, transport tearing down, ...).
            // With N peers in a full p2p mesh and K back-pressured pipelines,
            // the serial loop accumulated K × wait_before_close inside a
            // single `publisher.put(...).wait()` call (up to ~30 s observed
            // on a 50-peer / 3-churner workload with the default 5 s wait).
            //
            // Each destination has its own face / pipeline / link, so the
            // waits are mutually independent and safe to parallelize. Snapshot
            // the destination set into owned `(Arc<FaceState>, Push)` pairs
            // (the routing-table lock is dropped before the spawn), then
            // dispatch one `ZRuntime::Net.spawn_blocking` task per
            // destination. The total wall-clock for the fan-out is bounded by
            // `max(per-face wait)`, not by their sum.
            //
            // Implementation notes:
            // - `spawn_blocking` reuses tokio's shared blocking pool
            //   (default cap ~512), so the per-put cost is an enqueue, not
            //   an OS-thread create/destroy.
            // - `futures::executor::block_on` is runtime-agnostic and is
            //   used to wait for every JoinHandle on the calling thread.
            //   The blocking work runs on tokio's separate blocking-pool
            //   workers, so there is no nested-runtime deadlock even when
            //   `route_data` is invoked from a tokio recv-task worker.
            // - Single-destination case stays on the synchronous path to
            //   avoid the (small) spawn / join overhead.
            // - Panics in spawned tasks are propagated via `resume_unwind`,
            //   preserving the pre-change fail-fast behavior.
            let dispatch: Vec<(Arc<FaceState>, Push)> = route
                .iter()
                .filter(|dir| {
                    inter_region_filter(dir) && rtables.egress_filter(src_face, &dir.dst_face)
                })
                .map(|dir| {
                    let push = Push {
                        wire_expr: dir.wire_expr.clone(),
                        ext_qos: msg.ext_qos,
                        ext_tstamp: None,
                        ext_nodeid: ext::NodeIdType {
                            node_id: dir.node_id,
                        },
                        payload: msg.payload.clone(),
                    };
                    (dir.dst_face.clone(), push)
                })
                .collect();

            drop(rtables);

            if dispatch.len() <= 1 {
                // Single destination: skip the spawn/join overhead and stay on
                // the synchronous path.
                for (dst_face, mut push) in dispatch {
                    send_push(&dst_face, &mut push, reliability);
                }
            } else {
                let mut joins = Vec::with_capacity(dispatch.len());
                for (dst_face, mut push) in dispatch {
                    joins.push(zenoh_runtime::ZRuntime::Net.spawn_blocking(move || {
                        let sent = dst_face.primitives.send_push(&mut push, reliability);
                        (dst_face, push, sent)
                    }));
                }
                // Block synchronously on every task. Iterating in order is fine:
                // tasks run in parallel on the blocking pool, so the cumulative
                // wait equals the slowest task (not the sum).
                for join in joins {
                    let result: Result<_, tokio::task::JoinError> =
                        futures::executor::block_on(join);
                    match result {
                        Ok((dst_face, push, sent)) => {
                            #[cfg(feature = "stats")]
                            if sent {
                                payload_observer
                                    .observe_payload(zenoh_stats::Tx, &dst_face, &push);
                            }
                            #[cfg(not(feature = "stats"))]
                            {
                                let _ = (dst_face, push, sent);
                            }
                        }
                        Err(join_err) => {
                            if join_err.is_panic() {
                                std::panic::resume_unwind(join_err.into_panic());
                            }
                            // Cancellation should not happen here (we hold the
                            // JoinHandle), but log it so we don't silently lose
                            // a fanout slot if it ever does.
                            tracing::error!(
                                "concurrent fanout task cancelled unexpectedly: {join_err}"
                            );
                        }
                    }
                }
            }
        }
    }
}

impl LocalResourceInfoTrait<Arc<Resource>> for SubscriberInfo {
    fn aggregate(
        _self_val: Option<Self>,
        _self_res: &Arc<Resource>,
        other_val: &Self,
        _other_res: &Arc<Resource>,
    ) -> Self {
        *other_val
    }

    fn aggregate_many<'a>(
        _self_res: &Arc<Resource>,
        mut iter: impl Iterator<Item = (&'a Arc<Resource>, Self)>,
    ) -> Option<Self> {
        iter.next().map(|(_, val)| val)
    }
}

pub(crate) type LocalSubscribers = LocalResources<SubscriberId, Arc<Resource>, SubscriberInfo>;
