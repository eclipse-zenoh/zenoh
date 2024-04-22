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

#[cfg(feature = "shared-memory")]
use crate::ZSliceKind;
use crate::{
    buffer::{Buffer, SplitBuffer},
    reader::{BacktrackableReader, DidntRead, DidntSiphon, HasReader, Reader, SiphonableReader},
    writer::{BacktrackableWriter, DidntWrite, HasWriter, Writer},
    ZSlice,
};
use alloc::{sync::Arc, vec::Vec};
use core::{cmp, iter, mem, num::NonZeroUsize, ops::RangeBounds, ptr};
use zenoh_collections::SingleOrVec;

fn get_mut_unchecked<T>(arc: &mut Arc<T>) -> &mut T {
    unsafe { &mut (*(Arc::as_ptr(arc) as *mut T)) }
}

#[derive(Debug, Clone, Default, Eq)]
pub struct ZBuf {
    slices: SingleOrVec<ZSlice>,
}

impl ZBuf {
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.slices.clear();
    }

    pub fn zslices(&self) -> impl Iterator<Item = &ZSlice> + '_ {
        self.slices.as_ref().iter()
    }

    pub fn zslices_mut(&mut self) -> impl Iterator<Item = &mut ZSlice> + '_ {
        self.slices.as_mut().iter_mut()
    }

    pub fn push_zslice(&mut self, zslice: ZSlice) {
        if !zslice.is_empty() {
            self.slices.push(zslice);
        }
    }

    pub fn splice<Range: RangeBounds<usize>>(&mut self, erased: Range, replacement: &[u8]) {
        let start = match erased.start_bound() {
            core::ops::Bound::Included(n) => *n,
            core::ops::Bound::Excluded(n) => n + 1,
            core::ops::Bound::Unbounded => 0,
        };
        let end = match erased.end_bound() {
            core::ops::Bound::Included(n) => n + 1,
            core::ops::Bound::Excluded(n) => *n,
            core::ops::Bound::Unbounded => self.len(),
        };
        if start != end {
            self.remove(start, end);
        }
        self.insert(start, replacement);
    }
    fn remove(&mut self, mut start: usize, mut end: usize) {
        assert!(start <= end);
        assert!(end <= self.len());
        let mut start_slice_idx = 0;
        let mut start_idx_in_start_slice = 0;
        let mut end_slice_idx = 0;
        let mut end_idx_in_end_slice = 0;
        for (i, slice) in self.slices.as_mut().iter_mut().enumerate() {
            if slice.len() > start {
                start_slice_idx = i;
                start_idx_in_start_slice = start;
            }
            if slice.len() >= end {
                end_slice_idx = i;
                end_idx_in_end_slice = end;
                break;
            }
            start -= slice.len();
            end -= slice.len();
        }
        let start_slice = &mut self.slices.as_mut()[start_slice_idx];
        start_slice.end = start_slice.start + start_idx_in_start_slice;
        let drain_start = start_slice_idx + (start_slice.start < start_slice.end) as usize;
        let end_slice = &mut self.slices.as_mut()[end_slice_idx];
        end_slice.start += end_idx_in_end_slice;
        let drain_end = end_slice_idx + (end_slice.start >= end_slice.end) as usize;
        self.slices.drain(drain_start..drain_end);
    }
    fn insert(&mut self, mut at: usize, slice: &[u8]) {
        if slice.is_empty() {
            return;
        }
        let old_at = at;
        let mut slice_index = usize::MAX;
        for (i, slice) in self.slices.as_ref().iter().enumerate() {
            if at < slice.len() {
                slice_index = i;
                break;
            }
            if let Some(new_at) = at.checked_sub(slice.len()) {
                at = new_at
            } else {
                panic!(
                    "Out of bounds insert attempted: at={old_at}, len={}",
                    self.len()
                )
            }
        }
        if at != 0 {
            let split = &self.slices.as_ref()[slice_index];
            let (l, r) = (
                split.subslice(0, at).unwrap(),
                split.subslice(at, split.len()).unwrap(),
            );
            self.slices.drain(slice_index..(slice_index + 1));
            self.slices.insert(slice_index, l);
            self.slices.insert(slice_index + 1, Vec::from(slice).into());
            self.slices.insert(slice_index + 2, r);
        } else {
            self.slices.insert(slice_index, Vec::from(slice).into())
        }
    }
}

