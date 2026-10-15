//
// Copyright (c) 2024 ZettaScale Technology
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
    collections::HashSet,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

use nonempty_collections::NEVec;
use zenoh_config::{DataMessage, DownsamplingItemConf, DownsamplingRuleConf};
use zenoh_core::zlock;
use zenoh_keyexpr::keyexpr_tree::{
    impls::KeyedSetProvider, support::UnknownWildness, IKeyExprTree, IKeyExprTreeMut,
    IKeyExprTreeNode, IKeyExprTreeNodeMut, KeBoxTree,
};
use zenoh_protocol::{
    network::{NetworkBodyMut, Push},
    zenoh::PushBody,
};
use zenoh_result::ZResult;

use crate::net::routing::interceptor::*;

pub(crate) fn downsampling_interceptor_factories(
    config: &Vec<DownsamplingItemConf>,
) -> ZResult<Vec<InterceptorFactory>> {
    let mut res: Vec<InterceptorFactory> = vec![];

    let mut id_set = HashSet::new();
    for ds in config {
        // check unicity of rule id
        if let Some(id) = &ds.id {
            if !id_set.insert(id.clone()) {
                bail!("Invalid Downsampling config: id '{id}' is repeated");
            }
        }

        res.push(Box::new(DownsamplingInterceptorFactory::new(ds.clone())));
    }

    Ok(res)
}

pub struct DownsamplingInterceptorFactory {
    interfaces: Option<NEVec<String>>,
    link_protocols: Option<NEVec<InterceptorLink>>,
    rules: NEVec<DownsamplingRuleConf>,
    flows: InterfaceEnabled,
    messages: DownsamplingFilters,
}

impl DownsamplingInterceptorFactory {
    pub fn new(conf: DownsamplingItemConf) -> Self {
        Self {
            interfaces: conf.interfaces,
            rules: conf.rules,
            link_protocols: conf.link_protocols,
            flows: conf.flows.map(|f| (&f).into()).unwrap_or(InterfaceEnabled {
                ingress: true,
                egress: true,
            }),
            messages: DownsamplingFilters::new(conf.messages.iter().copied()),
        }
    }
}

impl InterceptorFactoryTrait for DownsamplingInterceptorFactory {
    fn new_transport_unicast(
        &self,
        transport: &TransportUnicast,
    ) -> (Option<IngressInterceptor>, Option<EgressInterceptor>) {
        if let Some(interfaces) = &self.interfaces {
            if let Ok(links) = transport.get_links() {
                for link in links {
                    if !link.interfaces.iter().any(|x| interfaces.contains(x)) {
                        return (None, None);
                    }
                }
            }
        }
        if let Some(config_protocols) = &self.link_protocols {
            match transport.get_auth_ids() {
                Ok(auth_ids) => {
                    if !auth_ids
                        .link_auth_ids()
                        .iter()
                        .map(|auth_id| InterceptorLinkWrapper::from(auth_id).0)
                        .any(|v| config_protocols.contains(&v))
                    {
                        return (None, None);
                    }
                }
                Err(e) => {
                    tracing::error!("Error loading transport AuthIds: {e}");
                    return (None, None);
                }
            }
        }

        #[cfg(feature = "stats")]
        let Ok(stats) = transport
            .get_stats()
            .map(|stats| stats.drop_stats(zenoh_stats::ReasonLabel::Downsampling))
        else {
            // `get_stats` returning an error means the transport is closed
            return (None, None);
        };
        let interceptor = |flow| {
            let direction = match flow {
                InterceptorFlow::Ingress => "ingress",
                InterceptorFlow::Egress => "egress",
            };
            tracing::debug!("New {direction} downsampler on transport unicast {transport:?}");
            Box::new(DownsamplingInterceptor::new(
                self.messages,
                &self.rules,
                flow,
                #[cfg(feature = "stats")]
                stats.clone(),
            )) as Interceptor
        };
        (
            self.flows
                .ingress
                .then(|| interceptor(InterceptorFlow::Ingress)),
            self.flows
                .egress
                .then(|| interceptor(InterceptorFlow::Egress)),
        )
    }

    fn new_transport_multicast(
        &self,
        _transport: &TransportMulticast,
    ) -> Option<EgressInterceptor> {
        None
    }

