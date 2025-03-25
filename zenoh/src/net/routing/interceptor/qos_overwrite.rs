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

use zenoh_config::qos::{QosOverwriteItemConf, QosOverwriteMessage, QosOverwrites};
use zenoh_keyexpr::{
    keyexpr,
    keyexpr_tree::{IKeyExprTree, IKeyExprTreeMut, IKeyExprTreeNode, KeBoxTree},
};
use zenoh_protocol::{
    network::{Declare, DeclareBody, NetworkBody, Push, Request, Response},
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
        let mut q = q.clone();
        // check for undefined flows and initialize them
        let flows = q
            .flows
            .get_or_insert(vec![InterceptorFlow::Ingress, InterceptorFlow::Egress]);
        if flows.is_empty() {
            bail!("Invalid Qos Overwrite config: flows list must not be empty");
        }
        // check for empty interfaces
        if q.interfaces.as_ref().map(|i| i.is_empty()).unwrap_or(false) {
            bail!("Invalid Qos Overwrite config: interfaces list must not be empty");
        }
        // check for empty messages list
        if q.messages.is_empty() {
            bail!("Invalid Qos Overwrite config: messages list must not be empty");
        }
        res.push(Box::new(QosOverwriteFactory::new(q)));
    }

    Ok(res)
}

pub struct QosOverwriteFactory {
    interfaces: Option<Vec<String>>,
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
            overwrite: conf.overwrite.clone(),
            flows: conf
                .flows
                .expect("config flows should be set")
                .as_slice()
                .into(),
            filter: conf.messages.as_slice().into(),
            keys: Arc::new(keys),
        }
    }
}

impl InterceptorFactoryTrait for QosOverwriteFactory {
    fn new_transport_unicast(
        &self,
        transport: &TransportUnicast,
    ) -> (Option<IngressInterceptor>, Option<EgressInterceptor>) {
        tracing::debug!("New qos overwriter transport unicast {:?}", transport);
        if let Some(interfaces) = &self.interfaces {
            tracing::debug!(
                "New qos overwriter transport unicast config interfaces: {:?}",
                interfaces
            );
            if let Ok(links) = transport.get_links() {
                for link in links {
                    tracing::debug!(
                        "New qos overwriter transport unicast link interfaces: {:?}",
                        link.interfaces
                    );
                    if !link.interfaces.iter().any(|x| interfaces.contains(x)) {
                        return (None, None);
                    }
                }
            }
        }
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

impl From<&[QosOverwriteMessage]> for QosOverwriteFilter {
    fn from(value: &[QosOverwriteMessage]) -> Self {
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
    key: String,
    should_overwrite: bool,
}

impl QosInterceptor {
    fn should_overwrite(&self, cached: Option<bool>, key: &str) -> bool {
        if let Some(val) = cached {
            return val;
        }
        match keyexpr::new(key) {
            Ok(ke) => self.keys.nodes_including(ke).any(|n| n.weight().is_some()),
            Err(_) => false,
        }
    }

    fn overwrite_qos<const ID: u8>(&self, qos: &mut zenoh_protocol::network::ext::QoSType<ID>) {
        if let Some(p) = self.overwrite.priority {
            qos.set_priority(p.into());
        }
        if let Some(c) = self.overwrite.congestion_control {
            qos.set_congestion_control(c.into());
        }
        if let Some(e) = self.overwrite.express {
            qos.set_is_express(e);
        }
    }
}

impl InterceptorTrait for QosInterceptor {
    fn compute_keyexpr_cache(&self, key_expr: &KeyExpr<'_>) -> Option<Box<dyn Any + Send + Sync>> {
        let cache = Cache {
            key: key_expr.as_str().to_owned(),
            should_overwrite: self.should_overwrite(None, key_expr),
        };
        Some(Box::new(cache))
    }

    fn intercept(
        &self,
        mut ctx: RoutingContext<NetworkMessage>,
        cache: Option<&Box<dyn Any + Send + Sync>>,
    ) -> Option<RoutingContext<NetworkMessage>> {
        let cache = cache.and_then(|i| match i.downcast_ref::<Cache>() {
            Some(c) => Some(c),
            None => {
                tracing::debug!("Cache content type is incorrect");
                None
            }
        });

        let key = cache
            .map(|c| c.key.as_str())
            .or_else(|| ctx.full_expr())
            .unwrap_or("");
        if key.is_empty() {
            return Some(ctx);
        }
        let should_overwrite = self.should_overwrite(cache.map(|c| c.should_overwrite), key);
        if !should_overwrite {
            return Some(ctx);
        }
        tracing::trace!("Processing message on {}", key);
        match &mut ctx.msg.body {
            NetworkBody::Request(Request { ext_qos, .. }) => {
                if self.filter.query {
                    self.overwrite_qos(ext_qos);
                    tracing::trace!(
                        "Overwriting QoS for {:?} to {:?}",
                        QosOverwriteMessage::Query,
                        ext_qos
                    );
                }
            }
            NetworkBody::Response(Response { ext_qos, .. }) => {
                if self.filter.reply {
                    self.overwrite_qos(ext_qos);
                    tracing::trace!(
                        "Overwriting QoS for {:?} to {:?}",
                        QosOverwriteMessage::Reply,
                        ext_qos
                    );
                }
            }
            NetworkBody::Push(Push {
                payload: PushBody::Put(_),
                ext_qos,
                ..
            }) => {
                if self.filter.put {
                    self.overwrite_qos(ext_qos);
                    tracing::trace!(
                        "Overwriting QoS for {:?} to {:?}",
                        QosOverwriteMessage::Put,
                        ext_qos
                    );
                }
            }
            NetworkBody::Push(Push {
                payload: PushBody::Del(_),
                ext_qos,
                ..
            }) => {
                if self.filter.delete {
                    self.overwrite_qos(ext_qos);
                    tracing::trace!(
                        "Overwriting QoS for {:?} to {:?}",
                        QosOverwriteMessage::Delete,
                        ext_qos
                    );
                }
            }
            // unaffected message types
            NetworkBody::Declare(Declare {
                body: DeclareBody::DeclareKeyExpr(_),
                ..
            })
            | NetworkBody::Declare(Declare {
                body: DeclareBody::DeclareToken(_),
                ..
            })
            | NetworkBody::Declare(Declare {
                body: DeclareBody::DeclareFinal(_),
                ..
            })
            | NetworkBody::Declare(Declare {
                body: DeclareBody::DeclareQueryable(_),
                ..
            })
            | NetworkBody::Declare(Declare {
                body: DeclareBody::DeclareSubscriber(_),
                ..
            })
            | NetworkBody::Declare(Declare {
                body: DeclareBody::UndeclareToken(_),
                ..
            })
            | NetworkBody::Declare(Declare {
                body: DeclareBody::UndeclareKeyExpr(_),
                ..
            })
            | NetworkBody::Declare(Declare {
                body: DeclareBody::UndeclareQueryable(_),
                ..
            })
            | NetworkBody::Declare(Declare {
                body: DeclareBody::UndeclareSubscriber(_),
                ..
            }) => {}
            NetworkBody::Interest(_) | NetworkBody::OAM(_) | NetworkBody::ResponseFinal(_) => {}
        }
        Some(ctx)
    }
}
