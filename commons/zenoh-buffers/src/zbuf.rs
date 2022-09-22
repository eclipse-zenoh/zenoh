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
#[cfg(feature = "shared-memory")]
use super::shm::{SharedMemoryBuf, SharedMemoryReader};
use super::ZSlice;
#[cfg(feature = "shared-memory")]
use super::ZSliceBuffer;
use crate::reader::Reader;
use crate::SplitBuffer;
use std::fmt;
use std::io;
use std::io::IoSlice;
use std::num::NonZeroUsize;
#[cfg(feature = "shared-memory")]
use std::sync::{Arc, RwLock};
#[cfg(feature = "shared-memory")]
use zenoh_core::Result as ZResult;

/*************************************/
/*           ZBUF POSITION           */
/*************************************/
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ZBufPos {
    slice: usize, // The ZSlice index
    byte: usize,  // The byte in the ZSlice
    read: usize,  // The amount read so far in the ZBuf
}

/*************************************/
/*            ZBUF INNER             */
/*************************************/
#[derive(Clone)]
enum ZBufInner {
    Single(ZSlice),
    Multiple(Vec<ZSlice>),
    Empty,
}

impl Default for ZBufInner {
    fn default() -> ZBufInner {
        ZBufInner::Empty
    }
}

/*************************************/
/*              ZBUF                 */
/*************************************/
/// A zenoh buffer.
///
/// [`ZBuf`][ZBuf] is a buffer that contains one or more [`ZSlice`][ZSlice]s. It is used
/// to efficiently send and receive data in zenoh. It provides transparent usage for
/// both network and shared memory operations through a simple API.
///
/// By storing a set of [`ZSlice`][ZSlice], it is possible to compose the target payload
/// starting from a set of non-contiguous memory regions. This provides a twofold benefit:
/// (1) the user can compose the payload in an incremental manner without requiring reallocations
/// and (2) the payload is received and recomposed as it arrives from the network without reallocating
/// any receiving buffer.
///
/// Example for creating a data buffer:
/// ```
/// use zenoh_buffers::{ZBuf, ZSlice, traits::SplitBuffer, traits::buffer::InsertBuffer};
///
/// // Create a ZBuf containing a newly allocated vector of bytes.
/// let zbuf: ZBuf = vec![0_u8; 16].into();
/// assert_eq!(&vec![0_u8; 16], zbuf.contiguous().as_ref());
///
/// // Create a ZBuf containing twice a newly allocated vector of bytes.
/// // Allocate first a vectore of bytes and convert it into a ZSlice.
/// let zslice: ZSlice = vec![0_u8; 16].into();
///
/// let mut zbuf = ZBuf::default();
/// zbuf.append(zslice.clone()).unwrap(); // Cloning a ZSlice does not allocate
/// zbuf.append(zslice).unwrap();
///
/// assert_eq!(&vec![0_u8; 32], zbuf.contiguous().as_ref());
/// ```
///
/// Calling [`contiguous()`][ZBuf::contiguous] allows to acces to the whole payload as a contiguous `&[u8]` via the
/// [`ZSlice`][ZSlice] type. However, this operation has a drawback when the original message was large
/// enough to cause network fragmentation. Because of that, the actual message payload may have
/// been received in multiple fragments (i.e. [`ZSlice`][ZSlice]) which are non-contiguous in memory.
///
/// ```
/// use zenoh_buffers::{ZBuf, ZSlice, traits::SplitBuffer, traits::buffer::InsertBuffer};
///
/// // Create a ZBuf containing twice a newly allocated vector of bytes.
/// let zslice: ZSlice = vec![0_u8; 16].into();
/// let mut zbuf = ZBuf::default();
/// zbuf.append(zslice.clone());
///
/// // contiguous() does not allocate since zbuf contains only one slice
/// assert_eq!(&vec![0_u8; 16], zbuf.contiguous().as_ref());
///
/// // Add a second slice to zbuf
/// zbuf.append(zslice.clone());
///
/// // contiguous() allocates since zbuf contains two slices
/// assert_eq!(&vec![0_u8; 32], zbuf.contiguous().as_ref());
/// ```
///
/// [`zslices_num()`][ZBuf::zslices_num] returns the number of [`ZSlice`][ZSlice]s the [`ZBuf`][ZBuf] is composed of. If
/// the returned value is greater than 1, then [`contiguous()`][ZBuf::contiguous] will allocate. In order to retrieve the
/// content of the [`ZBuf`][ZBuf] without allocating, it is possible to loop over its [`ZSlice`][ZSlice]s.
#[derive(Clone, Default)]
pub struct ZBuf {
    slices: ZBufInner,
    len: usize,
    #[cfg(feature = "shared-memory")]
    has_shminfo: bool,
    #[cfg(feature = "shared-memory")]
    has_shmbuf: bool,
}

