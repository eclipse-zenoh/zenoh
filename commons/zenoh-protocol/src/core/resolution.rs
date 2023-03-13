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
use super::ZInt;
use crate::defaults::{FRAME_SN_RESOLUTION, REQUEST_ID_RESOLUTION};
use core::{fmt, str::FromStr};
use zenoh_result::{bail, ZError};

#[repr(u8)]
// The value represents the 2-bit encoded value
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Bits {
    U8 = 0b00,
    U16 = 0b01,
    U32 = 0b10,
    U64 = 0b11,
}

impl Bits {
    const S8: &str = "8bit";
    const S16: &str = "16bit";
    const S32: &str = "32bit";
    const S64: &str = "64bit";

    pub const fn mask(&self) -> ZInt {
        match self {
            Bits::U8 => u8::MAX as ZInt,
            Bits::U16 => u16::MAX as ZInt,
            Bits::U32 => u32::MAX as ZInt,
            Bits::U64 => u64::MAX as ZInt,
        }
    }

    pub const fn to_str(self) -> &'static str {
        match self {
            Bits::U8 => Self::S8,
            Bits::U16 => Self::S16,
            Bits::U32 => Self::S32,
            Bits::U64 => Self::S64,
        }
    }
}

impl FromStr for Bits {
    type Err = ZError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            Bits::S8 => Ok(Bits::U8),
            Bits::S16 => Ok(Bits::U16),
            Bits::S32 => Ok(Bits::U32),
            Bits::S64 => Ok(Bits::U64),
            _ => bail!(
                "{s} is not a valid Bits value. Valid values are: '{}', '{}', '{}', '{}'.",
                Bits::S8,
                Bits::S16,
                Bits::S32,
                Bits::S64
            ),
        }
    }
}

impl fmt::Display for Bits {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.to_str())
    }
}

#[repr(u8)]
// The value indicates the bit offest
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    FrameSN = 0,
    RequestID = 2,
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resolution(u8);

impl Resolution {
    pub const fn as_u8(&self) -> u8 {
        self.0
    }

    pub const fn get(&self, field: Field) -> Bits {
        let value = (self.0 >> (field as u8)) & 0b11;
        unsafe { core::mem::transmute(value) }
    }

    pub fn set(&mut self, field: Field, bits: Bits) {
        self.0 &= !(0b11 << field as u8); // Clear bits
        self.0 |= (bits as u8) << (field as u8); // Set bits
    }

    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();
        let v: u8 = rng.gen();
        Self(v & 0b00001111)
    }
}

impl Default for Resolution {
    fn default() -> Self {
        let frame_sn = FRAME_SN_RESOLUTION as u8;
        let request_id = (REQUEST_ID_RESOLUTION as u8) << 2;
        Self(frame_sn | request_id)
    }
}

impl From<u8> for Resolution {
    fn from(v: u8) -> Self {
        Self(v)
    }
}

// Serde
impl serde::Serialize for Bits {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.to_str())
    }
}

pub struct BitsVisitor;
impl<'de> serde::de::Visitor<'de> for BitsVisitor {
    type Value = Bits;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(
            formatter,
            "either '{}', '{}', '{}', '{}'.",
            Bits::S8,
            Bits::S16,
            Bits::S32,
            Bits::S64,
        )
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        v.parse().map_err(|_| {
            serde::de::Error::unknown_variant(v, &[Bits::S8, Bits::S16, Bits::S32, Bits::S64])
        })
    }

    fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        self.visit_str(v)
    }

    fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        self.visit_str(&v)
    }
}

impl<'de> serde::Deserialize<'de> for Bits {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(BitsVisitor)
    }
}
