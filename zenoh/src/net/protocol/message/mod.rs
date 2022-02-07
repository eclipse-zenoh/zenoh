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
pub mod extensions;
mod reader;
mod scouting;
mod shm;
mod transport;
mod writer;
mod zenoh;

pub use self::zenoh::*;
use super::core;
use super::core::{Priority, ZInt};
use super::io;
use super::io::{WBuf, ZBuf};
pub use constants::*;
pub use reader::*;
pub use scouting::*;
pub use shm::*;
use std::collections::HashMap;
use std::fmt::Debug;
use std::ops::{Deref, DerefMut};
pub use transport::*;
pub use writer::*;

/*************************************/
/*           CONVENTIONS             */
/*************************************/
/// Zenoh protocol uses the following conventions for message definition and representation.
///
///
/// # Single byte field
///
/// A fixed size field of 8 bits.
///
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |      u8       |
/// +---------------+
/// ```
///
///
/// # Variable length field
///
/// The field size depends on the element definition and/or actual encoding. An example of variable
/// lenght element is an array of bytes (e.g., a payload or a string).
///
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// ~    element    ~
/// +---------------+
/// ```
///
///
/// # ZInt field
///
/// A ZInt is a specialized variable lenght field that is used to encode an unsigned integer.
///
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// %     zint      %
/// +---------------+
/// ```
///
/// The ZInt encoding represents the value of unsigned integer as follows:
/// 1. It uses a byte where the 7-least significant bits to represent 7 bits of the unsigned integer.
/// 2. If the unsigned integer can not be represented with 7 bits, then the 8th bit is set to 1
///    and a new byte is added. Step 1. is repeated with the following 7 bits of the unsigned integer.
///
/// As an example, the unsigned integer 179317 in a 32-bit little-endian binary representation is:
///
/// ```text
/// 0000 0000 0000 0010 1011 1100 0111 0101
/// ```
///
/// And encoded as ZInt is:
///
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |1|1|1|1|0|1|0|1| -- Byte0
/// +-+-------------+
/// |1|1|1|1|1|0|0|0| -- Byte1
/// +-+-------------+
/// |0|0|0|0|1|0|1|0| -- Byte2
/// +-+-------------+
/// ```
///
/// The maximum number of bytes required to encode a ZInt depends on its resolution
/// in bits, as shown in the following table:
///
/// ```text
/// +--------------+--------------+
/// |   ZInt size  |   Max bytes  |
/// +--------------+--------------+
/// |    8 bits    |   2 bytes    |
/// +--------------+--------------+
/// |   16 bits    |   3 bytes    |
/// +--------------+--------------+
/// |   32 bits    |   5 bytes    |
/// +--------------+--------------+
/// |   64 bits    |   10 bytes   |
/// +--------------+--------------+
///
/// Finally, provided a limit on the maximum number of bytes being used to encode a ZInt,
/// the maximum representable ZInt value is given by the following formula:
///
///     Max ZInt Value := 2 ^ (7 * #max bytes)
/// ```
///
/// # Array field
///
/// An array contains a fixed number of elements whose number is known a priori or indicated by
/// another field. Each element can be either a single byte field or a variable legnth field.
///
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// ~   [element]   ~
/// +---------------+
/// ```
///
///
/// # Vector field
///
/// A vector contains a variable number of elements and is represented as follows:
///
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// ~   <element>   ~
/// +---------------+
/// ```
///
/// A vector field is always expanded as follows:
///
///  ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// %      num      %
/// +---------------+
/// ~   [element]   ~
/// +---------------+
/// ```
///

