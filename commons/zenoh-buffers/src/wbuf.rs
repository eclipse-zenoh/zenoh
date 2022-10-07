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
use crate::buffer::Buffer;
use crate::reader::HasReader;
use crate::traits::buffer::DidntWrite;
use crate::writer::BacktrackableWriter;
use crate::writer::HasWriter;
use crate::writer::Writer;
use crate::zslice::ZSlice;
use crate::SplitBuffer;
use std::fmt;
use std::io;
use std::io::IoSlice;
use std::ops::Bound::{Excluded, Included, Unbounded};
use std::ops::RangeBounds;
use std::sync::Arc;

// Notes:
//  - Wbuf has 2 flavors:
//    - contiguous:
//      - it is a Vec<u8> which is contiguous in memory
//      - it is initialized with a fixed capacity and won't be extended
//      - if a write exceeds capacity, 'false' is returned
//      - the writing of ZSlices makes a copy of the buffer into WBuf
//    - non-contiguous:
//      - it manages a list of slices which could be:
//          - either ZSlices, passed by the user => 0-copy
//          - either slices of an internal Vec<u8> used for serialization
//      - the internal Vec<u8> is initialized with the specified capacity but can be expended
//      - user can get the WBuf content as a list of IoSlices
//      - user can request the copy any sub-part of WBuf into a slice using copy_into_slice()

#[derive(Clone)]
enum Slice {
    External(ZSlice),
    Internal(usize, Option<usize>),
}

impl Slice {
    fn is_external(&self) -> bool {
        match self {
            Slice::External(_) => true,
            Slice::Internal(_, _) => false,
        }
    }
}

impl fmt::Debug for Slice {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Slice::External(s) => write!(f, "ext({})", s.len()),
            Slice::Internal(start, Some(end)) => write!(f, "int({}, {})", start, end),
            Slice::Internal(start, None) => write!(f, "int({}, None)", start),
        }
    }
}

/// A writable zenoh buffer.
#[derive(Clone)]
pub struct WBuf {
    slices: Vec<Slice>,
    buf: Vec<u8>,
    contiguous: bool,
}
#[derive(Debug, Clone)]
pub struct WBufWriter {
    inner: WBuf,
    mark_slice: usize,
    mark_byte: usize,
}
#[derive(Clone)]
pub struct WBufReader<'a> {
    inner: &'a WBuf,
    slice: usize,
    byte: usize,
}

impl WBuf {
    pub fn new(capacity: usize, contiguous: bool) -> WBuf {
        let buf = Vec::with_capacity(capacity);
        let slices = vec![Slice::Internal(0, None)];
        WBuf {
            slices,
            buf,
            contiguous,
        }
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.buf.capacity()
    }

