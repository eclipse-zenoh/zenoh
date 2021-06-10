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
use super::{SharedMemoryBuf, SharedMemoryBufInfo, SharedMemoryManager};
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
pub enum HeapBuffer {
    RecyclingObject(Arc<RecyclingObject<Box<[u8]>>>),
    OwnedBuffer(Arc<Vec<u8>>),
    #[cfg(feature = "zero-copy")]
    ShmInfo(Arc<Vec<u8>>),
}

impl HeapBuffer {
    fn as_slice(&self) -> &[u8] {
        match self {
            Self::RecyclingObject(buf) => buf,
            Self::OwnedBuffer(buf) => buf,
            #[cfg(feature = "zero-copy")]
            Self::ShmInfo(buf) => buf,
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
            #[cfg(feature = "zero-copy")]
            Self::ShmInfo(buf) => &mut (*(Arc::as_ptr(buf) as *mut Vec<u8>)),
        }
    }
}

impl Deref for HeapBuffer {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl From<Arc<RecyclingObject<Box<[u8]>>>> for HeapBuffer {
    fn from(buf: Arc<RecyclingObject<Box<[u8]>>>) -> Self {
        Self::RecyclingObject(buf)
    }
}

impl From<Box<RecyclingObject<Box<[u8]>>>> for HeapBuffer {
    fn from(buf: Box<RecyclingObject<Box<[u8]>>>) -> Self {
        Self::RecyclingObject(buf.into())
    }
}

impl From<RecyclingObject<Box<[u8]>>> for HeapBuffer {
    fn from(buf: RecyclingObject<Box<[u8]>>) -> Self {
        Self::from(Arc::new(buf))
    }
}

impl From<Arc<Vec<u8>>> for HeapBuffer {
    fn from(buf: Arc<Vec<u8>>) -> Self {
        Self::OwnedBuffer(buf)
    }
}

impl From<Vec<u8>> for HeapBuffer {
    fn from(buf: Vec<u8>) -> Self {
        Self::from(Arc::new(buf))
    }
}

impl From<&[u8]> for HeapBuffer {
    fn from(buf: &[u8]) -> Self {
        Self::from(buf.to_vec())
    }
}

impl<'a> From<&IoSlice<'a>> for HeapBuffer {
    fn from(buf: &IoSlice) -> Self {
        Self::from(buf.to_vec())
    }
}

/*************************************/
/*            SHM BUFFER             */
/*************************************/
#[cfg(feature = "zero-copy")]
#[derive(Clone)]
pub struct ShmBuffer(Arc<SharedMemoryBuf>);

#[cfg(feature = "zero-copy")]
impl ShmBuffer {
    #[allow(clippy::missing_safety_doc)]
    #[allow(clippy::mut_from_ref)]
    unsafe fn as_mut_slice(&self) -> &mut [u8] {
        (&mut (*(Arc::as_ptr(&self.0) as *mut SharedMemoryBuf))).as_mut_slice()
    }
}

#[cfg(feature = "zero-copy")]
impl Deref for ShmBuffer {
    type Target = Arc<SharedMemoryBuf>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(feature = "zero-copy")]
impl From<Arc<SharedMemoryBuf>> for ShmBuffer {
    fn from(buf: Arc<SharedMemoryBuf>) -> Self {
        Self(buf)
    }
}

#[cfg(feature = "zero-copy")]
impl From<Box<SharedMemoryBuf>> for ShmBuffer {
    fn from(buf: Box<SharedMemoryBuf>) -> Self {
        Self(buf.into())
    }
}

#[cfg(feature = "zero-copy")]
impl From<SharedMemoryBuf> for ShmBuffer {
    fn from(buf: SharedMemoryBuf) -> Self {
        Self(Arc::new(buf))
    }
}

/*************************************/
/*           ZSLICE BUFFER           */
/*************************************/
#[derive(Clone)]
pub enum ZSliceBuffer {
    HeapBuffer(HeapBuffer),
    #[cfg(feature = "zero-copy")]
    ShmBuffer(ShmBuffer),
}

impl ZSliceBuffer {
    fn as_slice(&self) -> &[u8] {
        match self {
            Self::HeapBuffer(buf) => buf,
            #[cfg(feature = "zero-copy")]
            Self::ShmBuffer(buf) => buf.as_slice(),
        }
    }

    #[allow(clippy::missing_safety_doc)]
    #[allow(clippy::mut_from_ref)]
    unsafe fn as_mut_slice(&self) -> &mut [u8] {
        match self {
            Self::HeapBuffer(hb) => hb.as_mut_slice(),
            #[cfg(feature = "zero-copy")]
            Self::ShmBuffer(shmb) => shmb.as_mut_slice(),
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
        Self::HeapBuffer(buf.into())
    }
}

impl From<RecyclingObject<Box<[u8]>>> for ZSliceBuffer {
    fn from(buf: RecyclingObject<Box<[u8]>>) -> Self {
        Self::HeapBuffer(buf.into())
    }
}

impl From<Arc<Vec<u8>>> for ZSliceBuffer {
    fn from(buf: Arc<Vec<u8>>) -> Self {
        Self::HeapBuffer(buf.into())
    }
}

impl From<Vec<u8>> for ZSliceBuffer {
    fn from(buf: Vec<u8>) -> Self {
        Self::HeapBuffer(buf.into())
    }
}

impl From<&[u8]> for ZSliceBuffer {
    fn from(buf: &[u8]) -> Self {
        Self::HeapBuffer(buf.into())
    }
}

impl<'a> From<&IoSlice<'a>> for ZSliceBuffer {
    fn from(buf: &IoSlice) -> Self {
        Self::HeapBuffer(buf.into())
    }
}

#[cfg(feature = "zero-copy")]
impl From<Arc<SharedMemoryBuf>> for ZSliceBuffer {
    fn from(buf: Arc<SharedMemoryBuf>) -> Self {
        Self::ShmBuffer(buf.into())
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
    pub fn is_shm(&self) -> bool {
        match &self.buf {
            ZSliceBuffer::HeapBuffer(_) => false,
            ZSliceBuffer::ShmBuffer(_) => true,
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
    pub(crate) fn from_shm_info_to_shm_buff(
        &mut self,
        shmm: &mut SharedMemoryManager,
    ) -> ZResult<bool> {
        if let ZSliceBuffer::HeapBuffer(HeapBuffer::ShmInfo(info)) = &self.buf {
            // Deserialize the shmb info into shm buff
            let shmbinfo = SharedMemoryBufInfo::deserialize(&info)?;
            let smb = shmm.try_make_buf(shmbinfo)?;
            // Replace the content of the slice
            self.buf = smb.into();
            // Update the indexes
            self.start = 0;
            self.end = self.buf.len();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    #[cfg(feature = "zero-copy")]
    pub(crate) fn from_shm_buff_to_shm_info(&mut self) -> ZResult<bool> {
        if let ZSliceBuffer::ShmBuffer(shmb) = &self.buf {
            // Serialize the shmb info
            let info = shmb.info.serialize()?;
            // Increase the reference count so to keep the SharedMemoryBuf valid
            shmb.inc_ref_count();
            // Replace the content of the slice
            self.buf = ZSliceBuffer::HeapBuffer(HeapBuffer::ShmInfo(info.into()));
            // Update the indexes
            self.start = 0;
            self.end = self.buf.len();
            Ok(true)
        } else {
            Ok(false)
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
