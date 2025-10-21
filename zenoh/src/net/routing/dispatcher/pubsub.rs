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
    core::{key_expr::keyexpr, Reliability, WireExpr},
    network::{declare::SubscriberId, push::ext, Push},
    zenoh::PushBody,
};
use zenoh_sync::get_mut_unchecked;

use super::{
    face::FaceState,
    resource::{Direction, Resource},
    tables::{NodeId, Route, Tables, TablesLock},
};
use crate::{
    key_expr::KeyExpr,
    net::routing::{
        dispatcher::{
            local_resources::{LocalResourceInfoTrait, LocalResources},
            tables::RoutingExpr,
        },
        hat::{HatTrait, SendDeclare},
        router::get_or_set_route,
    },
};

#[derive(Copy, Clone, Default, PartialEq, Eq)]
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
                    let mut res = Resource::make_resource(
                        hat_code,
                        &mut wtables,
                        &mut prefix,
                        expr.suffix.as_ref(),
                    );
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
    hat_code: &(dyn HatTrait + Send + Sync),
    tables: &Tables,
    face: &FaceState,
    expr: &RoutingExpr,
    routing_context: NodeId,
) -> Arc<Route> {
    let local_context = hat_code.map_routing_context(tables, face, routing_context);
    let compute_route = || hat_code.compute_data_route(tables, expr, local_context, face.whatami);
    if let Some(data_routes) = expr
        .resource()
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

#[inline]
pub(crate) fn get_matching_subscriptions(
    hat_code: &(dyn HatTrait + Send + Sync),
    tables: &Tables,
    key_expr: &KeyExpr<'_>,
) -> HashMap<usize, Arc<FaceState>> {
    hat_code.get_matching_subscriptions(tables, key_expr)
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
    msg: &mut Push,
    reliability: Reliability,
) {
    let tables = zread!(tables_ref.tables);
    match tables.get_mapping(face, &msg.wire_expr.scope, msg.wire_expr.mapping) {
        Some(prefix) => {
            tracing::trace!(
                "{} Route data for res {}{}",
                face,
                prefix.expr(),
                msg.wire_expr.suffix.as_ref()
            );
            let expr = RoutingExpr::new(prefix, msg.wire_expr.suffix.as_ref());

            #[cfg(feature = "stats")]
            let admin = expr.key_expr().is_some_and(|ke| ke.starts_with("@/"));
            #[cfg(feature = "stats")]
            if !admin {
                inc_stats!(face, rx, user, msg.payload);
            } else {
                inc_stats!(face, rx, admin, msg.payload);
            }

            if tables_ref.hat_code.ingress_filter(&tables, face, &expr) {
                let route = get_data_route(
                    tables_ref.hat_code.as_ref(),
                    &tables,
                    face,
                    &expr,
                    msg.ext_nodeid.node_id,
                );

                if !route.is_empty() {
                    treat_timestamp!(&tables.hlc, msg.payload, tables.drop_future_timestamp);

                    if route.len() == 1 {
                        let (outface, key_expr, context) = route.iter().next().unwrap();
                        if tables_ref
                            .hat_code
                            .egress_filter(&tables, face, outface, &expr)
                        {
                            drop(tables);
                            #[cfg(feature = "stats")]
                            if !admin {
                                inc_stats!(outface, tx, user, msg.payload);
                            } else {
                                inc_stats!(outface, tx, admin, msg.payload);
                            }
                            msg.wire_expr = key_expr.into();
                            msg.ext_nodeid = ext::NodeIdType { node_id: *context };
                            outface.primitives.send_push(msg, reliability);
                            // Reset the wire_expr to indicate the message has been consumed
                            msg.wire_expr = WireExpr::empty();
                        }
                    } else {
                        let route = route
                            .iter()
                            .filter(|(outface, _key_expr, _context)| {
                                tables_ref
                                    .hat_code
                                    .egress_filter(&tables, face, outface, &expr)
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
                                &mut Push {
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
