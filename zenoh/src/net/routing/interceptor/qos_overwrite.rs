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

use std::{collections::HashSet, ops::RangeBounds as _, sync::Arc};

use zenoh_buffers::buffer::Buffer as _;
use zenoh_config::{
    qos::{QosFilter, QosOverwriteMessage, QosOverwrites},
    ConfRange, QosOverwriteItemConf, ZenohId,
};
use zenoh_keyexpr::keyexpr_tree::{IKeyExprTree, IKeyExprTreeMut, IKeyExprTreeNode, KeBoxTree};
use zenoh_protocol::{
    core::Priority,
    network::{NetworkBodyMut, NetworkMessageExt as _, Push, Request, Response},
    zenoh::{reply::ReplyBody, PushBody, Query, Reply, RequestBody, ResponseBody},
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
    zids: Option<NEVec<ZenohId>>,
    interfaces: Option<NEVec<String>>,
    link_protocols: Option<NEVec<InterceptorLink>>,
    overwrite: QosOverwrites,
    flows: InterfaceEnabled,
    filter: QosOverwriteFilter,
    keys: Option<Arc<KeBoxTree<()>>>,
}

impl QosOverwriteFactory {
    pub fn new(conf: QosOverwriteItemConf) -> Self {
        let keys = if let Some(key_exprs) = conf.key_exprs.as_ref() {
            let mut keys = KeBoxTree::new();
            for k in key_exprs {
                keys.insert(k, ());
            }
            Some(Arc::new(keys))
        } else {
            None
        };

        let mut filter = QosOverwriteFilter::default();
        for v in conf.messages {
            match v {
                QosOverwriteMessage::Put => filter.put = true,
                QosOverwriteMessage::Delete => filter.delete = true,
                QosOverwriteMessage::Query => filter.query = true,
                QosOverwriteMessage::Reply => filter.reply = true,
            }
        }
        filter.qos = conf.qos;
        filter.payload_size = conf.payload_size;

        Self {
            zids: conf.zids,
            interfaces: conf.interfaces,
            link_protocols: conf.link_protocols,
            overwrite: conf.overwrite.clone(),
            flows: conf.flows.map(|f| (&f).into()).unwrap_or(InterfaceEnabled {
                ingress: true,
                egress: true,
            }),
            filter,
            keys,
        }
    }
}

impl InterceptorFactoryTrait for QosOverwriteFactory {
    fn new_transport_unicast(
        &self,
        transport: &TransportUnicast,
    ) -> (Option<IngressInterceptor>, Option<EgressInterceptor>) {
        if let Some(zids) = &self.zids {
            if let Ok(zid) = transport.get_zid() {
                if !zids.contains(&zid.into()) {
                    return (None, None);
                }
            }
        }
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
    qos: Option<QosFilter>,
    payload_size: Option<ConfRange>,
}

pub(crate) struct QosInterceptor {
    filter: QosOverwriteFilter,
    overwrite: QosOverwrites,
    keys: Option<Arc<KeBoxTree<()>>>,
}

struct Cache {
    is_ke_affected: bool,
}

impl QosInterceptor {
    #[inline]
    fn is_ke_affected(&self, ke: &keyexpr) -> bool {
        match &self.keys {
            Some(keys) => keys.nodes_including(ke).any(|n| n.weight().is_some()),
            None => true,
        }
    }

    #[inline]
    fn overwrite_qos<const ID: u8>(
        &self,
        message: QosOverwriteMessage,
        qos: &mut zenoh_protocol::network::ext::QoSType<ID>,
    ) {
        if let Some(p) = self.overwrite.priority {
            match p {
                zenoh_config::qos::PriorityUpdateConf::Priority(p) => {
                    qos.set_priority(p.into());
                }
                zenoh_config::qos::PriorityUpdateConf::Increment(i) => {
                    if 1 >= 0 {
                        qos.set_priority(
                            (qos.get_priority() as u8)
                                .saturating_add(i.try_into().unwrap_or(0))
                                .try_into()
                                .unwrap_or(Priority::Background),
                        );
                    } else {
                        qos.set_priority(
                            (qos.get_priority() as u8)
                                .saturating_sub((-i).try_into().unwrap_or(0))
                                .try_into()
                                .unwrap_or(Priority::Control),
                        );
                    }
                }
            }
        }
        if let Some(c) = self.overwrite.congestion_control {
            qos.set_congestion_control(c.into());
        }
        if let Some(e) = self.overwrite.express {
            qos.set_is_express(e);
        }
        tracing::trace!("Overwriting QoS for {:?} to {:?}", message, qos);
    }

