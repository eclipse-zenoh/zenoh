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

use arc_swap::ArcSwap;
use zenoh_protocol::{
    core::Reliability,
    network::{
        interest::Interest, Declare, NetworkBodyMut, NetworkMessageMut, Push, Request, Response,
        ResponseFinal,
    },
};
use zenoh_transport::{multicast::TransportMulticast, unicast::TransportUnicast};

use super::EPrimitives;
use crate::net::routing::{interceptor::InterceptorsChain, RoutingContext};

pub struct Mux {
    pub handler: TransportUnicast,
}

impl Mux {
    pub(crate) fn new(handler: TransportUnicast) -> Mux {
        Mux { handler }
    }
}

impl EPrimitives for Mux {
    fn send_interest(&self, ctx: RoutingContext<&mut Interest>) {
        let msg = NetworkMessageMut {
            body: NetworkBodyMut::Interest(ctx.msg),
            reliability: Reliability::Reliable,
        };
        let _ = self.handler.schedule(msg);
    }

    fn send_declare(&self, ctx: RoutingContext<&mut Declare>) {
        let msg = NetworkMessageMut {
            body: NetworkBodyMut::Declare(ctx.msg),
            reliability: Reliability::Reliable,
        };
        let _ = self.handler.schedule(msg);
    }

    fn send_push(&self, msg: &mut Push, reliability: Reliability) {
        let msg = NetworkMessageMut {
            body: NetworkBodyMut::Push(msg),
            reliability,
        };

        let _ = self.handler.schedule(msg);
    }

    fn send_request(&self, msg: &mut Request) {
        let msg = NetworkMessageMut {
            body: NetworkBodyMut::Request(msg),
            reliability: Reliability::Reliable,
        };

        let _ = self.handler.schedule(msg);
    }

    fn send_response(&self, msg: &mut Response) {
        let msg = NetworkMessageMut {
            body: NetworkBodyMut::Response(msg),
            reliability: Reliability::Reliable,
        };
        let _ = self.handler.schedule(msg);
    }

    fn send_response_final(&self, msg: &mut ResponseFinal) {
        let msg = NetworkMessageMut {
            body: NetworkBodyMut::ResponseFinal(msg),
            reliability: Reliability::Reliable,
        };
        let _ = self.handler.schedule(msg);
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub struct McastMux {
    pub handler: TransportMulticast,
    pub(crate) interceptor: ArcSwap<InterceptorsChain>,
}

impl McastMux {
    pub(crate) fn new(handler: TransportMulticast, interceptor: InterceptorsChain) -> McastMux {
        McastMux {
            handler,
            interceptor: ArcSwap::new(interceptor.into()),
        }
    }
}

impl EPrimitives for McastMux {
    fn send_interest(&self, ctx: RoutingContext<&mut Interest>) {
        let msg = NetworkMessageMut {
            body: NetworkBodyMut::Interest(ctx.msg),
            reliability: Reliability::Reliable,
        };
        let _ = self.handler.schedule(msg);
    }

    fn send_declare(&self, ctx: RoutingContext<&mut Declare>) {
        let msg = NetworkMessageMut {
            body: NetworkBodyMut::Declare(ctx.msg),
            reliability: Reliability::Reliable,
        };
        let _ = self.handler.schedule(msg);
    }

    fn send_push(&self, msg: &mut Push, reliability: Reliability) {
        let msg = NetworkMessageMut {
            body: NetworkBodyMut::Push(msg),
            reliability,
        };
        let _ = self.handler.schedule(msg);
    }

    fn send_request(&self, msg: &mut Request) {
        let msg = NetworkMessageMut {
            body: NetworkBodyMut::Request(msg),
            reliability: Reliability::Reliable,
        };
        let _ = self.handler.schedule(msg);
    }

    fn send_response(&self, msg: &mut Response) {
        let msg = NetworkMessageMut {
            body: NetworkBodyMut::Response(msg),
            reliability: Reliability::Reliable,
        };
        let _ = self.handler.schedule(msg);
    }

    fn send_response_final(&self, msg: &mut ResponseFinal) {
        let msg = NetworkMessageMut {
            body: NetworkBodyMut::ResponseFinal(msg),
            reliability: Reliability::Reliable,
        };
        let _ = self.handler.schedule(msg);
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
