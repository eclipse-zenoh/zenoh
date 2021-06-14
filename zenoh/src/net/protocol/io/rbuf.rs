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
#[cfg(feature = "zero-copy")]
use super::shm::{SharedMemoryBuf, SharedMemoryReader};
use super::{ZSlice, ZSliceType};
use std::fmt;
use std::io;
use std::io::IoSlice;
#[cfg(feature = "zero-copy")]
use std::sync::Arc;
#[cfg(feature = "zero-copy")]
use zenoh_util::core::ZResult;

/*************************************/
/*           RBUF POSITION           */
/*************************************/
#[derive(Clone, Copy, PartialEq)]
pub struct RBufPos {
    slice: usize, // The ZSlice index
    byte: usize,  // The byte in the ZSlice
    len: usize,   // The total lenght in bytes of the RBuf
    read: usize,  // The amount read so far in the RBuf
}

impl RBufPos {
    fn reset(&mut self) {
        self.slice = 0;
        self.byte = 0;
        self.read = 0;
    }

    fn clear(&mut self) {
        self.reset();
        self.len = 0;
    }
}

impl Default for RBufPos {
    fn default() -> RBufPos {
        RBufPos {
            slice: 0,
            byte: 0,
            len: 0,
            read: 0,
        }
    }
}

impl fmt::Display for RBufPos {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.read)
    }
}

impl fmt::Debug for RBufPos {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "(Slice: {}, Byte: {}, Len: {}, Read: {})",
            self.slice, self.byte, self.len, self.read
        )
    }
}

/*************************************/
/*            RBUF INNER             */
/*************************************/
#[derive(Clone)]
enum RBufInner {
    Single(ZSlice),
    Multiple(Vec<ZSlice>),
    Empty,
}

impl Default for RBufInner {
    fn default() -> RBufInner {
        RBufInner::Empty
    }
}

/*************************************/
/*              RBUF                 */
/*************************************/
#[derive(Clone, Default)]
pub struct RBuf {
    slices: RBufInner,
    pos: RBufPos,
}

impl RBuf {
    pub fn new() -> RBuf {
        RBuf {
            slices: RBufInner::default(),
            pos: RBufPos::default(),
        }
    }

