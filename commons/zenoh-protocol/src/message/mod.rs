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
pub mod core;
pub mod scouting;
pub mod transport;
use zenoh_protocol_core::ZInt;

// Common message traits
pub trait Header {
    fn header(&self) -> u8;
}

pub trait Options {
    fn options(&self) -> ZInt;
    fn has_options(&self) -> bool;
}

// Inner Message IDs
pub(crate) mod imsg {
    use zenoh_protocol_core::ZInt;

    pub(crate) mod id {
        // Transport Messages
        pub(crate) const JOIN: u8 = 0x00; // For multicast communications only
        pub(crate) const SCOUT: u8 = 0x01;
        pub(crate) const HELLO: u8 = 0x02;
        pub(crate) const INIT: u8 = 0x03; // For unicast communications only
        pub(crate) const OPEN: u8 = 0x04; // For unicast communications only
        pub(crate) const CLOSE: u8 = 0x05;
        pub(crate) const SYNC: u8 = 0x06;
        pub(crate) const ACK_NACK: u8 = 0x07;
        pub(crate) const KEEP_ALIVE: u8 = 0x08;
        pub(crate) const PING_PONG: u8 = 0x09;
        pub(crate) const FRAME: u8 = 0x0a;

        // Zenoh Messages
        pub(crate) const DECLARE: u8 = 0x0b;
        pub(crate) const DATA: u8 = 0x0c;
        pub(crate) const QUERY: u8 = 0x0d;
        pub(crate) const PULL: u8 = 0x0e;
        pub(crate) const UNIT: u8 = 0x0f;
        pub(crate) const LINK_STATE_LIST: u8 = 0x10;

        // Message decorators
        pub(crate) const PRIORITY: u8 = 0x1c;
        pub(crate) const ROUTING_CONTEXT: u8 = 0x1d;
        pub(crate) const REPLY_CONTEXT: u8 = 0x1e;
        pub(crate) const ATTACHMENT: u8 = 0x1f;
    }

    // Header mask
    pub const HEADER_BITS: u8 = 5;
    pub const HEADER_MASK: u8 = !(0xff << HEADER_BITS);

    pub const fn mid(header: u8) -> u8 {
        header & HEADER_MASK
    }

    pub const fn flags(header: u8) -> u8 {
        header & !HEADER_MASK
    }

    pub const fn has_flag(byte: u8, flag: u8) -> bool {
        byte & flag != 0
    }

    pub const fn has_option(options: ZInt, flag: ZInt) -> bool {
        options & flag != 0
    }
}
