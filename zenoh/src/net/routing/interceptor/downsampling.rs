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
//! [Click here for Zenoh's documentation](../zenoh/index.html)

use crate::net::routing::interceptor::*;
use std::sync::{Arc, Mutex};
use zenoh_config::{DownsamplingFlow, DownsamplingItemConf, DownsamplingRuleConf};
use zenoh_core::zlock;
use zenoh_keyexpr::keyexpr_tree::impls::KeyedSetProvider;
use zenoh_keyexpr::keyexpr_tree::IKeyExprTreeMut;
use zenoh_keyexpr::keyexpr_tree::{support::UnknownWildness, KeBoxTree};
use zenoh_protocol::network::NetworkBody;
use zenoh_result::ZResult;

pub(crate) fn downsampling_interceptor_factories(
    config: &Vec<DownsamplingItemConf>,
) -> ZResult<Vec<InterceptorFactory>> {
    let mut res: Vec<InterceptorFactory> = vec![];

    for ds in config {
        res.push(Box::new(DownsamplingInterceptorFactory::new(ds.clone())));
    }

    Ok(res)
}

pub struct DownsamplingInterceptorFactory {
    interfaces: Option<Vec<String>>,
    rules: Vec<DownsamplingRuleConf>,
    flow: DownsamplingFlow,
}

impl DownsamplingInterceptorFactory {
    pub fn new(conf: DownsamplingItemConf) -> Self {
        Self {
            interfaces: conf.interfaces,
            rules: conf.rules,
            flow: conf.flow,
        }
    }
}

impl InterceptorFactoryTrait for DownsamplingInterceptorFactory {
    fn new_transport_unicast(
        &self,
        transport: &TransportUnicast,
    ) -> (Option<IngressInterceptor>, Option<EgressInterceptor>) {
        log::debug!("New downsampler transport unicast {:?}", transport);
        if let Some(interfaces) = &self.interfaces {
            log::debug!(
                "New downsampler transport unicast config interfaces: {:?}",
                interfaces
            );
            if let Ok(links) = transport.get_links() {
                for link in links {
                    log::debug!(
                        "New downsampler transport unicast link interfaces: {:?}",
                        link.interfaces
                    );
                    if !link.interfaces.iter().any(|x| interfaces.contains(x)) {
                        return (None, None);
                    }
                }
            }
        };

        match self.flow {
            DownsamplingFlow::Ingress => (
                Some(Box::new(DownsamplingInterceptor::new(self.rules.clone()))),
                None,
            ),
            DownsamplingFlow::Egress => (
                None,
                Some(Box::new(DownsamplingInterceptor::new(self.rules.clone()))),
            ),
        }
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

struct Timestate {
    pub threshold: tokio::time::Duration,
    pub latest_message_timestamp: tokio::time::Instant,
}

pub(crate) struct DownsamplingInterceptor {
    ke_state: Arc<Mutex<KeBoxTree<Timestate, UnknownWildness, KeyedSetProvider>>>,
}

impl InterceptorTrait for DownsamplingInterceptor {
    fn compute_keyexpr_cache(&self, _key_expr: &KeyExpr<'_>) -> Option<Box<dyn Any + Send + Sync>> {
        None
    }

    fn intercept(
        &self,
        ctx: RoutingContext<NetworkMessage>,
        _cache: Option<&Box<dyn Any + Send + Sync>>,
    ) -> Option<RoutingContext<NetworkMessage>> {
        if matches!(ctx.msg.body, NetworkBody::Push(_)) {
            if let Some(key_expr) = ctx.full_key_expr() {
                let mut ke_state = zlock!(self.ke_state);
                if let Some(state) = ke_state.weight_at_mut(&key_expr.clone()) {
                    let timestamp = tokio::time::Instant::now();

                    if timestamp - state.latest_message_timestamp >= state.threshold {
                        state.latest_message_timestamp = timestamp;
                        return Some(ctx);
                    } else {
                        return None;
                    }
                }
            }
        }

        Some(ctx)
    }
}

const NANOS_PER_SEC: f64 = 1_000_000_000.0;

impl DownsamplingInterceptor {
    pub fn new(rules: Vec<DownsamplingRuleConf>) -> Self {
        let mut ke_state = KeBoxTree::default();
        for rule in rules {
            let mut threshold = tokio::time::Duration::MAX;
            let mut latest_message_timestamp = tokio::time::Instant::now();
            if rule.rate != 0.0 {
                threshold =
                    tokio::time::Duration::from_nanos((1. / rule.rate * NANOS_PER_SEC) as u64);
                latest_message_timestamp -= threshold;
            }
            ke_state.insert(
                &rule.key_expr,
                Timestate {
                    threshold,
                    latest_message_timestamp,
                },
            );
        }
        Self {
            ke_state: Arc::new(Mutex::new(ke_state)),
        }
    }
}
