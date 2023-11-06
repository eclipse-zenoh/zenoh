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
use super::super::{TransportMulticast, TransportUnicast};
use super::Primitives;
use zenoh_protocol::network::{
    Declare, NetworkBody, NetworkMessage, Push, Request, Response, ResponseFinal,
};

pub struct Mux {
    handler: TransportUnicast,
}

impl Mux {
    pub fn new(handler: TransportUnicast) -> Mux {
        Mux { handler }
    }
}

impl Primitives for Mux {
    fn send_declare(&self, msg: Declare) {
        let _ = self.handler.schedule(NetworkMessage {
            body: NetworkBody::Declare(msg),
            #[cfg(feature = "stats")]
            size: None,
        });
    }

    fn send_push(&self, msg: Push) {
        let _ = self.handler.schedule(NetworkMessage {
            body: NetworkBody::Push(msg),
            #[cfg(feature = "stats")]
            size: None,
        });
    }

    fn send_request(&self, msg: Request) {
        let _ = self.handler.schedule(NetworkMessage {
            body: NetworkBody::Request(msg),
            #[cfg(feature = "stats")]
            size: None,
        });
    }

    fn send_response(&self, msg: Response) {
        let _ = self.handler.schedule(NetworkMessage {
            body: NetworkBody::Response(msg),
            #[cfg(feature = "stats")]
            size: None,
        });
    }

    fn send_response_final(&self, msg: ResponseFinal) {
        let _ = self.handler.schedule(NetworkMessage {
            body: NetworkBody::ResponseFinal(msg),
            #[cfg(feature = "stats")]
            size: None,
        });
    }

    fn send_close(&self) {
        // self.handler.closing().await;
    }
}

pub struct McastMux {
    handler: TransportMulticast,
}

impl McastMux {
    pub fn new(handler: TransportMulticast) -> McastMux {
        McastMux { handler }
    }
}

impl Primitives for McastMux {
    fn send_declare(&self, msg: Declare) {
        let _ = self.handler.handle_message(NetworkMessage {
            body: NetworkBody::Declare(msg),
            #[cfg(feature = "stats")]
            size: None,
        });
    }

    fn send_push(&self, msg: Push) {
        let _ = self.handler.handle_message(NetworkMessage {
            body: NetworkBody::Push(msg),
            #[cfg(feature = "stats")]
            size: None,
        });
    }

    fn send_request(&self, msg: Request) {
        let _ = self.handler.handle_message(NetworkMessage {
            body: NetworkBody::Request(msg),
            #[cfg(feature = "stats")]
            size: None,
        });
    }

    fn send_response(&self, msg: Response) {
        let _ = self.handler.handle_message(NetworkMessage {
            body: NetworkBody::Response(msg),
            #[cfg(feature = "stats")]
            size: None,
        });
    }

    fn send_response_final(&self, msg: ResponseFinal) {
        let _ = self.handler.handle_message(NetworkMessage {
            body: NetworkBody::ResponseFinal(msg),
            #[cfg(feature = "stats")]
            size: None,
        });
    }

    fn send_close(&self) {
        // self.handler.closing().await;
    }
}
