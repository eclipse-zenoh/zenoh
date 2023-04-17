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
pub mod attachment;
pub use attachment::*;

/*************************************/
/*               IDS                 */
/*************************************/
// Inner Message IDs
pub mod imsg {
    use crate::core::ZInt;

    pub mod id {
        // Transport Messages
        pub const JOIN: u8 = 0x00; // For multicast communications only
        pub const SCOUT: u8 = 0x01;
        pub const HELLO: u8 = 0x02;
        pub const INIT: u8 = 0x03; // For unicast communications only
        pub const OPEN: u8 = 0x04; // For unicast communications only
        pub const CLOSE: u8 = 0x05;
        pub const SYNC: u8 = 0x06;
        pub const ACK_NACK: u8 = 0x07;
        pub const KEEP_ALIVE: u8 = 0x08;
        pub const PING_PONG: u8 = 0x09;
        pub const FRAME: u8 = 0x0a;

        // Zenoh Messages
        pub const DECLARE: u8 = 0x0b;
        pub const DATA: u8 = 0x0c;
        pub const QUERY: u8 = 0x0d;
        pub const PULL: u8 = 0x0e;
        pub const UNIT: u8 = 0x0f;
        pub const LINK_STATE_LIST: u8 = 0x10;

        // Message decorators
        pub const PRIORITY: u8 = 0x1c;
        pub const ROUTING_CONTEXT: u8 = 0x1d;
        pub const REPLY_CONTEXT: u8 = 0x1e;
        pub const ATTACHMENT: u8 = 0x1f;
    }

    // Header mask
    pub const HEADER_BITS: u8 = 5;
    pub const HEADER_MASK: u8 = !(0xff << HEADER_BITS);

    pub fn mid(header: u8) -> u8 {
        header & HEADER_MASK
    }

    pub fn flags(header: u8) -> u8 {
        header & !HEADER_MASK
    }

    pub fn has_flag(byte: u8, flag: u8) -> bool {
        byte & flag != 0
    }

    pub fn has_option(options: ZInt, flag: ZInt) -> bool {
        options & flag != 0
    }
}
