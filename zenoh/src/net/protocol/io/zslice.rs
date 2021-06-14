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
#[cfg(feature = "zero-copy")]
use super::{SharedMemoryBuf, SharedMemoryBufInfo, SharedMemoryReader};
use std::fmt;
use std::io::IoSlice;
use std::ops::{
    Deref, Index, Range, RangeFrom, RangeFull, RangeInclusive, RangeTo, RangeToInclusive,
};
use std::sync::Arc;
use zenoh_util::collections::RecyclingObject;
#[cfg(feature = "zero-copy")]
use zenoh_util::core::ZResult;

/*************************************/
/*            HEAP BUFFER            */
/*************************************/
#[derive(Clone)]
pub enum ZSliceBufferNet {
    RecyclingObject(Arc<RecyclingObject<Box<[u8]>>>),
    OwnedBuffer(Arc<Vec<u8>>),
}

impl ZSliceBufferNet {
    fn as_slice(&self) -> &[u8] {
        match self {
            Self::RecyclingObject(buf) => buf,
            Self::OwnedBuffer(buf) => buf,
        }
    }

    #[allow(clippy::missing_safety_doc)]
    #[allow(clippy::mut_from_ref)]
    unsafe fn as_mut_slice(&self) -> &mut [u8] {
        match self {
            Self::RecyclingObject(buf) => {
                &mut (*(Arc::as_ptr(buf) as *mut RecyclingObject<Box<[u8]>>))
            }
            Self::OwnedBuffer(buf) => &mut (*(Arc::as_ptr(buf) as *mut Vec<u8>)),
        }
    }
}

impl Deref for ZSliceBufferNet {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl From<Arc<RecyclingObject<Box<[u8]>>>> for ZSliceBufferNet {
    fn from(buf: Arc<RecyclingObject<Box<[u8]>>>) -> Self {
        Self::RecyclingObject(buf)
    }
}

impl From<Box<RecyclingObject<Box<[u8]>>>> for ZSliceBufferNet {
    fn from(buf: Box<RecyclingObject<Box<[u8]>>>) -> Self {
        Self::RecyclingObject(buf.into())
    }
}

impl From<RecyclingObject<Box<[u8]>>> for ZSliceBufferNet {
    fn from(buf: RecyclingObject<Box<[u8]>>) -> Self {
        Self::from(Arc::new(buf))
    }
}

impl From<Arc<Vec<u8>>> for ZSliceBufferNet {
    fn from(buf: Arc<Vec<u8>>) -> Self {
        Self::OwnedBuffer(buf)
    }
}

impl From<Vec<u8>> for ZSliceBufferNet {
    fn from(buf: Vec<u8>) -> Self {
        Self::from(Arc::new(buf))
    }
}

impl From<&[u8]> for ZSliceBufferNet {
    fn from(buf: &[u8]) -> Self {
        Self::from(buf.to_vec())
    }
}

impl<'a> From<&IoSlice<'a>> for ZSliceBufferNet {
    fn from(buf: &IoSlice) -> Self {
        Self::from(buf.to_vec())
    }
}

/*************************************/
/*            SHM BUFFER             */
/*************************************/
#[cfg(feature = "zero-copy")]
#[derive(Clone)]
pub enum ZSliceBufferShm {
    Buffer(Arc<SharedMemoryBuf>),
    Info(Arc<Vec<u8>>),
}

#[cfg(feature = "zero-copy")]
impl ZSliceBufferShm {
    fn as_slice(&self) -> &[u8] {
        match self {
            Self::Buffer(buf) => buf.as_slice(),
            Self::Info(buf) => buf,
        }
    }

    #[allow(clippy::missing_safety_doc)]
    #[allow(clippy::mut_from_ref)]
    unsafe fn as_mut_slice(&self) -> &mut [u8] {
        match self {
            Self::Buffer(buf) => {
                (&mut (*(Arc::as_ptr(buf) as *mut SharedMemoryBuf))).as_mut_slice()
            }
            Self::Info(buf) => &mut (*(Arc::as_ptr(buf) as *mut Vec<u8>)),
        }
    }
}

#[cfg(feature = "zero-copy")]
impl Deref for ZSliceBufferShm {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

#[cfg(feature = "zero-copy")]
impl From<Arc<SharedMemoryBuf>> for ZSliceBufferShm {
    fn from(buf: Arc<SharedMemoryBuf>) -> Self {
        Self::Buffer(buf)
    }
}

#[cfg(feature = "zero-copy")]
impl From<Box<SharedMemoryBuf>> for ZSliceBufferShm {
    fn from(buf: Box<SharedMemoryBuf>) -> Self {
        Self::Buffer(buf.into())
    }
}

#[cfg(feature = "zero-copy")]
impl From<SharedMemoryBuf> for ZSliceBufferShm {
    fn from(buf: SharedMemoryBuf) -> Self {
        Self::Buffer(Arc::new(buf))
    }
}

/*************************************/
/*           ZSLICE BUFFER           */
/*************************************/
#[derive(Clone)]
pub enum ZSliceBuffer {
    ZSliceBufferNet(ZSliceBufferNet),
    #[cfg(feature = "zero-copy")]
    ZSliceBufferShm(ZSliceBufferShm),
}

impl ZSliceBuffer {
    fn as_slice(&self) -> &[u8] {
        match self {
            Self::ZSliceBufferNet(buf) => buf,
            #[cfg(feature = "zero-copy")]
            Self::ZSliceBufferShm(buf) => buf.as_slice(),
        }
    }