#[derive(Debug, Clone)]
pub struct ZBufReader<'a> {
    inner: &'a ZBuf,
    read: usize,
    slice: usize,
    byte: usize,
}

impl std::ops::Deref for ZBufReader<'_> {
    type Target = ZBuf;
    fn deref(&self) -> &Self::Target {
        self.inner
    }
}

impl ZBuf {
    fn with_slice_capacity(n: usize) -> ZBuf {
        ZBuf {
            slices: match n {
                0 | 1 => ZBufInner::Empty,
                _ => ZBufInner::Multiple(Vec::with_capacity(n)),
            },
            len: 0,
            #[cfg(feature = "shared-memory")]
            has_shminfo: false,
            #[cfg(feature = "shared-memory")]
            has_shmbuf: false,
        }
    }

    #[inline]
    fn add_zslice(&mut self, slice: ZSlice) {
        #[cfg(feature = "shared-memory")]
        match &slice.buf {
            ZSliceBuffer::ShmInfo(_) => self.has_shminfo = true,
            ZSliceBuffer::ShmBuffer(_) => self.has_shmbuf = true,
            _ => {}
        }

        self.len += slice.len();
        match &mut self.slices {
            ZBufInner::Single(s) => {
                let m = vec![s.clone(), slice];
                self.slices = ZBufInner::Multiple(m);
            }
            ZBufInner::Multiple(m) => {
                m.push(slice);
            }
            ZBufInner::Empty => {
                self.slices = ZBufInner::Single(slice);
            }
        }
    }

    #[inline(always)]
    pub fn get_zslice(&self, index: usize) -> Option<&ZSlice> {
        match &self.slices {
            ZBufInner::Single(s) => match index {
                0 => Some(s),
                _ => None,
            },
            ZBufInner::Multiple(m) => m.get(index),
            ZBufInner::Empty => None,
        }
    }

    #[inline(always)]
    pub fn zslices_num(&self) -> usize {
        match &self.slices {
            ZBufInner::Single(_) => 1,
            ZBufInner::Multiple(m) => m.len(),
            ZBufInner::Empty => 0,
        }
    }

    #[inline(always)]
    pub fn clear(&mut self) {
        self.len = 0;
        self.slices = ZBufInner::Empty;
    }

    // same than read_bytes() but not moving read position (allow non-mutable self)
    fn copy_bytes(&self, bs: &mut [u8], mut pos: (usize, usize)) -> usize {
        let len = bs.len();

        let mut written = 0;
        while written < len {
            if let Some(slice) = self.get_zslice(pos.0) {
                let remaining = slice.len() - pos.1;
                let to_read = remaining.min(bs.len() - written);
                bs[written..written + to_read]
                    .copy_from_slice(&slice.as_slice()[pos.1..pos.1 + to_read]);
                written += to_read;
                pos = (pos.0 + 1, 0);
            } else {
                return written;
            }
        }
        written
    }

    #[cfg(feature = "shared-memory")]
    #[inline(always)]
    pub fn has_shminfo(&self) -> bool {
        self.has_shminfo
    }

    #[cfg(feature = "shared-memory")]
    #[inline(always)]
    pub fn has_shmbuf(&self) -> bool {
        self.has_shmbuf
    }

    #[cfg(feature = "shared-memory")]
    #[inline(never)]
    pub fn map_to_shmbuf(&mut self, shmr: Arc<RwLock<SharedMemoryReader>>) -> ZResult<bool> {
        if !self.has_shminfo() {
            return Ok(false);
        }

        let mut new_len = 0;

        let mut res = false;
        match &mut self.slices {
            ZBufInner::Single(s) => {
                res = s.map_to_shmbuf(shmr)?;
                new_len += s.len();
            }
            ZBufInner::Multiple(m) => {
                for s in m.iter_mut() {
                    res = res || s.map_to_shmbuf(shmr.clone())?;
                    new_len += s.len();
                }
            }
            ZBufInner::Empty => {}
        }
        self.len = new_len;
        self.has_shminfo = false;
        self.has_shmbuf = true;

        Ok(res)
    }

