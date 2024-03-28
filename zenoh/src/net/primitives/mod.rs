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
mod demux;
mod mux;

use std::any::Any;

pub use demux::*;
pub use mux::*;
use zenoh_protocol::network::{Declare, Push, Request, Response, ResponseFinal};

use super::routing::RoutingContext;

pub trait IngressPrimitives: Send + Sync {
    fn ingress_declare(&self, msg: Declare);

    fn ingress_push(&self, msg: Push);

    fn ingress_request(&self, msg: Request);

    fn ingress_response(&self, msg: Response);

    fn ingress_response_final(&self, msg: ResponseFinal);

    fn ingress_close(&self);
}

pub(crate) trait EgressPrimitives: Send + Sync {
    fn as_any(&self) -> &dyn Any;

    fn egress_declare(&self, ctx: RoutingContext<Declare>);

    fn egress_push(&self, msg: Push);

    fn egress_request(&self, ctx: RoutingContext<Request>);

    fn egress_response(&self, ctx: RoutingContext<Response>);

    fn egress_response_final(&self, ctx: RoutingContext<ResponseFinal>);

    fn egress_close(&self);
}

#[derive(Default)]
pub struct DummyPrimitives;

impl IngressPrimitives for DummyPrimitives {
    fn ingress_declare(&self, _msg: Declare) {}

    fn ingress_push(&self, _msg: Push) {}

    fn ingress_request(&self, _msg: Request) {}

    fn ingress_response(&self, _msg: Response) {}

    fn ingress_response_final(&self, _msg: ResponseFinal) {}

    fn ingress_close(&self) {}
}

impl EgressPrimitives for DummyPrimitives {
    fn egress_declare(&self, _ctx: RoutingContext<Declare>) {}

    fn egress_push(&self, _msg: Push) {}

    fn egress_request(&self, _ctx: RoutingContext<Request>) {}

    fn egress_response(&self, _ctx: RoutingContext<Response>) {}

    fn egress_response_final(&self, _ctx: RoutingContext<ResponseFinal>) {}

    fn egress_close(&self) {}

    fn as_any(&self) -> &dyn Any {
        self
    }
}