// Buffer
impl Buffer for ZBuf {
    #[inline(always)]
    fn len(&self) -> usize {
        self.slices
            .as_ref()
            .iter()
            .fold(0, |len, slice| len + slice.len())
    }
}

// SplitBuffer
impl SplitBuffer for ZBuf {
    type Slices<'a> = iter::Map<core::slice::Iter<'a, ZSlice>, fn(&'a ZSlice) -> &'a [u8]>;

    fn slices(&self) -> Self::Slices<'_> {
        self.slices.as_ref().iter().map(ZSlice::as_slice)
    }
}

impl PartialEq for ZBuf {
    fn eq(&self, other: &Self) -> bool {
        let mut self_slices = self.slices();
        let mut other_slices = other.slices();
        let mut current_self = self_slices.next();
        let mut current_other = other_slices.next();
        loop {
            match (current_self, current_other) {
                (None, None) => return true,
                (None, _) | (_, None) => return false,
                (Some(l), Some(r)) => {
                    let cmp_len = l.len().min(r.len());
                    // SAFETY: cmp_len is the minimum length between l and r slices.
                    let lhs = crate::unsafe_slice!(l, ..cmp_len);
                    let rhs = crate::unsafe_slice!(r, ..cmp_len);
                    if lhs != rhs {
                        return false;
                    }
                    if cmp_len == l.len() {
                        current_self = self_slices.next();
                    } else {
                        // SAFETY: cmp_len is the minimum length between l and r slices.
                        let lhs = crate::unsafe_slice!(l, cmp_len..);
                        current_self = Some(lhs);
                    }
                    if cmp_len == r.len() {
                        current_other = other_slices.next();
                    } else {
                        // SAFETY: cmp_len is the minimum length between l and r slices.
                        let rhs = crate::unsafe_slice!(r, cmp_len..);
                        current_other = Some(rhs);
                    }
                }
            }
        }
    }
}

// From impls
impl<T> From<T> for ZBuf
where
    T: Into<ZSlice>,
{
    fn from(t: T) -> Self {
        let mut zbuf = ZBuf::empty();
        let zslice: ZSlice = t.into();
        zbuf.push_zslice(zslice);
        zbuf
    }
}
// Reader
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ZBufPos {
    slice: usize,
    byte: usize,
}

#[derive(Debug, Clone)]
pub struct ZBufReader<'a> {
    inner: &'a ZBuf,
    cursor: ZBufPos,
}

impl<'a> HasReader for &'a ZBuf {
    type Reader = ZBufReader<'a>;

    fn reader(self) -> Self::Reader {
        ZBufReader {
            inner: self,
            cursor: ZBufPos { slice: 0, byte: 0 },
        }
    }
}

impl<'a> Reader for ZBufReader<'a> {
    fn read(&mut self, mut into: &mut [u8]) -> Result<NonZeroUsize, DidntRead> {
        let mut read = 0;
        while let Some(slice) = self.inner.slices.get(self.cursor.slice) {
            // Subslice from the current read slice
            // SAFETY: validity of self.cursor.byte is ensured by the read logic.
            let from = crate::unsafe_slice!(slice.as_slice(), self.cursor.byte..);
            // Take the minimum length among read and write slices
            let len = from.len().min(into.len());
            // Copy the slice content
            // SAFETY: len is the minimum length between from and into slices.
            let lhs = crate::unsafe_slice_mut!(into, ..len);
            let rhs = crate::unsafe_slice!(from, ..len);
            lhs.copy_from_slice(rhs);
            // Advance the write slice
            // SAFETY: len is the minimum length between from and into slices.
            into = crate::unsafe_slice_mut!(into, len..);
            // Update the counter
            read += len;
            // Move the byte cursor
            self.cursor.byte += len;
            // We consumed all the current read slice, move to the next slice
            if self.cursor.byte == slice.len() {
                self.cursor.slice += 1;
                self.cursor.byte = 0;
            }
            // We have read everything we had to read
            if into.is_empty() {
                break;
            }
        }
        NonZeroUsize::new(read).ok_or(DidntRead)
    }

