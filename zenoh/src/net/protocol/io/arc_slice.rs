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
use super::SharedMemoryBuf;
use async_std::sync::Arc;
use std::fmt;
use std::io::IoSlice;
use std::ops::{
    Deref, Index, Range, RangeFrom, RangeFull, RangeInclusive, RangeTo, RangeToInclusive,
};
use zenoh_util::collections::RecyclingBuffer;

/*************************************/
/*         ARC SLICE BUFFER          */
/*************************************/
#[derive(Clone)]
pub enum ArcSliceBuffer {
    RecyclingBuffer(Arc<RecyclingBuffer>),
    OwnedBuffer(Arc<Vec<u8>>),
    #[cfg(feature = "zero-copy")]
    SharedBuffer(Arc<SharedMemoryBuf>),
}

impl Deref for ArcSliceBuffer {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        match self {
            Self::RecyclingBuffer(buf) => buf.as_slice(),
            Self::OwnedBuffer(buf) => buf.as_slice(),
            #[cfg(feature = "zero-copy")]
            Self::SharedBuffer(buf) => buf.as_slice(),
        }
    }
}

// Index traits
impl Index<usize> for ArcSliceBuffer {
    type Output = u8;

    fn index(&self, index: usize) -> &Self::Output {
        &(&self.deref())[index]
    }
}

impl Index<Range<usize>> for ArcSliceBuffer {
    type Output = [u8];

    fn index(&self, range: Range<usize>) -> &Self::Output {
        &(&self.deref())[range]
    }
}

impl Index<RangeFrom<usize>> for ArcSliceBuffer {
    type Output = [u8];

    fn index(&self, range: RangeFrom<usize>) -> &Self::Output {
        &(&self.deref())[range]
    }
}

impl Index<RangeFull> for ArcSliceBuffer {
    type Output = [u8];

    fn index(&self, range: RangeFull) -> &Self::Output {
        &(&self.deref())[range]
    }
}

impl Index<RangeInclusive<usize>> for ArcSliceBuffer {
    type Output = [u8];

    fn index(&self, range: RangeInclusive<usize>) -> &Self::Output {
        &(&self.deref())[range]
    }
}

impl Index<RangeTo<usize>> for ArcSliceBuffer {
    type Output = [u8];

    fn index(&self, range: RangeTo<usize>) -> &Self::Output {
        &(&self.deref())[range]
    }
}

impl Index<RangeToInclusive<usize>> for ArcSliceBuffer {
    type Output = [u8];

    fn index(&self, range: RangeToInclusive<usize>) -> &Self::Output {
        &(&self.deref())[range]
    }
}

// From traits
impl From<Arc<RecyclingBuffer>> for ArcSliceBuffer {
    fn from(buf: Arc<RecyclingBuffer>) -> ArcSliceBuffer {
        ArcSliceBuffer::RecyclingBuffer(buf)
    }
}

impl From<RecyclingBuffer> for ArcSliceBuffer {
    fn from(buf: RecyclingBuffer) -> ArcSliceBuffer {
        ArcSliceBuffer::from(Arc::new(buf))
    }
}

impl From<Arc<Vec<u8>>> for ArcSliceBuffer {
    fn from(buf: Arc<Vec<u8>>) -> ArcSliceBuffer {
        ArcSliceBuffer::OwnedBuffer(buf)
    }
}

impl From<Vec<u8>> for ArcSliceBuffer {
    fn from(buf: Vec<u8>) -> ArcSliceBuffer {
        ArcSliceBuffer::from(Arc::new(buf))
    }
}

impl From<&[u8]> for ArcSliceBuffer {
    fn from(buf: &[u8]) -> ArcSliceBuffer {
        ArcSliceBuffer::from(buf.to_vec())
    }
}

#[cfg(feature = "zero-copy")]
impl From<Arc<SharedMemoryBuf>> for ArcSliceBuffer {
    fn from(buf: Arc<SharedMemoryBuf>) -> ArcSliceBuffer {
        ArcSliceBuffer::SharedBuffer(buf)
    }
}

#[cfg(feature = "zero-copy")]
impl From<SharedMemoryBuf> for ArcSliceBuffer {
    fn from(buf: SharedMemoryBuf) -> ArcSliceBuffer {
        ArcSliceBuffer::from(Arc::new(buf))
    }
}

/*************************************/
/*             ARC SLICE             */
/*************************************/
#[derive(Clone)]
pub struct ArcSlice {
    buf: ArcSliceBuffer,
    start: usize,
    end: usize,
}

impl ArcSlice {
    pub fn new(buf: ArcSliceBuffer, start: usize, end: usize) -> ArcSlice {
        assert!(end <= buf.len());
        ArcSlice { buf, start, end }
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

    #[inline]
    pub fn as_ioslice(&self) -> IoSlice {
        IoSlice::new(self.as_slice())
    }

    pub fn get_sub_slice(&self, start: usize, end: usize) -> &[u8] {
        assert!(end <= self.len());
        &self.buf[self.start + start..self.start + end]
    }

    pub fn new_sub_slice(&self, start: usize, end: usize) -> ArcSlice {
        assert!(end <= self.len());
        ArcSlice {
            buf: self.buf.clone(),
            start: self.start + start,
            end: self.start + end,
        }
    }
}

impl Index<usize> for ArcSlice {
    type Output = u8;

    fn index(&self, index: usize) -> &Self::Output {
        &self.buf[self.start + index]
    }
}

impl fmt::Display for ArcSlice {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:02x?}", self.as_slice())
    }
}

impl fmt::Debug for ArcSlice {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "ArcSlice{{ start: {}, end:{}, buf:\n {:02x?} \n}}",
            self.start,
            self.end,
            &self.buf[..]
        )
    }
}

impl From<Arc<Vec<u8>>> for ArcSlice {
    fn from(buf: Arc<Vec<u8>>) -> ArcSlice {
        let len = buf.len();
        ArcSlice::new(buf.into(), 0, len)
    }
}

impl From<Vec<u8>> for ArcSlice {
    fn from(buf: Vec<u8>) -> ArcSlice {
        ArcSlice::from(Arc::new(buf))
    }
}

impl From<&[u8]> for ArcSlice {
    fn from(buf: &[u8]) -> ArcSlice {
        ArcSlice::from(buf.to_vec())
    }
}

#[cfg(feature = "zero-copy")]
impl From<SharedMemoryBuf> for ArcSlice {
    fn from(buf: SharedMemoryBuf) -> ArcSlice {
        let len = buf.len();
        ArcSlice::new(buf.into(), 0, len)
    }
}

impl<'a> From<&IoSlice<'a>> for ArcSlice {
    fn from(buf: &IoSlice) -> ArcSlice {
        ArcSlice::from(buf.to_vec())
    }
}

impl PartialEq for ArcSlice {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl Eq for ArcSlice {}