    #[cfg(feature = "shared-memory")]
    #[inline(never)]
    pub fn map_to_shminfo(&mut self) -> ZResult<bool> {
        if !self.has_shmbuf() {
            return Ok(false);
        }

        let mut new_len = 0;

        let mut res = false;
        match &mut self.slices {
            ZBufInner::Single(s) => {
                res = s.map_to_shminfo()?;
                new_len = s.len();
            }
            ZBufInner::Multiple(m) => {
                for s in m.iter_mut() {
                    res = res || s.map_to_shminfo()?;
                    new_len += s.len();
                }
            }
            ZBufInner::Empty => {}
        }
        self.has_shminfo = true;
        self.has_shmbuf = false;
        self.len = new_len;

        Ok(res)
    }
}

impl fmt::Display for ZBuf {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ZBuf{{ content: ",)?;
        match &self.slices {
            ZBufInner::Single(s) => write!(f, "{}", hex::encode_upper(s.as_slice()))?,
            ZBufInner::Multiple(m) => {
                for s in m.iter() {
                    write!(f, "{}", hex::encode_upper(s.as_slice()))?;
                }
            }
            ZBufInner::Empty => {}
        }
        write!(f, " }}")
    }
}

impl fmt::Debug for ZBuf {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        macro_rules! zsliceprint {
            ($slice:expr) => {
                #[cfg(feature = "shared-memory")]
                {
                    match $slice.buf {
                        ZSliceBuffer::NetSharedBuffer(_) => write!(f, " BUF:")?,
                        ZSliceBuffer::NetOwnedBuffer(_) => write!(f, " BUF:")?,
                        ZSliceBuffer::ShmBuffer(_) => write!(f, " SHM_BUF:")?,
                        ZSliceBuffer::ShmInfo(_) => write!(f, " SHM_INFO:")?,
                    }
                }
                #[cfg(not(feature = "shared-memory"))]
                {
                    write!(f, " BUF:")?;
                }
            };
        }

        write!(f, "ZBuf{{ ")?;
        write!(f, "slices: [")?;
        match &self.slices {
            ZBufInner::Single(s) => {
                zsliceprint!(s);
                write!(f, "{}", hex::encode_upper(s.as_slice()))?;
            }
            ZBufInner::Multiple(m) => {
                for s in m.iter() {
                    zsliceprint!(s);
                    write!(f, " {},", hex::encode_upper(s.as_slice()))?;
                }
            }
            ZBufInner::Empty => {
                write!(f, " None")?;
            }
        }
        write!(f, " ] }}")
    }
}

impl<'a> ZBufReader<'a> {
    #[inline(always)]
    pub fn reset(&mut self) {
        self.read = 0;
        self.slice = 0;
        self.byte = 0;
    }
    // Read 'len' bytes from 'self' and add those to 'dest'
    // This is 0-copy, only ZSlices from 'self' are added to 'dest', without cloning the original buffer.
    pub fn read_into_zbuf(&mut self, dest: &mut ZBuf, len: usize) -> bool {
        if self.remaining() < len {
            return false;
        }
        let mut n = len;
        while n > 0 {
            let pos_1 = self.byte;
            let current = self.curr_slice().unwrap();
            let slice_len = current.len();
            let remain_in_slice = slice_len - pos_1;
            let l = n.min(remain_in_slice);
            let zs = match current.new_sub_slice(pos_1, pos_1 + l) {
                Some(zs) => zs,
                None => return false,
            };
            dest.add_zslice(zs);
            self.skip_bytes_no_check(l);
            n -= l;
        }
        true
    }
    // // Read all the bytes from 'self' and add those to 'dest'
    // #[inline(always)]
    // pub(crate) fn drain_into_zbuf(&mut self, dest: &mut ZBuf) -> bool {
    //     self.read_into_zbuf(dest, self.readable())
    // }
    // Read a subslice of current slice
    pub fn read_zslice(&mut self, len: usize) -> Option<ZSlice> {
        let slice = self.curr_slice()?;
        if len <= slice.len() {
            let slice = slice.new_sub_slice(self.byte, self.byte + len)?;
            self.skip_bytes_no_check(len);
            Some(slice)
        } else {
            None
        }
    }
    #[inline(always)]
    fn curr_slice(&self) -> Option<&ZSlice> {
        self.inner.get_zslice(self.slice)
    }
    // #[inline(always)]
    // fn curr_slice_mut(&mut self) -> Option<&mut ZSlice> {
    //     self.inner.get_zslice_mut(self.slice)
    // }
    fn skip_bytes_no_check(&mut self, mut n: usize) {
        while n > 0 {
            let current = self.curr_slice().unwrap();
            let len = current.len();
            if self.byte + n < len {
                self.read += n;
                self.byte += n;
                return;
            } else {
                let read = len - self.byte;
                self.slice += 1;
                self.read += len - self.byte;
                self.byte = 0;
                n -= read;
            }
        }
    }

