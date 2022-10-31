//
// Copyright (c) 2022 ZettaScale Technology
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
mod common;
mod core;
mod scouting;
mod transport;

pub trait WCodec<Buffer, Message> {
    type Output;
    fn write(self, buffer: Buffer, message: Message) -> Self::Output;
}

pub trait RCodec<Buffer, Message> {
    type Error;
    fn read(self, buffer: Buffer) -> Result<Message, Self::Error>;
}

#[derive(Clone, Copy, Default)]
#[non_exhaustive]
pub struct Zenoh060;

#[derive(Clone, Copy, Default)]
#[non_exhaustive]
pub struct Zenoh060RCodec {
    pub header: u8,
    pub codec: Zenoh060,
}

impl Zenoh060RCodec {
    pub fn new(header: u8) -> Self {
        Self {
            header,
            codec: Zenoh060::default(),
        }
    }
}
