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
use alloc::{sync::Arc, vec::Vec};
use core::{cmp, iter, num::NonZeroUsize, ptr::NonNull};
#[cfg(feature = "std")]
use std::io;

use zenoh_collections::SingleOrVec;

use crate::{
    buffer::{Buffer, SplitBuffer},
    reader::{
        AdvanceableReader, BacktrackableReader, DidntRead, DidntSiphon, HasReader, Reader,
        SiphonableReader,
    },
    writer::{BacktrackableWriter, DidntWrite, HasWriter, Writer},
    ZSlice, ZSliceBuffer, ZSliceWriter,
};

#[derive(Debug, Clone, Default, Eq)]
pub struct ZBuf {
    slices: SingleOrVec<ZSlice>,
}

impl ZBuf {
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            slices: SingleOrVec::empty(),
        }
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

    pub fn into_zslices(self) -> impl Iterator<Item = ZSlice> {
        self.slices.into_iter()
    }

    pub fn push_zslice(&mut self, zslice: ZSlice) {
        if !zslice.is_empty() {
            self.slices.push(zslice);
        }
    }

    pub fn to_zslice(&self) -> ZSlice {
        let mut slices = self.zslices();
        match self.slices.len() {
            0 => ZSlice::empty(),
            // SAFETY: it's safe to use unwrap_unchecked() because we are explicitly checking the length is 1.
            1 => unsafe { slices.next().unwrap_unchecked().clone() },
            _ => slices
                .fold(Vec::new(), |mut acc, it| {
                    acc.extend(it.as_slice());
                    acc
                })
                .into(),
        }
    }

    #[inline]
    fn opt_zslice_writer(&mut self) -> Option<ZSliceWriter<'_>> {
        self.slices.last_mut().and_then(|s| s.writer())
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
impl From<ZSlice> for ZBuf {
    fn from(t: ZSlice) -> Self {
        let mut zbuf = ZBuf::empty();
        zbuf.push_zslice(t);
        zbuf
    }
}

impl<T> From<Arc<T>> for ZBuf
where
    T: ZSliceBuffer + 'static,
{
    fn from(t: Arc<T>) -> Self {
        let zslice: ZSlice = t.into();
        Self::from(zslice)
    }
}

impl<T> From<T> for ZBuf
where
    T: ZSliceBuffer + 'static,
{
    fn from(t: T) -> Self {
        let zslice: ZSlice = t.into();
        Self::from(zslice)
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

impl Reader for ZBufReader<'_> {
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
        let len = Reader::read(self, into)?;
        if len.get() == into.len() {
            Ok(())
        } else {
            Err(DidntRead)
        }
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
                Reader::read_exact(self, &mut buffer)?;
                Ok(buffer.into())
            }
            cmp::Ordering::Equal => {
                let s = slice.subslice(self.cursor.byte..).ok_or(DidntRead)?;
                self.cursor.slice += 1;
                self.cursor.byte = 0;
                Ok(s)
            }
            cmp::Ordering::Greater => {
                let start = self.cursor.byte;
                self.cursor.byte += len;
                slice.subslice(start..self.cursor.byte).ok_or(DidntRead)
            }
        }
    }

    fn read_u8(&mut self) -> Result<u8, DidntRead> {
        let slice = self.inner.slices.get(self.cursor.slice).ok_or(DidntRead)?;

        let byte = *slice.get(self.cursor.byte).ok_or(DidntRead)?;
        self.cursor.byte += 1;
        if self.cursor.byte == slice.len() {
            self.cursor.slice += 1;
            self.cursor.byte = 0;
        }
        Ok(byte)
    }

    fn can_read(&self) -> bool {
        self.inner.slices.get(self.cursor.slice).is_some()
    }
}

impl BacktrackableReader for ZBufReader<'_> {
    type Mark = ZBufPos;

    fn mark(&mut self) -> Self::Mark {
        self.cursor
    }

    fn rewind(&mut self, mark: Self::Mark) -> bool {
        self.cursor = mark;
        true
    }
}

impl SiphonableReader for ZBufReader<'_> {
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
impl io::Read for ZBufReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match <Self as Reader>::read(self, buf) {
            Ok(n) => Ok(n.get()),
            Err(_) => Ok(0),
        }
    }
}

