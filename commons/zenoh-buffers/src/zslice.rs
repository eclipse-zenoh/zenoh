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
    mem::ManuallyDrop,
    num::NonZeroUsize,
    ops::{Bound, Deref, DerefMut, RangeBounds},
    ptr::NonNull,
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
    ptr: *const u8,
    len: usize,
    buf: Arc<dyn ZSliceBuffer>,
    #[cfg(feature = "shared-memory")]
    pub kind: ZSliceKind,
}

unsafe impl Send for ZSlice {}
unsafe impl Sync for ZSlice {}

impl ZSlice {
    #[inline]
    pub fn new(
        buf: Arc<dyn ZSliceBuffer>,
        start: usize,
        end: usize,
    ) -> Result<ZSlice, Arc<dyn ZSliceBuffer>> {
        let slice = buf.as_slice();
        if start <= end && end <= slice.len() {
            Ok(Self {
                ptr: unsafe { slice.as_ptr().add(start) },
                len: end - start,
                buf,
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
    /// [`BufferMutGuard`] destructor must be run.
    #[inline]
    #[must_use]
    pub unsafe fn downcast_mut<T: Any + AsRef<[u8]>>(&mut self) -> Option<BufferMutGuard<'_, T>> {
        let mut zslice = NonNull::from(self);
        let buffer = ManuallyDrop::new(
            Arc::get_mut(&mut unsafe { zslice.as_mut() }.buf)?
                .as_any_mut()
                .downcast_mut()?,
        );
        Some(BufferMutGuard { zslice, buffer })
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
        if self.len == vec.len() {
            debug_assert_eq!(self.ptr, vec.as_ptr());
            Some(ZSliceWriter {
                vec,
                ptr: &mut self.ptr,
                len: &mut self.len,
            })
        } else {
            None
        }
    }

    #[inline]
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
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
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
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
                ptr: unsafe { self.ptr.add(start) },
                len: end - start,
                buf: self.buf.clone(),
                #[cfg(feature = "shared-memory")]
                kind: self.kind,
            })
        } else {
            None
        }
    }

    unsafe fn advance_by(&mut self, n: usize) {
        self.ptr = unsafe { self.ptr.add(n) };
        self.len -= n;
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
        let slice = buf.as_slice();
        Self {
            ptr: slice.as_ptr(),
            len: slice.len(),
            buf,
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

pub struct BufferMutGuard<'a, B: AsRef<[u8]>> {
    zslice: NonNull<ZSlice>,
    buffer: ManuallyDrop<&'a mut B>,
}

impl<'a, B: AsRef<[u8]>> BufferMutGuard<'a, B> {
    /// # Safety
    ///
    /// The returned buffer must still be the internal buffer of the zslice.
    pub unsafe fn map<B2: AsRef<[u8]>>(
        self,
        f: impl FnOnce(&mut B) -> &mut B2,
    ) -> BufferMutGuard<'a, B2> {
        let mut this = ManuallyDrop::new(self);
        BufferMutGuard {
            zslice: this.zslice,
            buffer: ManuallyDrop::new(f(unsafe { ManuallyDrop::take(&mut this.buffer) })),
        }
    }
}

impl<B: AsRef<[u8]>> Deref for BufferMutGuard<'_, B> {
    type Target = B;
    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl<B: AsRef<[u8]>> DerefMut for BufferMutGuard<'_, B> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buffer
    }
}

impl<B: AsRef<[u8]>> Drop for BufferMutGuard<'_, B> {
    fn drop(&mut self) {
        let slice = self.buffer.as_ref();
        let ptr = slice.as_ptr();
        let len = slice.len();
        unsafe {
            self.zslice.as_mut().ptr = ptr;
            self.zslice.as_mut().len = len;
        }
    }
}

#[derive(Debug)]
pub(crate) struct ZSliceWriter<'a> {
    vec: &'a mut Vec<u8>,
    ptr: &'a mut *const u8,
    len: &'a mut usize,
}

impl Writer for ZSliceWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> Result<NonZeroUsize, DidntWrite> {
        let len = self.vec.write(bytes)?;
        *self.ptr = self.vec.as_ptr();
        *self.len = self.vec.len();
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
        *self.ptr = self.vec.as_ptr();
        *self.len = self.vec.len();
        Ok(len)
    }
}

impl BacktrackableWriter for ZSliceWriter<'_> {
    type Mark = usize;

    fn mark(&mut self) -> Self::Mark {
        *self.len
    }

    fn rewind(&mut self, mark: Self::Mark) -> bool {
        assert!(mark <= self.vec.len());
        self.vec.truncate(mark);
        *self.len = mark;
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
        unsafe { self.advance_by(len.get()) }
        Ok(len)
    }

    fn read_exact(&mut self, into: &mut [u8]) -> Result<(), DidntRead> {
        let mut reader = self.as_slice().reader();
        reader.read_exact(into)?;
        // we trust `Reader` impl for `&[u8]` to not overflow the size of the slice
        unsafe { self.advance_by(into.len()) };
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
        unsafe { self.advance_by(len) };
        Ok(res)
    }

    fn read_u8(&mut self) -> Result<u8, DidntRead> {
        let mut reader = self.as_slice().reader();
        let res = reader.read_u8()?;
        // we trust `Reader` impl for `&[u8]` to not overflow the size of the slice
        unsafe { self.advance_by(1) };
        Ok(res)
    }

    fn can_read(&self) -> bool {
        !self.is_empty()
    }
}

impl BacktrackableReader for ZSlice {
    type Mark = (*const u8, usize);

    fn mark(&mut self) -> Self::Mark {
        (self.ptr, self.len)
    }

    fn rewind(&mut self, mark: Self::Mark) -> bool {
        (self.ptr, self.len) = mark;
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
        let mut mut_slice = unsafe { zslice.downcast_mut::<Vec<u8>>() }.unwrap();

        mut_slice[..buf.len()].clone_from_slice(&buf[..]);
        drop(mut_slice);

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