/// # Zenoh options
///
/// A zenoh option represents an array of bytes that may contain bitflags or embedded values.
/// Each byte option provides 7 bits for encoding bitflags or embedded values as follow:
///
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |O|X|X|X|X|X|X|X| -- Opt0
/// +-+-+-+-+-+-+-+-+
/// ```
///
/// As a result, the option bits are defined as:
///
/// ```text
///  - 0 X: Free to use
///  - 1 X: Free to use
///  - 2 X: Free to use
///  - 3 X: Free to use
///  - 4 X: Free to use
///  - 5 X: Free to use
///  - 6 X: Free to use
///  - 7 O: Additional options      If O==1 then another option will follow
/// ```
///
/// An example of 2 bytes option is illustrated below.
///
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |1|X|X|X|X|X|X|X| -- Opt0. Bit 7 is set to 1. An additional option byte will follow.
/// +-+-+-+-+-+-+-+-+
/// |0|X|X|X|X|X|X|X| -- Opt1. Bit 7 is set to 0. This is the last option byte.
/// +-+-+-+-+-+-+-+-+
/// ```
///
/// Any information MAY be interleaved between options, e.g.:
///
///  ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |1|X|X|X|X|X|X|A| -- Opt0. Bit 7 is set to 1. An additional option byte will follow.
/// +-+-+-+-+-+-+-+-+
/// ~      <u8>     ~ if Opt0(A)==1
/// +---------------+
/// |0|X|X|X|X|X|X|B| -- Opt1. Bit 7 is set to 0. This is the last option byte.
/// +-+-+-+-+-+-+-+-+
/// ~      <u8>     ~ if Opt1(B)==1
/// +---------------+
///  ```
///
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZOpts<const NUM: usize> {
    inner: [u8; NUM],
}

impl From<u8> for ZOpts<1> {
    fn from(obj: u8) -> ZOpts<1> {
        ZOpts { inner: [obj] }
    }
}

impl ZOpts<1> {
    pub const fn new() -> ZOpts<1> {
        ZOpts { inner: [0] }
    }
}

impl Default for ZOpts<1> {
    fn default() -> ZOpts<1> {
        ZOpts::new()
    }
}

impl Deref for ZOpts<1> {
    type Target = [u8; 1];

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for ZOpts<1> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

/// # Wire properties
///
/// Properties can be used to attach custom information to messages. Zenoh protocol
/// only defines a common framework to encode and transmit them withouth enforcing
/// any predefined structure on its content nor type. They are represented as an hashamp
/// indexed by a ZInt whose value is considered an array of bytes by the zenoh protocol.
///
/// Properties are encoded as vector of properties.
///
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// ~  <property>   ~
/// +---------------+
/// ```
///
/// Each property is encoded as TLV (Type, Length, Value) as illustrated below:
///
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// %      key      % -- The ZInt key associated to the proporties, i.e. the type
/// +---------------+
/// ~    <uint8>    ~
/// +---------------+
/// ```
///
/// The internal structure of the property payload is not defined. In order to
/// access the value, the property payload will need to be first deserialized
/// according to its definition.
///
pub type WireProperties = HashMap<ZInt, Vec<u8>>;

impl WBuf {
    pub fn write_wire_properties(&mut self, wps: &WireProperties) -> bool {
        zcheck!(self.write_usize_as_zint(wps.len()));
        for (k, v) in wps.iter() {
            zcheck!(self.write_zint(*k));
            zcheck!(self.write_bytes_array(v.as_slice()));
        }
        true
    }
}

impl ZBuf {
    pub fn read_wire_properties(&mut self) -> Option<WireProperties> {
        let len = self.read_zint_as_usize()?;
        let mut wps = WireProperties::with_capacity(len);
        for _ in 0..len {
            let k = self.read_zint()?;
            let v = self.read_bytes_array()?;
            if wps.insert(k, v).is_some() {
                return None;
            }
        }
        Some(wps)
    }
}

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

pub trait ZMessage: Clone + PartialEq + Debug {
    type Proto;
    const ID: u8;

    fn write(&self, wbuf: &mut WBuf) -> bool;
    fn read(zbuf: &mut ZBuf, header: u8) -> Option<Self>
    where
        Self: Sized;
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
