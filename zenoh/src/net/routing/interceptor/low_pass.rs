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

use std::{any::Any, collections::HashSet, iter, sync::Arc};

use ahash::HashMap;
use itertools::Itertools;
use nonempty_collections::nev;
use zenoh_buffers::buffer::Buffer;
use zenoh_config::{InterceptorFlow, InterceptorLink, LowPassFilterConf, LowPassFilterMessage};
use zenoh_keyexpr::{
    keyexpr,
    keyexpr_tree::{IKeyExprTree, IKeyExprTreeMut, IKeyExprTreeNode, KeBoxTree},
};
use zenoh_protocol::{
    network::{NetworkBodyMut, NetworkMessageMut, Push, Request, Response},
    zenoh::{PushBody, Reply, RequestBody, ResponseBody},
};
use zenoh_result::ZResult;
#[cfg(feature = "stats")]
use zenoh_transport::stats::TransportStats;
use zenoh_transport::{multicast::TransportMulticast, unicast::TransportUnicast};

use super::{
    authorization::SubjectProperty, EgressInterceptor, IngressInterceptor, InterceptorFactory,
    InterceptorFactoryTrait, InterceptorLinkWrapper, InterceptorTrait, InterfaceEnabled,
};
use crate::net::routing::interceptor::InterceptorContext;

pub(crate) fn low_pass_interceptor_factories(
    config: &Vec<LowPassFilterConf>,
) -> ZResult<Vec<InterceptorFactory>> {
    let mut res: Vec<InterceptorFactory> = vec![];

    if !config.is_empty() {
        validate_config(config).map_err(|e| format!("Invalid low-pass filter config: {e}"))?;
        res.push(Box::new(LowPassInterceptorFactory::new(config)));
    }

    Ok(res)
}

fn validate_config(config: &Vec<LowPassFilterConf>) -> ZResult<()> {
    let mut id_set = HashSet::new();
    for lpf in config {
        if let Some(id) = &lpf.id {
            if !id_set.insert(id.clone()) {
                bail!("id '{id}' is repeated");
            }
        }
    }
    Ok(())
}

#[derive(Default)]
pub struct LowPassInterceptorFactory {
    state: Arc<LowPassFilter>,
}

impl LowPassInterceptorFactory {
    pub fn new(config: &Vec<LowPassFilterConf>) -> Self {
        let mut low_pass_filter = LowPassFilter::default();

        for lpf_config in config {
            let mut lpf_config = lpf_config.clone();
            let flows = lpf_config
                .flows
                .get_or_insert(nev![InterceptorFlow::Ingress, InterceptorFlow::Egress]);
            let interfaces = lpf_config
                .interfaces
                .map(|v| {
                    v.iter()
                        .map(|face| SubjectProperty::Exactly(face.clone()))
                        .collect_vec()
                })
                .unwrap_or(vec![SubjectProperty::Wildcard]);
            let link_protocols = lpf_config
                .link_protocols
                .map(|v| {
                    v.iter()
                        .map(|proto| SubjectProperty::Exactly(proto.clone()))
                        .collect_vec()
                })
                .unwrap_or(vec![SubjectProperty::Wildcard]);
            let rule_subject_ids = interfaces
                .into_iter()
                .cartesian_product(link_protocols)
                .map(|(interface, link_type)| {
                    let subject = LowPassSubject {
                        interface,
                        link_type,
                    };
                    low_pass_filter.subjects.get_or_insert(&subject)
                })
                .collect_vec();
            for message in &lpf_config.messages {
                for key_expr in &lpf_config.key_exprs {
                    for flow in &*flows {
                        for id in &rule_subject_ids {
                            low_pass_filter
                                .filters
                                .entry(*id)
                                .or_default()
                                .flow_mut(flow)
                                .message_mut(message)
                                .insert(key_expr, LowPassFilterRule::new(lpf_config.size_limit));
                            match flow {
                                InterceptorFlow::Egress => {
                                    low_pass_filter.interface_enabled.egress = true
                                }
                                InterceptorFlow::Ingress => {
                                    low_pass_filter.interface_enabled.ingress = true
                                }
                            }
                        }
                    }
                }
            }
        }
        Self {
            state: low_pass_filter.into(),
        }
    }
}

