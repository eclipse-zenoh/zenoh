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
use super::{is_forward, ZExtension};
use crate::net::protocol::core::ZInt;
use crate::net::protocol::io::{zint_len, WBuf, ZBuf};

/// # Unknown extension
///
/// It includes an array of bytes.
///  
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// ~     [u8]      ~
/// +---------------+
///
#[derive(Clone, PartialEq, Debug)]
pub struct ZExtUnknown {
    pub header: u8,
    pub body: ZBuf,
}

impl ZExtension for ZExtUnknown {
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
        if is_forward(header) {
            let mut body = ZBuf::new();
            if zbuf.read_into_zbuf(&mut body, length) {
                return Some(Self { header, body });
            }
        }
        None
    }
}
