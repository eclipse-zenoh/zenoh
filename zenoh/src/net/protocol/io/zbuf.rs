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
#[cfg(feature = "shared-memory")]
use super::shm::{SharedMemoryBuf, SharedMemoryReader};
use super::ZSlice;
#[cfg(feature = "shared-memory")]
use super::ZSliceBuffer;
use std::fmt;
use std::io;
use std::io::IoSlice;
#[cfg(feature = "shared-memory")]
use std::sync::{Arc, RwLock};
#[cfg(feature = "shared-memory")]
use zenoh_util::core::Result as ZResult;

/*************************************/
/*           ZBUF POSITION           */
/*************************************/
#[derive(Clone, Copy, PartialEq, Default)]
pub struct ZBufPos {
    slice: usize, // The ZSlice index
    byte: usize,  // The byte in the ZSlice
    len: usize,   // The total lenght in bytes of the ZBuf
    read: usize,  // The amount read so far in the ZBuf
}

impl ZBufPos {
    #[inline(always)]
    fn reset(&mut self) {
        self.slice = 0;
        self.byte = 0;
        self.read = 0;
    }

    #[inline(always)]
    fn clear(&mut self) {
        self.reset();
        self.len = 0;
    }
}

impl fmt::Display for ZBufPos {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.read)
    }
}

impl fmt::Debug for ZBufPos {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "(Slice: {}, Byte: {}, Len: {}, Read: {})",
            self.slice, self.byte, self.len, self.read
        )
    }
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
/// starting from a set of non-contigous memory regions. This provides a twofold benefit:
/// (1) the user can compose the payload in an incremental manner without requiring reallocations
/// and (2) the payload is received and recomposed as it arrives from the network without reallocating
/// any receiving buffer.
///
/// Example for creating a data buffer:
/// ```
/// use zenoh::buf::{ZBuf, ZSlice};
///
/// // Create a ZBuf containing a newly allocated vector of bytes.
/// let zbuf: ZBuf = vec![0_u8; 16].into();
/// assert_eq!(&vec![0_u8; 16], zbuf.contiguous().as_slice());
///
/// // Create a ZBuf containing twice a newly allocated vector of bytes.
/// // Allocate first a vectore of bytes and convert it into a ZSlice.
/// let zslice: ZSlice = vec![0_u8; 16].into();
/// let mut zbuf = ZBuf::new();
/// zbuf.add_zslice(zslice.clone()); // Cloning a ZSlice does not allocate
/// zbuf.add_zslice(zslice);
///
/// assert_eq!(&vec![0_u8; 32], zbuf.contiguous().as_slice());
/// ```
///
/// Calling [`contiguous()`][ZBuf::contiguous] allows to acces to the whole payload as a contigous `&[u8]` via the
/// [`ZSlice`][ZSlice] type. However, this operation has a drawback when the original message was large
/// enough to cause network fragmentation. Because of that, the actual message payload may have
/// been received in multiple fragments (i.e. [`ZSlice`][ZSlice]) which are non-contigous in memory.
///
/// ```
/// use zenoh::buf::{ZBuf, ZSlice};
///
/// // Create a ZBuf containing twice a newly allocated vector of bytes.
/// let zslice: ZSlice = vec![0_u8; 16].into();
/// let mut zbuf = ZBuf::new();
/// zbuf.add_zslice(zslice.clone());
///
/// // contiguous() does not allocate since zbuf contains only one slice
/// assert_eq!(&vec![0_u8; 16], zbuf.contiguous().as_slice());
///
/// // Add a second slice to zbuf
/// zbuf.add_zslice(zslice.clone());
///
/// // contiguous() allocates since zbuf contains two slices
/// assert_eq!(&vec![0_u8; 32], zbuf.contiguous().as_slice());
/// ```
///
/// [`zslices_num()`][ZBuf::zslices_num] returns the number of [`ZSlice`][ZSlice]s the [`ZBuf`][ZBuf] is composed of. If
/// the returned value is greater than 1, then [`contiguous()`][ZBuf::contiguous] will allocate. In order to retrieve the
/// content of the [`ZBuf`][ZBuf] without allocating, it is possible to loop over its [`ZSlice`][ZSlice]s.
/// Iterating over the payload ensures that no dynamic allocations are performed. This is really useful when
/// dealing with shared memory access or with large data that has been likely fragmented on the network.
///
/// ```
/// use zenoh::buf::{ZBuf, ZSlice};
///
/// let zslice: ZSlice = vec![0_u8; 16].into();
///
/// let mut zbuf = ZBuf::new();
/// zbuf.add_zslice(zslice.clone());
/// zbuf.add_zslice(zslice.clone());
/// zbuf.add_zslice(zslice);
///
/// assert_eq!(zbuf.zslices_num(), 3);
/// for z in zbuf.next() {
///     assert_eq!(z.len(), 16);
/// }
/// ```
#[derive(Clone, Default)]
pub struct ZBuf {
    slices: ZBufInner,
    pos: ZBufPos,
    #[cfg(feature = "shared-memory")]
    has_shminfo: bool,
    #[cfg(feature = "shared-memory")]
    has_shmbuf: bool,
}

