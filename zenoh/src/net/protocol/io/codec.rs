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
use super::core::{PeerId, ZInt, ZINT_MAX_BYTES};
use super::link::Locator;
#[cfg(feature = "zero-copy")]
use super::SharedMemoryBufInfo;
#[cfg(feature = "zero-copy")]
use super::ZSliceBuffer;
#[cfg(feature = "zero-copy")]
use super::ZSliceType;
use super::{RBuf, WBuf, ZSlice};
#[cfg(feature = "zero-copy")]
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
#[cfg(feature = "zero-copy")]
use zenoh_util::zerror;

mod zslice {
    pub(crate) mod kind {
        pub(crate) const RAW: u8 = 0;
        #[cfg(feature = "zero-copy")]
        pub(crate) const SHM_INFO: u8 = 1;
    }
}

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
            return Some(v);
        } else {
            log::trace!("Invalid ZInt (larget than ZInt max value: {})", ZInt::MAX);
            return None;
        }
    };
}

#[cfg(feature = "zero-copy")]
impl SharedMemoryBufInfo {
    pub fn serialize(&self) -> ZResult<Vec<u8>> {
        bincode::serialize(self).map_err(|e| {
            zerror2!(ZErrorKind::ValueEncodingFailed {
                descr: format!("Unable to serialize SharedMemoryBufInfo: {}", e)
            })
        })
    }

    pub fn deserialize(bs: &[u8]) -> ZResult<SharedMemoryBufInfo> {
        match bincode::deserialize::<SharedMemoryBufInfo>(&bs) {
            Ok(info) => Ok(info),
            Err(e) => zerror!(ZErrorKind::ValueDecodingFailed {
                descr: format!("Unable to deserialize SharedMemoryBufInfo: {}", e)
            }),
        }
    }
}

// RBuf encoding
//
// When non-sliced:
//
//  7 6 5 4 3 2 1 0
// +-+-+-+-+-+-+-+-+
// ~  rbuf length  ~
// +---------------+
// ~  rbuf bytes   ~
// +---------------+
//
//
// When sliced:
//
//  7 6 5 4 3 2 1 0
// +-+-+-+-+-+-+-+-+
// ~  slices num   ~
// +---------------+
// |  slice type   |
// +---------------+
// ~  slice length ~
// +---------------+
// ~  slice bytes  ~
// +---------------+
//        ...
// +---------------+
// |  slice type   |
// +---------------+
// ~  slice length ~
// +---------------+
// ~  slice bytes  ~
// +---------------+

impl RBuf {
    pub fn read_zint(&mut self) -> Option<ZInt> {
        read_zint!(self, ZInt);
    }

    pub fn read_zint_as_u64(&mut self) -> Option<u64> {
        read_zint!(self, u64);
    }

    pub fn read_zint_as_usize(&mut self) -> Option<usize> {
        read_zint!(self, usize);
    }

    // Same as read_bytes but with array length before the bytes.
    pub fn read_bytes_array(&mut self) -> Option<Vec<u8>> {
        let len = self.read_zint_as_usize()?;
        let mut buf = vec![0; len];
        if self.read_bytes(buf.as_mut_slice()) {
            Some(buf)
        } else {
            None
        }
    }

    pub fn read_string(&mut self) -> Option<String> {
        let bytes = self.read_bytes_array()?;
        Some(String::from(String::from_utf8_lossy(&bytes)))
    }

    pub fn read_peerid(&mut self) -> Option<PeerId> {
        let size = self.read_zint_as_usize()?;
        if size > PeerId::MAX_SIZE {
            log::trace!("Reading a PeerId size that exceed 16 bytes: {}", size);
            return None;
        }
        let mut id = [0u8; PeerId::MAX_SIZE];
        if self.read_bytes(&mut id[..size]) {
            Some(PeerId::new(size, id))
        } else {
            None
        }
    }

    pub fn read_locator(&mut self) -> Option<Locator> {
        self.read_string()?.parse().ok()
    }