    #[inline]
    fn is_ke_affected_from_cache_or_ctx(
        &self,
        cache: Option<&Cache>,
        msg: &mut NetworkMessageMut,
        ctx: &dyn InterceptorContext,
    ) -> bool {
        self.keys.is_none()
            || cache.map(|v| v.is_ke_affected).unwrap_or_else(|| {
                ctx.full_keyexpr(msg)
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

    fn intercept(&self, msg: &mut NetworkMessageMut, ctx: &mut dyn InterceptorContext) -> bool {
        let cache = self
            .keys
            .is_some()
            .then(|| {
                ctx.get_cache(msg)
                    .and_then(|i| match i.downcast_ref::<Cache>() {
                        Some(c) => Some(c),
                        None => {
                            tracing::debug!("Cache content type is incorrect");
                            None
                        }
                    })
            })
            .flatten();

        let mut should_overwrite = match &msg.body {
            NetworkBodyMut::Push(Push {
                payload: PushBody::Put(_),
                ..
            }) => self.filter.put && self.is_ke_affected_from_cache_or_ctx(cache, msg, ctx),
            NetworkBodyMut::Push(Push {
                payload: PushBody::Del(_),
                ..
            }) => self.filter.delete && self.is_ke_affected_from_cache_or_ctx(cache, msg, ctx),
            NetworkBodyMut::Request(_) => {
                self.filter.query && self.is_ke_affected_from_cache_or_ctx(cache, msg, ctx)
            }
            NetworkBodyMut::Response(_) => {
                self.filter.reply && self.is_ke_affected_from_cache_or_ctx(cache, msg, ctx)
            }
            NetworkBodyMut::ResponseFinal(_) => false,
            NetworkBodyMut::Interest(_) => false,
            NetworkBodyMut::Declare(_) => false,
            NetworkBodyMut::OAM(_) => false,
        };
        if let Some(qos) = self.filter.qos.as_ref() {
            if let Some(prio) = qos.priority.as_ref() {
                if !msg.priority().eq(&((*prio).into())) {
                    should_overwrite = false;
                }
            }
            if let Some(cc) = qos.congestion_control.as_ref() {
                if !msg.congestion_control().eq(&((*cc).into())) {
                    should_overwrite = false;
                }
            }
            if let Some(express) = qos.express.as_ref() {
                if msg.is_express() != *express {
                    should_overwrite = false;
                }
            }
            if let Some(reliability) = qos.reliability.as_ref() {
                if msg.reliability() != (*reliability).into() {
                    should_overwrite = false;
                }
            }
        }
        if let Some(payload_size_range) = self.filter.payload_size.as_ref() {
            let msg_payload_size = match &msg.body {
                NetworkBodyMut::Push(Push {
                    payload: PushBody::Put(put),
                    ..
                }) => put.payload.len(),
                NetworkBodyMut::Request(Request {
                    payload:
                        RequestBody::Query(Query {
                            ext_body: Some(body),
                            ..
                        }),
                    ..
                }) => body.payload.len(),
                NetworkBodyMut::Response(Response {
                    payload:
                        ResponseBody::Reply(Reply {
                            payload: ReplyBody::Put(put),
                            ..
                        }),
                    ..
                }) => put.payload.len(),
                NetworkBodyMut::Response(Response {
                    payload: ResponseBody::Err(err),
                    ..
                }) => err.payload.len(),
                _ => 0,
            };
            if !payload_size_range.contains(&(msg_payload_size as u64)) {
                should_overwrite = false;
            }
        }
        if !should_overwrite {
            return true;
        }

        match &mut msg.body {
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
