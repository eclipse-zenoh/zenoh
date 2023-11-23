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
use super::Primitives;
use crate::net::routing::interceptor::IngressIntercept;
use std::any::Any;
use zenoh_link::Link;
use zenoh_protocol::network::{NetworkBody, NetworkMessage};
use zenoh_result::ZResult;
use zenoh_transport::TransportPeerEventHandler;

pub struct DeMux<P: Primitives> {
    primitives: P,
    pub(crate) intercept: IngressIntercept,
}

impl<P: Primitives> DeMux<P> {
    pub(crate) fn new(primitives: P, intercept: IngressIntercept) -> DeMux<P> {
        DeMux {
            primitives,
            intercept,
        }
    }
}

impl<P: 'static + Primitives> TransportPeerEventHandler for DeMux<P> {
    fn handle_message(&self, msg: NetworkMessage) -> ZResult<()> {
        let msg = match self.intercept.intercept(msg) {
            Some(msg) => msg,
            None => return Ok(()),
        };

        match msg.body {
            NetworkBody::Declare(m) => self.primitives.send_declare(m),
            NetworkBody::Push(m) => self.primitives.send_push(m),
            NetworkBody::Request(m) => self.primitives.send_request(m),
            NetworkBody::Response(m) => self.primitives.send_response(m),
            NetworkBody::ResponseFinal(m) => self.primitives.send_response_final(m),
            NetworkBody::OAM(_m) => (),
        }

        Ok(())
    }

    fn new_link(&self, _link: Link) {}

    fn del_link(&self, _link: Link) {}

    fn closing(&self) {
        self.primitives.send_close();
    }

    fn closed(&self) {}

    fn as_any(&self) -> &dyn Any {
        self
    }
}