    fn read_exact(&mut self, into: &mut [u8]) -> Result<(), DidntRead> {
        let len = self.read(into)?;
        if len.get() == into.len() {
            Ok(())
        } else {
            Err(DidntRead)
        }
    }

    fn read_u8(&mut self) -> Result<u8, DidntRead> {
        let slice = self.inner.slices.get(self.cursor.slice).ok_or(DidntRead)?;

        let byte = slice[self.cursor.byte];
        self.cursor.byte += 1;
        if self.cursor.byte == slice.len() {
            self.cursor.slice += 1;
            self.cursor.byte = 0;
        }
        Ok(byte)
    }

    fn remaining(&self) -> usize {
        // SAFETY: self.cursor.slice validity is ensured by the reader
        let s = crate::unsafe_slice!(self.inner.slices.as_ref(), self.cursor.slice..);
        s.iter().fold(0, |acc, it| acc + it.len()) - self.cursor.byte
    }

    fn read_zslices<F: FnMut(ZSlice)>(&mut self, len: usize, mut f: F) -> Result<(), DidntRead> {
        if self.remaining() < len {
            return Err(DidntRead);
        }

        let iter = ZBufSliceIterator {
            reader: self,
            remaining: len,
        };
        for slice in iter {
            f(slice);
        }

        Ok(())
    }

    fn read_zslice(&mut self, len: usize) -> Result<ZSlice, DidntRead> {
        let slice = self.inner.slices.get(self.cursor.slice).ok_or(DidntRead)?;
        match (slice.len() - self.cursor.byte).cmp(&len) {
            cmp::Ordering::Less => {
                let mut buffer = crate::vec::uninit(len);
                self.read_exact(&mut buffer)?;
                Ok(buffer.into())
            }
            cmp::Ordering::Equal => {
                let s = slice
                    .subslice(self.cursor.byte, slice.len())
                    .ok_or(DidntRead)?;

                self.cursor.slice += 1;
                self.cursor.byte = 0;
                Ok(s)
            }
            cmp::Ordering::Greater => {
                let start = self.cursor.byte;
                self.cursor.byte += len;
                slice.subslice(start, self.cursor.byte).ok_or(DidntRead)
            }
        }
    }

    fn can_read(&self) -> bool {
        self.inner.slices.get(self.cursor.slice).is_some()
    }
}

impl<'a> BacktrackableReader for ZBufReader<'a> {
    type Mark = ZBufPos;

    fn mark(&mut self) -> Self::Mark {
        self.cursor
    }

    fn rewind(&mut self, mark: Self::Mark) -> bool {
        self.cursor = mark;
        true
    }
}

impl<'a> SiphonableReader for ZBufReader<'a> {
    fn siphon<W>(&mut self, writer: &mut W) -> Result<NonZeroUsize, DidntSiphon>
    where
        W: Writer,
    {
        let mut read = 0;
        while let Some(slice) = self.inner.slices.get(self.cursor.slice) {
            // Subslice from the current read slice
            // SAFETY: self.cursor.byte is ensured by the reader.
            let from = crate::unsafe_slice!(slice.as_slice(), self.cursor.byte..);
            // Copy the slice content
            match writer.write(from) {
                Ok(len) => {
                    // Update the counter
                    read += len.get();
                    // Move the byte cursor
                    self.cursor.byte += len.get();
                    // We consumed all the current read slice, move to the next slice
                    if self.cursor.byte == slice.len() {
                        self.cursor.slice += 1;
                        self.cursor.byte = 0;
                    }
                }
                Err(_) => {
                    return NonZeroUsize::new(read).ok_or(DidntSiphon);
                }
            }
        }
        NonZeroUsize::new(read).ok_or(DidntSiphon)
    }
}

#[cfg(feature = "std")]
impl<'a> std::io::Read for ZBufReader<'a> {
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

// ZSlice iterator
pub struct ZBufSliceIterator<'a, 'b> {
    reader: &'a mut ZBufReader<'b>,
    remaining: usize,
}

impl Iterator for ZBufSliceIterator<'_, '_> {
    type Item = ZSlice;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }

