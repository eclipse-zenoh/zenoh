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
use super::core::{PeerId, Property, ZInt, ZINT_MAX_BYTES};
#[cfg(feature = "zero-copy")]
use super::SharedMemoryBufInfo;
#[cfg(feature = "zero-copy")]
use super::ZSliceBuffer;
use super::{WBuf, ZBuf, ZSlice};
use crate::net::link::Locator;
#[cfg(feature = "zero-copy")]
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
#[cfg(feature = "zero-copy")]
use zenoh_util::zerror;

#[cfg(feature = "zero-copy")]
mod zslice {
    pub(crate) mod kind {
        pub(crate) const RAW: u8 = 0;
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
        match bincode::deserialize::<SharedMemoryBufInfo>(bs) {
            Ok(info) => Ok(info),
            Err(e) => zerror!(ZErrorKind::ValueDecodingFailed {
                descr: format!("Unable to deserialize SharedMemoryBufInfo: {}", e)
            }),
        }
    }
}

// ZBuf encoding
//
// When non-sliced:
//
//  7 6 5 4 3 2 1 0
// +-+-+-+-+-+-+-+-+
// ~  zbuf length  ~
// +---------------+
// ~  zbuf bytes   ~
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

impl ZBuf {
    #[inline(always)]
    pub fn read_zint(&mut self) -> Option<ZInt> {
        read_zint!(self, ZInt);
    }

    #[inline(always)]
    pub fn read_zint_as_u64(&mut self) -> Option<u64> {
        read_zint!(self, u64);
    }

    #[inline(always)]
    pub fn read_zint_as_usize(&mut self) -> Option<usize> {
        read_zint!(self, usize);
    }