    pub fn empty() -> RBuf {
        RBuf::new()
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    pub fn add_slice(&mut self, slice: ZSlice) {
        self.pos.len += slice.len();
        match &mut self.slices {
            RBufInner::Single(s) => {
                let m = vec![s.clone(), slice];
                self.slices = RBufInner::Multiple(m);
            }
            RBufInner::Multiple(m) => {
                m.push(slice);
            }
            RBufInner::Empty => {
                self.slices = RBufInner::Single(slice);
            }
        }
    }

    #[inline]
    pub fn get_slice(&self, index: usize) -> Option<&ZSlice> {
        match &self.slices {
            RBufInner::Single(s) => match index {
                0 => Some(s),
                _ => None,
            },
            RBufInner::Multiple(m) => m.get(index),
            RBufInner::Empty => None,
        }
    }

    #[inline]
    pub fn get_slices_num(&self) -> usize {
        match &self.slices {
            RBufInner::Single(_) => 1,
            RBufInner::Multiple(m) => m.len(),
            RBufInner::Empty => 0,
        }
    }

    #[inline]
    pub fn get_slice_mut(&mut self, index: usize) -> Option<&mut ZSlice> {
        match &mut self.slices {
            RBufInner::Single(s) => match index {
                0 => Some(s),
                _ => None,
            },
            RBufInner::Multiple(m) => m.get_mut(index),
            RBufInner::Empty => None,
        }
    }

    pub fn as_slices(&self) -> Vec<ZSlice> {
        match &self.slices {
            RBufInner::Single(s) => vec![s.clone()],
            RBufInner::Multiple(m) => m.clone(),
            RBufInner::Empty => vec![],
        }
    }

    pub fn as_ioslices(&self) -> Vec<IoSlice> {
        match &self.slices {
            RBufInner::Single(s) => vec![s.as_ioslice()],
            RBufInner::Multiple(m) => m.iter().map(|s| s.as_ioslice()).collect(),
            RBufInner::Empty => vec![],
        }
    }

    #[inline(always)]
    pub fn clear(&mut self) {
        self.pos.clear();
        self.slices = RBufInner::Empty;
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
    pub fn reset_pos(&mut self) {
        self.pos.reset();
    }

    #[inline]
    pub fn set_pos(&mut self, pos: RBufPos) -> bool {
        if pos == self.pos {
            return true;
        }

        if let Some(slice) = self.get_slice(pos.slice) {
            if pos.byte < slice.len() {
                self.pos = pos;
                return true;
            }
        }

        false
    }

    #[inline(always)]
    pub fn get_pos(&self) -> RBufPos {
        self.pos
    }

    #[inline(always)]
    fn curr_slice(&self) -> Option<&ZSlice> {
        self.get_slice(self.pos.slice)
    }

    #[inline(always)]
    fn curr_slice_mut(&mut self) -> Option<&mut ZSlice> {
        self.get_slice_mut(self.pos.slice)
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

    pub fn skip_bytes(&mut self, n: usize) -> bool {
        if n <= self.readable() {
            self.skip_bytes_no_check(n);
            true
        } else {
            false
        }
    }

    // same than read() but not moving read position (allow not mutable self)
    #[inline(always)]
    pub fn get(&self) -> Option<u8> {
        self.curr_slice().map(|current| current[self.pos.byte])
    }

    pub fn read(&mut self) -> Option<u8> {
        let res = self.get();
        if res.is_some() {
            self.skip_bytes_no_check(1);
        }
        res
    }

    // same than read_bytes() but not moving read position (allow non-mutable self)
    pub fn copy_bytes(&self, bs: &mut [u8], mut pos: (usize, usize)) -> bool {
        let len = bs.len();
        if self.readable() < len {
            return false;
        }

        let mut written = 0;
        while written < len {
            let slice = self.get_slice(pos.0).unwrap();
            let remaining = slice.len() - pos.1;
            let to_read = remaining.min(bs.len() - written);
            bs[written..written + to_read]
                .copy_from_slice(&slice.as_slice()[pos.1..pos.1 + to_read]);
            written += to_read;
            pos = (pos.0 + 1, 0);
        }
        true
    }

    #[inline]
    pub fn read_bytes(&mut self, bs: &mut [u8]) -> bool {
        if !self.copy_bytes(bs, (self.pos.slice, self.pos.byte)) {
            return false;
        }
        self.skip_bytes_no_check(bs.len());
        true
    }

    #[inline]
    pub fn get_bytes(&self, bs: &mut [u8]) -> bool {
        self.copy_bytes(bs, (self.pos.slice, self.pos.byte))
    }

    #[inline]
    pub fn read_vec(&mut self) -> Vec<u8> {
        let mut vec = vec![0u8; self.readable()];
        self.read_bytes(&mut vec);
        vec
    }

    // same than read_vec() but not moving read position (allow not mutable self)
    #[inline]
    pub fn get_vec(&self) -> Vec<u8> {
        let mut vec = vec![0u8; self.readable()];
        self.get_bytes(&mut vec);
        vec
    }

    // returns a Vec<u8> containing a copy of RBuf content (not considering read position)
    #[inline]
    pub fn to_vec(&self) -> Vec<u8> {
        let mut vec = vec![0u8; self.len()];
        self.copy_bytes(&mut vec[..], (0, 0));
        vec
    }

    // returns a ZSlice that may allocate in case of RBuf being composed of multiple ZSlices
    #[inline]
    pub fn flatten(&self) -> ZSlice {
        match &self.slices {
            RBufInner::Single(s) => s.clone(),
            RBufInner::Multiple(_) => self.to_vec().into(),
            RBufInner::Empty => vec![].into(),
        }
    }

    // Read 'len' bytes from 'self' and add those to 'dest'
    // This is 0-copy, only ZSlices from 'self' are added to 'dest', without cloning the original buffer.
    pub fn read_into_rbuf(&mut self, dest: &mut RBuf, len: usize) -> bool {
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
            dest.add_slice(current.new_sub_slice(pos_1, pos_1 + l));
            self.skip_bytes_no_check(l);
            n -= l;
        }
        true
    }

    // Read all the bytes from 'self' and add those to 'dest'
    #[inline(always)]
    pub fn drain_into_rbuf(&mut self, dest: &mut RBuf) -> bool {
        self.read_into_rbuf(dest, self.readable())
    }

    #[cfg(feature = "zero-copy")]
    #[inline]
    pub fn map_to_shmbuf(&mut self, shmr: &mut SharedMemoryReader) -> ZResult<bool> {
        self.pos.clear();

        let mut res = false;
        match &mut self.slices {
            RBufInner::Single(s) => {
                res = s.map_to_shmbuf(shmr)?;
                self.pos.len += s.len();
            }
            RBufInner::Multiple(m) => {
                for s in m.iter_mut() {
                    res = res || s.map_to_shmbuf(shmr)?;
                    self.pos.len += s.len();
                }
            }
            RBufInner::Empty => {}
        }

        Ok(res)
    }

    #[cfg(feature = "zero-copy")]
    #[inline]
    pub fn try_map_to_shmbuf(&mut self, shmr: &SharedMemoryReader) -> ZResult<bool> {
        self.pos.clear();

        let mut res = false;
        match &mut self.slices {
            RBufInner::Single(s) => {
                res = s.try_map_to_shmbuf(shmr)?;
                self.pos.len += s.len();
            }
            RBufInner::Multiple(m) => {
                for s in m.iter_mut() {
                    res = res || s.try_map_to_shmbuf(shmr)?;
                    self.pos.len += s.len();
                }
            }
            RBufInner::Empty => {}
        }

        Ok(res)
    }

    #[cfg(feature = "zero-copy")]
    #[inline]
    pub fn map_to_shminfo(&mut self) -> ZResult<bool> {
        self.pos.clear();

        let mut res = false;
        match &mut self.slices {
            RBufInner::Single(s) => {
                res = s.map_to_shminfo()?;
                self.pos.len = s.len();
            }
            RBufInner::Multiple(m) => {
                for s in m.iter_mut() {
                    res = res || s.map_to_shminfo()?;
                    self.pos.len += s.len();
                }
            }
            RBufInner::Empty => {}
        }

        Ok(res)
    }

    #[cfg(feature = "zero-copy")]
    pub fn has_shminfo(&self) -> bool {
        macro_rules! is_shminfo {
            ($slice:expr) => {
                match $slice.get_type() {
                    ZSliceType::ShmInfo => true,
                    _ => false,
                }
            };
        }

        match &self.slices {
            RBufInner::Single(s) => is_shminfo!(s),
            RBufInner::Multiple(m) => m
                .iter()
                .fold(false, |has_shminfo, s| has_shminfo || is_shminfo!(s)),
            RBufInner::Empty => false,
        }
    }
}

impl fmt::Display for RBuf {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "RBuf{{ pos: {:?}, content: {} }}",
            self.get_pos(),
            hex::encode_upper(self.to_vec())
        )
    }
}

