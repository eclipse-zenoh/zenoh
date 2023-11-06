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

pub use demux::*;
pub use mux::*;
use zenoh_protocol::network::{Declare, Push, Request, Response, ResponseFinal};

pub trait Primitives: Send + Sync {
    fn send_declare(&self, msg: Declare);

    fn send_push(&self, msg: Push);

    fn send_request(&self, msg: Request);

    fn send_response(&self, msg: Response);

    fn send_response_final(&self, msg: ResponseFinal);

    fn send_close(&self);
}

#[derive(Default)]
pub struct DummyPrimitives;

impl DummyPrimitives {
    pub fn new() -> Self {
        Self
    }
}

impl Primitives for DummyPrimitives {
    fn send_declare(&self, _msg: Declare) {}

    fn send_push(&self, _msg: Push) {}

    fn send_request(&self, _msg: Request) {}

    fn send_response(&self, _msg: Response) {}

    fn send_response_final(&self, _msg: ResponseFinal) {}

    fn send_close(&self) {}
}
