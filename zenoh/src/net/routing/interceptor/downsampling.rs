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
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use nonempty_collections::NEVec;
use zenoh_config::{DownsamplingItemConf, DownsamplingMessage, DownsamplingRuleConf};
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

        res.push(Box::new(DownsamplingInterceptorFactory::new(ds.clone())));
    }

    Ok(res)
}

pub struct DownsamplingInterceptorFactory {
    interfaces: Option<NEVec<String>>,
    link_protocols: Option<NEVec<InterceptorLink>>,
    rules: NEVec<DownsamplingRuleConf>,
    flows: InterfaceEnabled,
    messages: Arc<DownsamplingFilters>,
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
            messages: Arc::new((&conf.messages).into()),
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
        };
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
        };

        tracing::debug!(
            "New{}{} downsampler on transport unicast {:?}",
            self.flows.ingress.then_some(" ingress").unwrap_or_default(),
            self.flows.egress.then_some(" egress").unwrap_or_default(),
            transport
        );
        (
            self.flows.ingress.then(|| {
                Box::new(ComputeOnMiss::new(DownsamplingInterceptor::new(
                    self.messages.clone(),
                    &self.rules,
                ))) as IngressInterceptor
            }),
            self.flows.egress.then(|| {
                Box::new(ComputeOnMiss::new(DownsamplingInterceptor::new(
                    self.messages.clone(),
                    &self.rules,
                ))) as EgressInterceptor
            }),
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

#[derive(Debug, Default, Clone)]
pub(crate) struct DownsamplingFilters {
    push: bool,
    query: bool,
    reply: bool,
}

impl From<&NEVec<DownsamplingMessage>> for DownsamplingFilters {
    fn from(value: &NEVec<DownsamplingMessage>) -> Self {
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

// The flag is used to print a message only once
static INFO_FLAG: AtomicBool = AtomicBool::new(false);

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
                                if !INFO_FLAG.swap(true, Ordering::Relaxed) {
                                    tracing::info!("Some message(s) have been dropped by the downsampling interceptor. Enable trace level tracing for more details.");
                                }
                                tracing::trace!(
                                    "Message dropped by the downsampling interceptor: {}({}) from:{} to:{}",
                                    ctx.msg,
                                    ctx.full_expr().unwrap_or_default(),
                                    ctx.inface().map(|f| f.to_string()).unwrap_or_default(),
                                    ctx.outface().map(|f| f.to_string()).unwrap_or_default(),
                                );
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
    pub fn new(messages: Arc<DownsamplingFilters>, rules: &NEVec<DownsamplingRuleConf>) -> Self {
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
                "New downsampler rule enabled: key_expr={:?}, threshold={:?}, messages={:?}",
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
