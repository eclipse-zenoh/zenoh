//
// Copyright (c) 2017, 2022 ADLINK Technology Inc.
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
use super::ZExtension;
use crate::net::protocol::core::ZInt;
use crate::net::protocol::io::{zint_len, WBuf, ZBuf};
use std::ops::{Deref, DerefMut};

/// # ZInt extension
///
/// It is an extension containing a ZInt.
///  
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// %     value     %
/// +---------------+
///
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ZExtZInt<const ID: u8> {
    pub value: ZInt,
}

impl<const ID: u8> ZExtZInt<{ ID }> {
    pub fn new(value: ZInt) -> Self {
        Self { value }
    }
}

impl<const ID: u8> ZExtension for ZExtZInt<{ ID }> {
    fn id(&self) -> u8 {
        ID
    }

    fn length(&self) -> usize {
        zint_len(self.value)
    }

    fn write(&self, wbuf: &mut WBuf) -> bool {
        wbuf.write_zint(self.value)
    }

    fn read(zbuf: &mut ZBuf, _header: u8, _length: usize) -> Option<Self> {
        let value = zbuf.read_zint()?;
        Some(Self { value })
    }
}

impl<const ID: u8> From<ZInt> for ZExtZInt<{ ID }> {
    fn from(v: ZInt) -> Self {
        Self::new(v)
    }
}

impl<const ID: u8> Deref for ZExtZInt<{ ID }> {
    type Target = ZInt;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<const ID: u8> DerefMut for ZExtZInt<{ ID }> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}
