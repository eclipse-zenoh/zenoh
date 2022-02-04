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
use crate::net::protocol::io::{WBuf, ZBuf};
use std::ops::{Deref, DerefMut};

/// # Byte extension
///
/// It is an extension containing a Byte.
///  
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |     value     |
/// +---------------+
///
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ZExtByte<const ID: u8> {
    pub value: u8,
}

impl<const ID: u8> ZExtByte<{ ID }> {
    pub fn new(value: u8) -> Self {
        Self { value }
    }
}

impl<const ID: u8> ZExtension for ZExtByte<{ ID }> {
    fn id(&self) -> u8 {
        ID
    }

    fn length(&self) -> usize {
        1
    }

    fn write(&self, wbuf: &mut WBuf) -> bool {
        wbuf.write(self.value)
    }

    fn read(zbuf: &mut ZBuf, _header: u8, _length: usize) -> Option<Self> {
        let value = zbuf.read()?;
        Some(Self { value })
    }
}

impl<const ID: u8> From<u8> for ZExtByte<{ ID }> {
    fn from(v: u8) -> Self {
        Self::new(v)
    }
}

impl<const ID: u8> Deref for ZExtByte<{ ID }> {
    type Target = u8;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<const ID: u8> DerefMut for ZExtByte<{ ID }> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}
