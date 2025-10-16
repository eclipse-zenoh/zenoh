//
// Copyright (c) 2025 ZettaScale Technology
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

//! ⚠️ WARNING ⚠️
//!
//! This module is intended for Zenoh's internal use.
//!
//! [Click here for Zenoh's documentation](https://docs.rs/zenoh/latest/zenoh)

use std::{
    borrow::Cow,
    sync::{Arc, Weak},
};

use zenoh_buffers::buffer::Buffer;
use zenoh_config::StatsConfig;
use zenoh_keyexpr::keyexpr_tree::{IKeyExprTree, IKeyExprTreeMut, IKeyExprTreeNode, KeBoxTree};
use zenoh_protocol::{
    network::NetworkBodyMut,
    zenoh::{ext::AttachmentType, reply::ReplyBody, PushBody, RequestBody, ResponseBody},
};
use zenoh_result::ZResult;
use zenoh_transport::stats::{FilteredStats, MessageStats, TransportStats};

use crate::net::routing::interceptor::*;

pub(crate) fn stats_interceptor_factories(
    config: &StatsConfig,
) -> ZResult<Vec<InterceptorFactory>> {
    if config.filters().is_empty() {
        return Ok(vec![]);
    }
    Ok(vec![Box::new(StatsInterceptorFactory::new(config))])
}

struct StatsInterceptorFactory {
    parent_stats: Arc<Vec<FilteredStats>>,
}

impl StatsInterceptorFactory {
    fn new(conf: &StatsConfig) -> Self {
        Self {
            parent_stats: Arc::new(
                conf.filters()
                    .iter()
                    .map(|filter| FilteredStats::new(filter.key.clone(), None, []))
                    .collect(),
            ),
        }
    }

    fn register_filtered_stats(&self, stats: &TransportStats) {
        if let Some(parent) = stats.parent().as_ref().and_then(Weak::upgrade) {
            parent.filtered().store(self.parent_stats.clone())
        }
        stats.filtered().store(Arc::new(
            self.parent_stats
                .iter()
                .map(|s| {
                    FilteredStats::new(
                        s.key_expr().clone(),
                        Some(Arc::downgrade(s.stats())),
                        stats.labels().clone(),
                    )
                })
                .collect(),
        ))
    }

    fn make_interceptor(&self, stats: &TransportStats, flow: InterceptorFlow) -> Interceptor {
        if let Some(parent) = stats.parent().as_ref().and_then(Weak::upgrade) {
            parent.filtered().store(self.parent_stats.clone())
        }
        let mut filters_tree = KeBoxTree::new();
        for s in stats.filtered().load().iter() {
            filters_tree.insert(s.key_expr(), s.stats().clone());
        }
        Box::new(StatsInterceptor { filters_tree, flow })
    }
}

impl InterceptorFactoryTrait for StatsInterceptorFactory {
    fn new_transport_unicast(
        &self,
        transport: &TransportUnicast,
    ) -> (Option<IngressInterceptor>, Option<EgressInterceptor>) {
        let Ok(stats) = transport.get_stats() else {
            return (None, None);
        };
        self.register_filtered_stats(&stats);
        (
            Some(self.make_interceptor(&stats, InterceptorFlow::Ingress)),
            Some(self.make_interceptor(&stats, InterceptorFlow::Egress)),
        )
    }

    fn new_transport_multicast(&self, transport: &TransportMulticast) -> Option<EgressInterceptor> {
        let stats = transport.get_stats().ok()?;
        self.register_filtered_stats(&stats);
        Some(self.make_interceptor(&stats, InterceptorFlow::Egress))
    }

    fn new_peer_multicast(&self, transport: &TransportMulticast) -> Option<IngressInterceptor> {
        let stats = transport.get_stats().ok()?;
        Some(self.make_interceptor(&stats, InterceptorFlow::Ingress))
    }
}

struct StatsInterceptor {
    filters_tree: KeBoxTree<Arc<MessageStats>>,
    flow: InterceptorFlow,
}

impl StatsInterceptor {
    fn compute_filtered_stats(&self, key_expr: &keyexpr) -> Vec<Arc<MessageStats>> {
        self.filters_tree
            .intersecting_nodes(key_expr)
            .filter_map(|n| n.weight().cloned())
            .collect()
    }

