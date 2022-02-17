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
use std::convert::TryFrom;
use zenoh_buffers::{
    buffer::{ConstructibleBuffer, CopyBuffer, InsertBuffer},
    reader::Reader,
    SplitBuffer, ZBufReader,
};
use zenoh_core::{bail, zcheck, zerror, Result as ZResult};
use zenoh_protocol_core::{Locator, PeerId, Property, Timestamp, ZInt};

#[cfg(feature = "shared-memory")]
mod zslice {
    pub(crate) mod kind {
        pub(crate) const RAW: u8 = 0;
        pub(crate) const SHM_INFO: u8 = 1;
    }
}

pub trait Decoder<T: Sized, R> {
    type Err: Sized;
    fn read(&self, reader: &mut R) -> Result<T, Self::Err>;
}

pub struct ZenohCodec;
#[derive(Debug, Clone, Copy)]
pub struct InsufficientDataErr;
impl std::fmt::Display for InsufficientDataErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}
impl std::error::Error for InsufficientDataErr {}
impl<R: Reader> Decoder<u64, R> for ZenohCodec {
    type Err = InsufficientDataErr;
    fn read(&self, reader: &mut R) -> Result<u64, InsufficientDataErr> {
        let mut v = 0;
        let mut b = match reader.read_byte() {
            Some(b) => b,
            None => return Err(InsufficientDataErr),
        };
        let mut i = 0;
        let mut k = 10;
        while b > 0x7f && k > 0 {
            v |= ((b & 0x7f) as u64) << i;
            i += 7;
            b = match reader.read_byte() {
                Some(b) => b,
                None => return Err(InsufficientDataErr),
            };
            k -= 1;
        }
        if k > 0 {
            v |= ((b & 0x7f) as u64) << i;
            Ok(v)
        } else {
            Err(InsufficientDataErr)
        }
    }
}
impl<R: Reader> Decoder<usize, R> for ZenohCodec {
    type Err = InsufficientDataErr;
    fn read(&self, reader: &mut R) -> Result<usize, InsufficientDataErr> {
        let mut v = 0;
        let mut b = match reader.read_byte() {
            Some(b) => b,
            None => return Err(InsufficientDataErr),
        };
        let mut i = 0;
        let mut k = 10;
        while b > 0x7f && k > 0 {
            v |= ((b & 0x7f) as usize) << i;
            i += 7;
            b = match reader.read_byte() {
                Some(b) => b,
                None => return Err(InsufficientDataErr),
            };
            k -= 1;
        }
        if k > 0 {
            v |= ((b & 0x7f) as usize) << i;
            Ok(v)
        } else {
            Err(InsufficientDataErr)
        }
    }
}
impl<R: Reader> Decoder<Vec<u8>, R> for ZenohCodec {
    type Err = InsufficientDataErr;
    fn read(&self, reader: &mut R) -> Result<Vec<u8>, InsufficientDataErr> {
        let len: usize = self.read(reader)?;
        let mut result = Vec::with_capacity(len);
        // Safety: `u8` is a Copy type (no Drop), and a read_exact is about to occur, ensuring exact length
        #[allow(clippy::uninit_vec)]
        unsafe {
            result.set_len(len)
        };
        if !reader.read_exact(&mut result) {
            return Err(InsufficientDataErr);
        }
        Ok(result)
    }
}
impl<R: Reader> Decoder<String, R> for ZenohCodec {
    type Err = zenoh_core::Error;
    fn read(&self, reader: &mut R) -> ZResult<String> {
        let vec = self.read(reader)?;
        String::from_utf8(vec).map_err(|e| zerror!("{}", e).into())
    }
}
impl<R: Reader> Decoder<PeerId, R> for ZenohCodec {
    type Err = zenoh_core::Error;
    fn read(&self, reader: &mut R) -> Result<PeerId, Self::Err> {
        let size: usize = self.read(reader)?;
        if size > PeerId::MAX_SIZE {
            bail!(
                "Reading a PeerId size that exceed {} bytes: {}",
                PeerId::MAX_SIZE,
                size
            )
        }
        let mut id = [0; PeerId::MAX_SIZE];
        if !reader.read_exact(&mut id[..size]) {
            return Err(InsufficientDataErr.into());
        }
        Ok(PeerId::new(size, id))
    }
}
impl<R: Reader> Decoder<Timestamp, R> for ZenohCodec {
    type Err = zenoh_core::Error;
    fn read(&self, reader: &mut R) -> Result<Timestamp, Self::Err> {
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
        if !reader.read_exact(&mut id[..size]) {
            return Err(InsufficientDataErr.into());
        }
        Ok(Timestamp::new(uhlc::NTP64(time), uhlc::ID::new(size, id)))
    }
}
impl<R: Reader> Decoder<Property, R> for ZenohCodec {
    type Err = zenoh_core::Error;
    fn read(&self, reader: &mut R) -> Result<Property, Self::Err> {
        let key = self.read(reader)?;
        let value = self.read(reader)?;
        Ok(Property { key, value })
    }
}
impl<R: Reader> Decoder<Locator, R> for ZenohCodec {
    type Err = zenoh_core::Error;
    fn read(&self, reader: &mut R) -> Result<Locator, Self::Err> {
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
impl<R: Reader> Decoder<Vec<Locator>, R> for ZenohCodec {
    type Err = zenoh_core::Error;
    fn read(&self, reader: &mut R) -> Result<Vec<Locator>, Self::Err> {
        let len = self.read(reader)?;
        let mut vec: Vec<Locator> = Vec::with_capacity(len);
        for _ in 0..len {
            vec.push(self.read(reader)?);
        }
        Ok(vec)
    }
}
#[cfg(feature = "shared-memory")]
struct SlicedZBuf(pub ZBuf);
#[cfg(feature = "shared-memory")]
impl<R: Reader> Decoder<SlicedZBuf, R> for ZenohCodec {
    type Err = zenoh_core::Error;
    fn read(&self, reader: &mut R) -> Result<SlicedZBuf, Self::Err> {
        let n_slices: usize = self.read(reader)?;
        let mut result = ZBuf::with_capacities(n_slices, 0);
        let mut buffer: Vec<u8> = Vec::new();
        let mut slice_kind = zslice::kind::RAW;
        for _ in 0..n_slices {
            let previous_kind = slice_kind;
            if !reader.read_exact(std::slice::from_mut(&mut slice_kind)) {
                return Err(InsufficientDataErr.into());
            }
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
                        };
                    }
                }
            }
            unsafe { buffer.set_len(slice_len) };
            if !reader.read_exact(&mut buffer) {
                return Err(InsufficientDataErr.into());
            }
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
            };
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
        ZenohCodec.read(self).ok()
    }
    #[inline(always)]
    fn read_zint_as_u64(&mut self) -> Option<u64> {
        ZenohCodec.read(self).ok()
    }
    #[inline(always)]
    fn read_zint_as_usize(&mut self) -> Option<usize> {
        ZenohCodec.read(self).ok()
    }
    // Same as read_bytes but with array length before the bytes.
    #[inline(always)]
    fn read_bytes_array(&mut self) -> Option<Vec<u8>> {
        ZenohCodec.read(self).ok()
    }
    #[inline(always)]
    fn read_string(&mut self) -> Option<String> {
        ZenohCodec.read(self).ok()
    }
    #[inline(always)]
    fn read_peeexpr_id(&mut self) -> Option<PeerId> {
        ZenohCodec.read(self).ok()
    }
    #[inline(always)]
    fn read_locator(&mut self) -> Option<Locator> {
        ZenohCodec.read(self).ok()
    }
    #[inline(always)]
    fn read_locators(&mut self) -> Option<Vec<Locator>> {
        ZenohCodec.read(self).ok()
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

pub trait Encoder<W, T> {
    type Err;
    fn write(&self, writer: &mut W, value: T) -> Result<usize, Self::Err>;
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct WriteRefusedErr<W>(std::marker::PhantomData<W>);
impl<W> Default for WriteRefusedErr<W> {
    fn default() -> Self {
        WriteRefusedErr(Default::default())
    }
}
impl<W> std::fmt::Debug for WriteRefusedErr<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} refused a write", std::any::type_name::<W>())
    }
}
impl<W> std::fmt::Display for WriteRefusedErr<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}
impl<W> std::error::Error for WriteRefusedErr<W> {}
impl<W: CopyBuffer> Encoder<W, u64> for ZenohCodec {
    type Err = WriteRefusedErr<W>;
    fn write(&self, writer: &mut W, mut c: u64) -> Result<usize, Self::Err> {
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
        if writer.write(&buffer[..len]).is_none() {
            Err(Default::default())
        } else {
            Ok(len)
        }
    }
}
impl<W: CopyBuffer> Encoder<W, usize> for ZenohCodec {
    type Err = WriteRefusedErr<W>;
    fn write(&self, writer: &mut W, mut c: usize) -> Result<usize, Self::Err> {
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
        if writer.write(&buffer[..len]).is_none() {
            Err(Default::default())
        } else {
            Ok(len)
        }
    }
}
impl<W: CopyBuffer> Encoder<W, &[u8]> for ZenohCodec {
    type Err = WriteRefusedErr<W>;
    fn write(&self, writer: &mut W, value: &[u8]) -> Result<usize, Self::Err> {
        let len = value.len();
        if len == 0 {
            self.write(writer, len)
        } else {
            let written = self.write(writer, len)?;
            match writer.write(value) {
                Some(w) if w.get() == len => Ok(written + len),
                _ => Err(Default::default()),
            }
        }
    }
}
impl<W: CopyBuffer> Encoder<W, &str> for ZenohCodec {
    type Err = WriteRefusedErr<W>;
    fn write(&self, writer: &mut W, value: &str) -> Result<usize, Self::Err> {
        self.write(writer, value.as_bytes())
    }
}
impl<W: CopyBuffer> Encoder<W, &PeerId> for ZenohCodec {
    type Err = WriteRefusedErr<W>;
    fn write(&self, writer: &mut W, value: &PeerId) -> Result<usize, Self::Err> {
        self.write(writer, value.as_slice())
    }
}
impl<W: CopyBuffer> Encoder<W, &Property> for ZenohCodec {
    type Err = WriteRefusedErr<W>;
    fn write(&self, writer: &mut W, value: &Property) -> Result<usize, Self::Err> {
        Ok(self.write(writer, value.key)? + self.write(writer, value.value.as_slice())?)
    }
}
impl<W: CopyBuffer> Encoder<W, &Timestamp> for ZenohCodec {
    type Err = WriteRefusedErr<W>;
    fn write(&self, writer: &mut W, value: &Timestamp) -> Result<usize, Self::Err> {
        Ok(self.write(writer, value.get_time().as_u64())?
            + self.write(writer, value.get_id().as_slice())?)
    }
}
pub struct Slice<'a, T>(pub &'a [T]);
impl<'a, T, W> Encoder<W, Slice<'a, T>> for ZenohCodec
where
    ZenohCodec:
        Encoder<W, &'a T, Err = WriteRefusedErr<W>> + Encoder<W, usize, Err = WriteRefusedErr<W>>,
{
    type Err = WriteRefusedErr<W>;
    fn write(&self, writer: &mut W, value: Slice<'a, T>) -> Result<usize, Self::Err> {
        let write_len = <Self as Encoder<W, usize>>::write(self, writer, value.0.len())?;
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
        ZenohCodec.write(self, v).is_ok()
    }

    #[inline(always)]
    fn write_u64_as_zint(&mut self, v: u64) -> bool {
        ZenohCodec.write(self, v).is_ok()
    }

    #[inline(always)]
    fn write_usize_as_zint(&mut self, v: usize) -> bool {
        ZenohCodec.write(self, v).is_ok()
    }

    // Same as write_bytes but with array length before the bytes.
    #[inline(always)]
    fn write_bytes_array(&mut self, s: &[u8]) -> bool {
        ZenohCodec.write(self, s).is_ok()
    }

    #[inline(always)]
    fn write_string(&mut self, s: &str) -> bool {
        ZenohCodec.write(self, s).is_ok()
    }

    #[inline(always)]
    fn write_peeexpr_id(&mut self, pid: &PeerId) -> bool {
        ZenohCodec.write(self, pid).is_ok()
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
        self.write_usize_as_zint(slice.len()) && self.append(slice).is_some()
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
                ZSliceBuffer::ShmInfo(_) => {
                    zcheck!(self.write_byte(zslice::kind::SHM_INFO).is_some())
                }
                _ => zcheck!(self.write_byte(zslice::kind::RAW).is_some()),
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
            zcheck!(self.append(slice.clone()).is_some());
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
