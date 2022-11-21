//
// Copyright (c) 2022 ZettaScale Technology
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
use crate::{
    reader::{BacktrackableReader, DidntRead, DidntSiphon, HasReader, Reader, SiphonableReader},
    writer::{BacktrackableWriter, DidntWrite, HasWriter, Writer},
    SplitBuffer, ZSlice, ZSliceBuffer,
};
#[cfg(feature = "shared-memory")]
use crate::{SharedMemoryBuf, SharedMemoryReader};
#[cfg(feature = "shared-memory")]
use std::sync::RwLock;
use std::{num::NonZeroUsize, sync::Arc};
use zenoh_collections::SingleOrVec;
use zenoh_core::zuninitbuff;
#[cfg(feature = "shared-memory")]
use zenoh_core::Result as ZResult;

#[derive(Debug, Clone, Default, Eq)]
pub struct ZBuf {
    slices: SingleOrVec<ZSlice>,
}

impl ZBuf {
    pub fn clear(&mut self) {
        self.slices.clear();
    }

    pub fn zslices(&self) -> impl Iterator<Item = &ZSlice> + '_ {
        self.slices.as_ref().iter()
    }

    pub fn push_zslice(&mut self, zslice: ZSlice) {
        self.slices.push(zslice);
    }

    #[cfg(feature = "shared-memory")]
    pub fn has_shminfo(&self) -> bool {
        self.slices
            .as_ref()
            .iter()
            .any(|s| matches!(&s.buf, ZSliceBuffer::ShmInfo(_)))
    }

    #[cfg(feature = "shared-memory")]
    pub fn map_to_shminfo(&mut self) -> ZResult<bool> {
        let mut res = false;
        for s in self.slices.as_mut().iter_mut() {
            res |= s.map_to_shminfo()?;
        }

        Ok(res)
    }

    #[cfg(feature = "shared-memory")]
    pub fn has_shmbuf(&self) -> bool {
        self.slices
            .as_ref()
            .iter()
            .any(|s| matches!(&s.buf, ZSliceBuffer::ShmBuffer(_)))
    }

    #[cfg(feature = "shared-memory")]
    pub fn map_to_shmbuf(&mut self, shmr: Arc<RwLock<SharedMemoryReader>>) -> ZResult<bool> {
        let mut res = false;
        for s in self.slices.as_mut().iter_mut() {
            res |= s.map_to_shmbuf(shmr.clone())?;
        }
        Ok(res)
    }
}

impl<'a> SplitBuffer<'a> for ZBuf {
    type Slices = std::iter::Map<std::slice::Iter<'a, ZSlice>, fn(&'a ZSlice) -> &'a [u8]>;

    fn slices(&'a self) -> Self::Slices {
        self.slices.as_ref().iter().map(ZSlice::as_slice)
    }

    #[inline(always)]
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline(always)]
    fn len(&self) -> usize {
        self.slices
            .as_ref()
            .iter()
            .fold(0, |len, slice| len + slice.len())
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
                    if l[..cmp_len] != r[..cmp_len] {
                        return false;
                    }
                    if cmp_len == l.len() {
                        current_self = self_slices.next()
                    } else {
                        current_self = Some(&l[cmp_len..])
                    }
                    if cmp_len == r.len() {
                        current_other = other_slices.next()
                    } else {
                        current_other = Some(&r[cmp_len..])
                    }
                }
            }
        }
    }
}

impl From<Vec<u8>> for ZBuf {
    fn from(v: Vec<u8>) -> ZBuf {
        let zs: ZSlice = v.into();
        let mut zbuf = ZBuf::default();
        zbuf.push_zslice(zs);
        zbuf
    }
}

#[cfg(feature = "shared-memory")]
impl From<Arc<SharedMemoryBuf>> for ZBuf {
    fn from(smb: Arc<SharedMemoryBuf>) -> ZBuf {
        let mut zbuf = ZBuf::default();
        zbuf.push_zslice(smb.into());
        zbuf
    }
}

#[cfg(feature = "shared-memory")]
impl From<Box<SharedMemoryBuf>> for ZBuf {
    fn from(smb: Box<SharedMemoryBuf>) -> ZBuf {
        let mut zbuf = ZBuf::default();
        zbuf.push_zslice(smb.into());
        zbuf
    }
}