impl fmt::Debug for RBuf {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        macro_rules! zsliceprint {
            ($slice:expr) => {
                #[cfg(feature = "zero-copy")]
                {
                    match $slice.get_type() {
                        ZSliceType::Net => write!(f, " BUF:")?,
                        ZSliceType::ShmBuf => write!(f, " SHM_BUF:")?,
                        ZSliceType::ShmInfo => write!(f, " SHM_INFO:")?,
                    }
                }
                #[cfg(not(feature = "zero-copy"))]
                {
                    write!(f, " BUF:")?;
                }
            };
        }

        write!(f, "RBuf{{ pos: {:?}, ", self.pos)?;
        write!(f, "slices: [")?;
        match &self.slices {
            RBufInner::Single(s) => {
                zsliceprint!(s);
                write!(f, "{}", hex::encode_upper(s.as_slice()))?;
            }
            RBufInner::Multiple(m) => {
                for s in m.iter() {
                    zsliceprint!(s);
                    write!(f, " {},", hex::encode_upper(s.as_slice()))?;
                }
            }
            RBufInner::Empty => {
                write!(f, " None")?;
            }
        }
        write!(f, " ] }}")
    }
}

/*************************************/
/*           RBUF ITERATOR           */
/*************************************/
pub struct RBufIterator {
    index: usize,
    slices: RBufInner,
}

impl Iterator for RBufIterator {
    type Item = ZSlice;

