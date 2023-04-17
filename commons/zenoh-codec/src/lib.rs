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

//! ⚠️ WARNING ⚠️
//!
//! This crate is intended for Zenoh's internal use.
//!
//! [Click here for Zenoh's documentation](../zenoh/index.html)
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

#[derive(Clone, Copy, Default)]
#[non_exhaustive]
pub struct Zenoh060;

#[derive(Clone, Copy, Default)]
#[non_exhaustive]
pub struct Zenoh060Header {
    pub header: u8,
    pub codec: Zenoh060,
}

#[derive(Clone, Copy, Default)]
#[non_exhaustive]
pub struct Zenoh060Condition {
    pub condition: bool,
    pub codec: Zenoh060,
}

impl Zenoh060Condition {
    pub const fn new(condition: bool) -> Self {
        Self {
            condition,
            codec: Zenoh060,
        }
    }
}

#[derive(Clone, Copy, Default)]
#[non_exhaustive]
pub struct Zenoh060Reliability {
    pub reliability: Reliability,
    pub codec: Zenoh060,
}

impl Zenoh060Reliability {
    pub const fn new(reliability: Reliability) -> Self {
        Self {
            reliability,
            codec: Zenoh060,
        }
    }
}

#[derive(Clone, Default)]
#[non_exhaustive]
pub struct Zenoh060HeaderReplyContext {
    pub header: u8,
    pub reply_context: Option<ReplyContext>,
    pub codec: Zenoh060,
}
