// Copyright (c) 2024 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>

use crate::{
    buffer::{Buffer, SplitBuffer},
    reader::{BacktrackableReader, DidntRead, HasReader, Reader},
};
use alloc::{boxed::Box, sync::Arc, vec::Vec};
use core::{
    any::Any,
    convert::AsRef,
    fmt,
    num::NonZeroUsize,
    ops::{Deref, Index, Range, RangeFrom, RangeFull, RangeInclusive, RangeTo, RangeToInclusive},
    option,
};

/*************************************/
/*           ZSLICE BUFFER           */
/*************************************/
pub trait ZSliceBuffer: Send + Sync + fmt::Debug {
    fn as_slice(&self) -> &[u8];
    fn as_mut_slice(&mut self) -> &mut [u8];
    fn as_any(&self) -> &dyn Any;
}

impl ZSliceBuffer for Vec<u8> {
    fn as_slice(&self) -> &[u8] {
        self.as_ref()
    }
    fn as_mut_slice(&mut self) -> &mut [u8] {
        self.as_mut()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl ZSliceBuffer for Box<[u8]> {
    fn as_slice(&self) -> &[u8] {
        self.as_ref()
    }
    fn as_mut_slice(&mut self) -> &mut [u8] {
        self.as_mut()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl<const N: usize> ZSliceBuffer for [u8; N] {
    fn as_slice(&self) -> &[u8] {
        self.as_ref()
    }
    fn as_mut_slice(&mut self) -> &mut [u8] {
        self.as_mut()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/*************************************/
/*               ZSLICE              */
/*************************************/
#[cfg(feature = "shared-memory")]
#[derive(Copy, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum ZSliceKind {
    Raw = 0,
    ShmPtr = 1,
}

/// A clonable wrapper to a contiguous slice of bytes.
#[derive(Clone)]
pub struct ZSlice {
    pub(crate) buf: Arc<dyn ZSliceBuffer>,
    pub(crate) start: usize,
    pub(crate) end: usize,
    #[cfg(feature = "shared-memory")]
    pub kind: ZSliceKind,
}

impl ZSlice {
    pub fn make(
        buf: Arc<dyn ZSliceBuffer>,
        start: usize,
        end: usize,
    ) -> Result<ZSlice, Arc<dyn ZSliceBuffer>> {
        if start <= end && end <= buf.as_slice().len() {
            Ok(ZSlice {
                buf,
                start,
                end,
                #[cfg(feature = "shared-memory")]
                kind: ZSliceKind::Raw,
            })
        } else {
            Err(buf)
        }
    }

    #[inline]
    #[must_use]
    pub fn downcast_ref<T>(&self) -> Option<&T>
    where
        T: Any,
    {
        self.buf.as_any().downcast_ref::<T>()
    }

    #[inline]
    #[must_use]
    pub const fn range(&self) -> Range<usize> {
        self.start..self.end
    }

    #[inline]
    #[must_use]
    pub const fn len(&self) -> usize {
        self.end - self.start
    }

    #[inline]
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        // SAFETY: bounds checks are performed at `ZSlice` construction via `make()` or `subslice()`.
        crate::unsafe_slice!(self.buf.as_slice(), self.range())
    }

    #[must_use]
    pub fn subslice(&self, start: usize, end: usize) -> Option<ZSlice> {
        if start <= end && end <= self.len() {
            Some(ZSlice {
                buf: self.buf.clone(),
                start: self.start + start,
                end: self.start + end,
                #[cfg(feature = "shared-memory")]
                kind: self.kind,
            })
        } else {
            None
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
        self
    }
}

impl Index<usize> for ZSlice {
    type Output = u8;

    fn index(&self, index: usize) -> &Self::Output {
        &self.buf.as_slice()[self.start + index]
    }
}

impl Index<Range<usize>> for ZSlice {
    type Output = [u8];

    fn index(&self, range: Range<usize>) -> &Self::Output {
        &(self.deref())[range]
    }
}

impl Index<RangeFrom<usize>> for ZSlice {
    type Output = [u8];

    fn index(&self, range: RangeFrom<usize>) -> &Self::Output {
        &(self.deref())[range]
    }
}

impl Index<RangeFull> for ZSlice {
    type Output = [u8];

    fn index(&self, _range: RangeFull) -> &Self::Output {
        self
    }
}

impl Index<RangeInclusive<usize>> for ZSlice {
    type Output = [u8];

    fn index(&self, range: RangeInclusive<usize>) -> &Self::Output {
        &(self.deref())[range]
    }
}

impl Index<RangeTo<usize>> for ZSlice {
    type Output = [u8];

    fn index(&self, range: RangeTo<usize>) -> &Self::Output {
        &(self.deref())[range]
    }
}

impl Index<RangeToInclusive<usize>> for ZSlice {
    type Output = [u8];

    fn index(&self, range: RangeToInclusive<usize>) -> &Self::Output {
        &(self.deref())[range]
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
        write!(f, "{:02x?}", self.as_slice())
    }
}

// From impls
impl<T> From<Arc<T>> for ZSlice
where
    T: ZSliceBuffer + 'static,
{
    fn from(buf: Arc<T>) -> Self {
        let end = buf.as_slice().len();
        Self {
            buf,
            start: 0,
            end,
            #[cfg(feature = "shared-memory")]
            kind: ZSliceKind::Raw,
        }
    }
}

impl<T> From<T> for ZSlice
where
    T: ZSliceBuffer + 'static,
{
    fn from(buf: T) -> Self {
        Self::from(Arc::new(buf))
    }
}

// Buffer
impl Buffer for ZSlice {
    fn len(&self) -> usize {
        ZSlice::len(self)
    }
}

impl Buffer for &ZSlice {
    fn len(&self) -> usize {
        ZSlice::len(self)
    }
}

impl Buffer for &mut ZSlice {
    fn len(&self) -> usize {
        ZSlice::len(self)
    }
}

// SplitBuffer
impl SplitBuffer for ZSlice {
    type Slices<'a> = option::IntoIter<&'a [u8]>;

    fn slices(&self) -> Self::Slices<'_> {
        Some(self.as_slice()).into_iter()
    }
}

// Reader
impl HasReader for &mut ZSlice {
    type Reader = Self;

    fn reader(self) -> Self::Reader {
        self
    }
}

impl Reader for &mut ZSlice {
    fn read(&mut self, into: &mut [u8]) -> Result<NonZeroUsize, DidntRead> {
        let mut reader = self.as_slice().reader();
        let len = reader.read(into)?;
        self.start += len.get();
        Ok(len)
    }

    fn read_exact(&mut self, into: &mut [u8]) -> Result<(), DidntRead> {
        let mut reader = self.as_slice().reader();
        reader.read_exact(into)?;
        self.start += into.len();
        Ok(())
    }

    fn read_u8(&mut self) -> Result<u8, DidntRead> {
        let mut reader = self.as_slice().reader();
        let res = reader.read_u8()?;
        self.start += 1;
        Ok(res)
    }

    fn read_zslices<F: FnMut(ZSlice)>(&mut self, len: usize, mut f: F) -> Result<(), DidntRead> {
        let zslice = self.read_zslice(len)?;
        f(zslice);
        Ok(())
    }

    fn read_zslice(&mut self, len: usize) -> Result<ZSlice, DidntRead> {
        let res = self.subslice(0, len).ok_or(DidntRead)?;
        self.start += len;
        Ok(res)
    }

    fn remaining(&self) -> usize {
        self.len()
    }

    fn can_read(&self) -> bool {
        !self.is_empty()
    }
}

impl BacktrackableReader for &mut ZSlice {
    type Mark = usize;

    fn mark(&mut self) -> Self::Mark {
        self.start
    }

    fn rewind(&mut self, mark: Self::Mark) -> bool {
        self.start = mark;
        true
    }
}

#[cfg(feature = "std")]
impl std::io::Read for &mut ZSlice {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match <Self as Reader>::read(self, buf) {
            Ok(n) => Ok(n.get()),
            Err(_) => Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "UnexpectedEof",
            )),
        }
    }
}

impl ZSlice {
    #[cfg(feature = "test")]
    pub fn rand(len: usize) -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();
        (0..len).map(|_| rng.gen()).collect::<Vec<u8>>().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zslice() {
        let buf = crate::vec::uninit(16);
        let mut zslice: ZSlice = buf.clone().into();
        assert_eq!(buf.as_slice(), zslice.as_slice());

        let range = zslice.range();
        let mbuf = Arc::get_mut(&mut zslice.buf).unwrap();
        mbuf.as_mut_slice()[range][..buf.len()].clone_from_slice(&buf[..]);

        assert_eq!(buf.as_slice(), zslice.as_slice());
    }
}
