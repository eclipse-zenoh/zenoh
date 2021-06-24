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
use std::convert::AsRef;
use std::fmt;
use std::io::IoSlice;
use std::ops::{
    Deref, Index, Range, RangeFrom, RangeFull, RangeInclusive, RangeTo, RangeToInclusive,
};
use std::sync::Arc;
#[cfg(feature = "zero-copy")]
use std::sync::RwLock;
use zenoh_util::collections::RecyclingObject;
#[cfg(feature = "zero-copy")]
use zenoh_util::core::ZResult;

/*************************************/
/*           ZSLICE BUFFER           */
/*************************************/
#[derive(Clone)]
pub enum ZSliceBuffer {
    NetSharedBuffer(Arc<RecyclingObject<Box<[u8]>>>),
    NetOwnedBuffer(Arc<Vec<u8>>),
    #[cfg(feature = "zero-copy")]
    ShmBuffer(Arc<SharedMemoryBuf>),
    #[cfg(feature = "zero-copy")]
    ShmInfo(Arc<Vec<u8>>),
}

impl ZSliceBuffer {
    fn as_slice(&self) -> &[u8] {
        match self {
            Self::NetSharedBuffer(buf) => buf,
            Self::NetOwnedBuffer(buf) => buf.as_slice(),
            #[cfg(feature = "zero-copy")]
            Self::ShmBuffer(buf) => buf.as_slice(),
            #[cfg(feature = "zero-copy")]
            Self::ShmInfo(buf) => buf.as_slice(),
        }
    }

    #[allow(clippy::missing_safety_doc)]
    #[allow(clippy::mut_from_ref)]
    unsafe fn as_mut_slice(&self) -> &mut [u8] {
        match self {
            Self::NetSharedBuffer(buf) => {
                &mut (*(Arc::as_ptr(buf) as *mut RecyclingObject<Box<[u8]>>))
            }
            Self::NetOwnedBuffer(buf) => &mut (*(Arc::as_ptr(buf) as *mut Vec<u8>)),
            #[cfg(feature = "zero-copy")]
            Self::ShmBuffer(buf) => {
                (&mut (*(Arc::as_ptr(buf) as *mut SharedMemoryBuf))).as_mut_slice()
            }
            #[cfg(feature = "zero-copy")]
            Self::ShmInfo(buf) => &mut (*(Arc::as_ptr(buf) as *mut Vec<u8>)),
        }
    }
}

impl Deref for ZSliceBuffer {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl AsRef<[u8]> for ZSliceBuffer {
    fn as_ref(&self) -> &[u8] {
        self.deref()
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
        Self::NetSharedBuffer(buf)
    }
}

impl From<RecyclingObject<Box<[u8]>>> for ZSliceBuffer {
    fn from(buf: RecyclingObject<Box<[u8]>>) -> Self {
        Self::NetSharedBuffer(buf.into())
    }
}

impl From<Arc<Vec<u8>>> for ZSliceBuffer {
    fn from(buf: Arc<Vec<u8>>) -> Self {
        Self::NetOwnedBuffer(buf)
    }
}

impl From<Vec<u8>> for ZSliceBuffer {
    fn from(buf: Vec<u8>) -> Self {
        Self::NetOwnedBuffer(buf.into())
    }
}

impl From<&[u8]> for ZSliceBuffer {
    fn from(buf: &[u8]) -> Self {
        Self::NetOwnedBuffer(buf.to_vec().into())
    }
}

impl<'a> From<&IoSlice<'a>> for ZSliceBuffer {
    fn from(buf: &IoSlice) -> Self {
        Self::NetOwnedBuffer(buf.to_vec().into())
    }
}

#[cfg(feature = "zero-copy")]
impl From<Arc<SharedMemoryBuf>> for ZSliceBuffer {
    fn from(buf: Arc<SharedMemoryBuf>) -> Self {
        Self::ShmBuffer(buf)
    }
}

#[cfg(feature = "zero-copy")]
impl From<Box<SharedMemoryBuf>> for ZSliceBuffer {
    fn from(buf: Box<SharedMemoryBuf>) -> Self {
        Self::ShmBuffer(buf.into())
    }
}

#[cfg(feature = "zero-copy")]
impl From<SharedMemoryBuf> for ZSliceBuffer {
    fn from(buf: SharedMemoryBuf) -> Self {
        Self::ShmBuffer(buf.into())
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
        assert!(end <= buf.as_slice().len());
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

    #[inline]
    pub fn get_type(&self) -> ZSliceType {
        match &self.buf {
            ZSliceBuffer::NetSharedBuffer(_) | ZSliceBuffer::NetOwnedBuffer(_) => ZSliceType::Net,
            #[cfg(feature = "zero-copy")]
            ZSliceBuffer::ShmBuffer(_) => ZSliceType::ShmBuf,
            #[cfg(feature = "zero-copy")]
            ZSliceBuffer::ShmInfo(_) => ZSliceType::ShmInfo,
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
    #[inline(never)]
    pub(crate) fn map_to_shmbuf(&mut self, shmr: Arc<RwLock<SharedMemoryReader>>) -> ZResult<bool> {
        match &self.buf {
            ZSliceBuffer::ShmInfo(info) => {
                // Deserialize the shmb info into shm buff
                let shmbinfo = SharedMemoryBufInfo::deserialize(&info)?;

                // First, try in read mode allowing concurrenct lookups
                let r_guard = zread!(shmr);
                let smb = r_guard.try_read_shmbuf(&shmbinfo).or_else(|_| {
                    // Next, try in write mode to eventual link the remote shm
                    drop(r_guard);
                    let mut w_guard = zwrite!(shmr);
                    w_guard.read_shmbuf(&shmbinfo)
                })?;

                // Replace the content of the slice
                self.buf = ZSliceBuffer::ShmBuffer(smb.into());
                // Update the indexes
                self.start = 0;
                self.end = self.buf.len();
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    #[cfg(feature = "zero-copy")]
    #[inline(never)]
    pub(crate) fn map_to_shminfo(&mut self) -> ZResult<bool> {
        match &self.buf {
            ZSliceBuffer::ShmBuffer(shmb) => {
                // Serialize the shmb info
                let info = shmb.info.serialize()?;
                // Increase the reference count so to keep the SharedMemoryBuf valid
                shmb.inc_ref_count();
                // Replace the content of the slice
                self.buf = ZSliceBuffer::ShmInfo(info.into());
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

impl AsRef<[u8]> for ZSlice {
    fn as_ref(&self) -> &[u8] {
        self.deref()
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
