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
    collections::{HashMap, HashSet},
    sync::Arc,
};

use zenoh_core::zread;
use zenoh_protocol::{
    core::{key_expr::keyexpr, Reliability, WireExpr},
    network::{declare::SubscriberId, interest::InterestId, push::ext, Push},
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
        dispatcher::tables::RoutingExpr,
        hat::{HatTrait, SendDeclare},
        router::get_or_set_route,
    },
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

pub(crate) struct SubscriberResourceData {
    pub(crate) id: SubscriberId,
    pub(crate) aggregated_to: HashSet<Arc<Resource>>,
    pub(crate) interest_ids: HashSet<InterestId>,
}

pub(crate) struct AggregatedSubscriberResourceData {
    pub(crate) id: SubscriberId,
    pub(crate) aggregates: HashSet<Arc<Resource>>,
    pub(crate) interest_ids: HashSet<InterestId>,
}

pub(crate) struct LocalSubscribers {
    subs: HashMap<Arc<Resource>, SubscriberResourceData>,
    aggregate_to_subs: HashMap<Arc<Resource>, AggregatedSubscriberResourceData>,
}

impl LocalSubscribers {
    pub(crate) fn new() -> Self {
        LocalSubscribers {
            subs: HashMap::new(),
            aggregate_to_subs: HashMap::new(),
        }
    }

    pub(crate) fn contains_key(&self, key: &Arc<Resource>) -> bool {
        self.subs.contains_key(key)
    }

    pub(crate) fn keys(&self) -> impl Iterator<Item = &Arc<Resource>> {
        self.subs.keys()
    }

    pub(crate) fn insert<F>(
        &mut self,
        key: Arc<Resource>,
        f_id: F,
        interests: HashSet<InterestId>,
    ) -> SubscriberId
    where
        F: FnOnce() -> SubscriberId,
    {
        match self.subs.entry(key.clone()) {
            std::collections::hash_map::Entry::Occupied(mut occupied_entry) => {
                occupied_entry.get_mut().interest_ids.extend(interests);
                occupied_entry.get().id
            }
            std::collections::hash_map::Entry::Vacant(vacant_entry) => {
                let mut aggregated_to = HashSet::new();
                for (a, subs) in &mut self.aggregate_to_subs {
                    if key.matches(a) {
                        subs.aggregates.insert(key.clone());
                        aggregated_to.insert(a.clone());
                    }
                }
                let id = self.aggregate_to_subs.get(&key).map_or_else(f_id, |r| r.id);
                vacant_entry.insert(SubscriberResourceData {
                    id,
                    aggregated_to,
                    interest_ids: interests,
                });
                id
            }
        }
    }

    pub(crate) fn insert_aggregate<F>(
        &mut self,
        key: Arc<Resource>,
        f_id: F,
        interests: HashSet<InterestId>,
    ) -> SubscriberId
    where
        F: FnOnce() -> SubscriberId,
    {
        match self.aggregate_to_subs.entry(key.clone()) {
            std::collections::hash_map::Entry::Occupied(mut occupied_entry) => {
                occupied_entry.get_mut().interest_ids.extend(interests);
                occupied_entry.get().id
            }
            std::collections::hash_map::Entry::Vacant(vacant_entry) => {
                let mut aggregates = HashSet::new();
                for (s, data) in &mut self.subs {
                    if s.matches(&key) {
                        data.aggregated_to.insert(key.clone());
                        aggregates.insert(s.clone());
                    }
                }
                let id = self.subs.get(&key).map_or_else(f_id, |r| r.id);
                vacant_entry.insert(AggregatedSubscriberResourceData {
                    id,
                    aggregates,
                    interest_ids: interests,
                });
                id
            }
        }
    }

    // Returns SubscribersId of removed subscribers (there can be more than one due to aggregation) for which an interest was declared
    pub(crate) fn remove(&mut self, key: &Arc<Resource>) -> HashMap<SubscriberId, Arc<Resource>> {
        let mut out = HashMap::new();
        if let Some(s) = self.subs.remove(key) {
            if !s.interest_ids.is_empty() {
                // there was an interest for this specific resource
                out.insert(s.id, key.clone());
            }
            if !s.aggregated_to.is_empty() {
                // resource is aggregated - return ids of all its aggregates that no longer point to any real resource
                for a in &s.aggregated_to {
                    let entry = self.aggregate_to_subs.get_mut(a).unwrap();
                    entry.aggregates.remove(key);
                    if entry.aggregates.is_empty() {
                        out.insert(entry.id, a.clone());
                    }
                }
            }
        }
        out
    }

    pub(crate) fn remove_key_interest(
        &mut self,
        key: &Option<Arc<Resource>>,
        interest: InterestId,
    ) {
        let mut subs_to_remove = Vec::new();
        for res in self.subs.keys() {
            if key.as_ref().map(|k| k.matches(res)).unwrap_or(true) {
                subs_to_remove.push(res.clone());
            }
        }

        for sub in subs_to_remove {
            if let std::collections::hash_map::Entry::Occupied(mut occupied_entry) =
                self.subs.entry(sub)
            {
                if occupied_entry.get_mut().interest_ids.remove(&interest) {
                    if occupied_entry.get().interest_ids.is_empty()
                        && occupied_entry.get().aggregated_to.is_empty()
                    {
                        occupied_entry.remove();
                    }
                }
            }
        }
    }

    pub(crate) fn remove_aggregate_interest(
        &mut self,
        key: &Arc<Resource>,
        interest: InterestId,
    ) -> bool {
        match self.aggregate_to_subs.entry(key.clone()) {
            std::collections::hash_map::Entry::Occupied(mut occupied_entry) => {
                if occupied_entry.get_mut().interest_ids.remove(&interest) {
                    if occupied_entry.get_mut().interest_ids.is_empty() {
                        // the aggregate can be removed if there is no other interest for it
                        let subs = occupied_entry.remove().aggregates;
                        for s in subs {
                            if let std::collections::hash_map::Entry::Occupied(mut e) =
                                self.subs.entry(s)
                            {
                                e.get_mut().aggregated_to.remove(key);
                                if e.get().interest_ids.is_empty()
                                    && e.get().aggregated_to.is_empty()
                                {
                                    // remove simple resource if there is no interest for it, nor it is aggregated into another one
                                    e.remove();
                                }
                            }
                        }
                    }
                    true
                } else {
                    false
                }
            }
            std::collections::hash_map::Entry::Vacant(_) => false,
        }
    }

    pub(crate) fn get_aggregate(
        &self,
        res: &Arc<Resource>,
    ) -> Option<&AggregatedSubscriberResourceData> {
        self.aggregate_to_subs.get(res)
    }

    pub(crate) fn get_aggregate_mut(
        &mut self,
        res: &Arc<Resource>,
    ) -> Option<&mut AggregatedSubscriberResourceData> {
        self.aggregate_to_subs.get_mut(res)
    }

    pub(crate) fn get_resource(&self, res: &Arc<Resource>) -> Option<&SubscriberResourceData> {
        self.subs.get(res)
    }

    pub(crate) fn get_resource_mut(
        &mut self,
        res: &Arc<Resource>,
    ) -> Option<&mut SubscriberResourceData> {
        self.subs.get_mut(res)
    }

    pub(crate) fn clear(&mut self) {
        self.subs.clear();
        self.aggregate_to_subs.clear();
    }
}
