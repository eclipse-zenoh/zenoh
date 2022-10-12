use std::sync::Arc;

use zenoh_collections::SingleOrVec;

use crate::{
    reader::{HasReader, Reader},
    writer::{BacktrackableWriter, Writer},
    SplitBuffer, ZSlice, ZSliceBuffer,
};

#[derive(Debug, Clone, Default)]
pub struct ZBuf {
    slices: SingleOrVec<ZSlice>,
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
        let mut len = 0;
        for slice in self.slices() {
            len += slice.len();
        }
        len
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
    fn read(&mut self, mut into: &mut [u8]) -> usize {
        let mut read = 0;
        let slices = self.inner.slices.as_ref();
        while let Some(slice) = slices.get(self.cursor.slice) {
            let from = &slice.as_ref()[self.cursor.byte..];
            let len = from.len().min(into.len());
            into[..len].copy_from_slice(&from[..len]);
            into = &mut into[len..];
            read += len;
            self.cursor.byte += len;
            if self.cursor.byte == from.len() {
                self.cursor.slice += 1;
                self.cursor.byte = 0;
            }
        }
        read
    }

    fn read_exact(&mut self, into: &mut [u8]) -> bool {
        self.read(into) == into.len()
    }

    fn remaining(&self) -> usize {
        self.inner.slices.as_ref()[self.cursor.slice..]
            .iter()
            .fold(0, |acc, it| acc + it.len())
            - self.cursor.byte
    }

    type ZSliceIterator = ZBufSliceIterator<'a>;

    fn read_zslices(&mut self, len: usize) -> Self::ZSliceIterator {
        let Self { inner, cursor } = *self;
        let mut remaining = (len + cursor.byte) as isize;
        let mut end = cursor;
        for slice in &self.inner.slices.as_ref()[cursor.slice..] {
            remaining -= slice.len() as isize;
            if remaining <= 0 {
                end.byte = (slice.len() as isize + remaining) as usize;
                return ZBufSliceIterator { inner, cursor, end };
            } else {
                end.slice += 1
            }
        }
        ZBufSliceIterator {
            inner,
            cursor,
            end: cursor,
        }
    }

    fn read_zslice(&mut self, len: usize) -> Option<ZSlice> {
        let slice = self.inner.slices.get(self.cursor.slice)?;
        match (slice.len() - self.cursor.byte).cmp(&len) {
            std::cmp::Ordering::Less => {
                let start = self.cursor.byte;
                self.cursor.byte += len;
                slice.new_sub_slice(start, self.cursor.byte)
            }
            std::cmp::Ordering::Equal => {
                self.cursor.slice += 1;
                self.cursor.byte = 0;
                Some(slice.clone())
            }
            std::cmp::Ordering::Greater => {
                self.cursor.slice += 1;
                self.cursor.byte = 0;
                let mut buffer = Vec::with_capacity(len);
                buffer.extend_from_slice(slice.as_slice());
                unsafe {
                    if self.read_exact(std::mem::transmute(buffer.spare_capacity_mut())) {
                        buffer.set_len(len)
                    } else {
                        return None;
                    }
                }
                Some(buffer.into())
            }
        }
    }
}

pub struct ZBufSliceIterator<'a> {
    inner: &'a ZBuf,
    cursor: ZBufPos,
    end: ZBufPos,
}
impl Iterator for ZBufSliceIterator<'_> {
    type Item = ZSlice;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor.slice > self.end.slice {
            return None;
        }
        let mut ret = self.inner.slices[self.cursor.slice].clone();
        if self.cursor.slice == self.end.slice {
            ret.end = ret.start + self.end.byte
        }
        ret.start += self.cursor.byte;
        self.cursor.slice += 1;
        self.cursor.byte = 0;
        Some(ret)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}
impl ExactSizeIterator for ZBufSliceIterator<'_> {
    fn len(&self) -> usize {
        self.end.slice - self.cursor.slice + 1
    }
}
#[derive(Debug)]
pub struct ZBufWriter<'a> {
    inner: &'a mut ZBuf,
    cache: Arc<Vec<u8>>,
}
impl Writer for ZBufWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> Result<usize, crate::writer::DidntWrite> {
        self.write_exact(bytes)?;
        Ok(bytes.len())
    }
    fn write_exact(&mut self, bytes: &[u8]) -> Result<(), crate::writer::DidntWrite> {
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
    fn remaining(&self) -> usize {
        usize::MAX
    }
    fn write_zslice(
        &mut self,
        slice: crate::zslice::ZSlice,
    ) -> Result<(), crate::writer::DidntWrite> {
        self.inner.slices.push(slice);
        Ok(())
    }
    fn with_slot<F: FnOnce(&mut [u8]) -> usize>(
        &mut self,
        mut len: usize,
        f: F,
    ) -> Result<(), crate::writer::DidntWrite> {
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
