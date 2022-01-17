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
/// The field size depends on the element definition and/or actual encofing. An example of variable
/// lenght element is the ZInt. A ZInt is a fixed-sized unsigned integer serialized with a variable
/// length encoding.
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// ~    element    ~ -- ZInt
/// +---------------+
/// ```
///
///
/// # Array field
///
/// An array contains a fixed number of elements whose number is known a priori or indicated by
/// another field. Each element can be either a single byte field or a variable legnth field.
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
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// ~   <element>   ~
/// +---------------+
///
/// A vector field is always expanded as follows:
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// ~ elements_num  ~ -- ZInt
/// +---------------+
/// ~   [element]   ~
/// +---------------+
/// ```

/// # Zenoh options
///
/// A zenoh option is encoded as TLV as follows:
///
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |O|X|X|   ID    |
/// +-+-+-+---------+
/// ~    length     ~
/// +---------------+
/// ~     [u8]      ~
/// +---------------+
/// ```
///
/// The option header bit 7 is reserved and indicates wether another
/// zenoh option will follow. Zenoh options with unknown IDs can be
/// skipped by reading the length and not decoding the body.
///
pub trait ZOpt: Clone + PartialEq + Eq + std::fmt::Debug {
    fn header(&self) -> u8;
    fn length(&self) -> usize;
    fn write_body(&self, wbuf: &mut WBuf) -> bool;
}

impl WBuf {
    fn write_option<T: ZOpt>(&mut self, opt: &T, more: bool) -> bool {
        const FLAG_O: u8 = 1 << 7;

        let mut header = opt.header();
        if more {
            header |= FLAG_O;
        }

        self.write(header) && self.write_usize_as_zint(opt.length()) && opt.write_body(self)
    }
}

/// # Wire options
///
/// Unknown options can be stored into a wire options structure
///
pub type WireOptions = HashMap<u8, Vec<u8>>;

/// # Wire properties
///
/// Properties can be used to attach custom information to messages. Zenoh protocol
/// only defines a common framework to encode and transmit them withouth enforcing
/// any predefined structure on its content nor type. They are represented as an hashamp
/// indexed by an integer whose value is considered an array of bytes by Zenoh protocol.
///
/// Properties are encoded as vector of properties.
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// ~  <property>   ~
/// +---------------+
/// ```
/// Each property is encoded as TLV (Type, Length, Value) as illustrated below:
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// ~      key      ~ -- The ZInt key associated to the proporties, i.e. the type
/// +---------------+
/// ~    <uint8>    ~
/// +---------------+
/// ```
///
/// The internal structure of the property payload is not defined. In order to
/// access the value, the property payload will need to be first deserialized
/// according to its definition.
///
/// There are two different types of properties:
/// - Zenoh properties
/// - User properties
///
/// Zenoh properties are used to attach implementation-specific information to the
/// messages in a compatible manner, i.e. the receiver is able to skip them.
/// This is particular useful during the zenoh session establishment, e.g. Init/Open/Join,
/// to test wether certain protocols extensions are supported by the communicating zenoh nodes.
/// Zenoh properties might be encoded/decoded at each hop. However, this mechanism requires
/// a two-steps serialization/deserialization of the message since proporties need to be encoded
/// first and then serialized in the message. Likewise, the message needs to be deserialized
/// first and the properties decoded next. As a result, Zenoh properties are meant to be used
/// on those messages where compatibility is more important than efficiency. In case of
/// prioritizing efficiency, Zenoh extensions should be used instead (see later).
///
/// User properties are used to attach user-specific information to the messages in a
/// compatible manner, i.e. the receiver is able to skip them. User properties are encoded
/// and decoded in the same way as the zenoh properties. However, only the user is supposed
/// to interpret them, i.e. in an end-to-end fashion, and not at every hop.
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
            wps.insert(k, v);
        }
        Some(wps)
    }
}

/// # Zenoh extensions
///
/// Zenoh extensions are well-defined extensions that follow a pre-agreed encoding.
/// This allows to efficiently encode and decode zenoh extensions in one go instead of
/// the two-steps serializarions/deserialization required in the properties.
/// Since zenoh extensions require both endpoints to be able to encode/decode them,
/// they MUST NOT be enabled if in the negotiation phase (i.e. Init/Open/Join)
/// both zenoh nodes explicitly agree on the set of extensions to enable.
/// Finally, zenoh extensions are contextualized for each protocol message. I.e.,
/// the encoding of those extensions may be different from message to message.

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
