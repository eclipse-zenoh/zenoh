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
            if self.flows.ingress {
                " ingress"
            } else {
                Default::default()
            },
            if self.flows.egress {
                " egress"
            } else {
                Default::default()
            },
            transport
        );
        (
            self.flows.ingress.then(|| {
                Box::new(DownsamplingInterceptor::new(
                    self.messages.clone(),
                    &self.rules,
                    #[cfg(feature = "stats")]
                    InterceptorFlow::Ingress,
                    #[cfg(feature = "stats")]
                    transport.get_stats().unwrap_or_default(),
                )) as IngressInterceptor
            }),
            self.flows.egress.then(|| {
                Box::new(DownsamplingInterceptor::new(
                    self.messages.clone(),
                    &self.rules,
                    #[cfg(feature = "stats")]
                    InterceptorFlow::Egress,
                    #[cfg(feature = "stats")]
                    transport.get_stats().unwrap_or_default(),
                )) as EgressInterceptor
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
    delete: bool,
    put: bool,
    query: bool,
    reply: bool,
}

impl From<&NEVec<DownsamplingMessage>> for DownsamplingFilters {
    fn from(value: &NEVec<DownsamplingMessage>) -> Self {
        let mut res = Self::default();
        for v in value {
            match v {
                DownsamplingMessage::Delete => res.put = true,
                #[allow(deprecated)]
                DownsamplingMessage::Push => {
                    res.put = true;
                    res.delete = true;
                }
                DownsamplingMessage::Put => res.put = true,
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
    #[cfg(feature = "stats")]
    flow: InterceptorFlow,
    #[cfg(feature = "stats")]
    stats: Arc<TransportStats>,
}

impl DownsamplingInterceptor {
    fn is_msg_filtered(&self, msg: &mut NetworkMessageMut) -> bool {
        match msg.body {
            NetworkBodyMut::Push(Push {
                payload: PushBody::Del(_),
                ..
            }) => self.filtered_messages.delete,
            NetworkBodyMut::Push(Push {
                payload: PushBody::Put(_),
                ..
            }) => self.filtered_messages.put,
            NetworkBodyMut::Request(_) => self.filtered_messages.query,
            NetworkBodyMut::Response(_) => self.filtered_messages.reply,
            NetworkBodyMut::ResponseFinal(_) => false,
            NetworkBodyMut::Interest(_) => false,
            NetworkBodyMut::Declare(_) => false,
            NetworkBodyMut::OAM(_) => false,
        }
    }
    fn compute_id(&self, key_expr: &keyexpr) -> Option<usize> {
        let ke_id = zlock!(self.ke_id);
        let node = ke_id.intersecting_keys(key_expr).next()?;
        ke_id.weight_at(&node).copied()
    }
}

// The flag is used to print a message only once
static INFO_FLAG: AtomicBool = AtomicBool::new(false);

impl InterceptorTrait for DownsamplingInterceptor {
    fn compute_keyexpr_cache(&self, key_expr: &keyexpr) -> Option<Box<dyn Any + Send + Sync>> {
        Some(Box::new(self.compute_id(key_expr)))
    }

    fn intercept(&self, msg: &mut NetworkMessageMut, ctx: &mut dyn InterceptorContext) -> bool {
        if !self.is_msg_filtered(msg) {
            return true;
        }
        let Some(id) = ctx
            .get_cache(msg)
            .and_then(|c| c.downcast_ref::<Option<usize>>())
            .cloned()
            .unwrap_or_else(|| self.compute_id(&ctx.full_keyexpr(msg)?))
        else {
            return true;
        };

        let mut ke_state = zlock!(self.ke_state);
        let Some(state) = ke_state.get_mut(&id) else {
            tracing::debug!("unexpected cache ID {}", id);
            return true;
        };
        let timestamp = tokio::time::Instant::now();
        if timestamp - state.latest_message_timestamp >= state.threshold {
            state.latest_message_timestamp = timestamp;
            true
        } else {
            if !INFO_FLAG.swap(true, Ordering::Relaxed) {
                tracing::info!("Some message(s) have been dropped by the downsampling interceptor. Enable trace level tracing for more details.");
            }
            tracing::trace!(
                "Message dropped by the downsampling interceptor: {}({}) from:{} to:{}",
                msg,
                ctx.full_expr(msg).unwrap_or_default(),
                ctx.face().map(|f| f.to_string()).unwrap_or_default(),
                ctx.face().map(|f| f.to_string()).unwrap_or_default(),
            );
            #[cfg(feature = "stats")]
            match self.flow {
                InterceptorFlow::Egress => {
                    self.stats.inc_tx_downsampler_dropped_msgs(1);
                }
                InterceptorFlow::Ingress => {
                    self.stats.inc_rx_downsampler_dropped_msgs(1);
                }
            }
            false
        }
    }
}

const NANOS_PER_SEC: f64 = 1_000_000_000.0;

impl DownsamplingInterceptor {
    pub fn new(
        messages: Arc<DownsamplingFilters>,
        rules: &NEVec<DownsamplingRuleConf>,
        #[cfg(feature = "stats")] flow: InterceptorFlow,
        #[cfg(feature = "stats")] stats: Arc<TransportStats>,
    ) -> Self {
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
            #[cfg(feature = "stats")]
            flow,
            #[cfg(feature = "stats")]
            stats,
        }
    }
}