        // SAFETY: self.reader.cursor.slice is ensured by the reader.
        let slice =
            crate::unsafe_slice!(self.reader.inner.slices.as_ref(), self.reader.cursor.slice);
        let start = self.reader.cursor.byte;
        // SAFETY: self.reader.cursor.byte is ensured by the reader.
        let current = crate::unsafe_slice!(slice, start..);
        let len = current.len();
        match self.remaining.cmp(&len) {
            cmp::Ordering::Less => {
                let end = start + self.remaining;
                let slice = slice.subslice(start, end);
                self.reader.cursor.byte = end;
                self.remaining = 0;
                slice
            }
            cmp::Ordering::Equal => {
                let end = start + self.remaining;
                let slice = slice.subslice(start, end);
                self.reader.cursor.slice += 1;
                self.reader.cursor.byte = 0;
                self.remaining = 0;
                slice
            }
            cmp::Ordering::Greater => {
                let end = start + len;
                let slice = slice.subslice(start, end);
                self.reader.cursor.slice += 1;
                self.reader.cursor.byte = 0;
                self.remaining -= len;
                slice
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (1, None)
    }
}

// Writer
#[derive(Debug)]
pub struct ZBufWriter<'a> {
    inner: &'a mut ZBuf,
    cache: Arc<Vec<u8>>,
}

impl<'a> HasWriter for &'a mut ZBuf {
    type Writer = ZBufWriter<'a>;

    fn writer(self) -> Self::Writer {
        let mut cache = None;
        if let Some(ZSlice { buf, end, .. }) = self.slices.last_mut() {
            // Verify the ZSlice is actually a Vec<u8>
            if let Some(b) = buf.as_any().downcast_ref::<Vec<u8>>() {
                // Check for the length
                if *end == b.len() {
                    cache = Some(unsafe { Arc::from_raw(Arc::into_raw(buf.clone()).cast()) })
                }
            }
        }

        ZBufWriter {
            inner: self,
            cache: cache.unwrap_or_else(|| Arc::new(Vec::new())),
        }
    }
}

impl Writer for ZBufWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> Result<NonZeroUsize, DidntWrite> {
        if bytes.is_empty() {
            return Err(DidntWrite);
        }
        self.write_exact(bytes)?;
        // SAFETY: this operation is safe since we check if bytes is empty
        Ok(unsafe { NonZeroUsize::new_unchecked(bytes.len()) })
    }

    fn write_exact(&mut self, bytes: &[u8]) -> Result<(), DidntWrite> {
        let cache = get_mut_unchecked(&mut self.cache);
        let prev_cache_len = cache.len();
        cache.extend_from_slice(bytes);
        let cache_len = cache.len();

        // Verify we are writing on the cache
        if let Some(ZSlice {
            buf, ref mut end, ..
        }) = self.inner.slices.last_mut()
        {
            // Verify the previous length of the cache is the right one
            if *end == prev_cache_len {
                // Verify the ZSlice is actually a Vec<u8>
                if let Some(b) = buf.as_any().downcast_ref::<Vec<u8>>() {
                    // Verify the Vec<u8> of the ZSlice is exactly the one from the cache
                    if core::ptr::eq(cache.as_ptr(), b.as_ptr()) {
                        // Simply update the slice length
                        *end = cache_len;
                        return Ok(());
                    }
                }
            }
        }

        self.inner.slices.push(ZSlice {
            buf: self.cache.clone(),
            start: prev_cache_len,
            end: cache_len,
            #[cfg(feature = "shared-memory")]
            kind: ZSliceKind::Raw,
        });
        Ok(())
    }

    fn write_u8(&mut self, byte: u8) -> Result<(), DidntWrite> {
        self.write_exact(core::slice::from_ref(&byte))
    }

    fn remaining(&self) -> usize {
        usize::MAX
    }

    fn write_zslice(&mut self, slice: &ZSlice) -> Result<(), DidntWrite> {
        self.inner.slices.push(slice.clone());
        Ok(())
    }

    fn with_slot<F>(&mut self, mut len: usize, f: F) -> Result<NonZeroUsize, DidntWrite>
    where
        F: FnOnce(&mut [u8]) -> usize,
    {
        let cache = get_mut_unchecked(&mut self.cache);
        let prev_cache_len = cache.len();
        cache.reserve(len);

        // SAFETY: we already reserved len elements on the vector.
        let s = crate::unsafe_slice_mut!(cache.spare_capacity_mut(), ..len);
        // SAFETY: converting MaybeUninit<u8> into [u8] is safe because we are going to write on it.
        //         The returned len tells us how many bytes have been written so as to update the len accordingly.
        len = unsafe { f(&mut *(s as *mut [mem::MaybeUninit<u8>] as *mut [u8])) };
        // SAFETY: we already reserved len elements on the vector.
        unsafe { cache.set_len(prev_cache_len + len) };

        let cache_len = cache.len();

        // Verify we are writing on the cache
        if let Some(ZSlice {
            buf, ref mut end, ..
        }) = self.inner.slices.last_mut()
        {
            // Verify the previous length of the cache is the right one
            if *end == prev_cache_len {
                // Verify the ZSlice is actually a Vec<u8>
                if let Some(b) = buf.as_any().downcast_ref::<Vec<u8>>() {
                    // Verify the Vec<u8> of the ZSlice is exactly the one from the cache
                    if ptr::eq(cache.as_ptr(), b.as_ptr()) {
                        // Simply update the slice length
                        *end = cache_len;
                        return NonZeroUsize::new(len).ok_or(DidntWrite);
                    }
                }
            }
        }

        self.inner.slices.push(ZSlice {
            buf: self.cache.clone(),
            start: prev_cache_len,
            end: cache_len,
            #[cfg(feature = "shared-memory")]
            kind: ZSliceKind::Raw,
        });
        NonZeroUsize::new(len).ok_or(DidntWrite)
    }
}