    // Same as read_bytes but with array length before the bytes.
    #[inline(always)]
    pub fn read_bytes_array(&mut self) -> Option<Vec<u8>> {
        let len = self.read_zint_as_usize()?;
        let mut buf = vec![0; len];
        if self.read_bytes(buf.as_mut_slice()) {
            Some(buf)
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn read_string(&mut self) -> Option<String> {
        let bytes = self.read_bytes_array()?;
        Some(String::from(String::from_utf8_lossy(&bytes)))
    }

    #[inline(always)]
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

    #[inline(always)]
    pub fn read_locator(&mut self) -> Option<Locator> {
        self.read_string()?.parse().ok()
    }

    #[inline(always)]
    pub fn read_locators(&mut self) -> Option<Vec<Locator>> {
        let len = self.read_zint()?;
        let mut vec: Vec<Locator> = Vec::new();
        for _ in 0..len {
            vec.push(self.read_locator()?);
        }
        Some(vec)
    }

    #[inline(always)]
    pub fn read_zslice_array(&mut self) -> Option<ZSlice> {
        let len = self.read_zint_as_usize()?;
        self.read_zslice(len)
    }

    #[cfg(feature = "zero-copy")]
    #[inline(always)]
    pub fn read_shminfo(&mut self) -> Option<ZSlice> {
        let len = self.read_zint_as_usize()?;
        let mut info = vec![0; len];
        if !self.read_bytes(&mut info) {
            return None;
        }
        Some(ZSliceBuffer::ShmInfo(info.into()).into())
    }

    #[inline(always)]
    fn read_zbuf_flat(&mut self) -> Option<ZBuf> {
        let len = self.read_zint_as_usize()?;
        let mut zbuf = ZBuf::new();
        if self.read_into_zbuf(&mut zbuf, len) {
            Some(zbuf)
        } else {
            None
        }
    }

    #[cfg(feature = "zero-copy")]
    #[inline(always)]
    fn read_zbuf_sliced(&mut self) -> Option<ZBuf> {
        let num = self.read_zint_as_usize()?;
        let mut zbuf = ZBuf::new();
        for _ in 0..num {
            let kind = self.read()?;
            match kind {
                zslice::kind::RAW => {
                    let len = self.read_zint_as_usize()?;
                    if !self.read_into_zbuf(&mut zbuf, len) {
                        return None;
                    }
                }
                zslice::kind::SHM_INFO => {
                    let slice = self.read_shminfo()?;
                    zbuf.add_zslice(slice);
                }
                _ => return None,
            }
        }
        Some(zbuf)
    }

    // Same as read_bytes_array but 0 copy on ZBuf.
    #[cfg(feature = "zero-copy")]
    #[inline(always)]
    pub fn read_zbuf(&mut self, sliced: bool) -> Option<ZBuf> {
        if !sliced {
            self.read_zbuf_flat()
        } else {
            self.read_zbuf_sliced()
        }
    }

    #[cfg(not(feature = "zero-copy"))]
    #[inline(always)]
    pub fn read_zbuf(&mut self) -> Option<ZBuf> {
        self.read_zbuf_flat()
    }

    pub fn read_properties(&mut self) -> Option<Vec<Property>> {
        let len = self.read_zint()?;
        let mut vec: Vec<Property> = Vec::new();
        for _ in 0..len {
            vec.push(self.read_property()?);
        }
        Some(vec)
    }

    fn read_property(&mut self) -> Option<Property> {
        let key = self.read_zint()?;
        let value = self.read_bytes_array()?;
        Some(Property { key, value })
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
    #[inline(always)]
    pub fn write_zint(&mut self, v: ZInt) -> bool {
        write_zint!(self, v);
    }

    #[inline(always)]
    pub fn write_u64_as_zint(&mut self, v: u64) -> bool {
        write_zint!(self, v);
    }

    #[inline(always)]
    pub fn write_usize_as_zint(&mut self, v: usize) -> bool {
        write_zint!(self, v);
    }

    // Same as write_bytes but with array length before the bytes.
    #[inline(always)]
    pub fn write_bytes_array(&mut self, s: &[u8]) -> bool {
        self.write_usize_as_zint(s.len()) && self.write_bytes(s)
    }

    #[inline(always)]
    pub fn write_string(&mut self, s: &str) -> bool {
        self.write_usize_as_zint(s.len()) && self.write_bytes(s.as_bytes())
    }

    #[inline(always)]
    pub fn write_peerid(&mut self, pid: &PeerId) -> bool {
        self.write_bytes_array(pid.as_slice())
    }

    #[inline(always)]
    pub fn write_locator(&mut self, locator: &Locator) -> bool {
        self.write_string(&locator.to_string())
    }

    #[inline(always)]
    pub fn write_locators(&mut self, locators: &[Locator]) -> bool {
        zcheck!(self.write_usize_as_zint(locators.len()));
        for l in locators {
            zcheck!(self.write_locator(l));
        }
        true
    }

    #[inline(always)]
    pub fn write_zslice_array(&mut self, slice: ZSlice) -> bool {
        self.write_usize_as_zint(slice.len()) && self.write_zslice(slice)
    }

    #[inline(always)]
    fn write_zbuf_flat(&mut self, zbuf: &ZBuf) -> bool {
        zcheck!(self.write_usize_as_zint(zbuf.len()));
        self.write_zbuf_slices(zbuf)
    }

    #[cfg(feature = "zero-copy")]
    #[inline(always)]
    fn write_zbuf_sliced(&mut self, zbuf: &ZBuf) -> bool {
        zcheck!(self.write_usize_as_zint(zbuf.zslices_num()));
        let mut idx = 0;
        while let Some(slice) = zbuf.get_zslice(idx) {
            match &slice.buf {
                ZSliceBuffer::ShmInfo(_) => zcheck!(self.write(zslice::kind::SHM_INFO)),
                _ => zcheck!(self.write(zslice::kind::RAW)),
            }

            zcheck!(self.write_zslice_array(slice.clone()));
            idx += 1;
        }
        true
    }

    #[cfg(feature = "zero-copy")]
    #[inline(always)]
    pub fn write_zbuf(&mut self, zbuf: &ZBuf, sliced: bool) -> bool {
        if !sliced {
            self.write_zbuf_flat(zbuf)
        } else {
            self.write_zbuf_sliced(zbuf)
        }
    }

    #[cfg(not(feature = "zero-copy"))]
    #[inline(always)]
    pub fn write_zbuf(&mut self, zbuf: &ZBuf) -> bool {
        self.write_zbuf_flat(zbuf)
    }

    #[inline(always)]
    pub fn write_zbuf_slices(&mut self, zbuf: &ZBuf) -> bool {
        let mut idx = 0;
        while let Some(slice) = zbuf.get_zslice(idx) {
            zcheck!(self.write_zslice(slice.clone()));
            idx += 1;
        }
        true
    }

    pub fn write_properties(&mut self, props: &[Property]) {
        self.write_usize_as_zint(props.len());
        for p in props {
            self.write_property(p);
        }
    }

    fn write_property(&mut self, p: &Property) -> bool {
        self.write_zint(p.key) && self.write_bytes_array(&p.value)
    }
}
