//
// Copyright (c) 2023 ZettaScale Technology
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
use std::{any::Any, cell::OnceCell, sync::Arc};

use arc_swap::ArcSwap;
use zenoh_link::Link;
use zenoh_protocol::network::{
    ext, response, Declare, DeclareBody, DeclareFinal, NetworkBodyMut, NetworkMessageExt as _,
    NetworkMessageMut, ResponseFinal,
};
use zenoh_result::ZResult;
use zenoh_transport::{unicast::TransportUnicast, TransportPeerEventHandler};

use super::Primitives;
use crate::net::routing::{
    dispatcher::face::Face,
    interceptor::{InterceptorContext, InterceptorTrait, InterceptorsChain},
    router::{InterceptorCacheValueType, Resource},
    RoutingContext,
};

pub struct DeMux {
    face: Face,
    pub(crate) transport: Option<TransportUnicast>,
    pub(crate) interceptor: Arc<ArcSwap<InterceptorsChain>>,
}

impl DeMux {
    pub(crate) fn new(
        face: Face,
        transport: Option<TransportUnicast>,
        interceptor: Arc<ArcSwap<InterceptorsChain>>,
    ) -> Self {
        Self {
            face,
            transport,
            interceptor,
        }
    }
}

struct DeMuxContext<'a> {
    demux: &'a DeMux,
    cache: OnceCell<InterceptorCacheValueType>,
    expr: OnceCell<String>,
}

impl DeMuxContext<'_> {
    fn prefix(&self, msg: &NetworkMessageMut) -> Option<Arc<Resource>> {
        if let Some(wire_expr) = msg.wire_expr() {
            let wire_expr = wire_expr.to_owned();
            if let Some(prefix) = zread!(self.demux.face.tables.tables)
                .get_mapping(&self.demux.face.state, &wire_expr.scope, wire_expr.mapping)
                .cloned()
            {
                return Some(prefix);
            }
        }
        None
    }
}

impl InterceptorContext for DeMuxContext<'_> {
    fn face(&self) -> Option<Face> {
        Some(self.demux.face.clone())
    }

    fn full_expr(&self, msg: &NetworkMessageMut) -> Option<&str> {
        if self.expr.get().is_none() {
            if let Some(wire_expr) = msg.wire_expr() {
                if let Some(prefix) = self.prefix(msg) {
                    self.expr
                        .set(prefix.expr().to_string() + wire_expr.suffix.as_ref())
                        .ok();
                }
            }
        }
        self.expr.get().map(|x| x.as_str())
    }
    fn get_cache(&self, msg: &NetworkMessageMut) -> Option<&Box<dyn Any + Send + Sync>> {
        if self.cache.get().is_none() && msg.wire_expr().is_some_and(|we| !we.has_suffix()) {
            if let Some(prefix) = self.prefix(msg) {
                if let Some(cache) =
                    prefix.get_ingress_cache(&self.demux.face, &self.demux.interceptor.load())
                {
                    self.cache.set(cache).ok();
                }
            }
        }
        self.cache.get().and_then(|c| c.get_ref().as_ref())
    }
}

impl TransportPeerEventHandler for DeMux {
    #[inline]
    fn handle_message(&self, mut msg: NetworkMessageMut) -> ZResult<()> {
        let interceptor = self.interceptor.load();
        if !interceptor.interceptors.is_empty() {
            let mut ctx = DeMuxContext {
                demux: self,
                cache: OnceCell::new(),
                expr: OnceCell::new(),
            };

            match &msg.body {
                NetworkBodyMut::Request(request) => {
                    let request_id = request.id;
                    if !interceptor.intercept(&mut msg, &mut ctx as &mut dyn InterceptorContext) {
                        // request was blocked by an interceptor, we need to send response final to avoid timeout error
                        self.face
                            .state
                            .primitives
                            .send_response_final(&mut ResponseFinal {
                                rid: request_id,
                                ext_qos: response::ext::QoSType::RESPONSE_FINAL,
                                ext_tstamp: None,
                            });
                        return Ok(());
                    }
                }
                NetworkBodyMut::Interest(interest) => {
                    let interest_id = interest.id;
                    if !interceptor.intercept(&mut msg, &mut ctx as &mut dyn InterceptorContext) {
                        // request was blocked by an interceptor, we need to send declare final to avoid timeout error
                        self.face.state.primitives.send_declare(RoutingContext::new(
                            &mut Declare {
                                interest_id: Some(interest_id),
                                ext_qos: ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: ext::NodeIdType::DEFAULT,
                                body: DeclareBody::DeclareFinal(DeclareFinal),
                            },
                        ));
                        return Ok(());
                    }
                }
                _ => {
                    if !interceptor.intercept(&mut msg, &mut ctx as &mut dyn InterceptorContext) {
                        return Ok(());
                    }
                }
            };
        }

        match msg.body {
            NetworkBodyMut::Push(m) => self.face.send_push(m, msg.reliability),
            NetworkBodyMut::Declare(m) => self.face.send_declare(m),
            NetworkBodyMut::Interest(m) => self.face.send_interest(m),
            NetworkBodyMut::Request(m) => self.face.send_request(m),
            NetworkBodyMut::Response(m) => self.face.send_response(m),
            NetworkBodyMut::ResponseFinal(m) => self.face.send_response_final(m),
            NetworkBodyMut::OAM(m) => {
                if let Some(transport) = self.transport.as_ref() {
                    let mut declares = vec![];
                    let ctrl_lock = zlock!(self.face.tables.ctrl_lock);
                    let mut tables = zwrite!(self.face.tables.tables);
                    self.face.tables.hat_code.handle_oam(
                        &mut tables,
                        &self.face.tables,
                        m,
                        transport,
                        &mut |p, m| declares.push((p.clone(), m)),
                    )?;
                    drop(tables);
                    drop(ctrl_lock);
                    for (p, m) in declares {
                        m.with_mut(|m| p.send_declare(m));
                    }
                }
            }
        }

        Ok(())
    }

    fn new_link(&self, _link: Link) {}

    fn del_link(&self, _link: Link) {}

    fn closed(&self) {
        self.face.send_close();
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
