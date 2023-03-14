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
use core::convert::TryFrom;
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
/// - E |: Encoding     The encoding of the extension
/// - E/
/// - Z: More           If Z==1 then another extension will follow.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|ENC|    ID   |
/// +-+-+-+---------+
/// %    length     % -- If ENC == u64 || ENC == ZBuf
/// +---------------+
/// ~     [u8]      ~ -- If ENC == ZBuf
/// +---------------+
/// ```
///
/// Encoding:
/// - 0b00: Unit
/// - 0b01: u64
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
    pub const ENC_Z64: u8 = 0b01 << ID_BITS;
    pub const ENC_ZBUF: u8 = 0b10 << ID_BITS;
    pub const ENC_MASK: u8 = 0b11 << ID_BITS;

    pub const FLAG_Z: u8 = 1 << 7;
}

pub struct DidntConvert;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZExtUnit<const ID: u8>;

impl<const ID: u8> ZExtUnit<{ ID }> {
    pub const ID: u8 = ID;

    pub const fn new() -> Self {
        Self
    }

    pub const fn transmute<const DI: u8>(self) -> ZExtUnit<{ DI }> {
        ZExtUnit::new()
    }

    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        Self::new()
    }
}

impl<const ID: u8> TryFrom<ZExtUnknown> for ZExtUnit<{ ID }> {
    type Error = DidntConvert;

    fn try_from(v: ZExtUnknown) -> Result<Self, Self::Error> {
        if v.id != ID {
            return Err(DidntConvert);
        }
        match v.body {
            ZExtensionBody::Unit => Ok(Self::new()),
            _ => Err(DidntConvert),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZExtu64<const ID: u8> {
    pub value: u64,
}

impl<const ID: u8> ZExtu64<{ ID }> {
    pub const ID: u8 = ID;

    pub const fn new(value: u64) -> Self {
        Self { value }
    }

    pub const fn transmute<const DI: u8>(self) -> ZExtu64<{ DI }> {
        ZExtu64::new(self.value)
    }

    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();
        let value: u64 = rng.gen();
        Self { value }
    }
}

impl<const ID: u8> TryFrom<ZExtUnknown> for ZExtu64<{ ID }> {
    type Error = DidntConvert;

    fn try_from(v: ZExtUnknown) -> Result<Self, Self::Error> {
        if v.id != ID {
            return Err(DidntConvert);
        }
        match v.body {
            ZExtensionBody::Z64(v) => Ok(Self::new(v)),
            _ => Err(DidntConvert),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZExtZBuf<const ID: u8> {
    pub value: ZBuf,
}

impl<const ID: u8> ZExtZBuf<{ ID }> {
    pub const ID: u8 = ID;

    pub const fn new(value: ZBuf) -> Self {
        Self { value }
    }

    pub fn transmute<const DI: u8>(self) -> ZExtZBuf<{ DI }> {
        ZExtZBuf::new(self.value)
    }

    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();
        let value = ZBuf::rand(rng.gen_range(8..=64));
        Self { value }
    }
}

impl<const ID: u8> TryFrom<ZExtUnknown> for ZExtZBuf<{ ID }> {
    type Error = DidntConvert;

    fn try_from(v: ZExtUnknown) -> Result<Self, Self::Error> {
        if v.id != ID {
            return Err(DidntConvert);
        }
        match v.body {
            ZExtensionBody::ZBuf(v) => Ok(Self::new(v)),
            _ => Err(DidntConvert),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZExtUnknown {
    pub id: u8,
    pub body: ZExtensionBody,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZExtensionBody {
    Unit,
    Z64(u64),
    ZBuf(ZBuf),
}

impl ZExtUnknown {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::{seq::SliceRandom, Rng};

        let mut rng = rand::thread_rng();
        let id: u8 = rng.gen_range(0x00..=iext::ID_MASK);
        let body = [
            ZExtensionBody::Unit,
            ZExtensionBody::Z64(rng.gen()),
            ZExtensionBody::ZBuf(ZBuf::rand(rng.gen_range(8..=64))),
        ]
        .choose(&mut rng)
        .unwrap()
        .clone();
        Self { id, body }
    }
}

impl<const ID: u8> From<ZExtUnit<{ ID }>> for ZExtUnknown {
    fn from(_: ZExtUnit<{ ID }>) -> Self {
        ZExtUnknown {
            id: ID,
            body: ZExtensionBody::Unit,
        }
    }
}

impl<const ID: u8> From<ZExtu64<{ ID }>> for ZExtUnknown {
    fn from(e: ZExtu64<{ ID }>) -> Self {
        ZExtUnknown {
            id: ID,
            body: ZExtensionBody::Z64(e.value),
        }
    }
}

impl<const ID: u8> From<ZExtZBuf<{ ID }>> for ZExtUnknown {
    fn from(e: ZExtZBuf<{ ID }>) -> Self {
        ZExtUnknown {
            id: ID,
            body: ZExtensionBody::ZBuf(e.value),
        }
    }
}
