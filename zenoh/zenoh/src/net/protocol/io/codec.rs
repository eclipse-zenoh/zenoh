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
#[cfg(feature = "shared-memory")]
use super::ZSliceBuffer;
use super::{WBuf, ZBuf, ZSlice};
use crate::net::link::Locator;
use zenoh_protocol_core::{PeerId, Property, Timestamp, ZInt, ZINT_MAX_BYTES};

#[cfg(feature = "shared-memory")]
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

pub trait ZBufCodec {
    fn read_zint(&mut self) -> Option<ZInt>;

    fn read_zint_as_u64(&mut self) -> Option<u64>;

    fn read_zint_as_usize(&mut self) -> Option<usize>;

    // Same as read_bytes but with array length before the bytes.
    fn read_bytes_array(&mut self) -> Option<Vec<u8>>;
    fn read_string(&mut self) -> Option<String>;
    fn read_peeexpr_id(&mut self) -> Option<PeerId>;
    fn read_locator(&mut self) -> Option<Locator>;
    fn read_locators(&mut self) -> Option<Vec<Locator>>;
    fn read_zslice_array(&mut self) -> Option<ZSlice>;
    #[cfg(feature = "shared-memory")]
    fn read_shminfo(&mut self) -> Option<ZSlice>;
    fn read_zbuf_flat(&mut self) -> Option<ZBuf>;
    #[cfg(feature = "shared-memory")]
    fn read_zbuf_sliced(&mut self) -> Option<ZBuf>;
    // Same as read_bytes_array but 0 copy on ZBuf.
    #[cfg(feature = "shared-memory")]
    fn read_zbuf(&mut self, sliced: bool) -> Option<ZBuf>;
    #[cfg(not(feature = "shared-memory"))]
    fn read_zbuf(&mut self) -> Option<ZBuf>;
    fn read_properties(&mut self) -> Option<Vec<Property>>;
    fn read_property(&mut self) -> Option<Property>;
    fn read_timestamp(&mut self) -> Option<Timestamp>;
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
impl ZBufCodec for ZBuf {
    #[inline(always)]
    fn read_zint(&mut self) -> Option<ZInt> {
        read_zint!(self, ZInt);
    }

    #[inline(always)]
    fn read_zint_as_u64(&mut self) -> Option<u64> {
        read_zint!(self, u64);
    }

    #[inline(always)]
    fn read_zint_as_usize(&mut self) -> Option<usize> {
        read_zint!(self, usize);
    }

    // Same as read_bytes but with array length before the bytes.
    #[inline(always)]
    fn read_bytes_array(&mut self) -> Option<Vec<u8>> {
        let len = self.read_zint_as_usize()?;
        let mut buf = vec![0; len];
        if self.read_bytes(buf.as_mut_slice()) {
            Some(buf)
        } else {
            None
        }
    }

    #[inline(always)]
    fn read_string(&mut self) -> Option<String> {
        let bytes = self.read_bytes_array()?;
        Some(String::from(String::from_utf8_lossy(&bytes)))
    }

    #[inline(always)]
    fn read_peeexpr_id(&mut self) -> Option<PeerId> {
        let size = self.read_zint_as_usize()?;
        if size > PeerId::MAX_SIZE {
            log::trace!("Reading a PeerId size that exceed 16 bytes: {}", size);
            return None;
        }
        let mut id = [0_u8; PeerId::MAX_SIZE];
        if self.read_bytes(&mut id[..size]) {
            Some(PeerId::new(size, id))
        } else {
            None
        }
    }

    #[inline(always)]
    fn read_locator(&mut self) -> Option<Locator> {
        self.read_string()?.parse().ok()
    }

    #[inline(always)]
    fn read_locators(&mut self) -> Option<Vec<Locator>> {
        let len = self.read_zint()?;
        let mut vec: Vec<Locator> = Vec::with_capacity(len as usize);
        for _ in 0..len {
            vec.push(self.read_locator()?);
        }
        Some(vec)
    }

    #[inline(always)]
    fn read_zslice_array(&mut self) -> Option<ZSlice> {
        let len = self.read_zint_as_usize()?;
        self.read_zslice(len)
    }