    fn slice_len(&self, s: &Slice) -> usize {
        match s {
            Slice::External(s) => s.len(),
            Slice::Internal(start, Some(end)) => end - start,
            Slice::Internal(start, None) => self.buf.len() - start,
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.slices
            .iter()
            .fold(0, |acc, slice| acc + self.slice_len(slice))
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty() && !self.slices.iter().any(|s| s.is_external())
    }

    pub fn clear(&mut self) {
        self.buf.clear();
        self.slices.clear();
        self.slices.push(Slice::Internal(0, None));
    }

    pub(crate) fn to_zslices(self) -> Vec<ZSlice> {
        let arc_buf = Arc::new(self.buf);
        if self.contiguous {
            if !arc_buf.is_empty() {
                vec![ZSlice::from(arc_buf)]
            } else {
                vec![]
            }
        } else {
            self.slices
                .iter()
                .map(|s| match s {
                    Slice::External(arcs) => arcs.clone(),
                    Slice::Internal(start, Some(end)) => {
                        ZSlice::make(arc_buf.clone().into(), *start, *end).unwrap()
                    }
                    Slice::Internal(start, None) => {
                        ZSlice::make(arc_buf.clone().into(), *start, arc_buf.len()).unwrap()
                    }
                })
                .filter(|s| !s.is_empty())
                .collect()
        }
    }

    pub fn get_first_slice<R>(&self, range: R) -> &[u8]
    where
        R: RangeBounds<usize>,
    {
        if let Some(Slice::Internal(_, _)) = self.slices.first() {
            let len = self.buf.len();
            let start = match range.start_bound() {
                Included(&n) => n,
                Excluded(&n) => n + 1,
                Unbounded => 0,
            };
            let end = match range.end_bound() {
                Included(&n) => n + 1,
                Excluded(&n) => n,
                Unbounded => len,
            };

            &self.buf[start..end]
        } else {
            panic!("Cannot return 1st wlice of WBuf as mutable: it's an external ZSlice");
        }
    }

    pub fn get_first_slice_mut<R>(&mut self, range: R) -> &mut [u8]
    where
        R: RangeBounds<usize>,
    {
        if let Some(Slice::Internal(_, _)) = self.slices.first() {
            let len = self.buf.len();
            let start = match range.start_bound() {
                Included(&n) => n,
                Excluded(&n) => n + 1,
                Unbounded => 0,
            };
            let end = match range.end_bound() {
                Included(&n) => n + 1,
                Excluded(&n) => n,
                Unbounded => len,
            };

            &mut self.buf[start..end]
        } else {
            panic!("Cannot return 1st wlice of WBuf as mutable: it's an external ZSlice");
        }
    }

    #[inline]
    fn can_write_in_buf(&self, size: usize) -> bool {
        // We can write in buf if
        //   - non contiguous
        //   - OR: writing won't exceed buf capacity
        !self.contiguous || self.buf.len() + size <= self.buf.capacity()
    }

    fn write(&mut self, b: u8) -> Result<(), DidntWrite> {
        if self.can_write_in_buf(1) {
            self.buf.push(b);
            Ok(())
        } else {
            Err(DidntWrite)
        }
    }

    // NOTE: this is different from write_zslice() as this makes a copy of bytes into WBuf.
    fn write_bytes(&mut self, s: &[u8]) -> Result<(), DidntWrite> {
        if self.can_write_in_buf(s.len()) {
            self.buf.extend_from_slice(s);
            Ok(())
        } else {
            Err(DidntWrite)
        }
    }

    // NOTE: if not-contiguous, this is 0-copy (the slice is just added to slices list)
    //       otherwise, it's a copy into buf, if doesn't exceed the capacity.
    fn write_zslice(&mut self, zslice: ZSlice) -> Result<(), DidntWrite> {
        if !self.contiguous {
            // If last slice was an internal without end, set it
            if let Some(&mut Slice::Internal(start, None)) = self.slices.last_mut() {
                self.slices.pop();
                self.slices
                    .push(Slice::Internal(start, Some(self.buf.len())));
            }
            // Push the ZSlice in slices list
            self.slices.push(Slice::External(zslice));
            // Push a new internal slice ready for future writes
            self.slices.push(Slice::Internal(self.buf.len(), None));
            Ok(())
        } else if self.buf.len() + zslice.len() <= self.buf.capacity() {
            // Copy the ZSlice into buf
            self.buf.extend_from_slice(zslice.as_slice());
            Ok(())
        } else {
            Err(DidntWrite)
        }
    }
    pub fn reader(&self) -> WBufReader {
        HasReader::reader(self)
    }
}
impl<'a> WBufReader<'a> {
    fn get_zslice_to_copy(&self) -> &[u8] {
        match self.inner.slices.get(self.slice) {
            Some(Slice::External(s)) => s.as_slice(),
            Some(Slice::Internal(start, Some(end))) => &self.inner.buf[*start..*end],
            Some(Slice::Internal(start, None)) => &self.inner.buf[*start..],
            None => panic!(
                "Shouln't happen: copy_pos.0 is out of bound in {:?}",
                self.inner
            ),
        }
    }

    pub fn copy_into_slice(&mut self, dest: &mut [u8]) {
        if self.slice >= self.inner.slices.len() {
            panic!("Not enough bytes to copy into dest");
        }
        let src = self.get_zslice_to_copy();
        let dest_len = dest.len();
        if src.len() - self.byte >= dest_len {
            // Copy a sub-part of src into dest
            let end_pos = self.byte + dest_len;
            dest.copy_from_slice(&src[self.byte..end_pos]);
            // Move copy_pos
            if end_pos < src.len() {
                self.byte = end_pos
            } else {
                self.slice += 1;
                self.byte = 0;
            }
        } else {
            // Copy the remaining of src into dest
            let copy_len = src.len() - self.byte;
            dest[..copy_len].copy_from_slice(&src[self.byte..]);
            // Move copy_pos to next slice and recurse
            self.slice += 1;
            self.byte = 0;
            self.copy_into_slice(&mut dest[copy_len..]);
        }
    }

