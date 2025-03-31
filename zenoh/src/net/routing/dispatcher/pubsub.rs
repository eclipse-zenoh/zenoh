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

#[zenoh_macros::unstable]
use std::collections::HashMap;
use std::sync::Arc;

use zenoh_core::zread;
use zenoh_protocol::{
    core::{Reliability, WireExpr},
    network::{declare::SubscriberId, push::ext, Push},
    zenoh::PushBody,
};

use super::{
    face::FaceState,
    resource::{Direction, Resource},
    tables::{NodeId, Route, RoutingExpr, Tables, TablesLock},
};
#[zenoh_macros::unstable]
use crate::key_expr::KeyExpr;
use crate::net::routing::{
    hat::{HatTrait, SendDeclare},
    router::get_or_set_route,
};

#[derive(Copy, Clone)]
pub(crate) struct SubscriberInfo;

#[allow(clippy::too_many_arguments)]
pub(crate) fn declare_subscription(
    hat_code: &(dyn HatTrait + Send + Sync),
    tables: &TablesLock,
    face: &mut Arc<FaceState>,
    id: SubscriberId,
    expr: &WireExpr,
    sub_info: &SubscriberInfo,
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
                "{} Declare subscriber {} ({}{})",
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
                    drop(rtables);
                    let mut wtables = zwrite!(tables.tables);
                    let res =
                        Resource::make_resource(&mut wtables, &mut prefix, expr.suffix.as_ref());
                    (res, wtables)
                };

            hat_code.declare_subscription(
                &mut wtables,
                face,
                id,
                &mut res,
                sub_info,
                node_id,
                send_declare,
            );

            disable_matches_data_routes(&mut wtables, &mut res);
            drop(wtables);
        }
        None => tracing::error!(
            "{} Declare subscriber {} for unknown scope {}!",
            face,
            id,
            expr.scope
        ),
    }
}

pub(crate) fn undeclare_subscription(
    hat_code: &(dyn HatTrait + Send + Sync),
    tables: &TablesLock,
    face: &mut Arc<FaceState>,
    id: SubscriberId,
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
                        "{} Undeclare unknown subscriber {}{}!",
                        face,
                        prefix.expr(),
                        expr.suffix
                    );
                    return;
                }
            },
            None => {
                tracing::error!(
                    "{} Undeclare subscriber with unknown scope {}",
                    face,
                    expr.scope
                );
                return;
            }
        }
    };
    let mut wtables = zwrite!(tables.tables);
    if let Some(mut res) =
        hat_code.undeclare_subscription(&mut wtables, face, id, res, node_id, send_declare)
    {
        tracing::debug!("{} Undeclare subscriber {} ({})", face, id, res.expr());
        disable_matches_data_routes(&mut wtables, &mut res);
        Resource::clean(&mut res);
        drop(wtables);
    } else {
        // NOTE: This is expected behavior if subscriber declarations are denied with ingress ACL interceptor.
        tracing::debug!("{} Undeclare unknown subscriber {}", face, id);
    }
}

pub(crate) fn disable_matches_data_routes(tables: &mut Tables, res: &mut Arc<Resource>) {
    if res.context.is_some() {
        if let Some(key_expr) = res.key_expr() {
            Resource::iter_matches(&tables.root_res, key_expr, |m| {
                unsafe { &mut *Arc::as_ptr(m).cast_mut() }
                    .context_mut()
                    .disable_data_routes()
            })
        }
    }
}

macro_rules! treat_timestamp {
    ($hlc:expr, $payload:expr, $drop:expr) => {
        // if an HLC was configured (via Config.add_timestamp),
        // check DataInfo and add a timestamp if there isn't
        if let Some(hlc) = $hlc {
            if let PushBody::Put(data) = &mut $payload {
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
fn get_data_route(
    tables: &Tables,
    face: &FaceState,
    res: &Option<Arc<Resource>>,
    expr: &mut RoutingExpr,
    routing_context: NodeId,
) -> Arc<Route> {
    let hat = &tables.hat_code;
    let local_context = hat.map_routing_context(tables, face, routing_context);
    let mut compute_route = || hat.compute_data_route(tables, expr, local_context, face.whatami);
    if let Some(data_routes) = res
        .as_ref()
        .and_then(|res| res.context.as_ref())
        .map(|ctx| &ctx.data_routes)
    {
        return get_or_set_route(
            data_routes,
            tables.routes_version,
            face.whatami,
            local_context,
            compute_route,
        );
    }
    compute_route()
}

#[zenoh_macros::unstable]
#[inline]
pub(crate) fn get_matching_subscriptions(
    tables: &Tables,
    key_expr: &KeyExpr<'_>,
) -> HashMap<usize, Arc<FaceState>> {
    tables.hat_code.get_matching_subscriptions(tables, key_expr)
}

#[cfg(feature = "stats")]
macro_rules! inc_stats {
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
                    PushBody::Put(p) => {
                        stats.[<$txrx _z_put_msgs>].[<inc_ $space>](1);
                        let mut n =  p.payload.len();
                        if let Some(a) = p.ext_attachment.as_ref() {
                           n += a.buffer.len();
                        }
                        stats.[<$txrx _z_put_pl_bytes>].[<inc_ $space>](n);
                    }
                    PushBody::Del(d) => {
                        stats.[<$txrx _z_del_msgs>].[<inc_ $space>](1);
                        let mut n = 0;
                        if let Some(a) = d.ext_attachment.as_ref() {
                           n += a.buffer.len();
                        }
                        stats.[<$txrx _z_del_pl_bytes>].[<inc_ $space>](n);
                    }
                }
            }
        }
    };
}

