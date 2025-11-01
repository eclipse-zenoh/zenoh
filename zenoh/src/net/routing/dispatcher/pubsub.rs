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
use zenoh_keyexpr::keyexpr;
use zenoh_protocol::{
    core::{Reliability, WireExpr},
    network::{declare::SubscriberId, push::ext, Push},
};
use zenoh_sync::get_mut_unchecked;

use super::{
    face::FaceState,
    resource::Resource,
    tables::{NodeId, Route, RoutingExpr, Tables, TablesLock},
};
use crate::{
    key_expr::KeyExpr,
    net::routing::{
        dispatcher::{
            face::Face,
            gateway::Bound,
            local_resources::{LocalResourceInfoTrait, LocalResources},
        },
        hat::{BaseContext, SendDeclare},
        router::{get_or_set_route, Direction, RouteBuilder},
    },
};

// FIXME(fuzzypixelz): this was added in e7f885ef due to sub reliability
// removal. It's a ZST and should not be passed in parameters.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) struct SubscriberInfo;

impl Face {
    #[tracing::instrument(level = "trace", skip_all, fields(id = id, expr = %expr, node_id = node_id))]
    pub(crate) fn declare_subscription(
        &self,
        id: SubscriberId,
        expr: &WireExpr,
        sub_info: &SubscriberInfo,
        node_id: NodeId,
        send_declare: &mut SendDeclare,
    ) {
        let rtables = zread!(self.tables.tables);
        match rtables
            .data
            .get_mapping(&self.state, &expr.scope, expr.mapping)
            .cloned()
        {
            Some(mut prefix) => {
                tracing::debug!(
                    "{} Declare subscriber {} ({}{})",
                    self.state,
                    id,
                    prefix.expr(),
                    expr.suffix
                );
                let res = Resource::get_resource(&prefix, &expr.suffix);
                let (mut res, mut wtables) =
                    if res.as_ref().map(|r| r.ctx.is_some()).unwrap_or(false) {
                        drop(rtables);
                        let tables_wguard = zwrite!(self.tables.tables);
                        (res.unwrap(), tables_wguard)
                    } else {
                        let mut fullexpr = prefix.expr().to_string();
                        fullexpr.push_str(expr.suffix.as_ref());
                        let mut matches = keyexpr::new(fullexpr.as_str())
                            .map(|ke| Resource::get_matches(&rtables.data, ke))
                            .unwrap_or_default();
                        drop(rtables);
                        let mut tables_wguard = zwrite!(self.tables.tables);
                        let tables = &mut *tables_wguard;
                        let mut res =
                            Resource::make_resource(tables, &mut prefix, expr.suffix.as_ref());
                        matches.push(Arc::downgrade(&res));
                        Resource::match_resource(&tables.data, &mut res, matches);
                        (res, tables_wguard)
                    };

                let tables = &mut *wtables;

                tracing::trace!(?self.state.local_bound);

                for (bound, hat) in tables.hats.iter_mut() {
                    hat.declare_subscription(
                        BaseContext {
                            tables_lock: &self.tables,
                            tables: &mut tables.data,
                            src_face: &mut self.state.clone(),
                            send_declare,
                        },
                        id,
                        &mut res,
                        node_id,
                        sub_info,
                    );

                    disable_matches_data_routes(&mut res, bound);
                }

                drop(wtables);
            }
            None => tracing::error!(
                "{} Declare subscriber {} for unknown scope {}!",
                self.state,
                id,
                expr.scope
            ),
        }
    }

    #[tracing::instrument(level = "trace", skip_all, fields(id = id, expr = %expr, node_id = node_id))]
    pub(crate) fn undeclare_subscription(
        &self,
        id: SubscriberId,
        expr: &WireExpr,
        node_id: NodeId,
        send_declare: &mut SendDeclare,
    ) {
        let res = if expr.is_empty() {
            None
        } else {
            let rtables = zread!(self.tables.tables);
            match rtables
                .data
                .get_mapping(&self.state, &expr.scope, expr.mapping)
            {
                Some(prefix) => match Resource::get_resource(prefix, expr.suffix.as_ref()) {
                    Some(res) => Some(res),
                    None => {
                        tracing::error!(
                            "{} Undeclare unknown subscriber {}{}!",
                            self.state,
                            prefix.expr(),
                            expr.suffix
                        );
                        return;
                    }
                },
                None => {
                    tracing::error!(
                        "{} Undeclare subscriber with unknown scope {}",
                        self.state,
                        expr.scope
                    );
                    return;
                }
            }
        };
        let mut wtables = zwrite!(self.tables.tables);
        let tables = &mut *wtables;

        let res_cleanup = tables.hats.iter_mut().filter_map(|(bound, hat)| {
            let res = hat.undeclare_subscription(
                BaseContext {
                    tables_lock: &self.tables,
                    tables: &mut tables.data,
                    src_face: &mut self.state.clone(),
                    send_declare,
                },
                id,
                res.clone(),
                node_id,
            );

            match res {
                Some(mut res) => {
                    tracing::debug!(
                        "{} Undeclare subscriber {} ({})",
                        &self.state,
                        id,
                        res.expr()
                    );
                    disable_matches_data_routes(&mut res, bound);
                    Some(res)
                }
                None => {
                    // NOTE: This is expected behavior if subscriber declarations are denied with ingress ACL interceptor.
                    tracing::debug!("{} Undeclare unknown subscriber {}", self.state, id);
                    None
                }
            }
        });

        // REVIEW(regions): this is necessary if HatFace is global
        for mut res in res_cleanup {
            Resource::clean(&mut res);
        }
    }
}

