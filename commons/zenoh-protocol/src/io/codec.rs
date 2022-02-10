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
use std::{convert::TryFrom, io::Write};
use zenoh_buffers::{buffer::ConstructibleBuffer, reader::Reader, SplitBuffer, ZBufReader};
use zenoh_core::{bail, zcheck, zerror, Result as ZResult};
use zenoh_protocol_core::{Locator, PeerId, Property, Timestamp, ZInt};

#[cfg(feature = "shared-memory")]
mod zslice {
    pub(crate) mod kind {
        pub(crate) const RAW: u8 = 0;
        pub(crate) const SHM_INFO: u8 = 1;
    }
}

pub trait Decoder<T: Sized> {
    type Err: Sized;
    fn read<R: std::io::Read>(&self, reader: &mut R) -> Result<T, Self::Err>;
}
pub struct ZenohCodec;
impl Decoder<u64> for ZenohCodec {
    type Err = zenoh_core::Error;
    fn read<R: std::io::Read>(&self, reader: &mut R) -> ZResult<u64> {
        let mut v = 0;
        let mut b = 0;
        reader.read_exact(std::slice::from_mut(&mut b))?;
        let mut i = 0;
        let mut k = 10;
        while b > 0x7f && k > 0 {
            v |= ((b & 0x7f) as u64) << i;
            i += 7;
            reader.read_exact(std::slice::from_mut(&mut b))?;
            k -= 1;
        }
        if k > 0 {
            v |= ((b & 0x7f) as u64) << i;
            Ok(v)
        } else {
            bail!("Invalid u64 (larget than u64 max value: {})", u64::MAX);
        }
    }
}
impl Decoder<usize> for ZenohCodec {
    type Err = zenoh_core::Error;
    fn read<R: std::io::Read>(&self, reader: &mut R) -> ZResult<usize> {
        let mut v = 0;
        let mut b = 0;
        reader.read_exact(std::slice::from_mut(&mut b))?;
        let mut i = 0;
        let mut k = 10;
        while b > 0x7f && k > 0 {
            v |= ((b & 0x7f) as usize) << i;
            i += 7;
            reader.read_exact(std::slice::from_mut(&mut b))?;
            k -= 1;
        }
        if k > 0 {
            v |= ((b & 0x7f) as usize) << i;
            Ok(v)
        } else {
            bail!("Invalid u64 (larget than u64 max value: {})", u64::MAX);
        }
    }
}
impl Decoder<Vec<u8>> for ZenohCodec {
    type Err = zenoh_core::Error;
    fn read<R: std::io::Read>(&self, reader: &mut R) -> ZResult<Vec<u8>> {
        let len: usize = self.read(reader)?;
        let mut result = Vec::with_capacity(len);
        // Safety: `u8` is a Copy type (no Drop), and a read_exact is about to occur, ensuring exact length
        #[allow(clippy::uninit_vec)]
        unsafe {
            result.set_len(len)
        };
        reader.read_exact(&mut result)?;
        Ok(result)
    }
}
impl Decoder<String> for ZenohCodec {
    type Err = zenoh_core::Error;
    fn read<R: std::io::Read>(&self, reader: &mut R) -> ZResult<String> {
        let vec = self.read(reader)?;
        String::from_utf8(vec).map_err(|e| zerror!("{}", e).into())
    }
}
impl Decoder<PeerId> for ZenohCodec {
    type Err = zenoh_core::Error;
    fn read<R: std::io::Read>(&self, reader: &mut R) -> Result<PeerId, Self::Err> {
        let size: usize = self.read(reader)?;
        if size > PeerId::MAX_SIZE {
            bail!(
                "Reading a PeerId size that exceed {} bytes: {}",
                PeerId::MAX_SIZE,
                size
            )
        }
        let mut id = [0; PeerId::MAX_SIZE];
        reader.read_exact(&mut id)?;
        Ok(PeerId::new(size, id))
    }
}
impl Decoder<Timestamp> for ZenohCodec {
    type Err = zenoh_core::Error;
    fn read<R: std::io::Read>(&self, reader: &mut R) -> Result<Timestamp, Self::Err> {
        let time = self.read(reader)?;
        let size = self.read(reader)?;
        if size > (uhlc::ID::MAX_SIZE) {
            bail!(
                "Reading a Timestamp's ID size that exceed {} bytes: {}",
                uhlc::ID::MAX_SIZE,
                size
            );
        }
        let mut id = [0_u8; PeerId::MAX_SIZE];
        reader.read_exact(&mut id[..size])?;
        Ok(Timestamp::new(uhlc::NTP64(time), uhlc::ID::new(size, id)))
    }
}
impl Decoder<Property> for ZenohCodec {
    type Err = zenoh_core::Error;
    fn read<R: std::io::Read>(&self, reader: &mut R) -> Result<Property, Self::Err> {
        let key = self.read(reader)?;
        let value = self.read(reader)?;
        Ok(Property { key, value })
    }
}
impl Decoder<Locator> for ZenohCodec {
    type Err = zenoh_core::Error;
    fn read<R: std::io::Read>(&self, reader: &mut R) -> Result<Locator, Self::Err> {
        let candidate: String = self.read(reader)?;
        Locator::try_from(candidate).map_err(|e| {
            zerror!(
                "Invalid locator {:?} (must respect regexp /[^?]+?([^=]+=[^;];)*([^=]+=[^;])?/)",
                e
            )
            .into()
        })
    }
}
#[cfg(feature = "shared-memory")]
struct SlicedZBuf(pub ZBuf);
#[cfg(feature = "shared-memory")]
impl Decoder<SlicedZBuf> for ZenohCodec {
    type Err = zenoh_core::Error;
    fn read<R: std::io::Read>(&self, reader: &mut R) -> Result<SlicedZBuf, Self::Err> {
        use zenoh_buffers::traits::buffer::InsertBuffer;
        let n_slices: usize = self.read(reader)?;
        let mut result = ZBuf::with_capacities(n_slices, 0);
        let mut buffer: Vec<u8> = Vec::new();
        let mut slice_kind = zslice::kind::RAW;
        for _ in 0..n_slices {
            let previous_kind = slice_kind;
            reader.read_exact(std::slice::from_mut(&mut slice_kind))?;
            let slice_len = self.read(reader)?;
            match (previous_kind, slice_kind) {
                (zslice::kind::RAW, zslice::kind::RAW) => {}
                _ => {
                    if !buffer.is_empty() {
                        let mut slice = Vec::with_capacity(slice_len);
                        std::mem::swap(&mut buffer, &mut slice);
                        match previous_kind {
                            zslice::kind::RAW => result
                                .append(zenoh_buffers::ZSliceBuffer::NetOwnedBuffer(slice.into())),
                            zslice::kind::SHM_INFO => {
                                result.append(zenoh_buffers::ZSliceBuffer::ShmInfo(slice.into()))
                            }
                            _ => bail!("Invalid zslice kind: {}", previous_kind),
                        }
                    }
                }
            }
            unsafe { buffer.set_len(slice_len) };
            reader.read_exact(&mut buffer)?;
        }
        if !buffer.is_empty() {
            match slice_kind {
                zslice::kind::RAW => {
                    result.append(zenoh_buffers::ZSliceBuffer::NetOwnedBuffer(buffer.into()))
                }
                zslice::kind::SHM_INFO => {
                    result.append(zenoh_buffers::ZSliceBuffer::ShmInfo(buffer.into()))
                }
                _ => bail!("Invalid zslice kind: {}", slice_kind),
            }
        }
        Ok(SlicedZBuf(result))
    }
}