    fn get_or_compute_filtered_stats<'a>(
        &self,
        msg: &NetworkMessageMut,
        ctx: &'a dyn InterceptorContext,
    ) -> Cow<'a, [Arc<MessageStats>]> {
        match ctx
            .get_cache(msg)
            .and_then(|c| c.downcast_ref::<Vec<Arc<MessageStats>>>())
        {
            Some(v) => Cow::Borrowed(v),
            None => ctx.full_keyexpr(msg).map_or_else(Cow::default, |k| {
                Cow::Owned(self.compute_filtered_stats(&k))
            }),
        }
    }

    fn incr_stats(
        &self,
        filtered_stats: &[Arc<MessageStats>],
        payload_bytes: usize,
        ingress: (IncMsgs, IncPlBytes),
        egress: (IncMsgs, IncPlBytes),
    ) {
        for stats in filtered_stats {
            match self.flow {
                InterceptorFlow::Ingress => {
                    let (rx_msgs, rx_pl_bytes) = ingress;
                    rx_msgs(stats, 1);
                    rx_pl_bytes(stats, payload_bytes);
                }
                InterceptorFlow::Egress => {
                    let (tx_msgs, tx_pl_bytes) = egress;
                    tx_msgs(stats, 1);
                    tx_pl_bytes(stats, payload_bytes);
                }
            }
        }
    }
}

type IncMsgs = fn(&MessageStats, usize);
type IncPlBytes = fn(&MessageStats, usize);

impl InterceptorTrait for StatsInterceptor {
    fn compute_keyexpr_cache(&self, key_expr: &keyexpr) -> Option<Box<dyn Any + Send + Sync>> {
        Some(Box::new(self.compute_filtered_stats(key_expr)))
    }

    fn intercept(&self, msg: &mut NetworkMessageMut, ctx: &mut dyn InterceptorContext) -> bool {
        fn attachment_size<const ID: u8>(attachment: &Option<AttachmentType<ID>>) -> usize {
            attachment.as_ref().map_or(0, |a| a.buffer.len())
        }
        let filtered_stats = || self.get_or_compute_filtered_stats(msg, ctx);
        match &msg.body {
            NetworkBodyMut::Push(msg) => match &msg.payload {
                PushBody::Put(put) => self.incr_stats(
                    &filtered_stats(),
                    put.payload.len() + attachment_size(&put.ext_attachment),
                    (
                        MessageStats::inc_rx_z_put_msgs,
                        MessageStats::inc_rx_z_put_pl_bytes,
                    ),
                    (
                        MessageStats::inc_tx_z_put_msgs,
                        MessageStats::inc_tx_z_put_pl_bytes,
                    ),
                ),
                PushBody::Del(del) => self.incr_stats(
                    &filtered_stats(),
                    attachment_size(&del.ext_attachment),
                    (
                        MessageStats::inc_rx_z_del_msgs,
                        MessageStats::inc_rx_z_del_pl_bytes,
                    ),
                    (
                        MessageStats::inc_tx_z_del_msgs,
                        MessageStats::inc_tx_z_del_pl_bytes,
                    ),
                ),
            },

            NetworkBodyMut::Request(msg) => match &msg.payload {
                RequestBody::Query(query) => self.incr_stats(
                    &filtered_stats(),
                    attachment_size(&query.ext_attachment),
                    (
                        MessageStats::inc_rx_z_query_msgs,
                        MessageStats::inc_rx_z_query_pl_bytes,
                    ),
                    (
                        MessageStats::inc_tx_z_query_msgs,
                        MessageStats::inc_tx_z_query_pl_bytes,
                    ),
                ),
            },
            NetworkBodyMut::Response(msg) => {
                let payload_bytes = match &msg.payload {
                    ResponseBody::Reply(reply) => match &reply.payload {
                        ReplyBody::Put(put) => {
                            put.payload.len() + attachment_size(&put.ext_attachment)
                        }
                        ReplyBody::Del(del) => attachment_size(&del.ext_attachment),
                    },
                    ResponseBody::Err(err) => err.payload.len(),
                };
                self.incr_stats(
                    &filtered_stats(),
                    payload_bytes,
                    (
                        MessageStats::inc_rx_z_reply_msgs,
                        MessageStats::inc_rx_z_reply_pl_bytes,
                    ),
                    (
                        MessageStats::inc_tx_z_reply_msgs,
                        MessageStats::inc_tx_z_reply_pl_bytes,
                    ),
                )
            }
            NetworkBodyMut::ResponseFinal(_)
            | NetworkBodyMut::Interest(_)
            | NetworkBodyMut::Declare(_)
            | NetworkBodyMut::OAM(_) => {}
        }
        true
    }
}
