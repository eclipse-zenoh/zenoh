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

#[cfg(feature = "stats")]
use std::sync::Arc;
use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

use nonempty_collections::NEVec;
use zenoh_config::{DownsamplingItemConf, DownsamplingMessage, DownsamplingRuleConf};
use zenoh_core::zlock;
use zenoh_keyexpr::keyexpr_tree::{
    impls::KeyedSetProvider, support::UnknownWildness, IKeyExprTree, IKeyExprTreeMut, KeBoxTree,
};
use zenoh_protocol::{
    network::{NetworkBodyMut, Push},
    zenoh::PushBody,
};
use zenoh_result::ZResult;
#[cfg(feature = "stats")]
use zenoh_transport::stats::TransportStats;

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
                transport.get_stats().unwrap_or_default(),
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
    fn new<T: IntoIterator<Item = DownsamplingMessage>>(iter: T) -> Self {
        let mut filters = Self::default();
        for m in iter {
            match m {
                DownsamplingMessage::Delete => filters.delete = true,
                #[allow(deprecated)]
                DownsamplingMessage::Push => {
                    filters.put = true;
                    filters.delete = true;
                }
                DownsamplingMessage::Put => filters.put = true,
                DownsamplingMessage::Query => filters.query = true,
                DownsamplingMessage::Reply => filters.reply = true,
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
            _ => false,
        }
    }
}

struct TimeState {
    pub threshold: std::time::Duration,
    pub latest_message_timestamp: Mutex<std::time::Instant>,
}

pub(crate) struct DownsamplingInterceptor {
    filters: DownsamplingFilters,
    ke_id: KeBoxTree<usize, UnknownWildness, KeyedSetProvider>,
    ke_state: Vec<TimeState>,
    flow: InterceptorFlow,
    #[cfg(feature = "stats")]
    stats: Arc<TransportStats>,
}

impl DownsamplingInterceptor {
    fn get_id(&self, key_expr: &keyexpr) -> Option<usize> {
        let node = self.ke_id.intersecting_keys(key_expr).next()?;
        self.ke_id.weight_at(&node).copied()
    }
}

// The flag is used to print a message only once
static INFO_FLAG: AtomicBool = AtomicBool::new(false);

impl InterceptorTrait for DownsamplingInterceptor {
    fn compute_keyexpr_cache(&self, key_expr: &keyexpr) -> Option<Box<dyn Any + Send + Sync>> {
        Some(Box::new(self.get_id(key_expr)))
    }

    fn intercept(&self, msg: &mut NetworkMessageMut, ctx: &mut dyn InterceptorContext) -> bool {
        if !self.filters.is_msg_filtered(msg) {
            return true;
        };
        let cache = ctx
            .get_cache(msg)
            .and_then(|c| c.downcast_ref::<Option<usize>>().copied());
        let Some(id) = cache.unwrap_or_else(|| self.get_id(&ctx.full_keyexpr(msg)?)) else {
            return true;
        };

        let Some(state) = self.ke_state.get(id) else {
            tracing::debug!("unexpected cache ID {id}");
            return true;
        };
        let mut latest_message_timestamp = zlock!(state.latest_message_timestamp);
        let timestamp = std::time::Instant::now();
        if timestamp - *latest_message_timestamp >= state.threshold {
            *latest_message_timestamp = timestamp;
            true
        } else {
            drop(latest_message_timestamp);
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
            match self.flow {
                InterceptorFlow::Egress => self.stats.inc_tx_downsampler_dropped_msgs(1),
                InterceptorFlow::Ingress => self.stats.inc_rx_downsampler_dropped_msgs(1),
            }
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
        #[cfg(feature = "stats")] stats: Arc<TransportStats>,
    ) -> Self {
        let mut ke_id = KeBoxTree::default();
        let mut ke_state = Vec::new();
        for (id, rule) in rules.into_iter().enumerate() {
            let mut threshold = std::time::Duration::MAX;
            let mut latest_message_timestamp = std::time::Instant::now();
            if rule.freq != 0.0 {
                threshold =
                    std::time::Duration::from_nanos((1. / rule.freq * NANOS_PER_SEC) as u64);
                latest_message_timestamp -= threshold;
            }
            ke_id.insert(&rule.key_expr, id);
            ke_state.push(TimeState {
                threshold,
                latest_message_timestamp: latest_message_timestamp.into(),
            });
            tracing::debug!(
                "New downsampler rule enabled: key_expr={key_expr:?}, threshold={threshold:?}, messages={filters:?}",
                key_expr = rule.key_expr,
            );
        }
        Self {
            filters,
            ke_id,
            ke_state,
            flow,
            #[cfg(feature = "stats")]
            stats,
        }
    }
}
