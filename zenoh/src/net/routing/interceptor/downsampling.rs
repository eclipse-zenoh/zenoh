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
    collections::{HashMap, HashSet},
    fmt::Display,
    sync::{Arc, Mutex},
};

use zenoh_config::{
    DownsamplingItemConf, DownsamplingMessage, DownsamplingRuleConf, InterceptorFlow,
};
use zenoh_core::zlock;
use zenoh_keyexpr::keyexpr_tree::{
    impls::KeyedSetProvider, support::UnknownWildness, IKeyExprTree, IKeyExprTreeMut, KeBoxTree,
};
use zenoh_protocol::network::NetworkBody;
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
        let mut ds = ds.clone();
        // check for undefined flows and initialize them
        let flows = ds
            .flows
            .get_or_insert(vec![InterceptorFlow::Ingress, InterceptorFlow::Egress]);
        if flows.is_empty() {
            bail!("Invalid Downsampling config: flows list must not be empty");
        }
        // check for empty messages list
        if ds.messages.is_empty() {
            bail!("Invalid Downsampling config: messages list must not be empty");
        }
        res.push(Box::new(DownsamplingInterceptorFactory::new(ds)));
    }

    Ok(res)
}

pub struct DownsamplingInterceptorFactory {
    interfaces: Option<Vec<String>>,
    rules: Vec<DownsamplingRuleConf>,
    flows: InterfaceEnabled,
    messages: Arc<DownsamplingFilters>,
}

impl DownsamplingInterceptorFactory {
    pub fn new(conf: DownsamplingItemConf) -> Self {
        Self {
            interfaces: conf.interfaces,
            rules: conf.rules,
            flows: conf
                .flows
                .expect("config flows should be set")
                .as_slice()
                .into(),
            messages: Arc::new(conf.messages.as_slice().into()),
        }
    }
}

impl InterceptorFactoryTrait for DownsamplingInterceptorFactory {
    fn new_transport_unicast(
        &self,
        transport: &TransportUnicast,
    ) -> (Option<IngressInterceptor>, Option<EgressInterceptor>) {
        tracing::debug!("New downsampler transport unicast {:?}", transport);
        if let Some(interfaces) = &self.interfaces {
            tracing::debug!(
                "New downsampler transport unicast config interfaces: {:?}",
                interfaces
            );
            if let Ok(links) = transport.get_links() {
                for link in links {
                    tracing::debug!(
                        "New downsampler transport unicast link interfaces: {:?}",
                        link.interfaces
                    );
                    if !link.interfaces.iter().any(|x| interfaces.contains(x)) {
                        return (None, None);
                    }
                }
            }
        };
        (
            self.flows.ingress.then_some(Box::new(ComputeOnMiss::new(
                DownsamplingInterceptor::new(self.messages.clone(), self.rules.clone()),
            ))),
            self.flows.egress.then_some(Box::new(ComputeOnMiss::new(
                DownsamplingInterceptor::new(self.messages.clone(), self.rules.clone()),
            ))),
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

#[derive(Default, Clone)]
pub(crate) struct DownsamplingFilters {
    push: bool,
    query: bool,
    reply: bool,
}

impl Display for DownsamplingFilters {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[ push={}, query={}, reply={} ]",
            self.push, self.query, self.reply
        )
    }
}

impl From<&[DownsamplingMessage]> for DownsamplingFilters {
    fn from(value: &[DownsamplingMessage]) -> Self {
        let mut res = Self::default();
        for v in value {
            match v {
                DownsamplingMessage::Push => res.push = true,
                DownsamplingMessage::Query => res.query = true,
                DownsamplingMessage::Reply => res.reply = true,
            }
        }
        res
    }
}

struct Timestate {
    pub threshold: tokio::time::Duration,
    pub latest_message_timestamp: tokio::time::Instant,
}

pub(crate) struct DownsamplingInterceptor {
    filtered_messages: Arc<DownsamplingFilters>,
    ke_id: Arc<Mutex<KeBoxTree<usize, UnknownWildness, KeyedSetProvider>>>,
    ke_state: Arc<Mutex<HashMap<usize, Timestate>>>,
}

impl DownsamplingInterceptor {
    fn is_msg_filtered(&self, ctx: &RoutingContext<NetworkMessage>) -> bool {
        match ctx.msg.body {
            NetworkBody::Push(_) => self.filtered_messages.push,
            NetworkBody::Request(_) => self.filtered_messages.query,
            NetworkBody::Response(_) => self.filtered_messages.reply,
            NetworkBody::ResponseFinal(_) => false,
            NetworkBody::Interest(_) => false,
            NetworkBody::Declare(_) => false,
            NetworkBody::OAM(_) => false,
        }
    }
}

impl InterceptorTrait for DownsamplingInterceptor {
    fn compute_keyexpr_cache(&self, key_expr: &KeyExpr<'_>) -> Option<Box<dyn Any + Send + Sync>> {
        let ke_id = zlock!(self.ke_id);
        if let Some(node) = ke_id.intersecting_keys(key_expr).next() {
            if let Some(id) = ke_id.weight_at(&node) {
                return Some(Box::new(Some(*id)));
            }
        }
        Some(Box::new(None::<usize>))
    }

    fn intercept(
        &self,
        ctx: RoutingContext<NetworkMessage>,
        cache: Option<&Box<dyn Any + Send + Sync>>,
    ) -> Option<RoutingContext<NetworkMessage>> {
        if self.is_msg_filtered(&ctx) {
            if let Some(cache) = cache {
                if let Some(id) = cache.downcast_ref::<Option<usize>>() {
                    if let Some(id) = id {
                        let mut ke_state = zlock!(self.ke_state);
                        if let Some(state) = ke_state.get_mut(id) {
                            let timestamp = tokio::time::Instant::now();

                            if timestamp - state.latest_message_timestamp >= state.threshold {
                                state.latest_message_timestamp = timestamp;
                                return Some(ctx);
                            } else {
                                return None;
                            }
                        } else {
                            tracing::debug!("unexpected cache ID {}", id);
                        }
                    }
                } else {
                    tracing::debug!("unexpected cache type {:?}", ctx.full_expr());
                }
            }
        }

        Some(ctx)
    }
}

const NANOS_PER_SEC: f64 = 1_000_000_000.0;

impl DownsamplingInterceptor {
    pub fn new(messages: Arc<DownsamplingFilters>, rules: Vec<DownsamplingRuleConf>) -> Self {
        let mut ke_id = KeBoxTree::default();
        let mut ke_state = HashMap::default();
        for (id, rule) in rules.into_iter().enumerate() {
            let mut threshold = tokio::time::Duration::MAX;
            let mut latest_message_timestamp = tokio::time::Instant::now();
            if rule.freq != 0.0 {
                threshold =
                    tokio::time::Duration::from_nanos((1. / rule.freq * NANOS_PER_SEC) as u64);
                latest_message_timestamp -= threshold;
            }
            ke_id.insert(&rule.key_expr, id);
            ke_state.insert(
                id,
                Timestate {
                    threshold,
                    latest_message_timestamp,
                },
            );
            tracing::debug!(
                "New downsampler rule enabled: key_expr={:?}, threshold={:?}, messages={}",
                rule.key_expr,
                threshold,
                messages,
            );
        }
        Self {
            filtered_messages: messages,
            ke_id: Arc::new(Mutex::new(ke_id)),
            ke_state: Arc::new(Mutex::new(ke_state)),
        }
    }
}
