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
use crate::core::ZInt;
use zenoh_buffers::ZBuf;

/// # Zenoh extensions
///
/// A zenoh extension is encoded as TLV (Type, Length, Value).
/// Zenoh extensions with unknown IDs (i.e., type) can be skipped by reading the length and
/// not decoding the body (i.e. value). In case the zenoh extension is unknown, it is
/// still possible to forward it to the next hops, which in turn may be able to understand it.
/// This results in the capability of introducing new extensions in an already running system
/// without requiring the redeployment of the totality of infrastructure nodes.
///
/// The zenoh extension wire format is the following:
///
/// ```text
/// Header flags:
/// - X: Reserved
/// - F: Forward        If F==1 then the extension needs to be forwarded. (*)
/// - Z: More           If Z==1 then another extension will follow.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|ENC|    ID   |
/// +-+-+-+---------+
/// %    length     % -- If ENC == ZInt || ENC == ZBuf
/// +---------------+
/// ~     [u8]      ~ -- If ENC == ZBuf
/// +---------------+
/// ```
///
/// Encoding:
/// - 0b00: Unit
/// - 0b01: ZInt
/// - 0b10: ZBuf
/// - 0b11: Reserved
///
/// (*) If the zenoh extension is not understood, then it SHOULD NOT be dropped and it
///     SHOULD be forwarded to the next hops.
///
pub mod iext {
    pub const ID_BITS: u8 = 5;
    pub const ID_MASK: u8 = !(u8::MAX << ID_BITS);

    pub const ENC_UNIT: u8 = 0b00 << ID_BITS;
    pub const ENC_ZINT: u8 = 0b01 << ID_BITS;
    pub const ENC_ZBUF: u8 = 0b10 << ID_BITS;
    pub const ENC_MASK: u8 = 0b11 << ID_BITS;

    pub const FLAG_Z: u8 = 1 << 7;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ZExtUnit<const ID: u8>;

impl<const ID: u8> ZExtUnit<{ ID }> {
    pub fn new() -> Self {
        Self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ZExtZInt<const ID: u8> {
    pub value: ZInt,
}

impl<const ID: u8> ZExtZInt<{ ID }> {
    pub fn new(value: ZInt) -> Self {
        Self { value }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ZExtZBuf<const ID: u8> {
    pub value: ZBuf,
}

impl<const ID: u8> ZExtZBuf<{ ID }> {
    pub fn new(value: ZBuf) -> Self {
        Self { value }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ZExtUnknown {
    pub id: u8,
    pub body: ZExtensionBody,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ZExtensionBody {
    Unit,
    ZInt(ZInt),
    ZBuf(ZBuf),
}