impl ZBuf {
    pub fn new() -> ZBuf {
        ZBuf {
            slices: ZBufInner::default(),
            pos: ZBufPos::default(),
            #[cfg(feature = "shared-memory")]
            has_shminfo: false,
            #[cfg(feature = "shared-memory")]
            has_shmbuf: false,
        }
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    pub fn add_zslice(&mut self, slice: ZSlice) {
        #[cfg(feature = "shared-memory")]
        match &slice.buf {
            ZSliceBuffer::ShmInfo(_) => self.has_shminfo = true,
            ZSliceBuffer::ShmBuffer(_) => self.has_shmbuf = true,
            _ => {}
        }

        self.pos.len += slice.len();
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
    pub fn get_zslice_mut(&mut self, index: usize) -> Option<&mut ZSlice> {
        match &mut self.slices {
            ZBufInner::Single(s) => match index {
                0 => Some(s),
                _ => None,
            },
            ZBufInner::Multiple(m) => m.get_mut(index),
            ZBufInner::Empty => None,
        }
    }

    pub fn as_zslices(&self) -> Vec<ZSlice> {
        match &self.slices {
            ZBufInner::Single(s) => vec![s.clone()],
            ZBufInner::Multiple(m) => m.clone(),
            ZBufInner::Empty => vec![],
        }
    }

    pub fn as_ioslices(&self) -> Vec<IoSlice> {
        match &self.slices {
            ZBufInner::Single(s) => vec![s.as_ioslice()],
            ZBufInner::Multiple(m) => m.iter().map(|s| s.as_ioslice()).collect(),
            ZBufInner::Empty => vec![],
        }
    }

    #[inline(always)]
    pub fn clear(&mut self) {
        self.pos.clear();
        self.slices = ZBufInner::Empty;
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.pos.len
    }

    #[inline(always)]
    pub fn can_read(&self) -> bool {
        self.readable() > 0
    }

    #[inline(always)]
    pub fn readable(&self) -> usize {
        self.pos.len - self.pos.read
    }

    #[inline(always)]
    pub fn reset(&mut self) {
        self.pos.reset();
    }

    #[inline]
    pub(crate) fn set_pos(&mut self, pos: ZBufPos) -> bool {
        if pos == self.pos {
            return true;
        }

        if let Some(slice) = self.get_zslice(pos.slice) {
            if pos.byte < slice.len() {
                self.pos = pos;
                return true;
            }
        }

        false
    }

    #[inline(always)]
    pub(crate) fn get_pos(&self) -> ZBufPos {
        self.pos
    }

    #[inline(always)]
    fn curr_slice(&self) -> Option<&ZSlice> {
        self.get_zslice(self.pos.slice)
    }

    #[inline(always)]
    fn curr_slice_mut(&mut self) -> Option<&mut ZSlice> {
        self.get_zslice_mut(self.pos.slice)
    }

    fn skip_bytes_no_check(&mut self, mut n: usize) {
        while n > 0 {
            let current = self.curr_slice().unwrap();
            let len = current.len();
            if self.pos.byte + n < len {
                self.pos.read += n;
                self.pos.byte += n;
                return;
            } else {
                let read = len - self.pos.byte;
                self.pos.slice += 1;
                self.pos.read += len - self.pos.byte;
                self.pos.byte = 0;
                n -= read;
            }
        }
    }

    // same than read_bytes() but not moving read position (allow non-mutable self)
    fn copy_bytes(&self, bs: &mut [u8], mut pos: (usize, usize)) -> bool {
        let len = bs.len();
        if self.readable() < len {
            return false;
        }

        let mut written = 0;
        while written < len {
            let slice = self.get_zslice(pos.0).unwrap();
            let remaining = slice.len() - pos.1;
            let to_read = remaining.min(bs.len() - written);
            bs[written..written + to_read]
                .copy_from_slice(&slice.as_slice()[pos.1..pos.1 + to_read]);
            written += to_read;
            pos = (pos.0 + 1, 0);
        }
        true
    }

    // same than read() but not moving read position (allow not mutable self)
    #[inline(always)]
    pub fn get(&self) -> Option<u8> {
        self.curr_slice().map(|current| current[self.pos.byte])
    }

    #[inline(always)]
    pub fn read(&mut self) -> Option<u8> {
        let res = self.get();
        if res.is_some() {
            self.skip_bytes_no_check(1);
        }
        res
    }

    #[inline(always)]
    pub fn read_bytes(&mut self, bs: &mut [u8]) -> bool {
        if !self.copy_bytes(bs, (self.pos.slice, self.pos.byte)) {
            return false;
        }
        self.skip_bytes_no_check(bs.len());
        true
    }

    #[inline(always)]
    pub fn skip_bytes(&mut self, len: usize) -> bool {
        if self.readable() >= len {
            self.skip_bytes_no_check(len);
            true
        } else {
            false
        }
    }

    #[inline(always)]
    pub fn read_vec(&mut self) -> Vec<u8> {
        let mut vec = vec![0_u8; self.readable()];
        self.read_bytes(&mut vec);
        vec
    }

    // returns a Vec<u8> containing a copy of ZBuf content (not considering read position)
    #[inline(always)]
    pub fn to_vec(&self) -> Vec<u8> {
        let mut vec = vec![0_u8; self.len()];
        self.copy_bytes(&mut vec[..], (0, 0));
        vec
    }

    /// Returns a [`ZSlice`][ZSlice].
    /// This operation will allocate in case the [`ZBuf`][ZBuf] is composed of multiple [`ZSlice`][ZSlice]s.
    /// [`zslices_num()`][ZBuf::zslices_num] returns the number of [`ZSlice`][ZSlice] in the [`ZBuf`][ZBuf].
    #[inline]
    pub fn contiguous(&self) -> ZSlice {
        match &self.slices {
            ZBufInner::Single(s) => s.clone(),
            ZBufInner::Multiple(_) => self.to_vec().into(),
            ZBufInner::Empty => vec![].into(),
        }
    }

    // Read 'len' bytes from 'self' and add those to 'dest'
    // This is 0-copy, only ZSlices from 'self' are added to 'dest', without cloning the original buffer.
    pub(crate) fn read_into_zbuf(&mut self, dest: &mut ZBuf, len: usize) -> bool {
        if self.readable() < len {
            return false;
        }

        let mut n = len;
        while n > 0 {
            let pos_1 = self.pos.byte;
            let current = self.curr_slice_mut().unwrap();
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
    pub(crate) fn read_zslice(&mut self, len: usize) -> Option<ZSlice> {
        let slice = self.curr_slice()?;
        if len <= slice.len() {
            let slice = slice.new_sub_slice(self.pos.byte, self.pos.byte + len)?;
            self.skip_bytes_no_check(len);
            Some(slice)
        } else {
            None
        }
    }

    #[cfg(feature = "shared-memory")]
    #[inline(never)]
    pub(crate) fn map_to_shmbuf(&mut self, shmr: Arc<RwLock<SharedMemoryReader>>) -> ZResult<bool> {
        if !self.has_shminfo() {
            return Ok(false);
        }

        self.pos.clear();

        let mut res = false;
        match &mut self.slices {
            ZBufInner::Single(s) => {
                res = s.map_to_shmbuf(shmr)?;
                self.pos.len += s.len();
            }
            ZBufInner::Multiple(m) => {
                for s in m.iter_mut() {
                    res = res || s.map_to_shmbuf(shmr.clone())?;
                    self.pos.len += s.len();
                }
            }
            ZBufInner::Empty => {}
        }
        self.has_shminfo = false;
        self.has_shmbuf = true;

        Ok(res)
    }

    #[cfg(feature = "shared-memory")]
    #[inline(never)]
    pub(crate) fn map_to_shminfo(&mut self) -> ZResult<bool> {
        if !self.has_shmbuf() {
            return Ok(false);
        }

        self.pos.clear();

        let mut res = false;
        match &mut self.slices {
            ZBufInner::Single(s) => {
                res = s.map_to_shminfo()?;
                self.pos.len = s.len();
            }
            ZBufInner::Multiple(m) => {
                for s in m.iter_mut() {
                    res = res || s.map_to_shminfo()?;
                    self.pos.len += s.len();
                }
            }
            ZBufInner::Empty => {}
        }
        self.has_shminfo = true;
        self.has_shmbuf = false;

        Ok(res)
    }

    #[cfg(feature = "shared-memory")]
    #[inline(always)]
    pub(crate) fn has_shminfo(&self) -> bool {
        self.has_shminfo
    }

    #[cfg(feature = "shared-memory")]
    #[inline(always)]
    pub(crate) fn has_shmbuf(&self) -> bool {
        self.has_shmbuf
    }
}

impl fmt::Display for ZBuf {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ZBuf{{ pos: {:?}, content: ", self.get_pos(),)?;
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

        write!(f, "ZBuf{{ pos: {:?}, ", self.pos)?;
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

impl Iterator for ZBuf {
    type Item = ZSlice;

    fn next(&mut self) -> Option<Self::Item> {
        let mut slice = self.curr_slice()?.clone();
        if self.pos.byte > 0 {
            slice = slice.new_sub_slice(self.pos.byte, slice.len())?;
            self.pos.byte = 0;
        }
        self.pos.slice += 1;
        self.pos.read += slice.len();

        Some(slice)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match &self.slices {
            ZBufInner::Single(_) => (1, Some(1)),
            ZBufInner::Multiple(m) => {
                let remaining = m.len() - self.pos.slice;
                (remaining, Some(remaining))
            }
            ZBufInner::Empty => (0, Some(0)),
        }
    }
}

/*************************************/
/*            ZBUF READ              */
/*************************************/
impl io::Read for ZBuf {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let remaining = self.readable();
        if remaining > buf.len() {
            self.read_bytes(buf);
            Ok(buf.len())
        } else {
            self.read_bytes(&mut buf[0..remaining]);
            Ok(remaining)
        }
    }

    #[inline]
    fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<()> {
        if self.read_bytes(buf) {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "failed to fill whole buffer",
            ))
        }
    }

    #[inline]
    fn read_vectored(&mut self, bufs: &mut [io::IoSliceMut<'_>]) -> io::Result<usize> {
        let mut nread = 0;
        for buf in bufs {
            nread += (self as &mut dyn io::Read).read(buf)?;
            if self.is_empty() {
                break;
            }
        }
        Ok(nread)
    }
}

/*************************************/
/*            ZBUF FROM              */
/*************************************/
impl From<ZSlice> for ZBuf {
    fn from(slice: ZSlice) -> ZBuf {
        let mut zbuf = ZBuf::new();
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
        let mut zbuf = ZBuf::new();
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
        let mut zbuf1 = self.clone();
        zbuf1.reset();
        let mut zbuf2 = other.clone();
        zbuf2.reset();

        while let Some(b1) = zbuf1.read() {
            if let Some(b2) = zbuf2.read() {
                if b1 != b2 {
                    return false;
                }
            } else {
                return false;
            }
        }
        // Check there are no bytes left in zbuf2
        zbuf2.read().is_none()
    }
}

#[cfg(feature = "shared-memory")]
impl From<Arc<SharedMemoryBuf>> for ZBuf {
    fn from(smb: Arc<SharedMemoryBuf>) -> ZBuf {
        let mut zbuf = ZBuf::new();
        zbuf.add_zslice(smb.into());
        zbuf
    }
}

#[cfg(feature = "shared-memory")]
impl From<Box<SharedMemoryBuf>> for ZBuf {
    fn from(smb: Box<SharedMemoryBuf>) -> ZBuf {
        let mut zbuf = ZBuf::new();
        zbuf.add_zslice(smb.into());
        zbuf
    }
}

#[cfg(feature = "shared-memory")]
impl From<SharedMemoryBuf> for ZBuf {
    fn from(smb: SharedMemoryBuf) -> ZBuf {
        let mut zbuf = ZBuf::new();
        zbuf.add_zslice(smb.into());
        zbuf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zbuf() {
        let v1 = ZSlice::from(vec![0_u8, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
        let v2 = ZSlice::from(vec![10_u8, 11, 12, 13, 14, 15, 16, 17, 18, 19]);
        let v3 = ZSlice::from(vec![20_u8, 21, 22, 23, 24, 25, 26, 27, 28, 29]);

        // test a 1st buffer
        let mut buf1 = ZBuf::new();
        assert!(buf1.is_empty());
        assert!(!buf1.can_read());
        assert_eq!(0, buf1.get_pos().read);
        assert_eq!(0, buf1.readable());
        assert_eq!(0, buf1.len());
        assert_eq!(0, buf1.as_ioslices().len());

        buf1.add_zslice(v1.clone());
        println!("[01] {:?}", buf1);
        assert!(!buf1.is_empty());
        assert!(buf1.can_read());
        assert_eq!(0, buf1.get_pos().read);
        assert_eq!(10, buf1.readable());
        assert_eq!(10, buf1.len());
        assert_eq!(1, buf1.as_ioslices().len());
        assert_eq!(
            Some(&[0_u8, 1, 2, 3, 4, 5, 6, 7, 8, 9][..]),
            buf1.as_ioslices()[0].get(0..10)
        );

        buf1.add_zslice(v2.clone());
        println!("[02] {:?}", buf1);
        assert!(!buf1.is_empty());
        assert!(buf1.can_read());
        assert_eq!(0, buf1.get_pos().read);
        assert_eq!(20, buf1.readable());
        assert_eq!(20, buf1.len());
        assert_eq!(2, buf1.as_ioslices().len());
        assert_eq!(
            Some(&[10_u8, 11, 12, 13, 14, 15, 16, 17, 18, 19][..]),
            buf1.as_ioslices()[1].get(0..10)
        );

        buf1.add_zslice(v3);
        println!("[03] {:?}", buf1);
        assert!(!buf1.is_empty());
        assert!(buf1.can_read());
        assert_eq!(0, buf1.get_pos().read);
        assert_eq!(30, buf1.readable());
        assert_eq!(30, buf1.len());
        assert_eq!(3, buf1.as_ioslices().len());
        assert_eq!(
            Some(&[20_u8, 21, 22, 23, 24, 25, 26, 27, 28, 29][..]),
            buf1.as_ioslices()[2].get(0..10)
        );

        // test PartialEq
        let v4 = vec![
            0_u8, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22,
            23, 24, 25, 26, 27, 28, 29,
        ];
        assert_eq!(buf1, ZBuf::from(v4));

        // test read
        for i in 0..buf1.len() - 1 {
            assert_eq!(i as u8, buf1.read().unwrap());
        }
        assert!(buf1.can_read());

        // test reset
        buf1.reset();
        println!("[04] {:?}", buf1);
        assert!(!buf1.is_empty());
        assert!(buf1.can_read());
        assert_eq!(30, buf1.readable());
        assert_eq!(30, buf1.len());
        assert_eq!(3, buf1.as_ioslices().len());

        // test set_pos / get_pos
        buf1.reset();
        println!("[05] {:?}", buf1);
        assert_eq!(30, buf1.readable());
        let mut bytes = [0_u8; 10];
        assert!(buf1.read_bytes(&mut bytes));
        assert_eq!(20, buf1.readable());
        let pos = buf1.get_pos();
        assert!(buf1.read_bytes(&mut bytes));
        assert_eq!(10, buf1.readable());
        assert!(buf1.set_pos(pos));
        assert_eq!(20, buf1.readable());
        assert!(buf1.read_bytes(&mut bytes));
        assert_eq!(10, buf1.readable());
        assert!(buf1.read_bytes(&mut bytes));
        assert_eq!(0, buf1.readable());
        let pos = buf1.get_pos();
        assert!(buf1.set_pos(pos));
        let pos = ZBufPos {
            slice: 4,
            byte: 128,
            len: 0,
            read: 0,
        };
        assert!(!buf1.set_pos(pos));

        // test read_bytes
        buf1.reset();
        println!("[06] {:?}", buf1);
        let mut bytes = [0_u8; 3];
        for i in 0..10 {
            assert!(buf1.read_bytes(&mut bytes));
            println!(
                "[06][{}] {:?} Bytes: {:?}",
                i,
                buf1,
                hex::encode_upper(bytes)
            );
            assert_eq!([i * 3, i * 3 + 1, i * 3 + 2], bytes);
        }

        // test other buffers sharing the same vecs
        let mut buf2 = ZBuf::from(v1.clone());
        buf2.add_zslice(v2);
        println!("[07] {:?}", buf1);
        assert!(!buf2.is_empty());
        assert!(buf2.can_read());
        assert_eq!(0, buf2.get_pos().read);
        assert_eq!(20, buf2.readable());
        assert_eq!(20, buf2.len());
        assert_eq!(2, buf2.as_ioslices().len());
        for i in 0..buf2.len() - 1 {
            assert_eq!(i as u8, buf2.read().unwrap());
        }

        let mut buf3 = ZBuf::from(v1);
        println!("[08] {:?}", buf1);
        assert!(!buf3.is_empty());
        assert!(buf3.can_read());
        assert_eq!(0, buf3.get_pos().read);
        assert_eq!(10, buf3.readable());
        assert_eq!(10, buf3.len());
        assert_eq!(1, buf3.as_ioslices().len());
        for i in 0..buf3.len() - 1 {
            assert_eq!(i as u8, buf3.read().unwrap());
        }

        // test read_into_zbuf
        buf1.reset();
        println!("[09] {:?}", buf1);
        let _ = buf1.read();
        let mut dest = ZBuf::new();
        assert!(buf1.read_into_zbuf(&mut dest, 24));
        let dest_slices = dest.as_ioslices();
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
        // let mut dest = ZBuf::new();
        // assert!(buf1.drain_into_zbuf(&mut dest));
        // assert_eq!(buf1.readable(), 0);
        // assert_eq!(buf1.len(), dest.readable());
    }
}