    pub fn read_locators(&mut self) -> Option<Vec<Locator>> {
        let len = self.read_zint()?;
        let mut vec: Vec<Locator> = Vec::new();
        for _ in 0..len {
            vec.push(self.read_locator()?);
        }
        Some(vec)
    }

    #[inline]
    pub fn read_zslice_element(&mut self) -> Option<ZSlice> {
        let len = self.read_zint_as_usize()?;
        self.read_zslice(len)
    }

    #[cfg(feature = "zero-copy")]
    pub fn read_shminfo(&mut self) -> Option<ZSlice> {
        let len = self.read_zint_as_usize()?;
        let mut info = vec![0; len];
        if !self.read_bytes(&mut info) {
            return None;
        }
        Some(ZSliceBuffer::ShmInfo(info.into()).into())
    }

    // Same as read_bytes_array but 0 copy on RBuf.
    pub fn read_rbuf(&mut self, is_sliced: bool) -> Option<RBuf> {
        if !is_sliced {
            let len = self.read_zint_as_usize()?;
            let mut rbuf = RBuf::new();
            if self.read_into_rbuf(&mut rbuf, len) {
                Some(rbuf)
            } else {
                None
            }
        } else {
            let num = self.read_zint_as_usize()?;
            let mut rbuf = RBuf::new();
            for _ in 0..num {
                let kind = self.read()?;
                match kind {
                    zslice::kind::RAW => {
                        let len = self.read_zint_as_usize()?;
                        if !self.read_into_rbuf(&mut rbuf, len) {
                            return None;
                        }
                    }
                    #[cfg(feature = "zero-copy")]
                    zslice::kind::SHM_INFO => {
                        let slice = self.read_shminfo()?;
                        rbuf.add_slice(slice);
                    }
                    _ => return None,
                }
            }
            Some(rbuf)
        }
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

    pub fn write_usize_as_zint(&mut self, v: usize) -> bool {
        write_zint!(self, v);
    }

    // Same as write_bytes but with array length before the bytes.
    #[inline]
    pub fn write_bytes_array(&mut self, s: &[u8]) -> bool {
        self.write_usize_as_zint(s.len()) && self.write_bytes(s)
    }

    #[inline]
    pub fn write_string(&mut self, s: &str) -> bool {
        self.write_usize_as_zint(s.len()) && self.write_bytes(s.as_bytes())
    }

    #[inline]
    pub fn write_peerid(&mut self, pid: &PeerId) -> bool {
        self.write_bytes_array(pid.as_slice())
    }

    #[inline]
    pub fn write_locator(&mut self, locator: &Locator) -> bool {
        self.write_string(&locator.to_string())
    }

    pub fn write_locators(&mut self, locators: &[Locator]) -> bool {
        zcheck!(self.write_usize_as_zint(locators.len()));
        for l in locators {
            zcheck!(self.write_locator(&l));
        }
        true
    }

    #[inline]
    pub fn write_zslice_element(&mut self, slice: ZSlice) -> bool {
        self.write_usize_as_zint(slice.len()) && self.write_zslice(slice)
    }

    pub fn write_rbuf(&mut self, rbuf: &RBuf, is_sliced: bool) -> bool {
        if !is_sliced {
            zcheck!(self.write_usize_as_zint(rbuf.len()));
            self.write_rbuf_slices(rbuf)
        } else {
            zcheck!(self.write_usize_as_zint(rbuf.get_slices_num()));
            let mut idx = 0;
            while let Some(slice) = rbuf.get_slice(idx) {
                match slice.get_type() {
                    #[cfg(feature = "zero-copy")]
                    ZSliceType::ShmInfo => zcheck!(self.write(zslice::kind::SHM_INFO)),
                    _ => zcheck!(self.write(zslice::kind::RAW)),
                }

                zcheck!(self.write_zslice_element(slice.clone()));
                idx += 1;
            }
            true
        }
    }

    pub fn write_rbuf_slices(&mut self, rbuf: &RBuf) -> bool {
        let mut idx = 0;
        while let Some(slice) = rbuf.get_slice(idx) {
            zcheck!(self.write_zslice(slice.clone()));
            idx += 1;
        }
        true
    }
}
