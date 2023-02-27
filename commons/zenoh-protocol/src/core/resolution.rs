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
    pub const fn mask(&self) -> ZInt {
        match self {
            Bits::U8 => u8::MAX as ZInt,
            Bits::U16 => u16::MAX as ZInt,
            Bits::U32 => u32::MAX as ZInt,
            Bits::U64 => u64::MAX as ZInt,
        }
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
