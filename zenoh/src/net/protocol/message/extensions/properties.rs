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
use crate::net::protocol::message::WireProperties;
use std::ops::{Deref, DerefMut};

/// # User extension
///
/// It includes the zenoh properties.
///  
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// ~  <property>   ~
/// +---------------+
///
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ZExtProperties<const ID: u8> {
    pub value: WireProperties,
}

impl<const ID: u8> ZExtProperties<{ ID }> {
    pub fn new(value: WireProperties) -> Self {
        Self { value }
    }
}

impl<const ID: u8> Default for ZExtProperties<{ ID }> {
    fn default() -> Self {
        Self::new(WireProperties::new())
    }
}

impl<const ID: u8> ZExtension for ZExtProperties<{ ID }> {
    fn id(&self) -> u8 {
        ID
    }

    fn length(&self) -> usize {
        self.value
            .iter()
            .fold(zint_len(self.value.len() as ZInt), |len, (k, v)| {
                len + zint_len(*k) + zint_len(v.len() as ZInt) + v.len()
            })
    }

    fn write(&self, wbuf: &mut WBuf) -> bool {
        wbuf.write_wire_properties(&self.value)
    }

    fn read(zbuf: &mut ZBuf, _header: u8, _length: usize) -> Option<Self> {
        let value = zbuf.read_wire_properties()?;
        Some(Self { value })
    }
}

impl<const ID: u8> From<WireProperties> for ZExtProperties<{ ID }> {
    fn from(value: WireProperties) -> Self {
        Self::new(value)
    }
}

impl<const ID: u8> From<ZExtProperties<{ ID }>> for WireProperties {
    fn from(ext: ZExtProperties<{ ID }>) -> WireProperties {
        ext.value
    }
}

impl<const ID: u8> Deref for ZExtProperties<{ ID }> {
    type Target = WireProperties;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<const ID: u8> DerefMut for ZExtProperties<{ ID }> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}