    pub fn get_pos(&self) -> ZBufPos {
        ZBufPos {
            slice: self.slice,
            byte: self.byte,
            read: self.read,
        }
    }
    pub fn set_pos(&mut self, pos: ZBufPos) {
        assert!(
            pos.read <= self.inner.len,
            "ZBufReader expected to go to {}, but underlying buffer only has {} bytes",
            pos.read,
            self.inner.len
        );
        self.slice = pos.slice;
        self.byte = pos.byte;
        self.read = pos.read;
    }
}

// impl Iterator for ZBuf {
//     type Item = ZSlice;

//     fn next(&mut self) -> Option<Self::Item> {
//         let mut slice = self.curr_slice()?.clone();
//         if self.pos.byte > 0 {
//             slice = slice.new_sub_slice(self.pos.byte, slice.len())?;
//             self.pos.byte = 0;
//         }
//         self.pos.slice += 1;
//         self.pos.read += slice.len();

//         Some(slice)
//     }

//     fn size_hint(&self) -> (usize, Option<usize>) {
//         match &self.slices {
//             ZBufInner::Single(_) => (1, Some(1)),
//             ZBufInner::Multiple(m) => {
//                 let remaining = m.len() - self.pos.slice;
//                 (remaining, Some(remaining))
//             }
//             ZBufInner::Empty => (0, Some(0)),
//         }
//     }
// }

/*************************************/
/*            ZBUF READ              */
/*************************************/
impl<'a> io::Read for ZBufReader<'a> {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        Ok(crate::reader::Reader::read(self, buf))
    }

    #[inline]
    fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<()> {
        if crate::reader::Reader::read_exact(self, buf) {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "failed to fill whole buffer",
            ))
        }
    }
}

/*************************************/
/*            ZBUF FROM              */
/*************************************/
impl From<ZSlice> for ZBuf {
    fn from(slice: ZSlice) -> ZBuf {
        let mut zbuf = ZBuf::with_slice_capacity(1);
        zbuf.add_zslice(slice);
        zbuf
    }
}

impl From<Vec<u8>> for ZBuf {
    fn from(buf: Vec<u8>) -> ZBuf {
        ZBuf::from(ZSlice::from(buf))
    }
}

impl From<Vec<ZSlice>> for ZBuf {
    fn from(mut slices: Vec<ZSlice>) -> ZBuf {
        let mut zbuf = ZBuf::with_slice_capacity(slices.len());
        for slice in slices.drain(..) {
            zbuf.add_zslice(slice);
        }
        zbuf
    }
}

impl<'a> From<Vec<IoSlice<'a>>> for ZBuf {
    fn from(slices: Vec<IoSlice>) -> ZBuf {
        let v: Vec<ZSlice> = slices.iter().map(ZSlice::from).collect();
        ZBuf::from(v)
    }
}

