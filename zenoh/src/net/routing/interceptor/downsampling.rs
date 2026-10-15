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
use smallvec::SmallVec;
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

/// Per-rule rate-limiting state, indexed by rule id.
struct RuleBudgets {
    /// Minimal interval between two forwarded messages for each rule.
    thresholds: Vec<std::time::Duration>,
    /// Timestamp of the latest forwarded message for each rule.
    latest_message_timestamps: Vec<std::time::Instant>,
}

impl RuleBudgets {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            thresholds: Vec::with_capacity(capacity),
            latest_message_timestamps: Vec::with_capacity(capacity),
        }
    }

    fn push(
        &mut self,
        threshold: std::time::Duration,
        latest_message_timestamp: std::time::Instant,
    ) {
        self.thresholds.push(threshold);
        self.latest_message_timestamps
            .push(latest_message_timestamp);
    }

    /// Atomically acquires the budget of every rule in `ids` at instant `now`.
    ///
    /// Returns `true` and records `now` as the latest forwarded message for all
    /// those rules if every budget is available; otherwise returns `false` and
    /// leaves all timestamps untouched.
    fn try_acquire(&mut self, ids: &[usize], now: std::time::Instant) -> bool {
        let available = ids.iter().all(|&id| {
            match (
                self.latest_message_timestamps.get(id),
                self.thresholds.get(id),
            ) {
                (Some(latest), Some(threshold)) => {
                    now.saturating_duration_since(*latest) >= *threshold
                }
                _ => {
                    tracing::debug!("unexpected rule ID {id}");
                    true
                }
            }
        });
        if available {
            for &id in ids {
                if let Some(latest) = self.latest_message_timestamps.get_mut(id) {
                    *latest = now;
                }
            }
        }
        available
    }
}

pub(crate) struct DownsamplingInterceptor {
    filters: DownsamplingFilters,
    ke_id: KeBoxTree<Vec<usize>, UnknownWildness, KeyedSetProvider>,
    // A single lock keeps the check-and-update over all matching rules atomic
    // without allocating a guard per rule on the hot path.
    budgets: Mutex<RuleBudgets>,
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

    fn try_acquire_budgets(&self, ids: &[usize]) -> bool {
        ids.is_empty() || zlock!(self.budgets).try_acquire(ids, std::time::Instant::now())
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
            self.try_acquire_budgets(ids)
        } else if let Some(ke) = ctx.full_keyexpr(msg) {
            // Walk the tree outside the lock; the buffer stays on the stack for typical rule counts.
            let ids: SmallVec<[usize; 8]> = self.matching_ids(&ke).collect();
            self.try_acquire_budgets(&ids)
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
        let mut budgets = RuleBudgets::with_capacity(rules.len().get());
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
            budgets.push(threshold, latest_message_timestamp);
            tracing::debug!(
                "New downsampler rule enabled: key_expr={key_expr:?}, threshold={threshold:?}, messages={filters:?}",
                key_expr = rule.key_expr,
            );
        }
        Self {
            filters,
            ke_id,
            budgets: budgets.into(),
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
        // A rule that never matches the test key expression: its budget must stay untouched.
        let unrelated = DownsamplingRuleConf {
            key_expr: "unrelated/**".try_into().unwrap(),
            freq: 1.0,
        };
        let rules = if reverse_rules {
            nev![second, first, unrelated]
        } else {
            nev![first, second, unrelated]
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
            let stale = now - Duration::from_secs(2);
            let before: Vec<Instant> = saturated
                .iter()
                .map(|&saturated| if saturated { now } else { stale })
                .chain([stale])
                .collect();
            interceptor
                .budgets
                .lock()
                .unwrap()
                .latest_message_timestamps
                .clone_from(&before);
            let decision = interceptor.intercept(&mut msg, &mut ctx);
            let after = interceptor
                .budgets
                .lock()
                .unwrap()
                .latest_message_timestamps
                .clone();
            assert_eq!(
                after[2], stale,
                "non-matching rule timestamp must never be touched"
            );
            if decision {
                assert!(
                    after[..2].iter().all(|&ts| ts >= now),
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