impl InterceptorFactoryTrait for LowPassInterceptorFactory {
    fn new_transport_unicast(
        &self,
        transport: &TransportUnicast,
    ) -> (Option<IngressInterceptor>, Option<EgressInterceptor>) {
        tracing::debug!("New low-pass filter transport unicast {:?}", transport);
        let links = match transport.get_links() {
            Ok(links) => links,
            Err(e) => {
                tracing::error!("Unable to get links from transport {:?}: {e}", transport);
                return (None, None);
            }
        };
        let auth_ids = match transport.get_auth_ids() {
            Ok(auth_ids) => auth_ids,
            Err(e) => {
                tracing::error!("Unable to get auth_ids for transport {:?}: {e}", transport);
                return (None, None);
            }
        };

        let interfaces = links
            .into_iter()
            .flat_map(|link| link.interfaces)
            .map(SubjectProperty::Exactly)
            .chain(iter::once(SubjectProperty::Wildcard));
        let link_types = auth_ids
            .link_auth_ids()
            .iter()
            .map(|auth_id| SubjectProperty::Exactly(InterceptorLinkWrapper::from(auth_id).0))
            .chain(iter::once(SubjectProperty::Wildcard));

        let subject_ids = interfaces
            .cartesian_product(link_types)
            .filter_map(|(interface, link_type)| {
                let subject = LowPassSubject {
                    interface,
                    link_type,
                };
                self.state.subjects.get_subject_id(&subject)
            })
            .collect_vec();
        if !subject_ids.is_empty() {
            let subject_ids = Arc::new(subject_ids);
            return (
                self.state.interface_enabled.ingress.then(|| {
                    Box::new(LowPassInterceptor::new(
                        self.state.clone(),
                        subject_ids.clone(),
                        InterceptorFlow::Ingress,
                        #[cfg(feature = "stats")]
                        transport.get_stats().unwrap_or_default(),
                    )) as IngressInterceptor
                }),
                self.state.interface_enabled.egress.then(|| {
                    Box::new(LowPassInterceptor::new(
                        self.state.clone(),
                        subject_ids,
                        InterceptorFlow::Egress,
                        #[cfg(feature = "stats")]
                        transport.get_stats().unwrap_or_default(),
                    )) as EgressInterceptor
                }),
            );
        }
        (None, None)
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

pub struct LowPassInterceptor {
    inner: Arc<LowPassFilter>,
    subjects: Arc<Vec<usize>>,
    // NOTE: memory usage could be optimized by replacing flow with a PhantomData<T>
    flow: InterceptorFlow,
    #[cfg(feature = "stats")]
    stats: Arc<TransportStats>,
}

impl LowPassInterceptor {
    fn new(
        inner: Arc<LowPassFilter>,
        subjects: Arc<Vec<usize>>,
        flow: InterceptorFlow,
        #[cfg(feature = "stats")] stats: Arc<TransportStats>,
    ) -> Self {
        Self {
            inner,
            subjects,
            flow,
            #[cfg(feature = "stats")]
            stats,
        }
    }

    fn message_passes_filters(
        &self,
        msg: &mut NetworkMessageMut,
        ctx: &dyn InterceptorContext,
        cache: Option<&Cache>,
    ) -> Result<(), usize> {
        let payload_size: usize;
        let attachment_size: usize;
        let message_type: LowPassFilterMessage;
        let max_allowed_size: Option<usize>;

        match &msg.body {
            NetworkBodyMut::Request(Request {
                payload: RequestBody::Query(query),
                ..
            }) => {
                message_type = LowPassFilterMessage::Query;
                payload_size = query
                    .ext_body
                    .as_ref()
                    .map(|body| body.payload.len())
                    .unwrap_or(0);
                attachment_size = query
                    .ext_attachment
                    .as_ref()
                    .map(|att| att.buffer.len())
                    .unwrap_or(0);
                max_allowed_size = cache.map(|c| c.query);
            }
            NetworkBodyMut::Response(Response {
                payload:
                    ResponseBody::Reply(Reply {
                        payload: PushBody::Put(put),
                        ..
                    }),
                ..
            }) => {
                message_type = LowPassFilterMessage::Reply;
                payload_size = put.payload.len();
                attachment_size = put
                    .ext_attachment
                    .as_ref()
                    .map(|att| att.buffer.len())
                    .unwrap_or(0);
                max_allowed_size = cache.map(|c| c.reply);
            }
            NetworkBodyMut::Response(Response {
                payload:
                    ResponseBody::Reply(Reply {
                        payload: PushBody::Del(delete),
                        ..
                    }),
                ..
            }) => {
                message_type = LowPassFilterMessage::Reply;
                payload_size = 0;
                attachment_size = delete
                    .ext_attachment
                    .as_ref()
                    .map(|att| att.buffer.len())
                    .unwrap_or(0);
                max_allowed_size = cache.map(|c| c.reply);
            }
            NetworkBodyMut::Push(Push {
                payload: PushBody::Put(put),
                ..
            }) => {
                message_type = LowPassFilterMessage::Put;
                payload_size = put.payload.len();
                attachment_size = put
                    .ext_attachment
                    .as_ref()
                    .map(|att| att.buffer.len())
                    .unwrap_or(0);
                max_allowed_size = cache.map(|c| c.put);
            }
            NetworkBodyMut::Push(Push {
                payload: PushBody::Del(delete),
                ..
            }) => {
                message_type = LowPassFilterMessage::Delete;
                payload_size = 0;
                attachment_size = delete
                    .ext_attachment
                    .as_ref()
                    .map(|att| att.buffer.len())
                    .unwrap_or(0);
                max_allowed_size = cache.map(|c| c.delete);
            }
            NetworkBodyMut::Response(Response {
                payload: ResponseBody::Err(zenoh_protocol::zenoh::Err { payload, .. }),
                ..
            }) => {
                message_type = LowPassFilterMessage::Reply;
                payload_size = payload.len();
                attachment_size = 0;
                max_allowed_size = cache.map(|c| c.reply);
            }
            NetworkBodyMut::ResponseFinal(_) => return Ok(()),
            NetworkBodyMut::Interest(_) => return Ok(()),
            NetworkBodyMut::Declare(_) => return Ok(()),
            NetworkBodyMut::OAM(_) => return Ok(()),
        }
        let max_allowed_size = match max_allowed_size {
            Some(v) => v,
            None => match ctx.full_keyexpr(msg).as_ref() {
                Some(ke) => self.get_max_allowed_message_size(message_type, ke),
                None => 0,
            },
        };
        match payload_size.checked_add(attachment_size) {
            Some(msg_size) => (msg_size <= max_allowed_size).then_some(()).ok_or(msg_size),
            None => Err(usize::MAX),
        }
    }

    fn get_max_allowed_message_size(
        &self,
        message: LowPassFilterMessage,
        key_expr: &keyexpr,
    ) -> usize {
        match self
            .subjects
            .iter()
            .filter_map(|s| {
                // get min of matching nodes within the KeBoxTree of the subject/flow/message
                self.inner
                    .filters
                    .get(s)
                    .expect("subject should have entry in map")
                    .flow(&self.flow)
                    .message(&message)
                    .nodes_including(key_expr)
                    .filter(|node| node.weight().is_some())
                    .min_by_key(|node| node.weight().expect("weight should not be none").0)
            })
            // get min of matching nodes from different KeBoxTrees
            .min_by_key(|node| node.weight().expect("weight should not be none").0)
            .map(|node| node.weight().expect("weight should not be none"))
        {
            Some(w) => w.0,
            None => usize::MAX,
        }
    }
}

struct Cache {
    put: usize,
    delete: usize,
    query: usize,
    reply: usize,
}

impl InterceptorTrait for LowPassInterceptor {
    fn compute_keyexpr_cache(&self, key_expr: &keyexpr) -> Option<Box<dyn Any + Send + Sync>> {
        Some(Box::new(Cache {
            put: self.get_max_allowed_message_size(LowPassFilterMessage::Put, key_expr),
            delete: self.get_max_allowed_message_size(LowPassFilterMessage::Delete, key_expr),
            query: self.get_max_allowed_message_size(LowPassFilterMessage::Query, key_expr),
            reply: self.get_max_allowed_message_size(LowPassFilterMessage::Reply, key_expr),
        }))
    }

    fn intercept(&self, msg: &mut NetworkMessageMut, ctx: &mut dyn InterceptorContext) -> bool {
        let cache = ctx.get_cache(msg).and_then(|i| i.downcast_ref::<Cache>());

        match self.message_passes_filters(msg, ctx, cache) {
            Ok(_) => true,
            #[allow(unused_variables)] // only used for stats
            Err(msg_size) => {
                #[cfg(feature = "stats")]
                match self.flow {
                    InterceptorFlow::Egress => {
                        self.stats.inc_tx_low_pass_dropped_bytes(msg_size);
                        self.stats.inc_tx_low_pass_dropped_msgs(1);
                    }
                    InterceptorFlow::Ingress => {
                        self.stats.inc_rx_low_pass_dropped_bytes(msg_size);
                        self.stats.inc_rx_low_pass_dropped_msgs(1);
                    }
                }
                false
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct LowPassSubject {
    interface: SubjectProperty<String>,
    link_type: SubjectProperty<InterceptorLink>,
}

#[derive(Default)]
struct SubjectStore {
    id: usize,
    subjects: HashMap<LowPassSubject, usize>,
}

impl SubjectStore {
    fn get_or_insert(&mut self, subject: &LowPassSubject) -> usize {
        if let Some(id) = self.subjects.get(subject) {
            return *id;
        }
        self.id += 1;
        self.subjects.insert(subject.clone(), self.id);
        self.id
    }

    fn get_subject_id(&self, subject: &LowPassSubject) -> Option<usize> {
        self.subjects.get(subject).copied()
    }
}

#[derive(Default)]
struct LowPassFilter {
    subjects: SubjectStore,
    filters: HashMap<usize, LowPassFilterFlows>,
    interface_enabled: InterfaceEnabled,
}

#[derive(Default)]
struct LowPassFilterFlows {
    ingress: LowPassFilterMessages,
    egress: LowPassFilterMessages,
}

impl LowPassFilterFlows {
    fn flow(&self, flow: &InterceptorFlow) -> &LowPassFilterMessages {
        match flow {
            InterceptorFlow::Egress => &self.egress,
            InterceptorFlow::Ingress => &self.ingress,
        }
    }

    fn flow_mut(&mut self, flow: &InterceptorFlow) -> &mut LowPassFilterMessages {
        match flow {
            InterceptorFlow::Egress => &mut self.egress,
            InterceptorFlow::Ingress => &mut self.ingress,
        }
    }
}

#[derive(Default)]
struct LowPassFilterMessages {
    put: LowPassFilterTree,
    delete: LowPassFilterTree,
    query: LowPassFilterTree,
    reply: LowPassFilterTree,
}

impl LowPassFilterMessages {
    fn message(&self, message: &LowPassFilterMessage) -> &LowPassFilterTree {
        match message {
            LowPassFilterMessage::Put => &self.put,
            LowPassFilterMessage::Delete => &self.delete,
            LowPassFilterMessage::Query => &self.query,
            LowPassFilterMessage::Reply => &self.reply,
        }
    }

    fn message_mut(&mut self, message: &LowPassFilterMessage) -> &mut LowPassFilterTree {
        match message {
            LowPassFilterMessage::Put => &mut self.put,
            LowPassFilterMessage::Delete => &mut self.delete,
            LowPassFilterMessage::Query => &mut self.query,
            LowPassFilterMessage::Reply => &mut self.reply,
        }
    }
}

type LowPassFilterTree = KeBoxTree<LowPassFilterRule>;

#[derive(Default)]
struct LowPassFilterRule(usize);

impl LowPassFilterRule {
    fn new(limit: usize) -> Self {
        Self(limit)
    }
}
