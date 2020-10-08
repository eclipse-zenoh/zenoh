//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
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
use std::convert::TryFrom;

use super::{ArcSlice, RBuf, WBuf};
use crate::core::{ZInt, ZINT_MAX_BYTES};

use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::{to_zint, zerror};

macro_rules! read_zint {
    ($buf:expr, $res:ty) => {
        let mut v: $res = 0;
        let mut b = $buf.read()?;
        let mut i = 0;
        let mut k = ZINT_MAX_BYTES;
        while b > 0x7f && k > 0 {
            v |= ((b & 0x7f) as $res) << i;
            i += 7;
            b = $buf.read()?;
            k -= 1;
        }
        if k > 0 {
            v |= ((b & 0x7f) as $res) << i;
            return Ok(v);
        } else {
            return zerror!(ZErrorKind::InvalidMessage {
                descr: format!("Invalid ZInt (larget than ZInt max value: {})", ZInt::MAX)
            });
        }
    };
}

impl RBuf {
    pub fn read_zint(&mut self) -> ZResult<ZInt> {
        read_zint!(self, ZInt);
    }

    pub fn read_zint_as_u64(&mut self) -> ZResult<u64> {
        read_zint!(self, u64);
    }

    // Same as read_bytes but with array length before the bytes.
    pub fn read_bytes_array(&mut self) -> ZResult<Vec<u8>> {
        let len = self.read_zint()?;
        let mut buf = vec![0; len as usize];
        self.read_bytes(buf.as_mut_slice())?;
        Ok(buf)
    }

    // Same as read_bytes_array but 0 copy on RBuf.
    pub fn read_rbuf(&mut self) -> ZResult<RBuf> {
        let len = self.read_zint()?;
        let mut rbuf = RBuf::new();
        self.read_into_rbuf(&mut rbuf, len as usize)?;
        Ok(rbuf)
    }

    pub fn read_string(&mut self) -> ZResult<String> {
        let bytes = self.read_bytes_array()?;
        Ok(String::from(String::from_utf8_lossy(&bytes)))
    }
}

macro_rules! write_zint {
    ($buf:expr, $val:expr) => {
        let mut c = $val;
        let mut b: u8 = (c & 0xff) as u8;
        while c > 0x7f && $buf.write(b | 0x80) {
            c >>= 7;
            b = (c & 0xff) as u8;
        }
        return $buf.write(b);
    };
}

impl WBuf {
    /// This the traditional VByte encoding, in which an arbirary integer
    /// is encoded as a sequence of 7 bits integers
    pub fn write_zint(&mut self, v: ZInt) -> bool {
        write_zint!(self, v);
    }

    pub fn write_u64_as_zint(&mut self, v: u64) -> bool {
        write_zint!(self, v);
    }

    // Same as write_bytes but with array length before the bytes.
    pub fn write_bytes_array(&mut self, s: &[u8]) -> bool {
        self.write_zint(to_zint!(s.len())) && self.write_bytes(s)
    }

    pub fn write_string(&mut self, s: &str) -> bool {
        self.write_zint(to_zint!(s.len())) && self.write_bytes(s.as_bytes())
    }

    // Similar than write_bytes_array but zero-copy as slice is shared
    pub fn write_bytes_slice(&mut self, slice: &ArcSlice) -> bool {
        self.write_zint(to_zint!(slice.len())) && self.write_slice(slice.clone())
    }

    // Similar than write_bytes_array but zero-copy as RBuf contains slices that are shared
    pub fn write_rbuf(&mut self, rbuf: &RBuf) -> bool {
        if self.write_zint(to_zint!(rbuf.len())) {
            self.write_rbuf_slices(&rbuf)
        } else {
            false
        }
    }

    // Writes all the slices of a given RBuf
    pub fn write_rbuf_slices(&mut self, rbuf: &RBuf) -> bool {
        for slice in rbuf.get_slices() {
            if !self.write_slice(slice.clone()) {
                return false;
            }
        }
        true
    }
}