// having all the arguments instead of an intermediate struct seems to enable a better inlining
// see https://github.com/eclipse-zenoh/zenoh/pull/1713#issuecomment-2590130026
#[allow(clippy::too_many_arguments)]
pub fn route_data(
    tables_ref: &Arc<TablesLock>,
    face: &FaceState,
    wire_expr: WireExpr,
    ext_qos: ext::QoSType,
    ext_tstamp: Option<ext::TimestampType>,
    ext_nodeid: ext::NodeIdType,
    payload: impl FnOnce() -> PushBody,
    reliability: Reliability,
) {
    let tables = zread!(tables_ref.tables);
    match tables
        .get_mapping(face, &wire_expr.scope, wire_expr.mapping)
        .cloned()
    {
        Some(prefix) => {
            tracing::trace!(
                "{} Route data for res {}{}",
                face,
                prefix.expr(),
                wire_expr.suffix.as_ref()
            );
            let mut expr = RoutingExpr::new(&prefix, wire_expr.suffix.as_ref());

            #[cfg(feature = "stats")]
            let admin = expr.full_expr().starts_with("@/");
            #[cfg(feature = "stats")]
            let mut payload = payload();
            #[cfg(feature = "stats")]
            if !admin {
                inc_stats!(face, rx, user, payload);
            } else {
                inc_stats!(face, rx, admin, payload);
            }

            if tables.hat_code.ingress_filter(&tables, face, &mut expr) {
                let res = Resource::get_resource(&prefix, expr.suffix);

                let route = get_data_route(&tables, face, &res, &mut expr, ext_nodeid.node_id);

                if !route.is_empty() {
                    #[cfg(not(feature = "stats"))]
                    let mut payload = payload();
                    treat_timestamp!(&tables.hlc, payload, tables.drop_future_timestamp);

                    if route.len() == 1 {
                        let (outface, key_expr, context) = route.values().next().unwrap();
                        if tables
                            .hat_code
                            .egress_filter(&tables, face, outface, &mut expr)
                        {
                            drop(tables);
                            #[cfg(feature = "stats")]
                            if !admin {
                                inc_stats!(outface, tx, user, payload);
                            } else {
                                inc_stats!(outface, tx, admin, payload);
                            }

                            outface.primitives.send_push(
                                Push {
                                    wire_expr: key_expr.into(),
                                    ext_qos,
                                    ext_tstamp,
                                    ext_nodeid: ext::NodeIdType { node_id: *context },
                                    payload,
                                },
                                reliability,
                            )
                        }
                    } else {
                        let route = route
                            .values()
                            .filter(|(outface, _key_expr, _context)| {
                                tables
                                    .hat_code
                                    .egress_filter(&tables, face, outface, &mut expr)
                            })
                            .cloned()
                            .collect::<Vec<Direction>>();

                        drop(tables);
                        for (outface, key_expr, context) in route {
                            #[cfg(feature = "stats")]
                            if !admin {
                                inc_stats!(outface, tx, user, payload)
                            } else {
                                inc_stats!(outface, tx, admin, payload)
                            }

                            outface.primitives.send_push(
                                Push {
                                    wire_expr: key_expr,
                                    ext_qos,
                                    ext_tstamp: None,
                                    ext_nodeid: ext::NodeIdType { node_id: context },
                                    payload: payload.clone(),
                                },
                                reliability,
                            )
                        }
                    }
                }
            }
        }
        None => {
            tracing::error!(
                "{} Route data with unknown scope {}!",
                face,
                wire_expr.scope
            );
        }
    }
}
