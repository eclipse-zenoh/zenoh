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

use std::{collections::HashSet, sync::Arc};

use zenoh_config::{
    qos::{QosOverwriteMessage, QosOverwrites},
    QosOverwriteItemConf,
};
use zenoh_keyexpr::keyexpr_tree::{IKeyExprTree, IKeyExprTreeMut, IKeyExprTreeNode, KeBoxTree};
use zenoh_protocol::{
    network::{NetworkBodyMut, Push, Request, Response},
    zenoh::PushBody,
};
use zenoh_result::ZResult;

use crate::net::routing::interceptor::*;

pub(crate) fn qos_overwrite_interceptor_factories(
    config: &Vec<QosOverwriteItemConf>,
) -> ZResult<Vec<InterceptorFactory>> {
    let mut res: Vec<InterceptorFactory> = vec![];

    let mut id_set = HashSet::new();
    for q in config {
        // check unicity of rule id
        if let Some(id) = &q.id {
            if !id_set.insert(id.clone()) {
                bail!("Invalid Qos Overwrite config: id '{id}' is repeated");
            }
        }
        // check for undefined flows and initialize them
        res.push(Box::new(QosOverwriteFactory::new(q.clone())));
    }

    Ok(res)
}

pub struct QosOverwriteFactory {
    interfaces: Option<NEVec<String>>,
    link_protocols: Option<NEVec<InterceptorLink>>,
    overwrite: QosOverwrites,
    flows: InterfaceEnabled,
    filter: QosOverwriteFilter,
    keys: Arc<KeBoxTree<()>>,
}

impl QosOverwriteFactory {
    pub fn new(conf: QosOverwriteItemConf) -> Self {
        let mut keys = KeBoxTree::new();
        for k in &conf.key_exprs {
            keys.insert(k, ());
        }

        Self {
            interfaces: conf.interfaces,
            link_protocols: conf.link_protocols,
            overwrite: conf.overwrite.clone(),
            flows: conf.flows.map(|f| (&f).into()).unwrap_or(InterfaceEnabled {
                ingress: true,
                egress: true,
            }),
            filter: (&conf.messages).into(),
            keys: Arc::new(keys),
        }
    }
}