    #[cfg(feature = "shared-memory")]
    #[inline(always)]
    fn read_shminfo(&mut self) -> Option<ZSlice> {
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

    #[cfg(feature = "shared-memory")]
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
    #[cfg(feature = "shared-memory")]
    #[inline(always)]
    fn read_zbuf(&mut self, sliced: bool) -> Option<ZBuf> {
        if !sliced {
            self.read_zbuf_flat()
        } else {
            self.read_zbuf_sliced()
        }
    }

    #[cfg(not(feature = "shared-memory"))]
    #[inline(always)]
    fn read_zbuf(&mut self) -> Option<ZBuf> {
        self.read_zbuf_flat()
    }

    fn read_properties(&mut self) -> Option<Vec<Property>> {
        let len = self.read_zint()?;
        let mut vec: Vec<Property> = Vec::with_capacity(len as usize);
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

    fn read_timestamp(&mut self) -> Option<Timestamp> {
        let time = self.read_zint_as_u64()?;
        let size = self.read_zint_as_usize()?;
        if size > (uhlc::ID::MAX_SIZE) {
            log::trace!(
                "Reading a Timestamp's ID size that exceed {} bytes: {}",
                uhlc::ID::MAX_SIZE,
                size
            );
            return None;
        }
        let mut id = [0_u8; PeerId::MAX_SIZE];
        if self.read_bytes(&mut id[..size]) {
            Some(Timestamp::new(uhlc::NTP64(time), uhlc::ID::new(size, id)))
        } else {
            None
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

pub trait WBufCodec {
    fn write_zint(&mut self, v: ZInt) -> bool;
    fn write_u64_as_zint(&mut self, v: u64) -> bool;
    fn write_usize_as_zint(&mut self, v: usize) -> bool;
    fn write_bytes_array(&mut self, s: &[u8]) -> bool;
    fn write_string(&mut self, s: &str) -> bool;
    fn write_peeexpr_id(&mut self, pid: &PeerId) -> bool;
    fn write_locator(&mut self, locator: &Locator) -> bool;
    fn write_locators(&mut self, locators: &[Locator]) -> bool;
    fn write_zslice_array(&mut self, slice: ZSlice) -> bool;
    fn write_zbuf_flat(&mut self, zbuf: &ZBuf) -> bool;
    #[cfg(feature = "shared-memory")]
    fn write_zbuf_sliced(&mut self, zbuf: &ZBuf) -> bool;
    #[cfg(feature = "shared-memory")]
    fn write_zbuf(&mut self, zbuf: &ZBuf, sliced: bool) -> bool;
    #[cfg(not(feature = "shared-memory"))]
    fn write_zbuf(&mut self, zbuf: &ZBuf) -> bool;
    fn write_zbuf_slices(&mut self, zbuf: &ZBuf) -> bool;
    fn write_properties(&mut self, props: &[Property]) -> bool;
    fn write_property(&mut self, p: &Property) -> bool;
    fn write_timestamp(&mut self, tstamp: &Timestamp) -> bool;
}
impl WBufCodec for WBuf {
    /// This the traditional VByte encoding, in which an arbirary integer
    /// is encoded as a sequence of 7 bits integers
    #[inline(always)]
    fn write_zint(&mut self, v: ZInt) -> bool {
        write_zint!(self, v);
    }

    #[inline(always)]
    fn write_u64_as_zint(&mut self, v: u64) -> bool {
        write_zint!(self, v);
    }

    #[inline(always)]
    fn write_usize_as_zint(&mut self, v: usize) -> bool {
        write_zint!(self, v);
    }

    // Same as write_bytes but with array length before the bytes.
    #[inline(always)]
    fn write_bytes_array(&mut self, s: &[u8]) -> bool {
        self.write_usize_as_zint(s.len()) && self.write_bytes(s)
    }

    #[inline(always)]
    fn write_string(&mut self, s: &str) -> bool {
        self.write_usize_as_zint(s.len()) && self.write_bytes(s.as_bytes())
    }

    #[inline(always)]
    fn write_peeexpr_id(&mut self, pid: &PeerId) -> bool {
        self.write_bytes_array(pid.as_slice())
    }

    #[inline(always)]
    fn write_locator(&mut self, locator: &Locator) -> bool {
        self.write_string(&locator.to_string())
    }

    #[inline(always)]
    fn write_locators(&mut self, locators: &[Locator]) -> bool {
        zcheck!(self.write_usize_as_zint(locators.len()));
        for l in locators {
            zcheck!(self.write_locator(l));
        }
        true
    }

    #[inline(always)]
    fn write_zslice_array(&mut self, slice: ZSlice) -> bool {
        self.write_usize_as_zint(slice.len()) && self.write_zslice(slice)
    }

    #[inline(always)]
    fn write_zbuf_flat(&mut self, zbuf: &ZBuf) -> bool {
        zcheck!(self.write_usize_as_zint(zbuf.len()));
        self.write_zbuf_slices(zbuf)
    }

    #[cfg(feature = "shared-memory")]
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

    #[cfg(feature = "shared-memory")]
    #[inline(always)]
    fn write_zbuf(&mut self, zbuf: &ZBuf, sliced: bool) -> bool {
        if !sliced {
            self.write_zbuf_flat(zbuf)
        } else {
            self.write_zbuf_sliced(zbuf)
        }
    }

    #[cfg(not(feature = "shared-memory"))]
    #[inline(always)]
    fn write_zbuf(&mut self, zbuf: &ZBuf) -> bool {
        self.write_zbuf_flat(zbuf)
    }

    #[inline(always)]
    fn write_zbuf_slices(&mut self, zbuf: &ZBuf) -> bool {
        let mut idx = 0;
        while let Some(slice) = zbuf.get_zslice(idx) {
            zcheck!(self.write_zslice(slice.clone()));
            idx += 1;
        }
        true
    }

    fn write_properties(&mut self, props: &[Property]) -> bool {
        zcheck!(self.write_usize_as_zint(props.len()));
        for p in props {
            zcheck!(self.write_property(p));
        }
        true
    }

    fn write_property(&mut self, p: &Property) -> bool {
        self.write_zint(p.key) && self.write_bytes_array(&p.value)
    }

    fn write_timestamp(&mut self, tstamp: &Timestamp) -> bool {
        self.write_u64_as_zint(tstamp.get_time().as_u64())
            && self.write_bytes_array(tstamp.get_id().as_slice())
    }
}