#[cfg(feature = "shared-memory")]
impl From<SharedMemoryBuf> for ZBuf {
    fn from(smb: SharedMemoryBuf) -> ZBuf {
        let mut zbuf = ZBuf::default();
        zbuf.push_zslice(smb.into());
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
            let from = &slice.as_slice()[self.cursor.byte..];
            // Take the minimum length among read and write slices
            let len = from.len().min(into.len());
            // Copy the slice content
            into[..len].copy_from_slice(&from[..len]);
            // Advance the write slice
            into = &mut into[len..];
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
        self.inner.slices.as_ref()[self.cursor.slice..]
            .iter()
            .fold(0, |acc, it| acc + it.len())
            - self.cursor.byte
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
            std::cmp::Ordering::Less => {
                let mut buffer = zuninitbuff!(len);
                self.read_exact(&mut buffer)?;
                Ok(buffer.into())
            }
            std::cmp::Ordering::Equal => {
                let s = slice
                    .new_sub_slice(self.cursor.byte, slice.len())
                    .ok_or(DidntRead)?;

                self.cursor.slice += 1;
                self.cursor.byte = 0;
                Ok(s)
            }
            std::cmp::Ordering::Greater => {
                let start = self.cursor.byte;
                self.cursor.byte += len;
                slice
                    .new_sub_slice(start, self.cursor.byte)
                    .ok_or(DidntRead)
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
    fn siphon<W>(&mut self, mut writer: W) -> Result<NonZeroUsize, DidntSiphon>
    where
        W: Writer,
    {
        let mut read = 0;
        while let Some(slice) = self.inner.slices.get(self.cursor.slice) {
            // Subslice from the current read slice
            let from = &slice.as_slice()[self.cursor.byte..];
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

        let slice = &self.reader.inner.slices[self.reader.cursor.slice];
        let start = self.reader.cursor.byte;
        let current = &slice[start..];
        let len = current.len();
        match self.remaining.cmp(&len) {
            std::cmp::Ordering::Less => {
                let end = start + self.remaining;
                let slice = slice.new_sub_slice(start, end);
                self.reader.cursor.byte = end;
                self.remaining = 0;
                slice
            }
            std::cmp::Ordering::Equal => {
                let end = start + self.remaining;
                let slice = slice.new_sub_slice(start, end);
                self.reader.cursor.slice += 1;
                self.reader.cursor.byte = 0;
                self.remaining = 0;
                slice
            }
            std::cmp::Ordering::Greater => {
                let end = start + len;
                let slice = slice.new_sub_slice(start, end);
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
        ZBufWriter {
            inner: self,
            cache: Arc::new(vec![]),
        }
    }
}

impl Writer for ZBufWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> Result<NonZeroUsize, DidntWrite> {
        if bytes.is_empty() {
            return Err(DidntWrite);
        }
        self.write_exact(bytes)?;
        // Safety: this operation is safe since we check if bytes is empty
        Ok(unsafe { NonZeroUsize::new_unchecked(bytes.len()) })
    }

    fn write_exact(&mut self, bytes: &[u8]) -> Result<(), DidntWrite> {
        let cache = zenoh_sync::get_mut_unchecked(&mut self.cache);
        let prev_cache_len = cache.len();
        cache.extend_from_slice(bytes);
        let cache_len = cache.len();
        match self.inner.slices.last_mut() {
            Some(ZSlice {
                buf: ZSliceBuffer::NetOwnedBuffer(buf),
                end,
                ..
            }) if *end == prev_cache_len && Arc::ptr_eq(buf, &self.cache) => *end = cache_len,
            _ => self.inner.slices.push(ZSlice {
                buf: ZSliceBuffer::NetOwnedBuffer(self.cache.clone()),
                start: prev_cache_len,
                end: cache_len,
            }),
        }
        Ok(())
    }

    fn write_u8(&mut self, byte: u8) -> Result<(), DidntWrite> {
        self.write_exact(std::slice::from_ref(&byte))
    }

    fn remaining(&self) -> usize {
        usize::MAX
    }

    fn write_zslice(&mut self, slice: &ZSlice) -> Result<(), DidntWrite> {
        self.inner.slices.push(slice.clone());
        Ok(())
    }

    fn with_slot<F>(&mut self, mut len: usize, f: F) -> Result<(), DidntWrite>
    where
        F: FnOnce(&mut [u8]) -> usize,
    {
        let cache = zenoh_sync::get_mut_unchecked(&mut self.cache);
        let prev_cache_len = cache.len();
        cache.reserve(len);
        unsafe {
            len = f(std::mem::transmute(&mut cache.spare_capacity_mut()[..len]));
            cache.set_len(prev_cache_len + len);
        }
        let cache_len = cache.len();
        match self.inner.slices.last_mut() {
            Some(ZSlice {
                buf: ZSliceBuffer::NetOwnedBuffer(buf),
                end,
                ..
            }) if *end == prev_cache_len && Arc::ptr_eq(buf, &self.cache) => *end = cache_len,
            _ => self.inner.slices.push(ZSlice {
                buf: ZSliceBuffer::NetOwnedBuffer(self.cache.clone()),
                start: prev_cache_len,
                end: cache_len,
            }),
        }
        Ok(())
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
            .truncate(mark.slice + (mark.byte != 0) as usize);
        if let Some(slice) = self.inner.slices.last_mut() {
            slice.end = mark.byte
        }
        true
    }
}

// Functions mainly used for testing
impl ZBuf {
    #[doc(hidden)]
    pub fn rand(len: usize) -> Self {
        let mut zbuf = ZBuf::default();
        zbuf.push_zslice(ZSlice::rand(len));
        zbuf
    }
}

mod tests {
    #[test]
    fn zbuf_eq() {
        use super::{ZBuf, ZSlice};

        let slice: ZSlice = [0u8, 1, 2, 3, 4, 5, 6, 7].to_vec().into();

        let mut zbuf1 = ZBuf::default();
        zbuf1.push_zslice(slice.new_sub_slice(0, 4).unwrap());
        zbuf1.push_zslice(slice.new_sub_slice(4, 8).unwrap());

        let mut zbuf2 = ZBuf::default();
        zbuf2.push_zslice(slice.new_sub_slice(0, 1).unwrap());
        zbuf2.push_zslice(slice.new_sub_slice(1, 4).unwrap());
        zbuf2.push_zslice(slice.new_sub_slice(4, 8).unwrap());

        assert_eq!(zbuf1, zbuf2);

        let mut zbuf1 = ZBuf::default();
        zbuf1.push_zslice(slice.new_sub_slice(2, 4).unwrap());
        zbuf1.push_zslice(slice.new_sub_slice(4, 8).unwrap());

        let mut zbuf2 = ZBuf::default();
        zbuf2.push_zslice(slice.new_sub_slice(2, 3).unwrap());
        zbuf2.push_zslice(slice.new_sub_slice(3, 6).unwrap());
        zbuf2.push_zslice(slice.new_sub_slice(6, 8).unwrap());

        assert_eq!(zbuf1, zbuf2);
    }
}