impl InterceptorFactoryTrait for QosOverwriteFactory {
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
        };

        tracing::debug!(
            "New{}{} qos overwriter on transport unicast {:?}",
            self.flows.ingress.then_some(" ingress").unwrap_or_default(),
            self.flows.egress.then_some(" egress").unwrap_or_default(),
            transport
        );
        (
            self.flows.ingress.then(|| {
                Box::new(QosInterceptor {
                    filter: self.filter.clone(),
                    keys: self.keys.clone(),
                    overwrite: self.overwrite.clone(),
                }) as IngressInterceptor
            }),
            self.flows.egress.then(|| {
                Box::new(QosInterceptor {
                    filter: self.filter.clone(),
                    keys: self.keys.clone(),
                    overwrite: self.overwrite.clone(),
                }) as EgressInterceptor
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
pub(crate) struct QosOverwriteFilter {
    put: bool,
    delete: bool,
    query: bool,
    reply: bool,
}

impl From<&NEVec<QosOverwriteMessage>> for QosOverwriteFilter {
    fn from(value: &NEVec<QosOverwriteMessage>) -> Self {
        let mut res = Self::default();
        for v in value {
            match v {
                QosOverwriteMessage::Put => res.put = true,
                QosOverwriteMessage::Delete => res.delete = true,
                QosOverwriteMessage::Query => res.query = true,
                QosOverwriteMessage::Reply => res.reply = true,
            }
        }
        res
    }
}

pub(crate) struct QosInterceptor {
    filter: QosOverwriteFilter,
    overwrite: QosOverwrites,
    keys: Arc<KeBoxTree<()>>,
}

struct Cache {
    is_ke_affected: bool,
}

impl QosInterceptor {
    fn is_ke_affected(&self, ke: &keyexpr) -> bool {
        self.keys.nodes_including(ke).any(|n| n.weight().is_some())
    }

    fn overwrite_qos<const ID: u8>(
        &self,
        message: QosOverwriteMessage,
        qos: &mut zenoh_protocol::network::ext::QoSType<ID>,
    ) {
        if let Some(p) = self.overwrite.priority {
            qos.set_priority(p.into());
        }
        if let Some(c) = self.overwrite.congestion_control {
            qos.set_congestion_control(c.into());
        }
        if let Some(e) = self.overwrite.express {
            qos.set_is_express(e);
        }
        tracing::trace!("Overwriting QoS for {:?} to {:?}", message, qos);
    }

    fn is_ke_affected_from_cache_or_ctx(
        &self,
        cache: Option<&Cache>,
        ctx: &RoutingContext<NetworkMessageMut<'_>>,
    ) -> bool {
        cache.map(|v| v.is_ke_affected).unwrap_or_else(|| {
            ctx.full_keyexpr()
                .as_ref()
                .map(|ke| self.is_ke_affected(ke))
                .unwrap_or(false)
        })
    }
}

impl InterceptorTrait for QosInterceptor {
    fn compute_keyexpr_cache(&self, key_expr: &keyexpr) -> Option<Box<dyn Any + Send + Sync>> {
        let cache = Cache {
            is_ke_affected: self.is_ke_affected(key_expr),
        };
        Some(Box::new(cache))
    }

    fn intercept(
        &self,
        ctx: &mut RoutingContext<NetworkMessageMut<'_>>,
        cache: Option<&Box<dyn Any + Send + Sync>>,
    ) -> bool {
        let cache = cache.and_then(|i| match i.downcast_ref::<Cache>() {
            Some(c) => Some(c),
            None => {
                tracing::debug!("Cache content type is incorrect");
                None
            }
        });

        let should_overwrite = match &ctx.msg.body {
            NetworkBodyMut::Push(Push {
                payload: PushBody::Put(_),
                ..
            }) => self.filter.put && self.is_ke_affected_from_cache_or_ctx(cache, ctx),
            NetworkBodyMut::Push(Push {
                payload: PushBody::Del(_),
                ..
            }) => self.filter.delete && self.is_ke_affected_from_cache_or_ctx(cache, ctx),
            NetworkBodyMut::Request(_) => {
                self.filter.query && self.is_ke_affected_from_cache_or_ctx(cache, ctx)
            }
            NetworkBodyMut::Response(_) => {
                self.filter.reply && self.is_ke_affected_from_cache_or_ctx(cache, ctx)
            }
            NetworkBodyMut::ResponseFinal(_) => false,
            NetworkBodyMut::Interest(_) => false,
            NetworkBodyMut::Declare(_) => false,
            NetworkBodyMut::OAM(_) => false,
        };
        if !should_overwrite {
            return true;
        }

        match &mut ctx.msg.body {
            NetworkBodyMut::Request(Request { ext_qos, .. }) => {
                self.overwrite_qos(QosOverwriteMessage::Query, ext_qos);
            }
            NetworkBodyMut::Response(Response { ext_qos, .. }) => {
                self.overwrite_qos(QosOverwriteMessage::Reply, ext_qos);
            }
            NetworkBodyMut::Push(Push {
                payload: PushBody::Put(_),
                ext_qos,
                ..
            }) => {
                self.overwrite_qos(QosOverwriteMessage::Put, ext_qos);
            }
            NetworkBodyMut::Push(Push {
                payload: PushBody::Del(_),
                ext_qos,
                ..
            }) => {
                self.overwrite_qos(QosOverwriteMessage::Delete, ext_qos);
            }
            // unaffected message types
            NetworkBodyMut::ResponseFinal(_) => {}
            NetworkBodyMut::Declare(_) => {}
            NetworkBodyMut::Interest(_) => {}
            NetworkBodyMut::OAM(_) => {}
        }
        true
    }
}
