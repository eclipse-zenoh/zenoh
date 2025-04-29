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
use std::any::Any;

use zenoh_link::Link;
use zenoh_protocol::network::{NetworkBodyMut, NetworkMessageMut};
use zenoh_result::ZResult;
use zenoh_transport::{unicast::TransportUnicast, TransportPeerEventHandler};

use super::Primitives;
use crate::net::routing::dispatcher::face::Face;

pub struct DeMux {
    face: Face,
    pub(crate) transport: Option<TransportUnicast>,
}

impl DeMux {
    pub(crate) fn new(face: Face, transport: Option<TransportUnicast>) -> Self {
        Self { face, transport }
    }
}

impl TransportPeerEventHandler for DeMux {
    #[inline]
    fn handle_message(&self, msg: NetworkMessageMut) -> ZResult<()> {
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
                    ctrl_lock.handle_oam(
                        &mut tables,
                        &self.face.tables,
                        m,
                        transport,
                        &mut |p, m, r| declares.push((p.clone(), m, r)),
                    )?;
                    drop(tables);
                    drop(ctrl_lock);
                    for (p, mut m, r) in declares {
                        p.intercept_declare(&mut m, r.as_ref());
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