    #[allow(clippy::missing_safety_doc)]
    #[allow(clippy::mut_from_ref)]
    unsafe fn as_mut_slice(&self) -> &mut [u8] {
        match self {
            Self::ZSliceBufferNet(hb) => hb.as_mut_slice(),
            #[cfg(feature = "zero-copy")]
            Self::ZSliceBufferShm(shmb) => shmb.as_mut_slice(),
        }
    }
}

impl Deref for ZSliceBuffer {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl Index<usize> for ZSliceBuffer {
    type Output = u8;

    fn index(&self, index: usize) -> &Self::Output {
        &(&self.deref())[index]
    }
}

impl Index<Range<usize>> for ZSliceBuffer {
    type Output = [u8];

    fn index(&self, range: Range<usize>) -> &Self::Output {
        &(&self.deref())[range]
    }
}

impl Index<RangeFrom<usize>> for ZSliceBuffer {
    type Output = [u8];

    fn index(&self, range: RangeFrom<usize>) -> &Self::Output {
        &(&self.deref())[range]
    }
}

impl Index<RangeFull> for ZSliceBuffer {
    type Output = [u8];

    fn index(&self, range: RangeFull) -> &Self::Output {
        &(&self.deref())[range]
    }
}

impl Index<RangeInclusive<usize>> for ZSliceBuffer {
    type Output = [u8];

    fn index(&self, range: RangeInclusive<usize>) -> &Self::Output {
        &(&self.deref())[range]
    }
}

impl Index<RangeTo<usize>> for ZSliceBuffer {
    type Output = [u8];

    fn index(&self, range: RangeTo<usize>) -> &Self::Output {
        &(&self.deref())[range]
    }
}

impl Index<RangeToInclusive<usize>> for ZSliceBuffer {
    type Output = [u8];

    fn index(&self, range: RangeToInclusive<usize>) -> &Self::Output {
        &(&self.deref())[range]
    }
}

impl From<Arc<RecyclingObject<Box<[u8]>>>> for ZSliceBuffer {
    fn from(buf: Arc<RecyclingObject<Box<[u8]>>>) -> Self {
        Self::ZSliceBufferNet(buf.into())
    }
}

impl From<RecyclingObject<Box<[u8]>>> for ZSliceBuffer {
    fn from(buf: RecyclingObject<Box<[u8]>>) -> Self {
        Self::ZSliceBufferNet(buf.into())
    }
}

impl From<Arc<Vec<u8>>> for ZSliceBuffer {
    fn from(buf: Arc<Vec<u8>>) -> Self {
        Self::ZSliceBufferNet(buf.into())
    }
}

impl From<Vec<u8>> for ZSliceBuffer {
    fn from(buf: Vec<u8>) -> Self {
        Self::ZSliceBufferNet(buf.into())
    }
}

impl From<&[u8]> for ZSliceBuffer {
    fn from(buf: &[u8]) -> Self {
        Self::ZSliceBufferNet(buf.into())
    }
}

impl<'a> From<&IoSlice<'a>> for ZSliceBuffer {
    fn from(buf: &IoSlice) -> Self {
        Self::ZSliceBufferNet(buf.into())
    }
}

impl From<ZSliceBufferNet> for ZSliceBuffer {
    fn from(buf: ZSliceBufferNet) -> Self {
        Self::ZSliceBufferNet(buf)
    }
}

#[cfg(feature = "zero-copy")]
impl From<ZSliceBufferShm> for ZSliceBuffer {
    fn from(buf: ZSliceBufferShm) -> Self {
        Self::ZSliceBufferShm(buf)
    }
}

#[cfg(feature = "zero-copy")]
impl From<Arc<SharedMemoryBuf>> for ZSliceBuffer {
    fn from(buf: Arc<SharedMemoryBuf>) -> Self {
        Self::ZSliceBufferShm(buf.into())
    }
}

#[cfg(feature = "zero-copy")]
impl From<Box<SharedMemoryBuf>> for ZSliceBuffer {
    fn from(buf: Box<SharedMemoryBuf>) -> Self {
        Self::ZSliceBufferShm(buf.into())
    }
}

#[cfg(feature = "zero-copy")]
impl From<SharedMemoryBuf> for ZSliceBuffer {
    fn from(buf: SharedMemoryBuf) -> Self {
        Self::ZSliceBufferShm(buf.into())
    }
}

/*************************************/
/*               ZSLICE              */
/*************************************/
pub enum ZSliceType {
    Net,
    ShmInfo,
    ShmBuf,
}

#[derive(Clone)]
pub struct ZSlice {
    buf: ZSliceBuffer,
    start: usize,
    end: usize,
}

impl ZSlice {
    pub fn new(buf: ZSliceBuffer, start: usize, end: usize) -> ZSlice {
        assert!(end <= (&buf).len());
        ZSlice { buf, start, end }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        &self.buf[self.start..self.end]
    }

