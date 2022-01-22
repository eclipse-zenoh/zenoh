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
use super::core::ZInt;
use super::io::{zint_len, WBuf, ZBuf};
use super::{zext, WireProperties, ZExtension, ZExtensionId};

/// # Experimental extension
///
/// It indicates the experimental version of zenoh.
///  
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// %    version    %
/// +---------------+
///
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ZExperimental<const ID: u8> {
    pub version: ZInt,
}

impl<const ID: u8> ZExperimental<{ ID }> {
    pub fn new(version: ZInt) -> Self {
        Self { version }
    }
}

impl<const ID: u8> ZExtensionId for ZExperimental<{ ID }> {
    const ID: u8 = ID;
}

impl<const ID: u8> ZExtension for ZExperimental<{ ID }> {
    fn id(&self) -> u8 {
        ID
    }

    fn length(&self) -> usize {
        zint_len(self.version)
    }

    fn write(&self, wbuf: &mut WBuf) -> bool {
        wbuf.write_zint(self.version)
    }

    fn read(zbuf: &mut ZBuf, _header: u8, _length: usize) -> Option<Self> {
        let version = zbuf.read_zint()?;
        Some(Self { version })
    }
}

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
pub struct ZUser<const ID: u8> {
    pub properties: WireProperties,
}

impl<const ID: u8> ZUser<{ ID }> {
    pub fn new(properties: WireProperties) -> Self {
        Self { properties }
    }
}

impl<const ID: u8> Default for ZUser<{ ID }> {
    fn default() -> Self {
        Self::new(WireProperties::new())
    }
}

impl<const ID: u8> ZExtensionId for ZUser<{ ID }> {
    const ID: u8 = ID;
}

impl<const ID: u8> ZExtension for ZUser<{ ID }> {
    fn id(&self) -> u8 {
        ID
    }

    fn length(&self) -> usize {
        self.properties
            .iter()
            .fold(zint_len(self.properties.len() as ZInt), |len, (k, v)| {
                len + zint_len(*k) + zint_len(v.len() as ZInt) + v.len()
            })
    }

    fn write(&self, wbuf: &mut WBuf) -> bool {
        wbuf.write_wire_properties(&self.properties)
    }

    fn read(zbuf: &mut ZBuf, _header: u8, _length: usize) -> Option<Self> {
        let properties = zbuf.read_wire_properties()?;
        Some(Self { properties })
    }
}

/// # Unknown extension
///
/// It includes the zenoh properties.
///  
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// ~     [u8]      ~
/// +---------------+
///
#[derive(Clone, PartialEq, Debug)]
pub struct ZUnknown {
    pub header: u8,
    pub body: ZBuf,
}

impl ZExtension for ZUnknown {
    fn id(&self) -> u8 {
        self.header
    }

    fn length(&self) -> usize {
        zint_len(self.body.len() as ZInt) + self.body.len()
    }

    fn write(&self, wbuf: &mut WBuf) -> bool {
        wbuf.write(self.header)
            && wbuf.write_usize_as_zint(self.length())
            && wbuf.write_zbuf(&self.body, false)
    }

    fn read(zbuf: &mut ZBuf, header: u8, length: usize) -> Option<Self> {
        if zext::is_forward(header) {
            let mut body = ZBuf::new();
            if zbuf.read_into_zbuf(&mut body, length) {
                return Some(Self { header, body });
            }
        }
        None
    }
}
