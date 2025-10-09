//
// Copyright (c) 2023 ZettaScale Technology
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
//
use alloc::{boxed::Box, sync::Arc, vec::Vec};
use core::{
    any::Any,
    fmt, iter,
    num::NonZeroUsize,
    ops::{Bound, Deref, RangeBounds},
};

use crate::{
    buffer::{Buffer, SplitBuffer},
    reader::{BacktrackableReader, DidntRead, HasReader, Reader},
    writer::{BacktrackableWriter, DidntWrite, Writer},
};

/*************************************/
/*           ZSLICE BUFFER           */
/*************************************/
pub trait ZSliceBuffer: Any + Send + Sync + fmt::Debug {
    fn as_slice(&self) -> &[u8];
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl ZSliceBuffer for Vec<u8> {
    fn as_slice(&self) -> &[u8] {
        self
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl ZSliceBuffer for Box<[u8]> {
    fn as_slice(&self) -> &[u8] {
        self
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl<const N: usize> ZSliceBuffer for [u8; N] {
    fn as_slice(&self) -> &[u8] {
        self
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
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

/// A cloneable wrapper to a contiguous slice of bytes.
#[derive(Clone)]
pub struct ZSlice {
    buf: Arc<dyn ZSliceBuffer>,
    start: usize,
    end: usize,
    #[cfg(feature = "shared-memory")]
    pub kind: ZSliceKind,
}

impl ZSlice {
    #[inline]
    pub fn new(
        buf: Arc<dyn ZSliceBuffer>,
        start: usize,
        end: usize,
    ) -> Result<ZSlice, Arc<dyn ZSliceBuffer>> {
        if start <= end && end <= buf.as_slice().len() {
            Ok(Self {
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
    pub fn empty() -> Self {
        Self::new(Arc::new(Vec::<u8>::new()), 0, 0).unwrap()
    }

    #[inline]
    #[must_use]
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        self.buf.as_any().downcast_ref()
    }

    /// # Safety
    ///
    /// Buffer modification must not modify slice range.
    #[inline]
    #[must_use]
    pub unsafe fn downcast_mut<T: Any>(&mut self) -> Option<&mut T> {
        Arc::get_mut(&mut self.buf)?.as_any_mut().downcast_mut()
    }

    // This method is internal and is only meant to be used in `ZBufWriter`.
    // It's implemented in this module because it plays with `ZSlice` invariant,
    // so it should stay in the same module.
    // See https://github.com/eclipse-zenoh/zenoh/pull/1289#discussion_r1701796640
    #[inline]
    pub(crate) fn writer(&mut self) -> Option<ZSliceWriter<'_>> {
        let vec = Arc::get_mut(&mut self.buf)?
            .as_any_mut()
            .downcast_mut::<Vec<u8>>()?;
        if self.end == vec.len() {
            Some(ZSliceWriter {
                vec,
                end: &mut self.end,
            })
        } else {
            None
        }
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
        unsafe { self.buf.as_slice().get_unchecked(self.start..self.end) }
    }

    pub fn subslice(&self, range: impl RangeBounds<usize>) -> Option<Self> {
        let start = match range.start_bound() {
            Bound::Included(&n) => n,
            Bound::Excluded(&n) => n + 1,
            Bound::Unbounded => 0,
        };
        let end = match range.end_bound() {
            Bound::Included(&n) => n + 1,
            Bound::Excluded(&n) => n,
            Bound::Unbounded => self.len(),
        };
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

impl<Rhs: AsRef<[u8]> + ?Sized> PartialEq<Rhs> for ZSlice {
    fn eq(&self, other: &Rhs) -> bool {
        self.as_slice() == other.as_ref()
    }
}

impl Eq for ZSlice {}

#[cfg(feature = "std")]
impl std::hash::Hash for ZSlice {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_slice().hash(state);
    }
}

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
    type Slices<'a> = iter::Once<&'a [u8]>;

    fn slices(&self) -> Self::Slices<'_> {
        iter::once(self.as_slice())
    }
}

#[derive(Debug)]
pub(crate) struct ZSliceWriter<'a> {
    vec: &'a mut Vec<u8>,
    end: &'a mut usize,
}

impl Writer for ZSliceWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> Result<NonZeroUsize, DidntWrite> {
        let len = self.vec.write(bytes)?;
        *self.end += len.get();
        Ok(len)
    }

    fn write_exact(&mut self, bytes: &[u8]) -> Result<(), DidntWrite> {
        self.write(bytes).map(|_| ())
    }

    fn remaining(&self) -> usize {
        self.vec.remaining()
    }

    unsafe fn with_slot<F>(&mut self, len: usize, write: F) -> Result<NonZeroUsize, DidntWrite>
    where
        F: FnOnce(&mut [u8]) -> usize,
    {
        // SAFETY: same precondition as the enclosing function
        let len = unsafe { self.vec.with_slot(len, write) }?;
        *self.end += len.get();
        Ok(len)
    }
}

impl BacktrackableWriter for ZSliceWriter<'_> {
    type Mark = usize;

    fn mark(&mut self) -> Self::Mark {
        *self.end
    }

    fn rewind(&mut self, mark: Self::Mark) -> bool {
        assert!(mark <= self.vec.len());
        self.vec.truncate(mark);
        *self.end = mark;
        true
    }
}

// Reader
impl HasReader for &mut ZSlice {
    type Reader = Self;

    fn reader(self) -> Self::Reader {
        self
    }
}

impl Reader for ZSlice {
    fn read(&mut self, into: &mut [u8]) -> Result<NonZeroUsize, DidntRead> {
        let mut reader = self.as_slice().reader();
        let len = reader.read(into)?;
        // we trust `Reader` impl for `&[u8]` to not overflow the size of the slice
        self.start += len.get();
        Ok(len)
    }

    fn read_exact(&mut self, into: &mut [u8]) -> Result<(), DidntRead> {
        let mut reader = self.as_slice().reader();
        reader.read_exact(into)?;
        // we trust `Reader` impl for `&[u8]` to not overflow the size of the slice
        self.start += into.len();
        Ok(())
    }

    fn remaining(&self) -> usize {
        self.len()
    }

    fn read_zslices<F: FnMut(ZSlice)>(&mut self, len: usize, mut f: F) -> Result<(), DidntRead> {
        let zslice = self.read_zslice(len)?;
        f(zslice);
        Ok(())
    }

    fn read_zslice(&mut self, len: usize) -> Result<ZSlice, DidntRead> {
        let res = self.subslice(..len).ok_or(DidntRead)?;
        self.start += len;
        Ok(res)
    }

    fn read_u8(&mut self) -> Result<u8, DidntRead> {
        let mut reader = self.as_slice().reader();
        let res = reader.read_u8()?;
        // we trust `Reader` impl for `&[u8]` to not overflow the size of the slice
        self.start += 1;
        Ok(res)
    }

    fn can_read(&self) -> bool {
        !self.is_empty()
    }
}

impl BacktrackableReader for ZSlice {
    type Mark = usize;

    fn mark(&mut self) -> Self::Mark {
        self.start
    }

    fn rewind(&mut self, mark: Self::Mark) -> bool {
        assert!(mark <= self.end);
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

#[cfg(feature = "test")]
impl ZSlice {
    #[doc(hidden)]
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

        // SAFETY: buffer slize size is not modified
        let mut_slice = unsafe { zslice.downcast_mut::<Vec<u8>>() }.unwrap();

        mut_slice[..buf.len()].clone_from_slice(&buf[..]);

        assert_eq!(buf.as_slice(), zslice.as_slice());
    }

    #[test]
    fn hash() {
        use std::{
            collections::hash_map::DefaultHasher,
            hash::{Hash, Hasher},
        };

        let buf = vec![1, 2, 3, 4, 5];
        let mut buf_hasher = DefaultHasher::new();
        buf.hash(&mut buf_hasher);
        let buf_hash = buf_hasher.finish();

        let zslice: ZSlice = buf.clone().into();
        let mut zslice_hasher = DefaultHasher::new();
        zslice.hash(&mut zslice_hasher);
        let zslice_hash = zslice_hasher.finish();

        assert_eq!(buf_hash, zslice_hash);
    }
}