impl AdvanceableReader for ZBufReader<'_> {
    fn skip(&mut self, offset: usize) -> Result<(), DidntRead> {
        let mut remaining_offset = offset;
        while remaining_offset > 0 {
            let s = self.inner.slices.get(self.cursor.slice).ok_or(DidntRead)?;
            let remains_in_current_slice = s.len() - self.cursor.byte;
            let advance = remaining_offset.min(remains_in_current_slice);
            remaining_offset -= advance;
            self.cursor.byte += advance;
            if self.cursor.byte == s.len() {
                self.cursor.slice += 1;
                self.cursor.byte = 0;
            }
        }
        Ok(())
    }

    fn backtrack(&mut self, offset: usize) -> Result<(), DidntRead> {
        let mut remaining_offset = offset;
        while remaining_offset > 0 {
            let backtrack = remaining_offset.min(self.cursor.byte);
            remaining_offset -= backtrack;
            self.cursor.byte -= backtrack;
            if self.cursor.byte == 0 {
                if self.cursor.slice == 0 {
                    break;
                }
                self.cursor.slice -= 1;
                self.cursor.byte = self
                    .inner
                    .slices
                    .get(self.cursor.slice)
                    .ok_or(DidntRead)?
                    .len();
            }
        }
        if remaining_offset == 0 {
            Ok(())
        } else {
            Err(DidntRead)
        }
    }
}

#[cfg(feature = "std")]
impl io::Seek for ZBufReader<'_> {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        let current_pos = self
            .inner
            .slices()
            .take(self.cursor.slice)
            .fold(0, |acc, s| acc + s.len())
            + self.cursor.byte;
        let current_pos =
            i64::try_from(current_pos).map_err(|e| std::io::Error::other(e.to_string()))?;

        let offset = match pos {
            std::io::SeekFrom::Start(s) => i64::try_from(s).unwrap_or(i64::MAX) - current_pos,
            std::io::SeekFrom::Current(s) => s,
            std::io::SeekFrom::End(s) => self.inner.len() as i64 + s - current_pos,
        };
        match self.advance(offset as isize) {
            Ok(()) => Ok((offset + current_pos) as u64),
            Err(_) => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "InvalidInput",
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
                let slice = slice.subslice(start..end);
                self.reader.cursor.byte = end;
                self.remaining = 0;
                slice
            }
            cmp::Ordering::Equal => {
                let end = start + self.remaining;
                let slice = slice.subslice(start..end);
                self.reader.cursor.slice += 1;
                self.reader.cursor.byte = 0;
                self.remaining = 0;
                slice
            }
            cmp::Ordering::Greater => {
                let end = start + len;
                let slice = slice.subslice(start..end);
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
    inner: NonNull<ZBuf>,
    zslice_writer: Option<ZSliceWriter<'a>>,
}

impl<'a> ZBufWriter<'a> {
    #[inline]
    fn zslice_writer(&mut self) -> &mut ZSliceWriter<'a> {
        // Cannot use `if let` because of  https://github.com/rust-lang/rust/issues/54663
        if self.zslice_writer.is_some() {
            return self.zslice_writer.as_mut().unwrap();
        }
        // SAFETY: `self.inner` is valid as guaranteed by `self.writer` borrow
        let zbuf = unsafe { self.inner.as_mut() };
        zbuf.slices.push(ZSlice::empty());
        self.zslice_writer = zbuf.slices.last_mut().unwrap().writer();
        self.zslice_writer.as_mut().unwrap()
    }
}

impl<'a> HasWriter for &'a mut ZBuf {
    type Writer = ZBufWriter<'a>;

    fn writer(self) -> Self::Writer {
        ZBufWriter {
            inner: NonNull::new(self).unwrap(),
            zslice_writer: self.opt_zslice_writer(),
        }
    }
}

impl Writer for ZBufWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> Result<NonZeroUsize, DidntWrite> {
        self.zslice_writer().write(bytes)
    }

    fn write_exact(&mut self, bytes: &[u8]) -> Result<(), DidntWrite> {
        self.zslice_writer().write_exact(bytes)
    }

    fn remaining(&self) -> usize {
        usize::MAX
    }

    fn write_zslice(&mut self, slice: &ZSlice) -> Result<(), DidntWrite> {
        self.zslice_writer = None;
        // SAFETY: `self.inner` is valid as guaranteed by `self.writer` borrow,
        // and `self.writer` has been overwritten
        unsafe { self.inner.as_mut() }.push_zslice(slice.clone());
        Ok(())
    }

    unsafe fn with_slot<F>(&mut self, len: usize, write: F) -> Result<NonZeroUsize, DidntWrite>
    where
        F: FnOnce(&mut [u8]) -> usize,
    {
        // SAFETY: same precondition as the enclosing function
        self.zslice_writer().with_slot(len, write)
    }
}