pub(crate) fn disable_matches_data_routes(res: &mut Arc<Resource>, bound: &Bound) {
    if res.ctx.is_some() {
        get_mut_unchecked(res).context_mut().hats[bound].disable_data_routes();
        for match_ in &res.context().matches {
            let mut match_ = match_.upgrade().unwrap();
            if !Arc::ptr_eq(&match_, res) {
                get_mut_unchecked(&mut match_).context_mut().hats[bound].disable_data_routes();
            }
        }
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
fn get_data_route(
    tables: &Tables,
    face: &FaceState,
    expr: &RoutingExpr,
    node_id: NodeId,
    dst_node_id: NodeId,
    bound: &Bound,
) -> Arc<Route> {
    let node_id = tables.hats[bound].map_routing_context(&tables.data, face, node_id);
    let dst_node_id = tables.hats[bound].map_routing_context(&tables.data, face, dst_node_id);
    let compute_route =
        || tables.hats[bound].compute_data_route(&tables.data, face, expr, node_id, dst_node_id);
    match expr
        .resource()
        .as_ref()
        .and_then(|res| res.ctx.as_ref())
        .map(|ctx| &ctx.hats[bound].data_routes)
    {
        Some(data_routes) => get_or_set_route(
            data_routes,
            tables.data.hats[bound].routes_version,
            face.whatami,
            node_id,
            compute_route,
        ),
        None => compute_route(),
    }
}

#[inline]
pub(crate) fn get_matching_subscriptions(
    tables: &Tables,
    key_expr: &KeyExpr<'_>,
) -> HashMap<usize, Arc<FaceState>> {
    // REVIEW(regions2): use the broker hat
    tables.hats[Bound::session()].get_matching_subscriptions(&tables.data, key_expr)
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
                    zenoh_protocol::zenoh::PushBody::Put(p) => {
                        stats.[<$txrx _z_put_msgs>].[<inc_ $space>](1);
                        let mut n =  p.payload.len();
                        if let Some(a) = p.ext_attachment.as_ref() {
                           n += a.buffer.len();
                        }
                        stats.[<$txrx _z_put_pl_bytes>].[<inc_ $space>](n);
                    }
                    zenoh_protocol::zenoh::PushBody::Del(d) => {
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
    let rtables = zread!(tables_ref.tables);
    let Some(prefix) = rtables
        .data
        .get_mapping(face, &msg.wire_expr.scope, msg.wire_expr.mapping)
    else {
        tracing::error!(
            "{} Route data with unknown scope {}!",
            face,
            msg.wire_expr.scope
        );
        return;
    };

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

    let mut dirs = RouteBuilder::<Direction>::new();

    for (bound, hat) in rtables.hats.iter() {
        if hat.ingress_filter(&rtables.data, face, &expr) {
            let route = get_data_route(
                &rtables,
                face,
                &expr,
                msg.ext_nodeid.node_id,
                msg.ext_dst_nodeid.node_id,
                bound,
            );

            for dir in route.iter() {
                if hat.egress_filter(&rtables.data, face, &dir.dst_face, &expr) {
                    dirs.insert(dir.dst_face.id, || dir.clone());
                }
            }
        }
    }

    let send_push = |dst_face: &FaceState, msg: &mut Push, reliability: Reliability| {
        #[cfg(feature = "stats")]
        if !admin {
            inc_stats!(dst_face, tx, user, msg.payload)
        } else {
            inc_stats!(dst_face, tx, admin, msg.payload)
        }
        dst_face.primitives.send_push(msg, reliability)
    };

    let mut dirs_iter = dirs.build().into_iter();

    if let Some(dir) = dirs_iter.next() {
        treat_timestamp!(
            &rtables.data.hlc,
            msg.payload,
            rtables.data.drop_future_timestamp
        );

        msg.wire_expr = dir.wire_expr.clone();
        msg.ext_nodeid = ext::NodeIdType {
            node_id: dir.node_id,
        };

        drop(rtables);

        for dir in dirs_iter {
            send_push(
                &dir.dst_face,
                &mut Push {
                    wire_expr: dir.wire_expr,
                    ext_qos: msg.ext_qos,
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType {
                        node_id: dir.node_id,
                    },
                    ext_dst_nodeid: ext::DstNodeIdType {
                        node_id: dir.dst_node_id,
                    },
                    payload: msg.payload.clone(),
                },
                reliability,
            );
        }

        send_push(&dir.dst_face, msg, reliability);
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
