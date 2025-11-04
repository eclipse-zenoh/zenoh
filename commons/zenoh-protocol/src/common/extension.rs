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
use core::{
    convert::TryFrom,
    fmt::{self, Debug},
};

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
/// |Z|ENC|M|   ID  |
/// +-+---+-+-------+
/// %    length     % -- If ENC == Z64 || ENC == ZBuf (z32)
/// +---------------+
/// ~     [u8]      ~ -- If ENC == ZBuf
/// +---------------+
///
/// Encoding:
/// - 0b00: Unit
/// - 0b01: Z64
/// - 0b10: ZBuf
/// - 0b11: Reserved
///
/// (*) If the zenoh extension is not understood, then it SHOULD NOT be dropped and it
///     SHOULD be forwarded to the next hops.
/// ```
///
pub mod iext {
    use core::fmt;

    pub const ID_BITS: u8 = 4;
    pub const ID_MASK: u8 = !(u8::MAX << ID_BITS);

    pub const FLAG_M: u8 = 1 << 4;
    pub const ENC_UNIT: u8 = 0b00 << 5;
    pub const ENC_Z64: u8 = 0b01 << 5;
    pub const ENC_ZBUF: u8 = 0b10 << 5;
    pub const ENC_MASK: u8 = 0b11 << 5;
    pub const FLAG_Z: u8 = 1 << 7;

    pub const fn eid(header: u8) -> u8 {
        header & !FLAG_Z
    }

    pub const fn mid(header: u8) -> u8 {
        header & ID_MASK
    }

    pub(super) const fn id(id: u8, mandatory: bool, encoding: u8) -> u8 {
        let mut id = id & ID_MASK;
        if mandatory {
            id |= FLAG_M;
        } else {
            id &= !FLAG_M;
        }
        id |= encoding;
        id
    }

    pub(super) const fn is_mandatory(id: u8) -> bool {
        crate::common::imsg::has_flag(id, FLAG_M)
    }

    pub(super) fn fmt(f: &mut fmt::DebugStruct, id: u8) {
        f.field("Id", &(id & ID_MASK))
            .field("Mandatory", &is_mandatory(id))
            .field(
                "Encoding",
                match id & ENC_MASK {
                    ENC_UNIT => &"Unit",
                    ENC_Z64 => &"Z64",
                    ENC_ZBUF => &"ZBuf",
                    _ => &"Unknown",
                },
            );
    }
}

pub struct DidntConvert;

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ZExtUnit<const ID: u8>;

impl<const ID: u8> Default for ZExtUnit<{ ID }> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const ID: u8> ZExtUnit<{ ID }> {
    pub const ID: u8 = ID;

    pub const fn new() -> Self {
        Self
    }

    pub const fn id(mandatory: bool) -> u8 {
        iext::id(ID, mandatory, iext::ENC_UNIT)
    }

    pub const fn is_mandatory(&self) -> bool {
        iext::is_mandatory(ID)
    }

    pub const fn transmute<const DI: u8>(self) -> ZExtUnit<{ DI }> {
        ZExtUnit::new()
    }

    #[cfg(feature = "test")]
    #[doc(hidden)]
    pub fn rand() -> Self {
        Self::new()
    }
}

impl<const ID: u8> Debug for ZExtUnit<{ ID }> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = f.debug_struct("ZExtUnit");
        iext::fmt(&mut s, ID);
        s.finish()
    }
}

impl<const ID: u8> TryFrom<ZExtUnknown> for ZExtUnit<{ ID }> {
    type Error = DidntConvert;

    fn try_from(v: ZExtUnknown) -> Result<Self, Self::Error> {
        if v.id != ID {
            return Err(DidntConvert);
        }
        match v.body {
            ZExtBody::Unit => Ok(Self::new()),
            _ => Err(DidntConvert),
        }
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ZExtZ64<const ID: u8> {
    pub value: u64,
}

impl<const ID: u8> ZExtZ64<{ ID }> {
    pub const ID: u8 = ID;

    pub const fn new(value: u64) -> Self {
        Self { value }
    }

    pub const fn id(mandatory: bool) -> u8 {
        iext::id(ID, mandatory, iext::ENC_Z64)
    }

    pub const fn is_mandatory(&self) -> bool {
        iext::is_mandatory(ID)
    }

    pub const fn transmute<const DI: u8>(self) -> ZExtZ64<{ DI }> {
        ZExtZ64::new(self.value)
    }

    #[cfg(feature = "test")]
    #[doc(hidden)]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();
        let value: u64 = rng.gen();
        Self { value }
    }
}

impl<const ID: u8> Debug for ZExtZ64<{ ID }> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = f.debug_struct("ZExtZ64");
        iext::fmt(&mut s, ID);
        s.field("Value", &self.value).finish()
    }
}

impl<const ID: u8> TryFrom<ZExtUnknown> for ZExtZ64<{ ID }> {
    type Error = DidntConvert;

    fn try_from(v: ZExtUnknown) -> Result<Self, Self::Error> {
        if v.id != ID {
            return Err(DidntConvert);
        }
        match v.body {
            ZExtBody::Z64(v) => Ok(Self::new(v)),
            _ => Err(DidntConvert),
        }
    }
}

#[repr(transparent)]
#[derive(Clone, PartialEq, Eq)]
pub struct ZExtZBuf<const ID: u8> {
    pub value: ZBuf,
}

impl<const ID: u8> ZExtZBuf<{ ID }> {
    pub const ID: u8 = ID;

    pub const fn new(value: ZBuf) -> Self {
        Self { value }
    }

