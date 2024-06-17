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
use super::resource::{DataRoutes, Direction, PullCaches, Resource};
use super::tables::{NodeId, Route, RoutingExpr, Tables, TablesLock};
use crate::net::routing::hat::{HatTrait, SendDeclare};
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;
use zenoh_core::zread;
use zenoh_protocol::core::key_expr::{keyexpr, OwnedKeyExpr};
use zenoh_protocol::network::declare::subscriber::ext::SubscriberInfo;
use zenoh_protocol::network::declare::Mode;
use zenoh_protocol::{
    core::{WhatAmI, WireExpr},
    network::{declare::ext, Push},
    zenoh::PushBody,
};
use zenoh_sync::get_mut_unchecked;

pub(crate) fn declare_subscription(
    hat_code: &(dyn HatTrait + Send + Sync),
    tables: &TablesLock,
    face: &mut Arc<FaceState>,
    expr: &WireExpr,
    sub_info: &SubscriberInfo,
    node_id: NodeId,
    send_declare: &mut SendDeclare,
) {
    tracing::debug!("Declare subscription {}", face);
    let rtables = zread!(tables.tables);
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
            for (mut res, data_routes, matching_pulls) in matches_data_routes {
                get_mut_unchecked(&mut res)
                    .context_mut()
                    .update_data_routes(data_routes);
                get_mut_unchecked(&mut res)
                    .context_mut()
                    .update_matching_pulls(matching_pulls);
            }
            drop(wtables);
        }
        None => tracing::error!("Declare subscription for unknown scope {}!", expr.scope),
    }
}

pub(crate) fn undeclare_subscription(
    hat_code: &(dyn HatTrait + Send + Sync),
    tables: &TablesLock,
    face: &mut Arc<FaceState>,
    expr: &WireExpr,
    node_id: NodeId,
    send_declare: &mut SendDeclare,
) {
    tracing::debug!("Undeclare subscription {}", face);
    let rtables = zread!(tables.tables);
    match rtables.get_mapping(face, &expr.scope, expr.mapping) {
        Some(prefix) => match Resource::get_resource(prefix, expr.suffix.as_ref()) {
            Some(mut res) => {
                drop(rtables);
                let mut wtables = zwrite!(tables.tables);

                hat_code.undeclare_subscription(
                    &mut wtables,
                    face,
                    &mut res,
                    node_id,
                    send_declare,
                );

                disable_matches_data_routes(&mut wtables, &mut res);
                drop(wtables);

                let rtables = zread!(tables.tables);
                let matches_data_routes = compute_matches_data_routes(&rtables, &res);
                drop(rtables);

                let wtables = zwrite!(tables.tables);
                for (mut res, data_routes, matching_pulls) in matches_data_routes {
                    get_mut_unchecked(&mut res)
                        .context_mut()
                        .update_data_routes(data_routes);
                    get_mut_unchecked(&mut res)
                        .context_mut()
                        .update_matching_pulls(matching_pulls);
                }
                Resource::clean(&mut res);
                drop(wtables);
            }
            None => tracing::error!("Undeclare unknown subscription!"),
        },
        None => tracing::error!("Undeclare subscription with unknown scope!"),
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
    update_matching_pulls(tables, res);
    let res = get_mut_unchecked(res);
    for child in res.children.values_mut() {
        update_data_routes_from(tables, child);
    }
}

pub(crate) fn compute_matches_data_routes<'a>(
    tables: &'a Tables,
    res: &'a Arc<Resource>,
) -> Vec<(Arc<Resource>, DataRoutes, Arc<PullCaches>)> {
    let mut routes = vec![];
    if res.context.is_some() {
        let mut expr = RoutingExpr::new(res, "");
        routes.push((
            res.clone(),
            compute_data_routes(tables, &mut expr),
            compute_matching_pulls(tables, &mut expr),
        ));
        for match_ in &res.context().matches {
            let match_ = match_.upgrade().unwrap();
            if !Arc::ptr_eq(&match_, res) {
                let mut expr = RoutingExpr::new(&match_, "");
                let match_routes = compute_data_routes(tables, &mut expr);
                let matching_pulls = compute_matching_pulls(tables, &mut expr);
                routes.push((match_, match_routes, matching_pulls));
            }
        }
    }
    routes
}