impl From<super::WBuf> for ZBuf {
    fn from(wbuf: super::WBuf) -> ZBuf {
        ZBuf::from(wbuf.to_zslices())
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

#[cfg(feature = "shared-memory")]
impl From<Arc<SharedMemoryBuf>> for ZBuf {
    fn from(smb: Arc<SharedMemoryBuf>) -> ZBuf {
        let mut zbuf = ZBuf::with_slice_capacity(1);
        zbuf.add_zslice(smb.into());
        zbuf
    }
}

#[cfg(feature = "shared-memory")]
impl From<Box<SharedMemoryBuf>> for ZBuf {
    fn from(smb: Box<SharedMemoryBuf>) -> ZBuf {
        let mut zbuf = ZBuf::with_slice_capacity(1);
        zbuf.add_zslice(smb.into());
        zbuf
    }
}

#[cfg(feature = "shared-memory")]
impl From<SharedMemoryBuf> for ZBuf {
    fn from(smb: SharedMemoryBuf) -> ZBuf {
        let mut zbuf = ZBuf::with_slice_capacity(1);
        zbuf.add_zslice(smb.into());
        zbuf
    }
}

type ZSliceIter<'a> = std::slice::Iter<'a, ZSlice>;
impl<'a> crate::traits::SplitBuffer<'a> for ZBuf {
    type Slices = std::iter::Map<ZSliceIter<'a>, fn(&'a ZSlice) -> &'a [u8]>;
    fn slices(&'a self) -> Self::Slices {
        match &self.slices {
            ZBufInner::Single(s) => std::slice::from_ref(s),
            ZBufInner::Multiple(s) => s.as_slice(),
            ZBufInner::Empty => &[],
        }
        .iter()
        .map(ZSlice::as_slice)
    }
    #[inline(always)]
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    #[inline(always)]
    fn len(&self) -> usize {
        self.len
    }
}
impl<'a> crate::traits::reader::Reader for ZBufReader<'a> {
    fn read(&mut self, into: &mut [u8]) -> usize {
        let read = self.inner.copy_bytes(into, (self.slice, self.byte));
        self.skip_bytes_no_check(read);
        read
    }
    fn read_exact(&mut self, into: &mut [u8]) -> bool {
        if self.inner.copy_bytes(into, (self.slice, self.byte)) < into.len() {
            return false;
        }
        self.skip_bytes_no_check(into.len());
        true
    }
    fn read_byte(&mut self) -> Option<u8> {
        if let Some(current) = self.curr_slice() {
            let byte = current[self.byte];
            self.skip_bytes_no_check(1);
            Some(byte)
        } else {
            None
        }
    }
    fn remaining(&self) -> usize {
        self.inner.len - self.read
    }
}
impl<'a> crate::traits::reader::HasReader for &'a ZBuf {
    type Reader = ZBufReader<'a>;
    fn reader(self) -> Self::Reader {
        ZBufReader {
            inner: self,
            read: 0,
            slice: 0,
            byte: 0,
        }
    }
}
impl crate::traits::buffer::ConstructibleBuffer for ZBuf {
    fn with_capacities(slice_capacity: usize, _cache_capacity: usize) -> Self {
        ZBuf::with_slice_capacity(slice_capacity)
    }
}

impl ZBuf {
    fn append_zslice(&mut self, slice: ZSlice) -> Option<NonZeroUsize> {
        let len = slice.len();
        if len > 0 {
            self.add_zslice(slice);
            Some(unsafe { NonZeroUsize::new_unchecked(len) })
        } else {
            None
        }
    }
}
impl<T: Into<ZSlice>> crate::traits::buffer::InsertBuffer<T> for ZBuf {
    fn append(&mut self, slice: T) -> Option<NonZeroUsize> {
        self.append_zslice(slice.into())
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        buffer::ConstructibleBuffer,
        reader::{HasReader, Reader},
        SplitBuffer,
    };

    use super::*;

