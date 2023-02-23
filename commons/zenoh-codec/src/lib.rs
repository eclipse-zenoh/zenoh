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
#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;

mod common;
mod core;
mod scouting;
mod transport;
mod zenoh;

use zenoh_protocol::{core::Reliability, zenoh::ReplyContext};

pub trait WCodec<Message, Buffer> {
    type Output;
    fn write(self, buffer: Buffer, message: Message) -> Self::Output;
}

pub trait RCodec<Message, Buffer> {
    type Error;
    fn read(self, buffer: Buffer) -> Result<Message, Self::Error>;
}

#[derive(Clone, Copy)]
pub struct Zenoh080;

impl Zenoh080 {
    pub const fn new() -> Self {
        Self
    }
}

#[derive(Clone, Copy)]
pub struct Zenoh080Header {
    pub header: u8,
    pub codec: Zenoh080,
}

impl Zenoh080Header {
    pub const fn new(header: u8) -> Self {
        Self {
            header,
            codec: Zenoh080,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Zenoh080Condition {
    pub condition: bool,
    pub codec: Zenoh080,
}

impl Zenoh080Condition {
    pub const fn new(condition: bool) -> Self {
        Self {
            condition,
            codec: Zenoh080,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Zenoh080Length {
    pub length: usize,
    pub codec: Zenoh080,
}

impl Zenoh080Length {
    pub const fn new(length: usize) -> Self {
        Self {
            length,
            codec: Zenoh080,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Zenoh080Reliability {
    pub reliability: Reliability,
    pub codec: Zenoh080,
}

impl Zenoh080Reliability {
    pub const fn new(reliability: Reliability) -> Self {
        Self {
            reliability,
            codec: Zenoh080,
        }
    }
}

#[derive(Clone)]
pub struct Zenoh080HeaderReplyContext {
    pub header: u8,
    pub reply_context: Option<ReplyContext>,
    pub codec: Zenoh080,
}