pub(crate) fn update_matches_data_routes<'a>(tables: &'a mut Tables, res: &'a mut Arc<Resource>) {
    if res.context.is_some() {
        update_data_routes(tables, res);
        update_matching_pulls(tables, res);
        for match_ in &res.context().matches {
            let mut match_ = match_.upgrade().unwrap();
            if !Arc::ptr_eq(&match_, res) {
                update_data_routes(tables, &mut match_);
                update_matching_pulls(tables, &mut match_);
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
                get_mut_unchecked(&mut match_)
                    .context_mut()
                    .disable_matching_pulls();
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
pub(crate) fn get_local_data_route(
    tables: &Tables,
    res: &Option<Arc<Resource>>,
    expr: &mut RoutingExpr,
) -> Arc<Route> {
    res.as_ref()
        .and_then(|res| res.data_route(WhatAmI::Client, 0))
        .unwrap_or_else(|| {
            tables
                .hat_code
                .compute_data_route(tables, expr, 0, WhatAmI::Client)
        })
}

fn compute_matching_pulls_(tables: &Tables, pull_caches: &mut PullCaches, expr: &mut RoutingExpr) {
    let ke = if let Ok(ke) = OwnedKeyExpr::try_from(expr.full_expr()) {
        ke
    } else {
        return;
    };
    let res = Resource::get_resource(expr.prefix, expr.suffix);
    let matches = res
        .as_ref()
        .and_then(|res| res.context.as_ref())
        .map(|ctx| Cow::from(&ctx.matches))
        .unwrap_or_else(|| Cow::from(Resource::get_matches(tables, &ke)));

    for mres in matches.iter() {
        let mres = mres.upgrade().unwrap();
        for context in mres.session_ctxs.values() {
            if let Some(subinfo) = &context.subs {
                if subinfo.mode == Mode::Pull {
                    pull_caches.push(context.clone());
                }
            }
        }
    }
}

pub(crate) fn compute_matching_pulls(tables: &Tables, expr: &mut RoutingExpr) -> Arc<PullCaches> {
    let mut pull_caches = PullCaches::default();
    compute_matching_pulls_(tables, &mut pull_caches, expr);
    Arc::new(pull_caches)
}

pub(crate) fn update_matching_pulls(tables: &Tables, res: &mut Arc<Resource>) {
    if res.context.is_some() {
        let mut res_mut = res.clone();
        let res_mut = get_mut_unchecked(&mut res_mut);
        if res_mut.context_mut().matching_pulls.is_none() {
            res_mut.context_mut().matching_pulls = Some(Arc::new(PullCaches::default()));
        }
        compute_matching_pulls_(
            tables,
            get_mut_unchecked(res_mut.context_mut().matching_pulls.as_mut().unwrap()),
            &mut RoutingExpr::new(res, ""),
        );
    }
}

#[inline]
fn get_matching_pulls(
    tables: &Tables,
    res: &Option<Arc<Resource>>,
    expr: &mut RoutingExpr,
) -> Arc<PullCaches> {
    res.as_ref()
        .and_then(|res| res.context.as_ref())
        .and_then(|ctx| ctx.matching_pulls.clone())
        .unwrap_or_else(|| compute_matching_pulls(tables, expr))
}

macro_rules! cache_data {
    (
        $matching_pulls:expr,
        $expr:expr,
        $payload:expr
    ) => {
        for context in $matching_pulls.iter() {
            get_mut_unchecked(&mut context.clone())
                .last_values
                .insert($expr.full_expr().to_string(), $payload.clone());
        }
    };
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
                        stats.[<$txrx _z_put_pl_bytes>].[<inc_ $space>](p.payload.len());
                    }
                    PushBody::Del(_) => {
                        stats.[<$txrx _z_del_msgs>].[<inc_ $space>](1);
                    }
                }
            }
        }
    };
}