impl BacktrackableWriter for ZBufWriter<'_> {
    type Mark = ZBufPos;

    fn mark(&mut self) -> Self::Mark {
        let byte = self.zslice_writer.as_mut().map(|w| w.mark());
        // SAFETY: `self.inner` is valid as guaranteed by `self.writer` borrow
        let zbuf = unsafe { self.inner.as_mut() };
        ZBufPos {
            slice: zbuf.slices.len(),
            byte: byte
                .or_else(|| Some(zbuf.opt_zslice_writer()?.mark()))
                .unwrap_or(0),
        }
    }

    fn rewind(&mut self, mark: Self::Mark) -> bool {
        // SAFETY: `self.inner` is valid as guaranteed by `self.writer` borrow,
        // and `self.writer` is reassigned after modification
        let zbuf = unsafe { self.inner.as_mut() };
        zbuf.slices.truncate(mark.slice);
        self.zslice_writer = zbuf.opt_zslice_writer();
        if let Some(writer) = &mut self.zslice_writer {
            writer.rewind(mark.byte);
        }
        true
    }
}

#[cfg(feature = "std")]
impl io::Write for ZBufWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        match <Self as Writer>::write(self, buf) {
            Ok(n) => Ok(n.get()),
            Err(_) => Err(io::ErrorKind::UnexpectedEof.into()),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(feature = "test")]
impl ZBuf {
    #[doc(hidden)]
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
        zbuf1.push_zslice(slice.subslice(..4).unwrap());
        zbuf1.push_zslice(slice.subslice(4..8).unwrap());

        let mut zbuf2 = ZBuf::empty();
        zbuf2.push_zslice(slice.subslice(..1).unwrap());
        zbuf2.push_zslice(slice.subslice(1..4).unwrap());
        zbuf2.push_zslice(slice.subslice(4..8).unwrap());

        assert_eq!(zbuf1, zbuf2);

        let mut zbuf1 = ZBuf::empty();
        zbuf1.push_zslice(slice.subslice(2..4).unwrap());
        zbuf1.push_zslice(slice.subslice(4..8).unwrap());

        let mut zbuf2 = ZBuf::empty();
        zbuf2.push_zslice(slice.subslice(2..3).unwrap());
        zbuf2.push_zslice(slice.subslice(3..6).unwrap());
        zbuf2.push_zslice(slice.subslice(6..8).unwrap());

        assert_eq!(zbuf1, zbuf2);
    }

    #[cfg(feature = "std")]
    #[test]
    fn zbuf_seek() {
        use std::io::Seek;

        use super::{HasReader, ZBuf};
        use crate::reader::Reader;

        let mut buf = ZBuf::empty();
        buf.push_zslice([0u8, 1u8, 2u8, 3u8].into());
        buf.push_zslice([4u8, 5u8, 6u8, 7u8, 8u8].into());
        buf.push_zslice([9u8, 10u8, 11u8, 12u8, 13u8, 14u8].into());
        let mut reader = buf.reader();

        assert_eq!(reader.stream_position().unwrap(), 0);
        assert_eq!(reader.read_u8().unwrap(), 0);
        assert_eq!(reader.seek(std::io::SeekFrom::Current(6)).unwrap(), 7);
        assert_eq!(reader.read_u8().unwrap(), 7);
        assert_eq!(reader.seek(std::io::SeekFrom::Current(-5)).unwrap(), 3);
        assert_eq!(reader.read_u8().unwrap(), 3);
        assert_eq!(reader.seek(std::io::SeekFrom::Current(10)).unwrap(), 14);
        assert_eq!(reader.read_u8().unwrap(), 14);
        reader.seek(std::io::SeekFrom::Current(100)).unwrap_err();

        assert_eq!(reader.seek(std::io::SeekFrom::Start(0)).unwrap(), 0);
        assert_eq!(reader.read_u8().unwrap(), 0);
        assert_eq!(reader.seek(std::io::SeekFrom::Start(12)).unwrap(), 12);
        assert_eq!(reader.read_u8().unwrap(), 12);
        assert_eq!(reader.seek(std::io::SeekFrom::Start(15)).unwrap(), 15);
        reader.read_u8().unwrap_err();
        reader.seek(std::io::SeekFrom::Start(100)).unwrap_err();

        assert_eq!(reader.seek(std::io::SeekFrom::End(0)).unwrap(), 15);
        reader.read_u8().unwrap_err();
        assert_eq!(reader.seek(std::io::SeekFrom::End(-5)).unwrap(), 10);
        assert_eq!(reader.read_u8().unwrap(), 10);
        assert_eq!(reader.seek(std::io::SeekFrom::End(-15)).unwrap(), 0);
        assert_eq!(reader.read_u8().unwrap(), 0);
        reader.seek(std::io::SeekFrom::End(-20)).unwrap_err();

        assert_eq!(reader.seek(std::io::SeekFrom::Start(10)).unwrap(), 10);
        reader.seek(std::io::SeekFrom::Current(-100)).unwrap_err();
    }
}
