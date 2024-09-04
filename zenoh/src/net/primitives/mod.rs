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
use zenoh_protocol::{
    core::Reliability,
    network::{interest::Interest, Declare, Push, Request, Response, ResponseFinal},
};

use super::routing::RoutingContext;

pub trait Primitives: Send + Sync {
    fn send_interest(&self, msg: Interest);

    fn send_declare(&self, msg: Declare);

    fn send_push(&self, msg: Push, reliability: Reliability);

    fn send_request(&self, msg: Request);

    fn send_response(&self, msg: Response);

    fn send_response_final(&self, msg: ResponseFinal);

    fn send_close(&self);
}

pub(crate) trait EPrimitives: Send + Sync {
    fn as_any(&self) -> &dyn Any;

    fn send_interest(&self, ctx: RoutingContext<Interest>);

    fn send_declare(&self, ctx: RoutingContext<Declare>);

    fn send_push(&self, msg: Push, reliability: Reliability);

    fn send_request(&self, msg: Request);

    fn send_response(&self, msg: Response);

    fn send_response_final(&self, msg: ResponseFinal);
}

#[derive(Default)]
pub struct DummyPrimitives;

impl Primitives for DummyPrimitives {
    fn send_interest(&self, _msg: Interest) {}

    fn send_declare(&self, _msg: Declare) {}

    fn send_push(&self, _msg: Push, _reliability: Reliability) {}

    fn send_request(&self, _msg: Request) {}

    fn send_response(&self, _msg: Response) {}

    fn send_response_final(&self, _msg: ResponseFinal) {}

    fn send_close(&self) {}
}

impl EPrimitives for DummyPrimitives {
    fn send_interest(&self, _ctx: RoutingContext<Interest>) {}

    fn send_declare(&self, _ctx: RoutingContext<Declare>) {}

    fn send_push(&self, _msg: Push, _reliability: Reliability) {}

    fn send_request(&self, _msg: Request) {}

    fn send_response(&self, _msg: Response) {}

    fn send_response_final(&self, _msg: ResponseFinal) {}

    fn as_any(&self) -> &dyn Any {
        self
    }
}
