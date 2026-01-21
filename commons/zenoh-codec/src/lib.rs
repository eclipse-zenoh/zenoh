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
//! [Click here for Zenoh's documentation](https://docs.rs/zenoh/latest/zenoh)
#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;

pub mod common;
pub mod core;
pub mod network;
pub mod scouting;
pub mod transport;
pub mod zenoh;

use ::core::marker::PhantomData;
use zenoh_protocol::core::Reliability;

pub trait WCodec<Message, Buffer> {
    type Output;
    fn write(self, buffer: Buffer, message: Message) -> Self::Output;
}

pub trait RCodec<Message, Buffer> {
    type Error;
    fn read(self, buffer: Buffer) -> Result<Message, Self::Error>;
}

// Calculate the length of the value once serialized
pub trait LCodec<Message> {
    fn w_len(self, message: Message) -> usize;
}

#[derive(Clone, Copy)]
pub struct Zenoh080;

impl Default for Zenoh080 {
    fn default() -> Self {
        Self::new()
    }
}

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

#[derive(Clone, Copy)]
pub struct Zenoh080Bounded<T> {
    _t: PhantomData<T>,
}

impl<T> Default for Zenoh080Bounded<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Zenoh080Bounded<T> {
    pub const fn new() -> Self {
        Self { _t: PhantomData }
    }
}

#[cfg(feature = "shared-memory")]
#[derive(Clone, Copy)]
pub struct Zenoh080Sliced<T> {
    is_sliced: bool,
    codec: Zenoh080Bounded<T>,
}

#[cfg(feature = "shared-memory")]
impl<T> Zenoh080Sliced<T> {
    pub const fn new(is_sliced: bool) -> Self {
        Self {
            is_sliced,
            codec: Zenoh080Bounded::<T>::new(),
        }
    }
}