    pub fn copy_into_wbuf(&mut self, dest: &mut WBuf, dest_len: usize) {
        if self.slice >= self.inner.slices.len() {
            panic!("Not enough bytes to copy into dest");
        }
        let src = self.get_zslice_to_copy();
        if src.len() - self.byte >= dest_len {
            // Copy a sub-part of src into dest
            let end_pos = self.byte + dest_len;
            dest.write_bytes(&src[self.byte..end_pos])
                .expect("Failed to copy bytes into wbuf: destination is probably not big enough");
            // Move copy_pos
            if end_pos < src.len() {
                self.byte = end_pos
            } else {
                self.slice += 1;
                self.byte = 0;
            }
        } else {
            // Copy the remaining of src into dest
            let copy_len = src.len() - self.byte;
            dest.write_bytes(&src[self.byte..])
                .expect("Failed to copy bytes into wbuf: destination is probably not big enough");
            // Move copy_pos to next slice and recurse
            self.slice += 1;
            self.byte = 0;
            self.copy_into_wbuf(dest, dest_len - copy_len);
        }
    }
}
impl HasWriter for WBuf {
    type Writer = WBufWriter;
    fn writer(self) -> Self::Writer {
        self.into()
    }
}
impl Writer for WBufWriter {
    type Buffer = WBuf;
}
impl AsRef<WBuf> for WBufWriter {
    fn as_ref(&self) -> &WBuf {
        &self.inner
    }
}
impl AsMut<WBuf> for WBufWriter {
    fn as_mut(&mut self) -> &mut WBuf {
        &mut self.inner
    }
}
impl WBufWriter {
    pub fn into_inner(self) -> WBuf {
        self.inner
    }
    pub fn clear(&mut self) {
        self.inner.clear();
        self.mark_slice = 1;
        self.mark_byte = 0;
    }
}
impl Buffer for WBufWriter {
    fn write(&mut self, bytes: &[u8]) -> Result<(), DidntWrite> {
        Buffer::write(&mut self.inner, bytes)
    }
    fn write_byte(&mut self, byte: u8) -> Result<(), DidntWrite> {
        Buffer::write_byte(&mut self.inner, byte)
    }
    fn append(&mut self, slice: ZSlice) -> Result<(), DidntWrite> {
        Buffer::append(&mut self.inner, slice)
    }
}
impl BacktrackableWriter for WBufWriter {
    fn mark(&mut self) {
        self.mark_slice = self.inner.slices.len();
        self.mark_byte = self.inner.buf.len();
    }
    fn revert(&mut self) -> bool {
        self.inner.slices.truncate(self.mark_slice);
        match self.inner.slices.last_mut() {
            Some(Slice::Internal(_, end)) => *end = None,
            _ => unreachable!(),
        }
        self.inner.buf.truncate(self.mark_byte);
        true
    }
}
impl From<WBuf> for WBufWriter {
    fn from(inner: WBuf) -> Self {
        WBufWriter {
            inner,
            mark_slice: 1,
            mark_byte: 0,
        }
    }
}
impl io::Write for WBuf {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.write_bytes(buf) {
            Ok(()) => Ok(buf.len()),
            Err(_) => Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "failed to write whole buffer",
            )),
        }
    }

    #[inline]
    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        self.write_bytes(buf)
            .map_err(|_| io::Error::new(io::ErrorKind::WriteZero, "failed to write whole buffer"))
    }

    #[inline]
    fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        let mut nwritten = 0;
        for buf in bufs {
            let len = buf.len();
            self.write_zslice(buf.into()).map_err(|_| {
                io::Error::new(io::ErrorKind::WriteZero, "failed to write whole buffer")
            })?;
            nwritten += len;
        }
        Ok(nwritten)
    }

    //#[inline]
    //fn is_write_vectored(&self) -> bool {
    //    true
    //}

    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl fmt::Display for WBuf {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.contiguous {
            write!(
                f,
                "WBuf{{ contiguous: {}, len: {}, capacity: {} }}",
                self.contiguous,
                self.buf.len(),
                self.buf.capacity()
            )
        } else {
            write!(
                f,
                "WBuf{{ contiguous: {}, buf len: {}, slices: [",
                self.contiguous,
                self.buf.len()
            )?;
            for s in &self.slices {
                write!(f, " {:?}", s)?;
            }
            write!(f, " ] }}")
        }
    }
}

impl fmt::Debug for WBuf {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.contiguous {
            write!(
                f,
                "WBuf{{ contiguous: {}, len: {}, capacity: {},\n  buf: {:02x?}\n}}",
                self.contiguous,
                self.buf.len(),
                self.buf.capacity(),
                self.buf
            )
        } else {
            writeln!(
                f,
                "WBuf{{ contiguous: {}, buf len: {}, slices: [",
                self.contiguous,
                self.buf.len()
            )?;
            for slice in &self.slices {
                match slice {
                    Slice::External(s) => writeln!(f, "  ext{}", s)?,
                    Slice::Internal(start, Some(end)) => {
                        writeln!(f, "  int{:02x?}", &self.buf[*start..*end])?
                    }
                    Slice::Internal(start, None) => {
                        writeln!(f, "  int{:02x?}", &self.buf[*start..])?
                    }
                }
            }
            writeln!(f, "] }}")
        }
    }
}