impl BacktrackableWriter for ZBufWriter<'_> {
    type Mark = ZBufPos;

    fn mark(&mut self) -> Self::Mark {
        if let Some(slice) = self.inner.slices.last() {
            ZBufPos {
                slice: self.inner.slices.len(),
                byte: slice.end,
            }
        } else {
            ZBufPos { slice: 0, byte: 0 }
        }
    }

    fn rewind(&mut self, mark: Self::Mark) -> bool {
        self.inner
            .slices
            .truncate(mark.slice + usize::from(mark.byte != 0));
        if let Some(slice) = self.inner.slices.last_mut() {
            slice.end = mark.byte;
        }
        true
    }
}

#[cfg(feature = "std")]
impl<'a> std::io::Write for ZBufWriter<'a> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match <Self as Writer>::write(self, buf) {
            Ok(n) => Ok(n.get()),
            Err(_) => Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "UnexpectedEof",
            )),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(feature = "test")]
impl ZBuf {
    pub fn rand(len: usize) -> Self {
        let mut zbuf = ZBuf::empty();
        zbuf.push_zslice(ZSlice::rand(len));
        zbuf
    }
}

mod tests {
    #[test]
    fn zbuf_eq() {
        use super::{ZBuf, ZSlice};

        let slice: ZSlice = [0u8, 1, 2, 3, 4, 5, 6, 7].to_vec().into();

        let mut zbuf1 = ZBuf::empty();
        zbuf1.push_zslice(slice.subslice(0, 4).unwrap());
        zbuf1.push_zslice(slice.subslice(4, 8).unwrap());

        let mut zbuf2 = ZBuf::empty();
        zbuf2.push_zslice(slice.subslice(0, 1).unwrap());
        zbuf2.push_zslice(slice.subslice(1, 4).unwrap());
        zbuf2.push_zslice(slice.subslice(4, 8).unwrap());

        assert_eq!(zbuf1, zbuf2);

        let mut zbuf1 = ZBuf::empty();
        zbuf1.push_zslice(slice.subslice(2, 4).unwrap());
        zbuf1.push_zslice(slice.subslice(4, 8).unwrap());

        let mut zbuf2 = ZBuf::empty();
        zbuf2.push_zslice(slice.subslice(2, 3).unwrap());
        zbuf2.push_zslice(slice.subslice(3, 6).unwrap());
        zbuf2.push_zslice(slice.subslice(6, 8).unwrap());

        assert_eq!(zbuf1, zbuf2);
    }
}