// #[deprecated = "Use the `zenoh_protocol::io::codec::Decoder` trait methods on `zenoh_protocol::io::codec::ZenohCodec` instead"]
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

macro_rules! read_zint {
    ($buf:expr, $res:ty) => {
        let mut v: $res = 0;
        let mut b = $buf.read_byte()?;
        let mut i = 0;
        let mut k = 10;
        while b > 0x7f && k > 0 {
            v |= ((b & 0x7f) as $res) << i;
            i += 7;
            b = $buf.read_byte()?;
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
#[allow(deprecated)]
impl ZBufCodec for ZBufReader<'_> {
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
        if self.read_exact(buf.as_mut_slice()) {
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
        if self.read_exact(&mut id[..size]) {
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
        if !self.read_exact(&mut info) {
            return None;
        }
        Some(ZSliceBuffer::ShmInfo(info.into()).into())
    }

    #[inline(always)]
    fn read_zbuf_flat(&mut self) -> Option<ZBuf> {
        let len = self.read_zint_as_usize()?;
        let mut zbuf = ZBuf::with_capacities(1, 0);
        if self.read_into_zbuf(&mut zbuf, len) {
            Some(zbuf)
        } else {
            None
        }
    }

    #[cfg(feature = "shared-memory")]
    #[inline(always)]
    fn read_zbuf_sliced(&mut self) -> Option<ZBuf> {
        use zenoh_buffers::traits::buffer::InsertBuffer;
        let num = self.read_zint_as_usize()?;
        let mut zbuf = ZBuf::with_capacities(num, 0);
        for _ in 0..num {
            let kind = self.read_byte()?;
            match kind {
                zslice::kind::RAW => {
                    let len = self.read_zint_as_usize()?;
                    if !self.read_into_zbuf(&mut zbuf, len) {
                        return None;
                    }
                }
                zslice::kind::SHM_INFO => {
                    let slice = self.read_shminfo()?;
                    zbuf.append(slice);
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
        if self.read_exact(&mut id[..size]) {
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

pub trait Encoder<T> {
    type Err;
    fn write<W: Write>(&self, writer: &mut W, value: T) -> Result<usize, Self::Err>;
}
impl Encoder<u64> for ZenohCodec {
    type Err = zenoh_core::Error;
    fn write<W: Write>(&self, writer: &mut W, mut c: u64) -> Result<usize, Self::Err> {
        let mut buffer = [0; 10];
        let mut len = 0;
        let mut b = c as u8;
        while c > 0x7f {
            buffer[len] = b | 0x80;
            len += 1;
            c >>= 7;
            b = c as u8;
        }
        buffer[len] = b;
        len += 1;
        writer.write_all(&buffer[..len])?;
        Ok(len)
    }
}
impl Encoder<usize> for ZenohCodec {
    type Err = zenoh_core::Error;
    fn write<W: Write>(&self, writer: &mut W, mut c: usize) -> Result<usize, Self::Err> {
        let mut buffer = [0; 10];
        let mut len = 0;
        let mut b = c as u8;
        while c > 0x7f {
            buffer[len] = b | 0x80;
            len += 1;
            c >>= 7;
            b = c as u8;
        }
        buffer[len] = b;
        len += 1;
        writer.write_all(&buffer[..len])?;
        Ok(len)
    }
}
impl Encoder<&[u8]> for ZenohCodec {
    type Err = zenoh_core::Error;
    fn write<W: Write>(&self, writer: &mut W, value: &[u8]) -> Result<usize, Self::Err> {
        let len = value.len();
        let write_len = self.write(writer, len)?;
        writer.write_all(value)?;
        Ok(write_len + len)
    }
}
impl Encoder<&str> for ZenohCodec {
    type Err = zenoh_core::Error;
    fn write<W: Write>(&self, writer: &mut W, value: &str) -> Result<usize, Self::Err> {
        self.write(writer, value.as_bytes())
    }
}
impl Encoder<&Property> for ZenohCodec {
    type Err = zenoh_core::Error;
    fn write<W: Write>(&self, writer: &mut W, value: &Property) -> Result<usize, Self::Err> {
        Ok(self.write(writer, value.key)? + self.write(writer, value.value.as_slice())?)
    }
}
impl Encoder<&Timestamp> for ZenohCodec {
    type Err = zenoh_core::Error;
    fn write<W: Write>(&self, writer: &mut W, value: &Timestamp) -> Result<usize, Self::Err> {
        Ok(self.write(writer, value.get_time().as_u64())?
            + self.write(writer, value.get_id().as_slice())?)
    }
}
pub struct Slice<'a, T>(pub &'a [T]);
impl<'a, T> Encoder<Slice<'a, T>> for ZenohCodec
where
    ZenohCodec: Encoder<&'a T, Err = zenoh_core::Error>,
{
    type Err = zenoh_core::Error;
    fn write<W: Write>(&self, writer: &mut W, value: Slice<'a, T>) -> Result<usize, Self::Err> {
        let write_len = <Self as Encoder<usize>>::write(self, writer, value.0.len())?;
        value.0.iter().try_fold(write_len, |acc, t| {
            Ok::<_, Self::Err>(acc + self.write(writer, t)?)
        })
    }
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
