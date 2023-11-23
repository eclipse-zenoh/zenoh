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
use crate::net::routing::interceptor::EgressIntercept;
use zenoh_protocol::network::{
    Declare, NetworkBody, NetworkMessage, Push, Request, Response, ResponseFinal,
};
use zenoh_transport::{TransportMulticast, TransportUnicast};

pub struct Mux {
    pub handler: TransportUnicast,
    pub(crate) intercept: EgressIntercept,
}

impl Mux {
    pub(crate) fn new(handler: TransportUnicast, intercept: EgressIntercept) -> Mux {
        Mux { handler, intercept }
    }
}

impl Primitives for Mux {
    fn send_declare(&self, msg: Declare) {
        let msg = NetworkMessage {
            body: NetworkBody::Declare(msg),
            #[cfg(feature = "stats")]
            size: None,
        };
        if let Some(msg) = self.intercept.intercept(msg) {
            let _ = self.handler.schedule(msg);
        }
    }

    fn send_push(&self, msg: Push) {
        let msg = NetworkMessage {
            body: NetworkBody::Push(msg),
            #[cfg(feature = "stats")]
            size: None,
        };
        if let Some(msg) = self.intercept.intercept(msg) {
            let _ = self.handler.schedule(msg);
        }
    }

    fn send_request(&self, msg: Request) {
        let msg = NetworkMessage {
            body: NetworkBody::Request(msg),
            #[cfg(feature = "stats")]
            size: None,
        };
        if let Some(msg) = self.intercept.intercept(msg) {
            let _ = self.handler.schedule(msg);
        }
    }

    fn send_response(&self, msg: Response) {
        let msg = NetworkMessage {
            body: NetworkBody::Response(msg),
            #[cfg(feature = "stats")]
            size: None,
        };
        if let Some(msg) = self.intercept.intercept(msg) {
            let _ = self.handler.schedule(msg);
        }
    }

    fn send_response_final(&self, msg: ResponseFinal) {
        let msg = NetworkMessage {
            body: NetworkBody::ResponseFinal(msg),
            #[cfg(feature = "stats")]
            size: None,
        };
        if let Some(msg) = self.intercept.intercept(msg) {
            let _ = self.handler.schedule(msg);
        }
    }

    fn send_close(&self) {
        // self.handler.closing().await;
    }
}

pub struct McastMux {
    pub handler: TransportMulticast,
    pub(crate) intercept: EgressIntercept,
}

impl McastMux {
    pub(crate) fn new(handler: TransportMulticast, intercept: EgressIntercept) -> McastMux {
        McastMux { handler, intercept }
    }
}

impl Primitives for McastMux {
    fn send_declare(&self, msg: Declare) {
        let msg = NetworkMessage {
            body: NetworkBody::Declare(msg),
            #[cfg(feature = "stats")]
            size: None,
        };
        if let Some(msg) = self.intercept.intercept(msg) {
            let _ = self.handler.schedule(msg);
        }
    }

    fn send_push(&self, msg: Push) {
        let msg = NetworkMessage {
            body: NetworkBody::Push(msg),
            #[cfg(feature = "stats")]
            size: None,
        };
        if let Some(msg) = self.intercept.intercept(msg) {
            let _ = self.handler.schedule(msg);
        }
    }

    fn send_request(&self, msg: Request) {
        let msg = NetworkMessage {
            body: NetworkBody::Request(msg),
            #[cfg(feature = "stats")]
            size: None,
        };
        if let Some(msg) = self.intercept.intercept(msg) {
            let _ = self.handler.schedule(msg);
        }
    }

    fn send_response(&self, msg: Response) {
        let msg = NetworkMessage {
            body: NetworkBody::Response(msg),
            #[cfg(feature = "stats")]
            size: None,
        };
        if let Some(msg) = self.intercept.intercept(msg) {
            let _ = self.handler.schedule(msg);
        }
    }

    fn send_response_final(&self, msg: ResponseFinal) {
        let msg = NetworkMessage {
            body: NetworkBody::ResponseFinal(msg),
            #[cfg(feature = "stats")]
            size: None,
        };
        if let Some(msg) = self.intercept.intercept(msg) {
            let _ = self.handler.schedule(msg);
        }
    }

    fn send_close(&self) {
        // self.handler.closing().await;
    }
}