    #[test]
    fn test_zbuf() {
        let v1 = ZSlice::from(vec![0_u8, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
        let v2 = ZSlice::from(vec![10_u8, 11, 12, 13, 14, 15, 16, 17, 18, 19]);
        let v3 = ZSlice::from(vec![20_u8, 21, 22, 23, 24, 25, 26, 27, 28, 29]);

        // test a 1st buffer
        let mut buf1 = ZBuf::with_capacities(0, 0);
        assert!(buf1.is_empty());
        assert_eq!(0, buf1.len());
        assert_eq!(0, buf1.slices().len());

        buf1.add_zslice(v1.clone());
        println!("[01] {:?}", buf1);
        assert!(!buf1.is_empty());
        assert_eq!(10, buf1.len());
        assert_eq!(1, buf1.slices().len());
        assert_eq!(
            Some(&[0_u8, 1, 2, 3, 4, 5, 6, 7, 8, 9][..]),
            buf1.slices().collect::<Vec<_>>()[0].get(0..10)
        );

        buf1.add_zslice(v2.clone());
        println!("[02] {:?}", buf1);
        assert!(!buf1.is_empty());
        assert_eq!(20, buf1.len());
        assert_eq!(2, buf1.slices().len());
        assert_eq!(
            Some(&[10_u8, 11, 12, 13, 14, 15, 16, 17, 18, 19][..]),
            buf1.slices().collect::<Vec<_>>()[1].get(0..10)
        );

        buf1.add_zslice(v3);
        println!("[03] {:?}", buf1);
        assert!(!buf1.is_empty());
        assert_eq!(30, buf1.len());
        assert_eq!(3, buf1.slices().len());
        assert_eq!(
            Some(&[20_u8, 21, 22, 23, 24, 25, 26, 27, 28, 29][..]),
            buf1.slices().collect::<Vec<_>>()[2].get(0..10)
        );

        // test PartialEq
        let v4 = vec![
            0_u8, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22,
            23, 24, 25, 26, 27, 28, 29,
        ];
        assert_eq!(buf1, ZBuf::from(v4));

        // test read
        let mut buf1_reader = buf1.reader();
        for i in 0..buf1_reader.remaining() - 1 {
            assert_eq!(i as u8, buf1_reader.read_byte().unwrap());
        }
        assert!(buf1_reader.can_read());

        // test reset
        buf1_reader.reset();
        println!("[04] {:?}", buf1_reader);
        assert!(!buf1_reader.is_empty());
        assert!(buf1_reader.can_read());
        assert_eq!(30, buf1_reader.remaining());
        assert_eq!(30, buf1_reader.len());
        assert_eq!(3, buf1_reader.slices().len());

        // test set_pos / get_pos
        buf1_reader.reset();
        println!("[05] {:?}", buf1_reader);
        assert_eq!(30, buf1_reader.remaining());
        let mut bytes = [0_u8; 10];
        assert!(buf1_reader.read_exact(&mut bytes));
        assert_eq!(20, buf1_reader.remaining());
        let pos = buf1_reader.get_pos();
        assert!(buf1_reader.read_exact(&mut bytes));
        assert_eq!(10, buf1_reader.remaining());
        buf1_reader.set_pos(pos);
        assert_eq!(20, buf1_reader.remaining());
        assert!(buf1_reader.read_exact(&mut bytes));
        assert_eq!(10, buf1_reader.remaining());
        assert!(buf1_reader.read_exact(&mut bytes));
        assert_eq!(0, buf1_reader.remaining());
        let pos = buf1_reader.get_pos();
        buf1_reader.set_pos(pos);

        // test read_bytes
        buf1_reader.reset();
        println!("[06] {:?}", buf1_reader);
        let mut bytes = [0_u8; 3];
        for i in 0..10 {
            assert!(buf1_reader.read_exact(&mut bytes));
            println!(
                "[06][{}] {:?} Bytes: {:?}",
                i,
                buf1_reader,
                hex::encode_upper(bytes)
            );
            assert_eq!([i * 3, i * 3 + 1, i * 3 + 2], bytes);
        }

        // test other buffers sharing the same vecs
        let mut buf2 = ZBuf::from(v1.clone());
        buf2.add_zslice(v2);
        println!("[07] {:?}", buf1_reader);
        assert!(!buf2.is_empty());
        assert_eq!(20, buf2.len());
        assert_eq!(buf2.len(), buf2.reader().remaining());
        assert_eq!(2, buf2.slices().len());
        let mut buf2_reader = buf2.reader();
        for i in 0..buf2.len() - 1 {
            assert_eq!(i as u8, buf2_reader.read_byte().unwrap());
        }

        let buf3 = ZBuf::from(v1);
        println!("[08] {:?}", buf1_reader);
        assert!(!buf3.is_empty());
        let mut buf3 = buf3.reader();
        assert!(buf3.can_read());
        assert_eq!(0, buf3.get_pos().read);
        assert_eq!(10, buf3.remaining());
        assert_eq!(10, buf3.len());
        assert_eq!(1, buf3.slices().len());
        for i in 0..buf3.len() - 1 {
            assert_eq!(i as u8, buf3.read_byte().unwrap());
        }

        // test read_into_zbuf
        buf1_reader.reset();
        println!("[09] {:?}", buf1_reader);
        let _ = buf1_reader.read_byte();
        let mut dest = ZBuf::with_capacities(0, 0);
        assert!(buf1_reader.read_into_zbuf(&mut dest, 24));
        let dest_slices = dest.slices().collect::<Vec<_>>();
        assert_eq!(3, dest_slices.len());
        assert_eq!(
            Some(&[1_u8, 2, 3, 4, 5, 6, 7, 8, 9][..]),
            dest_slices[0].get(..)
        );
        assert_eq!(
            Some(&[10_u8, 11, 12, 13, 14, 15, 16, 17, 18, 19][..]),
            dest_slices[1].get(..)
        );
        assert_eq!(Some(&[20_u8, 21, 22, 23, 24][..]), dest_slices[2].get(..));

        // test drain_into_zbuf
        // buf1.reset();
        // println!("[10] {:?}", buf1);
        // let mut dest = ZBuf::default();
        // assert!(buf1.drain_into_zbuf(&mut dest));
        // assert_eq!(buf1.readable(), 0);
        // assert_eq!(buf1.len(), dest.readable());
    }
}