    /// # Safety
    ///
    /// This function retrieves a mutable slice from a non-mutable reference.
    /// Mutating the content of the slice without proper syncrhonization is considered
    /// undefined behavior in Rust. To use with extreme caution.
    #[allow(clippy::mut_from_ref)]
    #[inline]
    pub unsafe fn as_mut_slice(&self) -> &mut [u8] {
        &mut self.buf.as_mut_slice()[self.start..self.end]
    }

    #[inline]
    pub fn as_ioslice(&self) -> IoSlice {
        IoSlice::new(self.as_slice())
    }

    #[cfg(feature = "zero-copy")]
    #[inline]
    pub fn get_type(&self) -> ZSliceType {
        match &self.buf {
            ZSliceBuffer::ZSliceBufferNet(_) => ZSliceType::Net,
            ZSliceBuffer::ZSliceBufferShm(ZSliceBufferShm::Buffer(_)) => ZSliceType::ShmBuf,
            ZSliceBuffer::ZSliceBufferShm(ZSliceBufferShm::Info(_)) => ZSliceType::ShmInfo,
        }
    }

    pub(crate) fn new_sub_slice(&self, start: usize, end: usize) -> ZSlice {
        assert!(end <= self.len());
        ZSlice {
            buf: self.buf.clone(),
            start: self.start + start,
            end: self.start + end,
        }
    }