struct SizedIter<I> {
    iter: I,
    remaining: usize,
}
impl<I: Iterator + Clone> SizedIter<I> {
    fn new(iter: I) -> Self {
        let remaining = iter.clone().count();
        SizedIter { iter, remaining }
    }
}
impl<I: Iterator> Iterator for SizedIter<I>
where
    I::Item: std::fmt::Debug,
{
    type Item = I::Item;
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining > 0 {
            self.remaining -= 1;
            self.iter.next()
        } else {
            None
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}
impl<I: Iterator> ExactSizeIterator for SizedIter<I>
where
    I::Item: std::fmt::Debug,
{
    fn len(&self) -> usize {
        self.remaining
    }
}
pub trait ByteSliceExactIter<'a>: Iterator<Item = &'a [u8]> + ExactSizeIterator {}
//noinspection ALL
impl<'a, T: Iterator<Item = &'a [u8]> + ExactSizeIterator> ByteSliceExactIter<'a> for T {}
impl<'a> crate::traits::SplitBuffer<'a> for WBuf {
    type Slices = Box<dyn ByteSliceExactIter<'a> + 'a>;

    fn slices(&'a self) -> Self::Slices {
        if self.contiguous {
            let slice = self.buf.as_slice();
            Box::new(if slice.is_empty() { None } else { Some(slice) }.into_iter())
        } else {
            let iter = SizedIter::new(self.slices.iter().filter_map(move |s| {
                let slice = match s {
                    Slice::External(arcs) => arcs.as_slice(),
                    Slice::Internal(start, Some(end)) => &self.buf[*start..*end],
                    Slice::Internal(start, None) => &self.buf[*start..],
                };
                if slice.is_empty() {
                    None
                } else {
                    Some(slice)
                }
            }));
            Box::new(iter)
        }
    }
}
impl<'a> crate::traits::reader::HasReader for &'a WBuf {
    type Reader = WBufReader<'a>;
    fn reader(self) -> Self::Reader {
        WBufReader {
            inner: self,
            slice: 0,
            byte: 0,
        }
    }
}
impl<'a> crate::traits::reader::Reader for WBufReader<'a> {
    fn read(&mut self, mut into: &mut [u8]) -> usize {
        let mut read = 0;
        for slice in self.inner.slices().skip(self.slice) {
            let subslice = &slice[self.byte..];
            let copy_len = subslice.len();
            if copy_len <= into.len() {
                let (l, r) = into.split_at_mut(copy_len);
                l.copy_from_slice(subslice);
                self.slice += 1;
                self.byte = 0;
                into = r;
                read += copy_len;
            } else {
                let copy_len = into.len();
                into.copy_from_slice(&subslice[..copy_len]);
                read += copy_len;
                self.byte += copy_len;
                break;
            }
        }
        read
    }
    fn read_exact(&mut self, into: &mut [u8]) -> bool {
        let fork = self.clone();
        if self.read(into) != into.len() {
            *self = fork;
            return false;
        }
        true
    }
    fn remaining(&self) -> usize {
        let mut slices = self.inner.slices().skip(self.slice);
        let mut len = match slices.next() {
            Some(s) => s.len() - self.byte,
            None => 0,
        };
        for slice in slices {
            len += slice.len()
        }
        len
    }
    fn can_read(&self) -> bool {
        self.inner.slices().nth(self.slice).is_some()
    }
    fn read_byte(&mut self) -> Option<u8> {
        let mut byte = 0;
        (self.read(std::slice::from_mut(&mut byte)) != 0).then_some(byte)
    }
    fn read_zslice(&mut self, len: usize) -> Option<ZSlice> {
        match self.inner.slices.get(self.slice)? {
            Slice::External(ZSlice { buf, start, end }) => {
                let start = *start + self.byte;
                let wanted_end = start + len;
                let end = *end;
                let buf = buf.clone();
                if end <= wanted_end {
                    self.slice += 1;
                    self.byte = 0;
                    Some(ZSlice { buf, start, end })
                } else {
                    self.byte += len;
                    Some(ZSlice {
                        buf,
                        start,
                        end: wanted_end,
                    })
                }
            }
            Slice::Internal(start, end) => {
                let mut buf =
                    &self.inner.buf[start + self.byte..end.unwrap_or(self.inner.buf.len())];
                let before = buf.len();
                let ret = buf.read_zslice(len);
                if buf.is_empty() {
                    self.slice += 1;
                    self.byte = 0;
                } else {
                    self.byte += before - buf.len();
                }
                ret
            }
        }
    }
}

impl Buffer for WBuf {
    fn write(&mut self, bytes: &[u8]) -> Result<(), DidntWrite> {
        self.write_bytes(bytes)
    }
    fn write_byte(&mut self, byte: u8) -> Result<(), DidntWrite> {
        self.write(byte)
    }
    fn append(&mut self, slice: ZSlice) -> Result<(), DidntWrite> {
        self.write_zslice(slice)
    }
}

impl crate::traits::buffer::ConstructibleBuffer for WBuf {
    fn with_capacities(slice_capacity: usize, cache_capacity: usize) -> Self {
        let buf = Vec::with_capacity(cache_capacity);
        let mut slices = Vec::with_capacity(slice_capacity);
        slices.push(Slice::Internal(0, None));
        WBuf {
            slices,
            buf,
            contiguous: true,
        }
    }
}
impl crate::traits::buffer::Indexable for WBuf {
    type Index = usize;
    fn get_index(&self) -> Self::Index {
        self.len()
    }

    fn replace(&mut self, _from: &Self::Index, _with: &[u8]) -> bool {
        todo!()
    }
}
impl AsRef<WBuf> for WBuf {
    fn as_ref(&self) -> &WBuf {
        self
    }
}
impl AsMut<WBuf> for WBuf {
    fn as_mut(&mut self) -> &mut WBuf {
        self
    }
}
impl crate::traits::writer::Writer for WBuf {
    type Buffer = Self;
}

#[cfg(test)]
mod tests {
    use crate::reader::Reader;
    use crate::traits::SplitBuffer;

    use super::*;

    macro_rules! to_vec_vec {
        ($wbuf:ident) => {
            $wbuf
                .as_ref()
                .slices()
                .map(|s| s.to_vec())
                .collect::<Vec<Vec<u8>>>()
        };
    }

    macro_rules! vec_vec_is_empty {
        ($wbuf:ident) => {
            $wbuf.as_ref().slices().next().is_none()
        };
    }

    #[test]
    fn wbuf_contiguous_new() {
        let buf = WBuf::new(10, true);
        assert_eq!(buf.len(), 0);
        assert!(buf.is_empty());
        assert_eq!(buf.capacity(), 10);
        assert!(vec_vec_is_empty!(buf));
    }

    #[test]
    fn wbuf_contiguous_write() {
        let mut buf = WBuf::new(2, true);
        assert!(buf.write(0).is_ok());
        assert_eq!(buf.len(), 1);
        assert!(!buf.is_empty());
        assert_eq!(buf.capacity(), 2);
        assert_eq!(to_vec_vec!(buf), [[0]]);

        assert!(buf.write(1).is_ok());
        assert_eq!(buf.len(), 2);
        assert!(!buf.is_empty());
        assert_eq!(buf.capacity(), 2);
        assert_eq!(to_vec_vec!(buf), [[0, 1]]);

        assert!(buf.write(2).is_err());
        assert_eq!(buf.len(), 2);
        assert!(!buf.is_empty());
        assert_eq!(buf.capacity(), 2);
        assert_eq!(to_vec_vec!(buf), [[0, 1]]);

        // also test clear()
        buf.clear();
        assert_eq!(buf.len(), 0);
        assert!(buf.is_empty());
        assert_eq!(buf.capacity(), 2);
        assert!(vec_vec_is_empty!(buf));
    }

    #[test]
    fn wbuf_contiguous_write_bytes() {
        let mut buf = WBuf::new(6, true);
        assert!(buf.write_bytes(&[0, 1, 2]).is_ok());
        assert_eq!(buf.len(), 3);
        assert_eq!(buf.capacity(), 6);
        assert_eq!(to_vec_vec!(buf), [[0, 1, 2]]);

        assert!(buf.write_bytes(&[3, 4]).is_ok());
        assert_eq!(buf.len(), 5);
        assert_eq!(buf.capacity(), 6);
        assert_eq!(to_vec_vec!(buf), [[0, 1, 2, 3, 4]]);

        assert!(buf.write_bytes(&[5, 6]).is_err());
        assert_eq!(buf.len(), 5);
        assert_eq!(buf.capacity(), 6);
        assert_eq!(to_vec_vec!(buf), [[0, 1, 2, 3, 4]]);

        assert!(buf.write_bytes(&[5]).is_ok());
        assert_eq!(buf.len(), 6);
        assert_eq!(buf.capacity(), 6);
        assert_eq!(to_vec_vec!(buf), [[0, 1, 2, 3, 4, 5]]);

        // also test clear()
        buf.clear();
        assert_eq!(buf.len(), 0);
        assert!(buf.is_empty());
        assert_eq!(buf.capacity(), 6);
        assert!(vec_vec_is_empty!(buf));
    }

    #[test]
    fn wbuf_contiguous_write_zslice() {
        let mut buf = WBuf::new(6, true);
        assert!(buf.write_zslice(ZSlice::from(vec![0_u8, 1, 2])).is_ok());
        assert_eq!(buf.len(), 3);
        assert_eq!(buf.capacity(), 6);
        assert_eq!(to_vec_vec!(buf), [[0, 1, 2]]);

        assert!(buf.write_zslice(ZSlice::from(vec![3_u8, 4])).is_ok());
        assert_eq!(buf.len(), 5);
        assert_eq!(buf.capacity(), 6);
        assert_eq!(to_vec_vec!(buf), [[0, 1, 2, 3, 4]]);

        assert!(buf.write_zslice(ZSlice::from(vec![5_u8, 6])).is_err());
        assert_eq!(buf.len(), 5);
        assert_eq!(buf.capacity(), 6);
        assert_eq!(to_vec_vec!(buf), [[0, 1, 2, 3, 4]]);

        assert!(buf.write_zslice(ZSlice::from(vec![5_u8])).is_ok());
        assert_eq!(buf.len(), 6);
        assert_eq!(buf.capacity(), 6);
        assert_eq!(to_vec_vec!(buf), [[0, 1, 2, 3, 4, 5]]);
    }

    #[test]
    fn wbuf_contiguous_get_first_slice_mut() {
        let mut buf = WBuf::new(10, true);
        // reserve 2 bytes writing 0x00
        assert!(buf.write(0).is_ok());
        assert!(buf.write(0).is_ok());

        // write a payload
        assert!(buf.write_bytes(&[1, 2, 3, 4, 5]).is_ok());

        // prepend size in 2 bytes
        let prefix: &mut [u8] = buf.get_first_slice_mut(..2);
        prefix[0] = 5;
        prefix[1] = 0;

        assert_eq!(to_vec_vec!(buf), [[5, 0, 1, 2, 3, 4, 5]]);
    }

    #[test]
    fn wbuf_contiguous_mark_reset() {
        let mut buf = WBuf::new(6, true).writer();
        assert!(buf.write(&[0, 1, 2]).is_ok());
        buf.revert();
        assert!(vec_vec_is_empty!(buf));

        assert!(buf.write(&[0, 1, 2]).is_ok());
        buf.mark();
        assert!(buf.write(&[3, 4]).is_ok());
        buf.revert();
        assert_eq!(to_vec_vec!(buf), [[0, 1, 2]]);

        assert!(buf.write(&[3, 4]).is_ok());
        assert!(buf.write(&[5, 6]).is_ok());
        buf.revert();
        assert_eq!(to_vec_vec!(buf), [[0, 1, 2]]);

        assert!(buf.write(&[3, 4]).is_ok());
        buf.mark();
        assert!(buf.write(&[5, 6]).is_ok());
        buf.revert();
        assert_eq!(to_vec_vec!(buf), [[0, 1, 2, 3, 4]]);
    }

    #[test]
    fn wbuf_contiguous_copy_into_slice() {
        let mut buf = WBuf::new(6, true);
        assert!(buf
            .write_zslice(ZSlice::from(vec![0_u8, 1, 2, 3, 4, 5]))
            .is_ok());
        let mut reader = buf.reader();
        let mut copy = vec![0; 10];
        assert!(reader.read_exact(&mut copy[0..3]));
        assert_eq!(copy, &[0, 1, 2, 0, 0, 0, 0, 0, 0, 0]);
        assert!(reader.read_exact(&mut copy[3..6]));
        assert_eq!(copy, &[0, 1, 2, 3, 4, 5, 0, 0, 0, 0]);
    }

    #[test]
    fn wbuf_contiguous_copy_into_wbuf() {
        let mut buf = WBuf::new(6, true);
        assert!(buf
            .write_zslice(ZSlice::from(vec![0_u8, 1, 2, 3, 4, 5]))
            .is_ok());
        let mut reader = buf.reader();
        let mut copy = WBuf::new(10, true);
        reader.copy_into_wbuf(&mut copy, 3);
        assert_eq!(to_vec_vec!(copy), &[[0, 1, 2,]]);
        reader.copy_into_wbuf(&mut copy, 3);
        assert_eq!(to_vec_vec!(copy), &[[0, 1, 2, 3, 4, 5]]);
    }

    #[test]
    fn wbuf_noncontiguous_new() {
        let buf = WBuf::new(10, false);
        assert_eq!(buf.len(), 0);
        assert!(buf.is_empty());
        assert_eq!(buf.capacity(), 10);
        assert!(vec_vec_is_empty!(buf));
    }

    #[test]
    fn wbuf_noncontiguous_write() {
        let mut buf = WBuf::new(2, false);
        assert!(buf.write(0).is_ok());
        assert_eq!(buf.len(), 1);
        assert!(!buf.is_empty());
        assert_eq!(buf.capacity(), 2);
        assert_eq!(to_vec_vec!(buf), [[0]]);

        assert!(buf.write(1).is_ok());
        assert_eq!(buf.len(), 2);
        assert!(!buf.is_empty());
        assert_eq!(buf.capacity(), 2);
        assert_eq!(to_vec_vec!(buf), [[0, 1]]);

        assert!(buf.write(2).is_ok());
        assert_eq!(buf.len(), 3);
        assert!(!buf.is_empty());
        assert!(buf.capacity() > 2);
        assert_eq!(to_vec_vec!(buf), [[0, 1, 2]]);

        // also test clear()
        buf.clear();
        assert_eq!(buf.len(), 0);
        assert!(buf.is_empty());
        assert!(buf.capacity() > 2);
        assert!(vec_vec_is_empty!(buf));
    }

    #[test]
    fn wbuf_noncontiguous_write_bytes() {
        let mut buf = WBuf::new(6, false);
        assert!(buf.write_bytes(&[0, 1, 2]).is_ok());
        assert_eq!(buf.len(), 3);
        assert_eq!(buf.capacity(), 6);
        assert_eq!(to_vec_vec!(buf), [[0, 1, 2]]);

        assert!(buf.write_bytes(&[3, 4]).is_ok());
        assert_eq!(buf.len(), 5);
        assert_eq!(buf.capacity(), 6);
        assert_eq!(to_vec_vec!(buf), [[0, 1, 2, 3, 4]]);

        assert!(buf.write_bytes(&[5, 6]).is_ok());
        assert_eq!(buf.len(), 7);
        assert!(buf.capacity() > 6);
        assert_eq!(to_vec_vec!(buf), [[0, 1, 2, 3, 4, 5, 6]]);

        // also test clear()
        buf.clear();
        assert_eq!(buf.len(), 0);
        assert!(buf.is_empty());
        assert!(buf.capacity() > 6);
        assert!(vec_vec_is_empty!(buf));
    }

    #[test]
    fn wbuf_noncontiguous_write_zslice() {
        let mut buf = WBuf::new(6, false);
        assert!(buf.write_zslice(ZSlice::from(vec![0_u8, 1, 2])).is_ok());
        assert_eq!(buf.len(), 3);
        assert_eq!(buf.capacity(), 6);
        assert_eq!(to_vec_vec!(buf), [[0, 1, 2]]);

        assert!(buf.write_zslice(ZSlice::from(vec![3_u8, 4])).is_ok());
        assert_eq!(buf.len(), 5);
        assert_eq!(buf.capacity(), 6);
        assert_eq!(to_vec_vec!(buf), [vec![0, 1, 2], vec![3, 4]]);

        assert!(buf.write_zslice(ZSlice::from(vec![5_u8, 6])).is_ok());
        assert_eq!(buf.len(), 7);
        assert_eq!(buf.capacity(), 6);
        assert_eq!(to_vec_vec!(buf), [vec![0, 1, 2], vec![3, 4], vec![5, 6]]);

        assert!(buf.write_zslice(ZSlice::from(vec![7_u8])).is_ok());
        assert_eq!(buf.len(), 8);
        assert_eq!(buf.capacity(), 6);
        assert_eq!(
            to_vec_vec!(buf),
            [vec![0, 1, 2], vec![3, 4], vec![5, 6], vec![7]]
        );
    }

    #[test]
    fn wbuf_noncontiguous_write_mixed() {
        let mut buf = WBuf::new(3, false);
        assert!(buf.write(0).is_ok());
        assert!(buf.write(1).is_ok());
        assert_eq!(to_vec_vec!(buf), [[0, 1]]);

        assert!(buf.write_zslice(ZSlice::from(vec![2_u8, 3, 4])).is_ok());
        assert_eq!(to_vec_vec!(buf), [vec![0, 1], vec![2, 3, 4]]);

        assert!(buf.write(5).is_ok());
        assert!(buf.write_bytes(&[6, 7]).is_ok());
        assert!(buf.write(8).is_ok());
        assert_eq!(
            to_vec_vec!(buf),
            [vec![0, 1], vec![2, 3, 4], vec![5, 6, 7, 8]]
        );

        assert!(buf.write_zslice(ZSlice::from(vec![9_u8, 10, 11])).is_ok());
        assert_eq!(
            to_vec_vec!(buf),
            [vec![0, 1], vec![2, 3, 4], vec![5, 6, 7, 8], vec![9, 10, 11]]
        );
    }

    #[test]
    fn wbuf_noncontiguous_get_first_slice_mut() {
        let mut buf = WBuf::new(10, false);
        // reserve 2 bytes writing 0x00
        assert!(buf.write(0).is_ok());
        assert!(buf.write(0).is_ok());

        // write some bytes
        assert!(buf.write_bytes(&[1, 2, 3, 4, 5]).is_ok());
        // add an ZSlice
        assert!(buf
            .write_zslice(ZSlice::from(vec![6_u8, 7, 8, 9, 10]))
            .is_ok());

        // prepend size in 2 bytes
        let prefix: &mut [u8] = buf.get_first_slice_mut(..2);
        prefix[0] = 10;
        prefix[1] = 0;

        assert_eq!(
            to_vec_vec!(buf),
            [vec![10, 0, 1, 2, 3, 4, 5], vec![6, 7, 8, 9, 10]]
        );
    }

    #[test]
    fn wbuf_noncontiguous_mark_reset() {
        let mut buf = WBuf::new(6, false).writer();
        assert!(buf.write_byte(0).is_ok());
        assert!(buf.write_byte(1).is_ok());
        dbg!(&buf);
        buf.revert();
        assert!(vec_vec_is_empty!(buf));

        assert!(buf.write(&[2, 3]).is_ok());
        dbg!(&buf);
        buf.revert();
        assert!(vec_vec_is_empty!(buf));

        assert!(buf.append(ZSlice::from(vec![0_u8, 1])).is_ok());
        buf.revert();
        assert!(vec_vec_is_empty!(buf));

        assert!(buf.write(&[0, 1, 2]).is_ok());
        buf.mark();
        assert!(buf.write(&[3, 4]).is_ok());
        assert!(buf.append(ZSlice::from(vec![5_u8, 6])).is_ok());
        assert!(buf.write_byte(7).is_ok());
        assert!(buf.append(ZSlice::from(vec![8_u8, 9])).is_ok());
        buf.revert();
        assert_eq!(to_vec_vec!(buf), [[0, 1, 2]]);

        assert!(buf.write(&[3, 4]).is_ok());
        buf.mark();
        assert!(buf.append(ZSlice::from(vec![5_u8, 6])).is_ok());
        assert!(buf.write_byte(7).is_ok());
        assert!(buf.append(ZSlice::from(vec![8_u8, 9])).is_ok());
        buf.revert();
        assert_eq!(to_vec_vec!(buf), [[0, 1, 2, 3, 4]]);

        assert!(buf.append(ZSlice::from(vec![5_u8, 6])).is_ok());
        buf.mark();
        assert!(buf.write_byte(7).is_ok());
        assert!(buf.append(ZSlice::from(vec![8_u8, 9])).is_ok());
        buf.revert();
        assert_eq!(to_vec_vec!(buf), [vec![0, 1, 2, 3, 4], vec![5, 6]]);

        assert!(buf.write_byte(7).is_ok());
        buf.mark();
        assert!(buf.append(ZSlice::from(vec![8_u8, 9])).is_ok());
        buf.revert();
        assert_eq!(to_vec_vec!(buf), [vec![0, 1, 2, 3, 4], vec![5, 6], vec![7]]);

        assert!(buf.append(ZSlice::from(vec![8_u8, 9])).is_ok());
        buf.mark();
        assert!(buf.append(ZSlice::from(vec![10_u8, 11])).is_ok());
        buf.revert();
        assert_eq!(
            to_vec_vec!(buf),
            [vec![0, 1, 2, 3, 4], vec![5, 6], vec![7], vec![8, 9]]
        );
    }

    #[test]
    fn wbuf_noncontiguous_copy_into_slice() {
        let mut buf = WBuf::new(6, false);
        assert!(buf.write(0).is_ok());
        assert!(buf.write(1).is_ok());
        assert!(buf.write_zslice(ZSlice::from(vec![2_u8, 3, 4])).is_ok());
        assert!(buf.write(5).is_ok());
        assert!(buf.write_bytes(&[6, 7]).is_ok());
        assert!(buf.write(8).is_ok());
        assert!(buf.write_zslice(ZSlice::from(vec![9_u8, 10, 11])).is_ok());
        assert_eq!(
            to_vec_vec!(buf),
            [vec![0, 1], vec![2, 3, 4], vec![5, 6, 7, 8], vec![9, 10, 11]]
        );

        let mut reader = buf.reader();
        let mut copy = vec![0; 12];
        reader.copy_into_slice(&mut copy[0..1]);
        assert_eq!(copy, &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        reader.copy_into_slice(&mut copy[1..6]);
        assert_eq!(copy, &[0, 1, 2, 3, 4, 5, 0, 0, 0, 0, 0, 0]);
        reader.copy_into_slice(&mut copy[6..8]);
        assert_eq!(copy, &[0, 1, 2, 3, 4, 5, 6, 7, 0, 0, 0, 0]);
        reader.copy_into_slice(&mut copy[8..12]);
        assert_eq!(copy, &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]);
    }

    #[test]
    fn wbuf_noncontiguous_copy_into_wbuf() {
        let mut buf = WBuf::new(6, false);
        assert!(buf.write(0).is_ok());
        assert!(buf.write(1).is_ok());
        assert!(buf.write_zslice(ZSlice::from(vec![2_u8, 3, 4])).is_ok());
        assert!(buf.write(5).is_ok());
        assert!(buf.write_bytes(&[6, 7]).is_ok());
        assert!(buf.write(8).is_ok());
        assert!(buf.write_zslice(ZSlice::from(vec![9_u8, 10, 11])).is_ok());
        assert_eq!(
            to_vec_vec!(buf),
            [vec![0, 1], vec![2, 3, 4], vec![5, 6, 7, 8], vec![9, 10, 11]]
        );

        let mut reader = buf.reader();
        let mut copy = WBuf::new(12, true);
        reader.copy_into_wbuf(&mut copy, 1);
        assert_eq!(to_vec_vec!(copy), &[[0]]);
        reader.copy_into_wbuf(&mut copy, 5);
        assert_eq!(to_vec_vec!(copy), &[[0, 1, 2, 3, 4, 5]]);
        reader.copy_into_wbuf(&mut copy, 2);
        assert_eq!(to_vec_vec!(copy), &[[0, 1, 2, 3, 4, 5, 6, 7]]);
        reader.copy_into_wbuf(&mut copy, 4);
        assert_eq!(to_vec_vec!(copy), &[[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]]);
    }
}