    fn new_peer_multicast(&self, _transport: &TransportMulticast) -> Option<IngressInterceptor> {
        None
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct DownsamplingFilters {
    delete: bool,
    put: bool,
    query: bool,
    reply: bool,
}

impl DownsamplingFilters {
    fn new<T: IntoIterator<Item = DataMessage>>(iter: T) -> Self {
        let mut filters = Self::default();
        for m in iter {
            match m {
                DataMessage::Delete => filters.delete = true,
                DataMessage::Put => filters.put = true,
                DataMessage::Query => filters.query = true,
                DataMessage::Reply => filters.reply = true,
            }
        }
        filters
    }

    fn is_msg_filtered(&self, msg: &mut NetworkMessageMut) -> bool {
        match msg.body {
            NetworkBodyMut::Push(Push { payload, .. }) => match payload {
                PushBody::Put(_) => self.put,
                PushBody::Del(_) => self.delete,
            },
            NetworkBodyMut::Request(_) => self.query,
            NetworkBodyMut::Response(_) => self.reply,
            NetworkBodyMut::ResponseFinal(_) => false,
            NetworkBodyMut::Interest(_) => false,
            NetworkBodyMut::Declare(_) => false,
            NetworkBodyMut::OAM(_) => false,
        }
    }
}

pub(crate) struct DownsamplingInterceptor {
    filters: DownsamplingFilters,
    ke_id: KeBoxTree<Vec<usize>, UnknownWildness, KeyedSetProvider>,
    // Per-rule minimal intervals, indexed by rule id.
    thresholds: Vec<std::time::Duration>,
    // Per-rule timestamps of the latest forwarded message, indexed by rule id.
    // A single lock keeps the check-and-update over all matching rules atomic
    // without allocating a guard per rule on the hot path.
    latest_message_timestamps: Mutex<Vec<std::time::Instant>>,
    flow: InterceptorFlow,
    #[cfg(feature = "stats")]
    stats: zenoh_stats::DropStats,
}

impl DownsamplingInterceptor {
    fn matching_ids<'a>(&'a self, key_expr: &'a keyexpr) -> impl Iterator<Item = usize> + 'a {
        self.ke_id
            .intersecting_nodes(key_expr)
            .filter_map(|node| node.weight())
            .flatten()
            .copied()
    }

    /// Returns `true` if every rule yielded by `ids()` has its budget available,
    /// in which case the timestamps of all those rules are updated.
    /// `ids()` is invoked twice (check, then update) to avoid collecting ids.
    fn consume_budgets<I: Iterator<Item = usize>>(&self, ids: impl Fn() -> I) -> bool {
        let mut latest = zlock!(self.latest_message_timestamps);
        let timestamp = std::time::Instant::now();
        let allowed = ids().all(|id| match (latest.get(id), self.thresholds.get(id)) {
            (Some(latest), Some(threshold)) => {
                timestamp.saturating_duration_since(*latest) >= *threshold
            }
            _ => {
                tracing::debug!("unexpected rule ID {id}");
                true
            }
        });
        if allowed {
            for id in ids() {
                if let Some(latest) = latest.get_mut(id) {
                    *latest = timestamp;
                }
            }
        }
        allowed
    }
}

// The flag is used to print a message only once
static INFO_FLAG: AtomicBool = AtomicBool::new(false);

impl InterceptorTrait for DownsamplingInterceptor {
    fn compute_keyexpr_cache(&self, key_expr: &keyexpr) -> Option<Box<dyn Any + Send + Sync>> {
        Some(Box::new(
            self.matching_ids(key_expr).collect::<Vec<usize>>(),
        ))
    }

    fn intercept(&self, msg: &mut NetworkMessageMut, ctx: &mut dyn InterceptorContext) -> bool {
        if !self.filters.is_msg_filtered(msg) {
            return true;
        };
        let allowed = if let Some(ids) = ctx
            .get_cache(msg)
            .and_then(|c| c.downcast_ref::<Vec<usize>>())
        {
            ids.is_empty() || self.consume_budgets(|| ids.iter().copied())
        } else if let Some(ke) = ctx.full_keyexpr(msg) {
            self.consume_budgets(|| self.matching_ids(&ke))
        } else {
            true
        };
        if allowed {
            true
        } else {
            if !INFO_FLAG.swap(true, Ordering::Relaxed) {
                tracing::info!("Some message(s) have been dropped by the downsampling interceptor. Enable trace level tracing for more details.");
            }
            let direction = match self.flow {
                InterceptorFlow::Egress => "to",
                InterceptorFlow::Ingress => "from",
            };
            tracing::trace!(
                "Message dropped by the downsampling interceptor: {msg}({key_expr}) {direction}:{face}",
                key_expr = ctx.full_expr(msg).unwrap_or_default(),
                face = ctx.face().map(|f| f.to_string()).unwrap_or_default(),
            );
            #[cfg(feature = "stats")]
            self.stats
                .observe_network_message_dropped_payload(stats_direction(self.flow), msg);
            false
        }
    }
}

const NANOS_PER_SEC: f64 = 1_000_000_000.0;

impl DownsamplingInterceptor {
    pub fn new(
        filters: DownsamplingFilters,
        rules: &NEVec<DownsamplingRuleConf>,
        flow: InterceptorFlow,
        #[cfg(feature = "stats")] stats: zenoh_stats::DropStats,
    ) -> Self {
        let mut ke_id: KeBoxTree<Vec<usize>, UnknownWildness, KeyedSetProvider> =
            KeBoxTree::default();
        let mut thresholds = Vec::with_capacity(rules.len().get());
        let mut latest_message_timestamps = Vec::with_capacity(rules.len().get());
        for (id, rule) in rules.into_iter().enumerate() {
            let mut threshold = std::time::Duration::MAX;
            let mut latest_message_timestamp = std::time::Instant::now();
            if rule.freq != 0.0 {
                threshold =
                    std::time::Duration::from_nanos((1. / rule.freq * NANOS_PER_SEC) as u64);
                latest_message_timestamp -= threshold;
            }
            let node = ke_id.node_mut_or_create(&rule.key_expr);
            if let Some(ids) = node.weight_mut() {
                ids.push(id);
            } else {
                node.insert_weight(vec![id]);
            }
            thresholds.push(threshold);
            latest_message_timestamps.push(latest_message_timestamp);
            tracing::debug!(
                "New downsampler rule enabled: key_expr={key_expr:?}, threshold={threshold:?}, messages={filters:?}",
                key_expr = rule.key_expr,
            );
        }
        Self {
            filters,
            ke_id,
            thresholds,
            latest_message_timestamps: latest_message_timestamps.into(),
            flow,
            #[cfg(feature = "stats")]
            stats,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use nonempty_collections::nev;
    use test_case::test_case;
    use zenoh_protocol::{core::Reliability, zenoh::Put};

    use super::*;

    struct TestContext {
        cache: Option<Box<dyn Any + Send + Sync>>,
    }

    impl InterceptorContext for TestContext {
        fn face(&self) -> Option<crate::net::routing::dispatcher::face::Face> {
            None
        }

        fn full_expr(&self, _msg: &NetworkMessageMut) -> Option<&str> {
            Some("robot/thermal_image/sensor_msgs::msg::dds_::Image_/frame")
        }

        fn get_cache(&self, _msg: &NetworkMessageMut) -> Option<&Box<dyn Any + Send + Sync>> {
            self.cache.as_ref()
        }
    }

    fn assert_matching_rules_enforced(
        first_key_expr: &str,
        second_key_expr: &str,
        reverse_rules: bool,
        cached: bool,
        flow: InterceptorFlow,
    ) {
        let first = DownsamplingRuleConf {
            key_expr: first_key_expr.try_into().unwrap(),
            freq: 10.0,
        };
        let second = DownsamplingRuleConf {
            key_expr: second_key_expr.try_into().unwrap(),
            freq: 1.0,
        };
        let rules = if reverse_rules {
            nev![second, first]
        } else {
            nev![first, second]
        };
        let interceptor = DownsamplingInterceptor::new(
            DownsamplingFilters::new([DataMessage::Put]),
            &rules,
            flow,
            #[cfg(feature = "stats")]
            zenoh_stats::StatsRegistry::new(
                zenoh_protocol::core::ZenohIdProto::default(),
                zenoh_protocol::core::WhatAmI::Peer,
                "test",
            )
            .unicast_transport_stats(
                zenoh_protocol::core::ZenohIdProto::default(),
                zenoh_protocol::core::WhatAmI::Peer,
                None,
            )
            .drop_stats(zenoh_stats::ReasonLabel::Downsampling),
        );
        let mut push = Push::from(Put::default());
        let mut msg = NetworkMessageMut {
            body: NetworkBodyMut::Push(&mut push),
            reliability: Reliability::Reliable,
        };
        let mut ctx = TestContext { cache: None };
        if cached {
            ctx.cache = interceptor
                .compute_keyexpr_cache(keyexpr::new(ctx.full_expr(&msg).unwrap()).unwrap());
        }

        let mut allowed = Vec::new();
        for saturated in [[false, false], [true, true], [true, false], [false, true]] {
            // Set independent rule budgets without sleeps or random tree traversal assumptions.
            let now = Instant::now();
            let before: Vec<Instant> = saturated
                .iter()
                .map(|&saturated| {
                    if saturated {
                        now
                    } else {
                        now - Duration::from_secs(2)
                    }
                })
                .collect();
            interceptor
                .latest_message_timestamps
                .lock()
                .unwrap()
                .clone_from(&before);
            let decision = interceptor.intercept(&mut msg, &mut ctx);
            let after = interceptor
                .latest_message_timestamps
                .lock()
                .unwrap()
                .clone();
            if decision {
                assert!(
                    after.iter().all(|&ts| ts >= now),
                    "accepted message must update every matching rule timestamp"
                );
                assert_eq!(
                    after[0], after[1],
                    "accepted message must update matching rules to the same timestamp"
                );
            } else {
                assert_eq!(
                    after, before,
                    "rejected message must leave every matching rule timestamp unchanged"
                );
            }
            allowed.push(decision);
        }

        assert_eq!(
            allowed,
            [true, false, false, false],
            "only a message with both matching rule budgets available may pass"
        );
    }

    #[test_case(false, false, InterceptorFlow::Ingress; "ingress_uncached")]
    #[test_case(false, true, InterceptorFlow::Ingress; "ingress_cached")]
    #[test_case(true, false, InterceptorFlow::Ingress; "ingress_reversed_uncached")]
    #[test_case(true, true, InterceptorFlow::Ingress; "ingress_reversed_cached")]
    #[test_case(false, false, InterceptorFlow::Egress; "egress_uncached")]
    #[test_case(false, true, InterceptorFlow::Egress; "egress_cached")]
    #[test_case(true, false, InterceptorFlow::Egress; "egress_reversed_uncached")]
    #[test_case(true, true, InterceptorFlow::Egress; "egress_reversed_cached")]
    fn overlapping_downsampling_enforces_every_matching_rule(
        reverse_rules: bool,
        cached: bool,
        flow: InterceptorFlow,
    ) {
        // Regression for #2200: both matching maximum-frequency budgets must apply.
        assert_matching_rules_enforced(
            "*/thermal_image/sensor_msgs::msg::dds_::Image_/*",
            "**/sensor_msgs::msg::dds_::Image_/*",
            reverse_rules,
            cached,
            flow,
        );
    }

    #[test_case(false, false, InterceptorFlow::Ingress; "ingress_uncached")]
    #[test_case(false, true, InterceptorFlow::Ingress; "ingress_cached")]
    #[test_case(true, false, InterceptorFlow::Ingress; "ingress_reversed_uncached")]
    #[test_case(true, true, InterceptorFlow::Ingress; "ingress_reversed_cached")]
    #[test_case(false, false, InterceptorFlow::Egress; "egress_uncached")]
    #[test_case(false, true, InterceptorFlow::Egress; "egress_cached")]
    #[test_case(true, false, InterceptorFlow::Egress; "egress_reversed_uncached")]
    #[test_case(true, true, InterceptorFlow::Egress; "egress_reversed_cached")]
    fn duplicate_keyexpr_downsampling_enforces_every_matching_rule(
        reverse_rules: bool,
        cached: bool,
        flow: InterceptorFlow,
    ) {
        assert_matching_rules_enforced(
            "*/thermal_image/sensor_msgs::msg::dds_::Image_/*",
            "*/thermal_image/sensor_msgs::msg::dds_::Image_/*",
            reverse_rules,
            cached,
            flow,
        );
    }
}
