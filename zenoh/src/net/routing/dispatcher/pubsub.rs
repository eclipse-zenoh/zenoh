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
use std::{collections::HashMap, sync::Arc};

use zenoh_core::zread;
use zenoh_protocol::{
    core::{key_expr::keyexpr, Reliability, WhatAmI, WireExpr},
    network::{
        declare::{ext, SubscriberId},
        Push,
    },
    zenoh::PushBody,
};
use zenoh_sync::get_mut_unchecked;

use super::{
    face::FaceState,
    resource::{DataRoutes, Direction, Resource},
    tables::{NodeId, Route, RoutingExpr, Tables, TablesLock},
};
#[zenoh_macros::unstable]
use crate::key_expr::KeyExpr;
use crate::net::routing::hat::{HatTrait, SendDeclare};

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

            let rtables = zread!(tables.tables);
            let matches_data_routes = compute_matches_data_routes(&rtables, &res);
            drop(rtables);

            let wtables = zwrite!(tables.tables);
            for (mut res, data_routes) in matches_data_routes {
                get_mut_unchecked(&mut res)
                    .context_mut()
                    .update_data_routes(data_routes);
            }
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
        drop(wtables);

        let rtables = zread!(tables.tables);
        let matches_data_routes = compute_matches_data_routes(&rtables, &res);
        drop(rtables);

        let wtables = zwrite!(tables.tables);
        for (mut res, data_routes) in matches_data_routes {
            get_mut_unchecked(&mut res)
                .context_mut()
                .update_data_routes(data_routes);
        }
        Resource::clean(&mut res);
        drop(wtables);
    } else {
        // NOTE: This is expected behavior if subscriber declarations are denied with ingress ACL interceptor.
        tracing::debug!("{} Undeclare unknown subscriber {}", face, id);
    }
}

fn compute_data_routes_(tables: &Tables, routes: &mut DataRoutes, expr: &mut RoutingExpr) {
    let indexes = tables.hat_code.get_data_routes_entries(tables);

    let max_idx = indexes.routers.iter().max().unwrap();
    routes
        .routers
        .resize_with((*max_idx as usize) + 1, || Arc::new(HashMap::new()));

    for idx in indexes.routers {
        routes.routers[idx as usize] =
            tables
                .hat_code
                .compute_data_route(tables, expr, idx, WhatAmI::Router);
    }

    let max_idx = indexes.peers.iter().max().unwrap();
    routes
        .peers
        .resize_with((*max_idx as usize) + 1, || Arc::new(HashMap::new()));

    for idx in indexes.peers {
        routes.peers[idx as usize] =
            tables
                .hat_code
                .compute_data_route(tables, expr, idx, WhatAmI::Peer);
    }

    let max_idx = indexes.clients.iter().max().unwrap();
    routes
        .clients
        .resize_with((*max_idx as usize) + 1, || Arc::new(HashMap::new()));

    for idx in indexes.clients {
        routes.clients[idx as usize] =
            tables
                .hat_code
                .compute_data_route(tables, expr, idx, WhatAmI::Client);
    }
}

pub(crate) fn compute_data_routes(tables: &Tables, expr: &mut RoutingExpr) -> DataRoutes {
    let mut routes = DataRoutes::default();
    compute_data_routes_(tables, &mut routes, expr);
    routes
}

pub(crate) fn update_data_routes(tables: &Tables, res: &mut Arc<Resource>) {
    if res.context.is_some() {
        let mut res_mut = res.clone();
        let res_mut = get_mut_unchecked(&mut res_mut);
        compute_data_routes_(
            tables,
            &mut res_mut.context_mut().data_routes,
            &mut RoutingExpr::new(res, ""),
        );
    }
}

pub(crate) fn update_data_routes_from(tables: &mut Tables, res: &mut Arc<Resource>) {
    update_data_routes(tables, res);
    let res = get_mut_unchecked(res);
    for child in res.children.values_mut() {
        update_data_routes_from(tables, child);
    }
}