pub fn full_reentrant_route_data(
    tables_ref: &Arc<TablesLock>,
    face: &FaceState,
    expr: &WireExpr,
    ext_qos: ext::QoSType,
    ext_tstamp: Option<ext::TimestampType>,
    mut payload: PushBody,
    routing_context: NodeId,
) {
    let tables = zread!(tables_ref.tables);
    match tables.get_mapping(face, &expr.scope, expr.mapping).cloned() {
        Some(prefix) => {
            tracing::trace!(
                "Route data for res {}{}",
                prefix.expr(),
                expr.suffix.as_ref()
            );
            let mut expr = RoutingExpr::new(&prefix, expr.suffix.as_ref());

            #[cfg(feature = "stats")]
            let admin = expr.full_expr().starts_with("@/");
            #[cfg(feature = "stats")]
            if !admin {
                inc_stats!(face, rx, user, payload)
            } else {
                inc_stats!(face, rx, admin, payload)
            }

            if tables.hat_code.ingress_filter(&tables, face, &mut expr) {
                let res = Resource::get_resource(&prefix, expr.suffix);

                let route = get_data_route(&tables, face, &res, &mut expr, routing_context);

                let matching_pulls = get_matching_pulls(&tables, &res, &mut expr);

                if !(route.is_empty() && matching_pulls.is_empty()) {
                    treat_timestamp!(&tables.hlc, payload, tables.drop_future_timestamp);

                    if route.len() == 1 && matching_pulls.len() == 0 {
                        let (outface, key_expr, context) = route.values().next().unwrap();
                        if tables
                            .hat_code
                            .egress_filter(&tables, face, outface, &mut expr)
                        {
                            drop(tables);
                            #[cfg(feature = "stats")]
                            if !admin {
                                inc_stats!(face, tx, user, payload)
                            } else {
                                inc_stats!(face, tx, admin, payload)
                            }

                            outface.primitives.send_push(Push {
                                wire_expr: key_expr.into(),
                                ext_qos,
                                ext_tstamp,
                                ext_nodeid: ext::NodeIdType { node_id: *context },
                                payload,
                            })
                        }
                    } else {
                        if !matching_pulls.is_empty() {
                            let lock = zlock!(tables.pull_caches_lock);
                            cache_data!(matching_pulls, expr, payload);
                            drop(lock);
                        }

                        if tables.whatami == WhatAmI::Router {
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
                                    inc_stats!(face, tx, user, payload)
                                } else {
                                    inc_stats!(face, tx, admin, payload)
                                }

                                outface.primitives.send_push(Push {
                                    wire_expr: key_expr,
                                    ext_qos,
                                    ext_tstamp,
                                    ext_nodeid: ext::NodeIdType { node_id: context },
                                    payload: payload.clone(),
                                })
                            }
                        } else {
                            drop(tables);
                            for (outface, key_expr, context) in route.values() {
                                if face.id != outface.id
                                    && match (
                                        face.mcast_group.as_ref(),
                                        outface.mcast_group.as_ref(),
                                    ) {
                                        (Some(l), Some(r)) => l != r,
                                        _ => true,
                                    }
                                {
                                    #[cfg(feature = "stats")]
                                    if !admin {
                                        inc_stats!(face, tx, user, payload)
                                    } else {
                                        inc_stats!(face, tx, admin, payload)
                                    }

                                    outface.primitives.send_push(Push {
                                        wire_expr: key_expr.into(),
                                        ext_qos,
                                        ext_tstamp,
                                        ext_nodeid: ext::NodeIdType { node_id: *context },
                                        payload: payload.clone(),
                                    })
                                }
                            }
                        }
                    }
                }
            }
        }
        None => {
            tracing::error!("Route data with unknown scope {}!", expr.scope);
        }
    }
}

pub fn pull_data(tables_ref: &RwLock<Tables>, face: &Arc<FaceState>, expr: WireExpr) {
    let tables = zread!(tables_ref);
    match tables.get_mapping(face, &expr.scope, expr.mapping) {
        Some(prefix) => match Resource::get_resource(prefix, expr.suffix.as_ref()) {
            Some(mut res) => {
                let res = get_mut_unchecked(&mut res);
                match res.session_ctxs.get_mut(&face.id) {
                    Some(ctx) => match &ctx.subs {
                        Some(_subinfo) => {
                            // let reliability = subinfo.reliability;
                            let lock = zlock!(tables.pull_caches_lock);
                            let route = get_mut_unchecked(ctx)
                                .last_values
                                .drain()
                                .map(|(name, sample)| {
                                    (
                                        Resource::get_best_key(&tables.root_res, &name, face.id)
                                            .to_owned(),
                                        sample,
                                    )
                                })
                                .collect::<Vec<(WireExpr, PushBody)>>();
                            drop(lock);
                            drop(tables);
                            for (key_expr, payload) in route {
                                face.primitives.send_push(Push {
                                    wire_expr: key_expr,
                                    ext_qos: ext::QoSType::push_default(),
                                    ext_tstamp: None,
                                    ext_nodeid: ext::NodeIdType::default(),
                                    payload,
                                });
                            }
                        }
                        None => {
                            tracing::error!(
                                "Pull data for unknown subscription {} (no info)!",
                                prefix.expr() + expr.suffix.as_ref()
                            );
                        }
                    },
                    None => {
                        tracing::error!(
                            "Pull data for unknown subscription {} (no context)!",
                            prefix.expr() + expr.suffix.as_ref()
                        );
                    }
                }
            }
            None => {
                tracing::error!(
                    "Pull data for unknown subscription {} (no resource)!",
                    prefix.expr() + expr.suffix.as_ref()
                );
            }
        },
        None => {
            tracing::error!("Pull data with unknown scope {}!", expr.scope);
        }
    };
}