    pub const fn id(mandatory: bool) -> u8 {
        iext::id(ID, mandatory, iext::ENC_ZBUF)
    }

    pub const fn is_mandatory(&self) -> bool {
        iext::is_mandatory(ID)
    }

    pub fn transmute<const DI: u8>(self) -> ZExtZBuf<{ DI }> {
        ZExtZBuf::new(self.value)
    }

    #[cfg(feature = "test")]
    #[doc(hidden)]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();
        let value = ZBuf::rand(rng.gen_range(8..=64));
        Self { value }
    }
}

impl<const ID: u8> Debug for ZExtZBuf<{ ID }> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = f.debug_struct("ZExtZBuf");
        iext::fmt(&mut s, ID);
        s.field("Value", &self.value).finish()
    }
}

impl<const ID: u8> TryFrom<ZExtUnknown> for ZExtZBuf<{ ID }> {
    type Error = DidntConvert;

    fn try_from(v: ZExtUnknown) -> Result<Self, Self::Error> {
        if v.id != ID {
            return Err(DidntConvert);
        }
        match v.body {
            ZExtBody::ZBuf(v) => Ok(Self::new(v)),
            _ => Err(DidntConvert),
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct ZExtZBufHeader<const ID: u8> {
    pub len: usize,
}

impl<const ID: u8> ZExtZBufHeader<{ ID }> {
    pub const ID: u8 = ID;

    pub const fn new(len: usize) -> Self {
        Self { len }
    }

    pub const fn id(mandatory: bool) -> u8 {
        iext::id(ID, mandatory, iext::ENC_ZBUF)
    }

    pub const fn is_mandatory(&self) -> bool {
        iext::is_mandatory(ID)
    }
}

impl<const ID: u8> Debug for ZExtZBufHeader<{ ID }> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = f.debug_struct("ZExtZBufHeader");
        iext::fmt(&mut s, ID);
        s.field("Len", &self.len).finish()
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum ZExtBody {
    #[default]
    Unit,
    Z64(u64),
    ZBuf(ZBuf),
}

impl ZExtBody {
    #[cfg(feature = "test")]
    #[doc(hidden)]
    pub fn rand() -> Self {
        use rand::{seq::SliceRandom, Rng};
        let mut rng = rand::thread_rng();
        [
            ZExtBody::Unit,
            ZExtBody::Z64(rng.gen()),
            ZExtBody::ZBuf(ZBuf::rand(rng.gen_range(8..=64))),
        ]
        .choose(&mut rng)
        .unwrap()
        .clone()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct ZExtUnknown {
    pub id: u8,
    pub body: ZExtBody,
}

impl ZExtUnknown {
    pub const fn new(id: u8, mandatory: bool, body: ZExtBody) -> Self {
        let enc = match &body {
            ZExtBody::Unit => iext::ENC_UNIT,
            ZExtBody::Z64(_) => iext::ENC_Z64,
            ZExtBody::ZBuf(_) => iext::ENC_ZBUF,
        };
        let id = iext::id(id, mandatory, enc);
        Self { id, body }
    }

    pub const fn is_mandatory(&self) -> bool {
        iext::is_mandatory(self.id)
    }

    #[cfg(feature = "test")]
    #[doc(hidden)]
    pub fn rand() -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        let id: u8 = rng.gen_range(0x00..=iext::ID_MASK);
        let mandatory = rng.gen_bool(0.5);
        let body = ZExtBody::rand();
        Self::new(id, mandatory, body)
    }

    #[cfg(feature = "test")]
    #[doc(hidden)]
    pub fn rand2(start: u8, mandatory: bool) -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        let id: u8 = rng.gen_range(start..=iext::ID_MASK);
        let body = ZExtBody::rand();
        Self::new(id, mandatory, body)
    }
}

impl Debug for ZExtUnknown {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = f.debug_struct("ZExtUnknown");
        iext::fmt(&mut s, self.id);
        match &self.body {
            ZExtBody::Unit => {}
            ZExtBody::Z64(v) => {
                s.field("Value", v);
            }
            ZExtBody::ZBuf(v) => {
                s.field("Value", v);
            }
        };
        s.finish()
    }
}

impl<const ID: u8> From<ZExtUnit<{ ID }>> for ZExtUnknown {
    fn from(_: ZExtUnit<{ ID }>) -> Self {
        ZExtUnknown {
            id: ID,
            body: ZExtBody::Unit,
        }
    }
}

impl<const ID: u8> From<ZExtZ64<{ ID }>> for ZExtUnknown {
    fn from(e: ZExtZ64<{ ID }>) -> Self {
        ZExtUnknown {
            id: ID,
            body: ZExtBody::Z64(e.value),
        }
    }
}

impl<const ID: u8> From<ZExtZBuf<{ ID }>> for ZExtUnknown {
    fn from(e: ZExtZBuf<{ ID }>) -> Self {
        ZExtUnknown {
            id: ID,
            body: ZExtBody::ZBuf(e.value),
        }
    }
}

// Macros
#[macro_export]
macro_rules! zextunit {
    ($id:expr, $m:expr) => {
        ZExtUnit<{ ZExtUnit::<$id>::id($m) }>
    }
}

#[macro_export]
macro_rules! zextz64 {
    ($id:expr, $m:expr) => {
        ZExtZ64<{ ZExtZ64::<$id>::id($m) }>
    }
}

#[macro_export]
macro_rules! zextzbuf {
    ($id:expr, $m:expr) => {
        ZExtZBuf<{ ZExtZBuf::<$id>::id($m) }>
    }
}
