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
use std::{any::Any, sync::Arc};

use arc_swap::ArcSwap;
use zenoh_link::Link;
use zenoh_protocol::network::{NetworkBody, NetworkMessage};
use zenoh_result::ZResult;
use zenoh_transport::{unicast::TransportUnicast, TransportPeerEventHandler};

use super::Primitives;
use crate::net::routing::{
    dispatcher::face::Face,
    interceptor::{InterceptorTrait, InterceptorsChain},
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

impl TransportPeerEventHandler for DeMux {
    #[inline]
    fn handle_message(&self, mut msg: NetworkMessage) -> ZResult<()> {
        let interceptor = self.interceptor.load();
        if !interceptor.interceptors.is_empty() {
            let ctx = RoutingContext::new_in(msg, self.face.clone());
            let prefix = ctx
                .wire_expr()
                .and_then(|we| (!we.has_suffix()).then(|| ctx.prefix()))
                .flatten()
                .cloned();
            let cache = prefix
                .as_ref()
                .and_then(|p| p.get_ingress_cache(&self.face));
            let ctx = match interceptor.intercept(ctx, cache) {
                Some(ctx) => ctx,
                None => return Ok(()),
            };
            msg = ctx.msg;
        }

        match msg.body {
            NetworkBody::Push(m) => self.face.send_push(m, msg.reliability),
            NetworkBody::Declare(m) => self.face.send_declare(m),
            NetworkBody::Interest(m) => self.face.send_interest(m),
            NetworkBody::Request(m) => self.face.send_request(m),
            NetworkBody::Response(m) => self.face.send_response(m),
            NetworkBody::ResponseFinal(m) => self.face.send_response_final(m),
            NetworkBody::OAM(m) => {
                if let Some(transport) = self.transport.as_ref() {
                    let mut declares = vec![];
                    let ctrl_lock = zlock!(self.face.tables.ctrl_lock);
                    let mut tables = zwrite!(self.face.tables.tables);
                    ctrl_lock.handle_oam(
                        &mut tables,
                        &self.face.tables,
                        m,
                        transport,
                        &mut |p, m| declares.push((p.clone(), m)),
                    )?;
                    drop(tables);
                    drop(ctrl_lock);
                    for (p, m) in declares {
                        p.send_declare(m);
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
