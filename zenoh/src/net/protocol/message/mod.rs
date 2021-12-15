//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//

mod constants;
mod reader;
mod shm;
mod transport;
mod writer;
mod zenoh;

pub use self::zenoh::*;
use super::core;
use super::core::{Priority, ZInt};
use super::io;
use super::io::ZBuf;
pub use constants::*;
pub use reader::*;
pub use shm::*;
pub use transport::*;
pub use writer::*;

/*************************************/
/*               IDS                 */
/*************************************/
// Header mask
pub const HEADER_BITS: u8 = 5;
pub const HEADER_MASK: u8 = !(u8::MAX << HEADER_BITS);

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

// Transport message flags
pub(crate) mod flag {
    pub(crate) const Z: u8 = 1 << 5; // 0x20 MixedSlices   if Z==1 then the payload contains a mix of raw and shm_info payload
}

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

/*************************************/
/*              TRAITS               */
/*************************************/
pub(crate) trait Header {
    fn header(&self) -> u8;
}

pub(crate) trait Options {
    fn options(&self) -> ZInt;
    fn has_options(&self) -> bool;
}

/*************************************/
/*            DECORATORS             */
/*************************************/
/// # Attachment decorator
///
/// ```text
/// NOTE: 16 bits (2 bytes) may be prepended to the serialized message indicating the total length
///       in bytes of the message, resulting in the maximum length of a message being 65_535 bytes.
///       This is necessary in those stream-oriented transports (e.g., TCP) that do not preserve
///       the boundary of the serialized messages. The length is encoded as little-endian.
///       In any case, the length of a message must not exceed 65_535 bytes.
///
/// The Attachment can decorate any message (i.e., TransportMessage and ZenohMessage) and it allows to
/// append to the message any additional information. Since the information contained in the
/// Attachement is relevant only to the layer that provided them (e.g., Transport, Zenoh, User) it
/// is the duty of that layer to serialize and de-serialize the attachment whenever deemed necessary.
/// The attachement always contains serialized properties.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |X|X|Z|  ATTCH  |
/// +-+-+-+---------+
/// ~   Attachment  ~
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Attachment {
    pub buffer: ZBuf,
}

impl Header for Attachment {
    #[inline(always)]
    fn header(&self) -> u8 {
        #[allow(unused_mut)]
        let mut header = id::ATTACHMENT;
        #[cfg(feature = "shared-memory")]
        if self.buffer.has_shminfo() {
            header |= flag::Z;
        }
        header
    }
}

impl Attachment {
    #[inline(always)]
    pub fn new(buffer: ZBuf) -> Attachment {
        Attachment { buffer }
    }
}

/// -- Priority decorator
///
/// ```text
/// The **Priority** is a message decorator containing
/// informations related to the priority of the frame/zenoh message.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// | ID  |  Prio   |
/// +-+-+-+---------+
///
/// ```
impl Priority {
    pub fn header(self) -> u8 {
        id::PRIORITY | ((self as u8) << HEADER_BITS)
    }
}
