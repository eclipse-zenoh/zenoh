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

use ahash::HashMap;
use itertools::Itertools;
use zenoh_buffers::buffer::Buffer;
use zenoh_config::{InterceptorFlow, LowPassFilterConf, LowPassFilterMessage};
use zenoh_keyexpr::{
    keyexpr,
    keyexpr_tree::{IKeyExprTree, IKeyExprTreeMut, IKeyExprTreeNode, KeBoxTree},
};
use zenoh_protocol::{
    network::{NetworkBody, NetworkMessage, Push, Request, Response},
    zenoh::{PushBody, Reply, RequestBody, ResponseBody},
};
use zenoh_result::ZResult;
use zenoh_transport::{multicast::TransportMulticast, unicast::TransportUnicast};

use super::{
    authorization::SubjectProperty, EgressInterceptor, IngressInterceptor, InterceptorFactory,
    InterceptorFactoryTrait, InterceptorTrait, InterfaceEnabled, RoutingContext,
};

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
        if let Some(flows) = &lpf.flows {
            if flows.is_empty() {
                bail!("flows list must not be empty");
            }
        }
        if lpf.messages.is_empty() {
            bail!("messages list must not be empty");
        }
        if let Some(faces) = &lpf.interfaces {
            if faces.is_empty() {
                bail!("interfaces list must not be empty");
            }
        }
        if lpf.key_exprs.is_empty() {
            bail!("key_exprs list must not be empty");
        }
        for key_expr in &lpf.key_exprs {
            if key_expr.is_empty() {
                bail!("key expressions must not be empty in key_expr list");
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
                .get_or_insert(vec![InterceptorFlow::Ingress, InterceptorFlow::Egress]);
            let interfaces = lpf_config
                .interfaces
                .map(|v| {
                    v.iter()
                        .map(|face| SubjectProperty::Exactly(face.clone()))
                        .collect_vec()
                })
                .unwrap_or(vec![SubjectProperty::Wildcard]);

            for face in interfaces {
                for message in &lpf_config.messages {
                    for key_expr in &lpf_config.key_exprs {
                        for flow in &*flows {
                            let id = low_pass_filter.interfaces.get_or_insert(&face);
                            low_pass_filter
                                .filters
                                .entry(id)
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
        match transport.get_links() {
            Ok(links) => {
                let interfaces = links
                    .into_iter()
                    .flat_map(|link| link.interfaces)
                    .collect_vec();
                let matches = self.state.interfaces.get_matches(&interfaces);
                if !matches.is_empty() {
                    let matches = Arc::new(matches);
                    return (
                        self.state.interface_enabled.ingress.then(|| {
                            Box::new(LowPassInterceptor::new(
                                self.state.clone(),
                                matches.clone(),
                                InterceptorFlow::Ingress,
                            )) as IngressInterceptor
                        }),
                        self.state.interface_enabled.egress.then(|| {
                            Box::new(LowPassInterceptor::new(
                                self.state.clone(),
                                matches,
                                InterceptorFlow::Egress,
                            )) as EgressInterceptor
                        }),
                    );
                }
            }
            Err(e) => {
                tracing::error!("Unable to get links from transport {:?}: {e}", transport);
            }
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
}

impl LowPassInterceptor {
    fn new(inner: Arc<LowPassFilter>, subjects: Arc<Vec<usize>>, flow: InterceptorFlow) -> Self {
        Self {
            inner,
            subjects,
            flow,
        }
    }

    fn message_passes_filters(&self, msg: &NetworkMessage, key_expr: &str) -> ZResult<bool> {
        let payload_size: Option<usize>;
        let attachment_size: Option<usize>;
        let message_type: LowPassFilterMessage;

        match &msg.body {
            NetworkBody::Request(Request {
                payload: RequestBody::Query(query),
                ..
            }) => {
                message_type = LowPassFilterMessage::Query;
                payload_size = query.ext_body.as_ref().map(|body| body.payload.len());
                attachment_size = query.ext_attachment.as_ref().map(|att| att.buffer.len());
            }
            NetworkBody::Response(Response {
                payload:
                    ResponseBody::Reply(Reply {
                        payload: PushBody::Put(put),
                        ..
                    }),
                ..
            }) => {
                message_type = LowPassFilterMessage::Reply;
                payload_size = Some(put.payload.len());
                attachment_size = put.ext_attachment.as_ref().map(|att| att.buffer.len());
            }
            NetworkBody::Response(Response {
                payload:
                    ResponseBody::Reply(Reply {
                        payload: PushBody::Del(delete),
                        ..
                    }),
                ..
            }) => {
                message_type = LowPassFilterMessage::Reply;
                payload_size = None;
                attachment_size = delete.ext_attachment.as_ref().map(|att| att.buffer.len());
            }
            NetworkBody::Push(Push {
                payload: PushBody::Put(put),
                ..
            }) => {
                message_type = LowPassFilterMessage::Put;
                payload_size = Some(put.payload.len());
                attachment_size = put.ext_attachment.as_ref().map(|att| att.buffer.len());
            }
            NetworkBody::Push(Push {
                payload: PushBody::Del(delete),
                ..
            }) => {
                message_type = LowPassFilterMessage::Delete;
                payload_size = None;
                attachment_size = delete.ext_attachment.as_ref().map(|att| att.buffer.len());
            }
            NetworkBody::Response(Response {
                payload: ResponseBody::Err(zenoh_protocol::zenoh::Err { payload, .. }),
                ..
            }) => {
                message_type = LowPassFilterMessage::Reply;
                payload_size = Some(payload.len());
                attachment_size = None;
            }
            NetworkBody::ResponseFinal(_) => return Ok(true),
            NetworkBody::Interest(_) => return Ok(true),
            NetworkBody::Declare(_) => return Ok(true),
            NetworkBody::OAM(_) => return Ok(true),
        }
        Ok(self.verify_message(
            &message_type,
            payload_size.unwrap_or(0),
            attachment_size.unwrap_or(0),
            keyexpr::new(key_expr)?,
        ))
    }

    fn verify_message(
        &self,
        message: &LowPassFilterMessage,
        payload_len: usize,
        attachment_len: usize,
        key_expr: &zenoh_keyexpr::keyexpr,
    ) -> bool {
        // early exit if total size is zero
        if payload_len == 0 && attachment_len == 0 {
            return true;
        }
        if let Some((_filter_key_expr, min_size_bytes)) = self
            .subjects
            .iter()
            .filter_map(|s| {
                // get min of matching nodes within the KeBoxTree of the subject/flow/message
                self.inner
                    .filters
                    .get(s)
                    .expect("subject should have entry in map")
                    .flow(&self.flow)
                    .message(message)
                    .nodes_including(key_expr)
                    .filter(|node| node.weight().is_some())
                    .min_by_key(|node| node.weight().expect("weight should not be none").0)
            })
            // get min of matching nodes from different KeBoxTrees
            .min_by_key(|node| node.weight().expect("weight should not be none").0)
            .map(|node| (node.keyexpr(), node.weight().expect("weight should not be none").0))
        {
            match payload_len.checked_add(attachment_len) {
                Some(message_size) => return message_size <= min_size_bytes,
                None => return false,
            }
        }
        true
    }
}

impl InterceptorTrait for LowPassInterceptor {
    fn compute_keyexpr_cache(
        &self,
        key_expr: &crate::key_expr::KeyExpr<'_>,
    ) -> Option<Box<dyn std::any::Any + Send + Sync>> {
        Some(Box::new(key_expr.to_string()))
    }

    fn intercept(
        &self,
        ctx: RoutingContext<NetworkMessage>,
        cache: Option<&Box<dyn std::any::Any + Send + Sync>>,
    ) -> Option<RoutingContext<NetworkMessage>> {
        let key_expr = cache
            .and_then(|i| match i.downcast_ref::<String>() {
                Some(e) => Some(e.as_str()),
                None => {
                    tracing::debug!("Cache content was not of type String");
                    None
                }
            })
            .or_else(|| ctx.full_expr());
        match self.message_passes_filters(&ctx.msg, key_expr?) {
            Ok(decision) => {
                if decision {
                    return Some(ctx);
                }
            }
            Err(e) => {
                tracing::error!("Error in evaluation of low-pass filter interceptor: {e}");
            }
        }
        None
    }
}

#[derive(Default)]
struct SubjectStore {
    id: usize,
    interfaces: HashMap<SubjectProperty<String>, usize>,
}

impl SubjectStore {
    fn get_or_insert(&mut self, face: &SubjectProperty<String>) -> usize {
        if let Some(id) = self.interfaces.get(face) {
            return *id;
        }
        self.id += 1;
        self.interfaces.insert(face.clone(), self.id);
        self.id
    }

    fn get_matches(&self, interfaces: &Vec<String>) -> Vec<usize> {
        let mut matches = Vec::new();
        // FIXME: since currently we only have one attribute per subject we can just look up its Exact and Wildcard variants
        for face in interfaces {
            if let Some(id) = self
                .interfaces
                .get(&SubjectProperty::Exactly(face.to_string()))
            {
                matches.push(*id);
            }
        }
        if let Some(id) = self.interfaces.get(&SubjectProperty::Wildcard) {
            matches.push(*id);
        }
        matches
    }
}

#[derive(Default)]
struct LowPassFilter {
    interfaces: SubjectStore,
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