    #[cfg(feature = "zero-copy")]
    pub(crate) fn map_to_shmbuf(&mut self, shmr: &mut SharedMemoryReader) -> ZResult<bool> {
        match &self.buf {
            ZSliceBuffer::ZSliceBufferShm(ZSliceBufferShm::Info(info)) => {
                // Deserialize the shmb info into shm buff
                let shmbinfo = SharedMemoryBufInfo::deserialize(&info)?;
                let smb = shmr.read_shmbuf(shmbinfo)?;
                // Replace the content of the slice
                self.buf = smb.into();
                // Update the indexes
                self.start = 0;
                self.end = self.buf.len();
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    #[cfg(feature = "zero-copy")]
    pub(crate) fn try_map_to_shmbuf(&mut self, shmr: &SharedMemoryReader) -> ZResult<bool> {
        match &self.buf {
            ZSliceBuffer::ZSliceBufferShm(ZSliceBufferShm::Info(info)) => {
                // Deserialize the shmb info into shm buff
                let shmbinfo = SharedMemoryBufInfo::deserialize(&info)?;
                let smb = shmr.try_read_shmbuf(shmbinfo)?;
                // Replace the content of the slice
                self.buf = smb.into();
                // Update the indexes
                self.start = 0;
                self.end = self.buf.len();
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    #[cfg(feature = "zero-copy")]
    pub(crate) fn map_to_shminfo(&mut self) -> ZResult<bool> {
        match &self.buf {
            ZSliceBuffer::ZSliceBufferShm(ZSliceBufferShm::Buffer(shmb)) => {
                // Serialize the shmb info
                let info = shmb.info.serialize()?;
                // Increase the reference count so to keep the SharedMemoryBuf valid
                shmb.inc_ref_count();
                // Replace the content of the slice
                self.buf = ZSliceBuffer::ZSliceBufferShm(ZSliceBufferShm::Info(info.into()));
                // Update the indexes
                self.start = 0;
                self.end = self.buf.len();
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

impl Deref for ZSlice {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl Index<usize> for ZSlice {
    type Output = u8;

    fn index(&self, index: usize) -> &Self::Output {
        &self.buf[self.start + index]
    }
}

impl Index<Range<usize>> for ZSlice {
    type Output = [u8];

    fn index(&self, range: Range<usize>) -> &Self::Output {
        &(&self.deref())[range]
    }
}

impl Index<RangeFrom<usize>> for ZSlice {
    type Output = [u8];

    fn index(&self, range: RangeFrom<usize>) -> &Self::Output {
        &(&self.deref())[range]
    }
}

impl Index<RangeFull> for ZSlice {
    type Output = [u8];

    fn index(&self, range: RangeFull) -> &Self::Output {
        &(&self.deref())[range]
    }
}

impl Index<RangeInclusive<usize>> for ZSlice {
    type Output = [u8];

    fn index(&self, range: RangeInclusive<usize>) -> &Self::Output {
        &(&self.deref())[range]
    }
}

impl Index<RangeTo<usize>> for ZSlice {
    type Output = [u8];

    fn index(&self, range: RangeTo<usize>) -> &Self::Output {
        &(&self.deref())[range]
    }
}

impl Index<RangeToInclusive<usize>> for ZSlice {
    type Output = [u8];

    fn index(&self, range: RangeToInclusive<usize>) -> &Self::Output {
        &(&self.deref())[range]
    }
}

impl PartialEq for ZSlice {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl Eq for ZSlice {}

impl fmt::Display for ZSlice {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:02x?}", self.as_slice())
    }
}

impl fmt::Debug for ZSlice {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "ZSlice{{ start: {}, end:{}, buf:\n {:02x?} \n}}",
            self.start,
            self.end,
            &self.buf[..]
        )
    }
}

// From impls
impl From<ZSliceBuffer> for ZSlice {
    fn from(buf: ZSliceBuffer) -> Self {
        let len = buf.len();
        Self::new(buf, 0, len)
    }
}

impl From<ZSliceBufferNet> for ZSlice {
    fn from(buf: ZSliceBufferNet) -> Self {
        let len = buf.len();
        Self::new(buf.into(), 0, len)
    }
}

impl From<ZSliceBufferShm> for ZSlice {
    fn from(buf: ZSliceBufferShm) -> Self {
        let len = buf.len();
        Self::new(buf.into(), 0, len)
    }
}

impl From<Arc<RecyclingObject<Box<[u8]>>>> for ZSlice {
    fn from(buf: Arc<RecyclingObject<Box<[u8]>>>) -> Self {
        let len = buf.len();
        Self::new(buf.into(), 0, len)
    }
}

impl From<RecyclingObject<Box<[u8]>>> for ZSlice {
    fn from(buf: RecyclingObject<Box<[u8]>>) -> Self {
        let len = buf.len();
        Self::new(buf.into(), 0, len)
    }
}

impl From<Arc<Vec<u8>>> for ZSlice {
    fn from(buf: Arc<Vec<u8>>) -> Self {
        let len = buf.len();
        Self::new(buf.into(), 0, len)
    }
}

impl From<Vec<u8>> for ZSlice {
    fn from(buf: Vec<u8>) -> Self {
        let len = buf.len();
        Self::new(buf.into(), 0, len)
    }
}

impl From<&[u8]> for ZSlice {
    fn from(buf: &[u8]) -> Self {
        let len = buf.len();
        Self::new(buf.into(), 0, len)
    }
}

impl<'a> From<&IoSlice<'a>> for ZSlice {
    fn from(buf: &IoSlice) -> Self {
        let len = buf.len();
        Self::new(buf.into(), 0, len)
    }
}

#[cfg(feature = "zero-copy")]
impl From<Arc<SharedMemoryBuf>> for ZSlice {
    fn from(buf: Arc<SharedMemoryBuf>) -> Self {
        let len = buf.len();
        Self::new(buf.into(), 0, len)
    }
}

#[cfg(feature = "zero-copy")]
impl From<Box<SharedMemoryBuf>> for ZSlice {
    fn from(buf: Box<SharedMemoryBuf>) -> Self {
        let len = buf.len();
        Self::new(buf.into(), 0, len)
    }
}

#[cfg(feature = "zero-copy")]
impl From<SharedMemoryBuf> for ZSlice {
    fn from(buf: SharedMemoryBuf) -> Self {
        let len = buf.len();
        Self::new(buf.into(), 0, len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zslice() {
        let buf = vec![0u8; 16];
        let zslice: ZSlice = buf.clone().into();
        println!("[01] {:?} {:?}", buf.as_slice(), zslice.as_slice());
        assert_eq!(buf.as_slice(), zslice.as_slice());

        let buf: Vec<u8> = (0u8..16).into_iter().collect();
        unsafe {
            let mbuf = zslice.as_mut_slice();
            for i in 0..buf.len() {
                mbuf[i] = buf[i];
            }
        }
        println!("[02] {:?} {:?}", buf.as_slice(), zslice.as_slice());
        assert_eq!(buf.as_slice(), zslice.as_slice());
    }
}