pub(crate) fn compute_matches_data_routes<'a>(
    tables: &'a Tables,
    res: &'a Arc<Resource>,
) -> Vec<(Arc<Resource>, DataRoutes)> {
    let mut routes = vec![];
    if res.context.is_some() {
        let mut expr = RoutingExpr::new(res, "");
        routes.push((res.clone(), compute_data_routes(tables, &mut expr)));
        for match_ in &res.context().matches {
            let match_ = match_.upgrade().unwrap();
            if !Arc::ptr_eq(&match_, res) {
                let mut expr = RoutingExpr::new(&match_, "");
                let match_routes = compute_data_routes(tables, &mut expr);
                routes.push((match_, match_routes));
            }
        }
    }
    routes
}

pub(crate) fn update_matches_data_routes<'a>(tables: &'a mut Tables, res: &'a mut Arc<Resource>) {
    if res.context.is_some() {
        update_data_routes(tables, res);
        for match_ in &res.context().matches {
            let mut match_ = match_.upgrade().unwrap();
            if !Arc::ptr_eq(&match_, res) {
                update_data_routes(tables, &mut match_);
            }
        }
    }
}

pub(crate) fn disable_matches_data_routes(_tables: &mut Tables, res: &mut Arc<Resource>) {
    if res.context.is_some() {
        get_mut_unchecked(res).context_mut().disable_data_routes();
        for match_ in &res.context().matches {
            let mut match_ = match_.upgrade().unwrap();
            if !Arc::ptr_eq(&match_, res) {
                get_mut_unchecked(&mut match_)
                    .context_mut()
                    .disable_data_routes();
            }
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
    let local_context = tables
        .hat_code
        .map_routing_context(tables, face, routing_context);
    res.as_ref()
        .and_then(|res| res.data_route(face.whatami, local_context))
        .unwrap_or_else(|| {
            tables
                .hat_code
                .compute_data_route(tables, expr, local_context, face.whatami)
        })
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

pub fn route_data(
    tables_ref: &Arc<TablesLock>,
    face: &FaceState,
    mut msg: Push,
    reliability: Reliability,
) {
    let tables = zread!(tables_ref.tables);
    match tables
        .get_mapping(face, &msg.wire_expr.scope, msg.wire_expr.mapping)
        .cloned()
    {
        Some(prefix) => {
            tracing::trace!(
                "{} Route data for res {}{}",
                face,
                prefix.expr(),
                msg.wire_expr.suffix.as_ref()
            );
            let mut expr = RoutingExpr::new(&prefix, msg.wire_expr.suffix.as_ref());

            #[cfg(feature = "stats")]
            let admin = expr.full_expr().starts_with("@/");
            #[cfg(feature = "stats")]
            if !admin {
                inc_stats!(face, rx, user, msg.payload)
            } else {
                inc_stats!(face, rx, admin, msg.payload)
            }

            if tables.hat_code.ingress_filter(&tables, face, &mut expr) {
                let res = Resource::get_resource(&prefix, expr.suffix);

                let route = get_data_route(&tables, face, &res, &mut expr, msg.ext_nodeid.node_id);

                if !route.is_empty() {
                    treat_timestamp!(&tables.hlc, msg.payload, tables.drop_future_timestamp);

                    if route.len() == 1 {
                        let (outface, key_expr, context) = route.values().next().unwrap();
                        if tables
                            .hat_code
                            .egress_filter(&tables, face, outface, &mut expr)
                        {
                            drop(tables);
                            #[cfg(feature = "stats")]
                            if !admin {
                                inc_stats!(outface, tx, user, msg.payload)
                            } else {
                                inc_stats!(outface, tx, admin, msg.payload)
                            }

                            outface.primitives.send_push(
                                Push {
                                    wire_expr: key_expr.into(),
                                    ext_qos: msg.ext_qos,
                                    ext_tstamp: msg.ext_tstamp,
                                    ext_nodeid: ext::NodeIdType { node_id: *context },
                                    payload: msg.payload,
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
                                inc_stats!(outface, tx, user, msg.payload)
                            } else {
                                inc_stats!(outface, tx, admin, msg.payload)
                            }

                            outface.primitives.send_push(
                                Push {
                                    wire_expr: key_expr,
                                    ext_qos: msg.ext_qos,
                                    ext_tstamp: None,
                                    ext_nodeid: ext::NodeIdType { node_id: context },
                                    payload: msg.payload.clone(),
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
                msg.wire_expr.scope
            );
        }
    }
}