    fn next(&mut self) -> Option<Self::Item> {
        match &self.slices {
            RBufInner::Single(s) => {
                let s = s.clone();
                self.slices = RBufInner::Empty;
                Some(s)
            }
            RBufInner::Multiple(m) => {
                let s = m.get(self.index)?.clone();
                self.index += 1;
                Some(s)
            }
            RBufInner::Empty => None,
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match &self.slices {
            RBufInner::Single(_) => (1, Some(1)),
            RBufInner::Multiple(m) => {
                let remaining = m.len() - self.index;
                (remaining, Some(remaining))
            }
            RBufInner::Empty => (0, Some(0)),
        }
    }
}

impl IntoIterator for RBuf {
    type Item = ZSlice;
    type IntoIter = RBufIterator;

    fn into_iter(self) -> Self::IntoIter {
        RBufIterator {
            index: 0,
            slices: self.slices,
        }
    }
}

/*************************************/
/*            RBUF READ              */
/*************************************/
impl io::Read for RBuf {
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

    //#[inline]
    //fn is_read_vectored(&self) -> bool {
    //    true
    //}
}

/*************************************/
/*            RBUF FROM              */
/*************************************/
impl From<ZSlice> for RBuf {
    fn from(slice: ZSlice) -> RBuf {
        let mut rbuf = RBuf::new();
        rbuf.add_slice(slice);
        rbuf
    }
}

impl From<Vec<u8>> for RBuf {
    fn from(buf: Vec<u8>) -> RBuf {
        let len = buf.len();
        RBuf::from(ZSlice::new(buf.into(), 0, len))
    }
}

impl From<&[u8]> for RBuf {
    fn from(slice: &[u8]) -> RBuf {
        RBuf::from(slice.to_vec())
    }
}

impl From<Vec<ZSlice>> for RBuf {
    fn from(mut slices: Vec<ZSlice>) -> RBuf {
        let mut rbuf = RBuf::new();
        for slice in slices.drain(..) {
            rbuf.add_slice(slice);
        }
        rbuf
    }
}

impl<'a> From<Vec<IoSlice<'a>>> for RBuf {
    fn from(slices: Vec<IoSlice>) -> RBuf {
        let v: Vec<ZSlice> = slices.iter().map(ZSlice::from).collect();
        RBuf::from(v)
    }
}

impl From<&super::WBuf> for RBuf {
    fn from(wbuf: &super::WBuf) -> RBuf {
        RBuf::from(wbuf.as_slices())
    }
}

impl From<super::WBuf> for RBuf {
    fn from(wbuf: super::WBuf) -> RBuf {
        Self::from(&wbuf)
    }
}

impl PartialEq for RBuf {
    fn eq(&self, other: &Self) -> bool {
        let mut rbuf1 = self.clone();
        rbuf1.reset_pos();
        let mut rbuf2 = other.clone();
        rbuf2.reset_pos();

        while let Some(b1) = rbuf1.read() {
            if let Some(b2) = rbuf2.read() {
                if b1 != b2 {
                    return false;
                }
            } else {
                return false;
            }
        }
        // Check there are no bytes left in rbuf2
        rbuf2.read().is_none()
    }
}

#[cfg(feature = "zero-copy")]
impl From<Arc<SharedMemoryBuf>> for RBuf {
    fn from(smb: Arc<SharedMemoryBuf>) -> RBuf {
        let mut rbuf = RBuf::new();
        rbuf.add_slice(smb.into());
        rbuf
    }
}

#[cfg(feature = "zero-copy")]
impl From<Box<SharedMemoryBuf>> for RBuf {
    fn from(smb: Box<SharedMemoryBuf>) -> RBuf {
        let mut rbuf = RBuf::new();
        rbuf.add_slice(smb.into());
        rbuf
    }
}

#[cfg(feature = "zero-copy")]
impl From<SharedMemoryBuf> for RBuf {
    fn from(smb: SharedMemoryBuf) -> RBuf {
        let mut rbuf = RBuf::new();
        rbuf.add_slice(smb.into());
        rbuf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rbuf() {
        let v1 = ZSlice::from(vec![0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
        let v2 = ZSlice::from(vec![10u8, 11, 12, 13, 14, 15, 16, 17, 18, 19]);
        let v3 = ZSlice::from(vec![20u8, 21, 22, 23, 24, 25, 26, 27, 28, 29]);

        // test a 1st buffer
        let mut buf1 = RBuf::new();
        assert!(buf1.is_empty());
        assert!(!buf1.can_read());
        assert_eq!(0, buf1.get_pos().read);
        assert_eq!(0, buf1.readable());
        assert_eq!(0, buf1.len());
        assert_eq!(0, buf1.as_ioslices().len());

        buf1.add_slice(v1.clone());
        println!("[01] {:?}", buf1);
        assert!(!buf1.is_empty());
        assert!(buf1.can_read());
        assert_eq!(0, buf1.get_pos().read);
        assert_eq!(10, buf1.readable());
        assert_eq!(10, buf1.len());
        assert_eq!(1, buf1.as_ioslices().len());
        assert_eq!(
            Some(&[0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9][..]),
            buf1.as_ioslices()[0].get(0..10)
        );

        buf1.add_slice(v2.clone());
        println!("[02] {:?}", buf1);
        assert!(!buf1.is_empty());
        assert!(buf1.can_read());
        assert_eq!(0, buf1.get_pos().read);
        assert_eq!(20, buf1.readable());
        assert_eq!(20, buf1.len());
        assert_eq!(2, buf1.as_ioslices().len());
        assert_eq!(
            Some(&[10u8, 11, 12, 13, 14, 15, 16, 17, 18, 19][..]),
            buf1.as_ioslices()[1].get(0..10)
        );

        buf1.add_slice(v3.clone());
        println!("[03] {:?}", buf1);
        assert!(!buf1.is_empty());
        assert!(buf1.can_read());
        assert_eq!(0, buf1.get_pos().read);
        assert_eq!(30, buf1.readable());
        assert_eq!(30, buf1.len());
        assert_eq!(3, buf1.as_ioslices().len());
        assert_eq!(
            Some(&[20u8, 21, 22, 23, 24, 25, 26, 27, 28, 29][..]),
            buf1.as_ioslices()[2].get(0..10)
        );

        // test PartialEq
        let v4 = vec![
            0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23,
            24, 25, 26, 27, 28, 29,
        ];
        assert_eq!(buf1, RBuf::from(v4));

        // test read
        for i in 0..buf1.len() - 1 {
            assert_eq!(i as u8, buf1.read().unwrap());
        }
        assert!(buf1.can_read());

        // test reset_pos
        buf1.reset_pos();
        println!("[04] {:?}", buf1);
        assert!(!buf1.is_empty());
        assert!(buf1.can_read());
        assert_eq!(30, buf1.readable());
        assert_eq!(30, buf1.len());
        assert_eq!(3, buf1.as_ioslices().len());

        // test set_pos / get_pos
        buf1.reset_pos();
        println!("[05] {:?}", buf1);
        assert_eq!(30, buf1.readable());
        let mut bytes = [0u8; 10];
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
        let pos = RBufPos {
            slice: 4,
            byte: 128,
            len: 0,
            read: 0,
        };
        assert!(!buf1.set_pos(pos));

        // test read_bytes
        buf1.reset_pos();
        println!("[06] {:?}", buf1);
        let mut bytes = [0u8; 3];
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
        let mut buf2 = RBuf::from(v1.clone());
        buf2.add_slice(v2.clone());
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

        let mut buf3 = RBuf::from(v1.clone());
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

        // test read_into_rbuf
        buf1.reset_pos();
        println!("[09] {:?}", buf1);
        let _ = buf1.read();
        let mut dest = RBuf::new();
        assert!(buf1.read_into_rbuf(&mut dest, 24));
        let dest_slices = dest.as_ioslices();
        assert_eq!(3, dest_slices.len());
        assert_eq!(
            Some(&[1u8, 2, 3, 4, 5, 6, 7, 8, 9][..]),
            dest_slices[0].get(..)
        );
        assert_eq!(
            Some(&[10u8, 11, 12, 13, 14, 15, 16, 17, 18, 19][..]),
            dest_slices[1].get(..)
        );
        assert_eq!(Some(&[20u8, 21, 22, 23, 24][..]), dest_slices[2].get(..));

        // test drain_into_rbuf
        buf1.reset_pos();
        println!("[10] {:?}", buf1);
        let mut dest = RBuf::new();
        assert!(buf1.drain_into_rbuf(&mut dest));
        assert_eq!(buf1.readable(), 0);
        assert_eq!(buf1.len(), dest.readable());
    }
}
