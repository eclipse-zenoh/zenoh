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
use super::shm::{SharedMemoryBuf, SharedMemoryBufInfo, SharedMemoryManager};
use super::ArcSlice;
#[cfg(feature = "zero-copy")]
use async_std::sync::Arc;
use std::fmt;
use std::io;
use std::io::IoSlice;

#[derive(Clone, Default)]
pub struct RBuf {
    zero: Option<ArcSlice>,
    slices: Vec<ArcSlice>,
    pos: (usize, usize),
    #[cfg(feature = "zero-copy")]
    shm_buf: Option<Arc<SharedMemoryBuf>>,
}

impl RBuf {
    pub fn new() -> RBuf {
        RBuf {
            zero: None,
            slices: vec![],
            pos: (0, 0),
            #[cfg(feature = "zero-copy")]
            shm_buf: None,
        }
    }

    pub fn empty() -> RBuf {
        RBuf::new()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    pub fn add_slice(&mut self, slice: ArcSlice) {
        if self.zero.is_none() {
            self.zero = Some(slice);
        } else {
            self.slices.push(slice);
        }
    }

    pub fn as_ioslices(&self) -> Vec<IoSlice> {
        let mut result = vec![];
        if let Some(z) = self.zero.as_ref() {
            result.reserve(1 + self.slices.len());
            result.push(z.as_ioslice());
            for s in self.slices.iter() {
                result.push(s.as_ioslice());
            }
        }
        result
    }

    pub fn len(&self) -> usize {
        let mut l = 0;
        if let Some(z) = self.zero.as_ref() {
            l += z.len();
            l += self.slices.iter().fold(0, |tot, s| tot + s.len());
        }
        l
    }

    #[inline]
    pub fn reset_pos(&mut self) {
        self.pos = (0, 0);
    }

    pub fn skip_bytes(&mut self, mut n: usize) -> bool {
        while let Some(current) = self.curr_slice() {
            let len = current.len();
            if self.pos.1 + n < len {
                self.pos.1 += n;
                return true;
            } else {
                n -= len - self.pos.1;
                self.pos = (self.pos.0 + 1, 0);
            }
        }
        n == 0
    }

    #[inline]
    pub fn set_pos(&mut self, index: usize) -> bool {
        self.reset_pos();
        self.skip_bytes(index)
    }

    pub fn get_pos(&self) -> usize {
        let mut pos = self.pos.1;
        if let Some(curr) = self.curr_slice() {
            let mut idx = 0;
            while let Some(slice) = self.get_slice(idx) {
                if curr != slice {
                    pos += slice.len();
                    idx += 1;
                } else {
                    break;
                }
            }
        }
        pos
    }

    #[inline]
    pub fn clear(&mut self) {
        self.zero = None;
        self.slices.clear();
        self.pos = (0, 0);
    }

    #[inline]
    pub(super) fn get_slice(&self, index: usize) -> Option<&ArcSlice> {
        if index == 0 {
            self.zero.as_ref()
        } else {
            self.slices.get(index - 1)
        }
    }

    #[inline]
    pub(super) fn get_slice_mut(&mut self, index: usize) -> Option<&mut ArcSlice> {
        if index == 0 {
            self.zero.as_mut()
        } else {
            self.slices.get_mut(index - 1)
        }
    }

    #[inline]
    fn curr_slice(&self) -> Option<&ArcSlice> {
        self.get_slice(self.pos.0)
    }

    #[inline]
    fn curr_slice_mut(&mut self) -> Option<&mut ArcSlice> {
        self.get_slice_mut(self.pos.0)
    }

    #[inline]
    fn next_slice(&self) -> Option<&ArcSlice> {
        self.get_slice(self.pos.0 + 1)
    }

    pub fn can_read(&self) -> bool {
        if let Some(current) = self.curr_slice() {
            if self.pos.1 < current.len() {
                true
            } else if let Some(next) = self.next_slice() {
                !next.is_empty()
            } else {
                false
            }
        } else {
            false
        }
    }

    pub fn readable(&self) -> usize {
        let mut readable = 0;
        if let Some(current) = self.curr_slice() {
            readable += current.len() - self.pos.1;
            let mut idx = self.pos.0 + 1;
            while let Some(next) = self.get_slice(idx) {
                readable += next.len();
                idx += 1;
            }
        }
        readable
    }

    // same than read() but not moving read position (allow not mutable self)
    pub fn get(&self) -> Option<u8> {
        if let Some(current) = self.curr_slice() {
            current.get(self.pos.1)
        } else {
            None
        }
    }

    pub fn read(&mut self) -> Option<u8> {
        let res = self.get();
        if res.is_some() {
            self.skip_bytes(1);
        }
        res
    }

    // same than read_bytes() but not moving read position (allow non-mutable self)
    pub fn copy_bytes(&self, bs: &mut [u8], mut pos: (usize, usize)) -> bool {
        let len = bs.len();
        let mut written = 0;
        while written < len {
            if let Some(slice) = self.get_slice(pos.0) {
                let remaining = slice.len() - pos.1;
                let to_read = remaining.min(len - written);
                bs[written..written + to_read]
                    .copy_from_slice(slice.get_sub_slice(pos.1, pos.1 + to_read));
                written += to_read;
                pos = (pos.0 + 1, 0);
            } else {
                return false;
            }
        }
        true
    }

    #[inline]
    pub fn read_bytes(&mut self, bs: &mut [u8]) -> bool {
        if !self.copy_bytes(bs, self.pos) {
            return false;
        };
        self.skip_bytes(bs.len())
    }

    #[inline]
    pub fn get_bytes(&self, bs: &mut [u8]) -> bool {
        self.copy_bytes(bs, self.pos)
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

    // Read 'len' bytes from 'self' and add those to 'dest'
    // This is 0-copy, only ArcSlices from 'self' are added to 'dest', without cloning the original buffer.
    pub fn read_into_rbuf(&mut self, dest: &mut RBuf, mut len: usize) -> bool {
        let dst_pos = dest.get_pos();
        while len > 0 {
            let pos_1 = self.pos.1;
            if let Some(current) = self.curr_slice_mut() {
                let slice_len = current.len();
                let remain_in_slice = slice_len - pos_1;
                let l = len.min(remain_in_slice);
                dest.add_slice(current.new_sub_slice(pos_1, pos_1 + l));
                self.skip_bytes(l);
                len -= l;
            } else {
                dest.set_pos(dst_pos);
                return false;
            }
        }
        true
    }

    // Read all the bytes from 'self' and add those to 'dest'
    pub fn drain_into_rbuf(&mut self, dest: &mut RBuf) -> bool {
        self.read_into_rbuf(dest, self.readable())
    }

    #[cfg(feature = "zero-copy")]
    pub fn into_shm(self, m: &mut SharedMemoryManager) -> Option<SharedMemoryBuf> {
        match bincode::deserialize::<SharedMemoryBufInfo>(&self.to_vec()) {
            Ok(info) => m.from_info(info),
            Err(_) => None,
        }
    }

    #[cfg(feature = "zero-copy")]
    pub fn flatten_shm(&mut self) {
        if let Some(shm) = self.shm_buf.take() {
            self.clear();
            self.add_slice(shm.into());
        }
    }
}

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

    #[inline]
    fn is_read_vectored(&self) -> bool {
        true
    }
}

impl fmt::Display for RBuf {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "RBuf{{ pos: {}, content: {} }}",
            self.get_pos(),
            hex::encode_upper(self.to_vec())
        )
    }
}

impl fmt::Debug for RBuf {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "RBuf{{ pos: {:?}, ", self.pos)?;
        if let Some(z) = self.zero.as_ref() {
            write!(f, "zero: {}, ", hex::encode_upper(z.as_slice()))?;
        } else {
            write!(f, "zero: None, ")?;
        }
        if self.slices.is_empty() {
            write!(f, "slices: None }}")
        } else {
            write!(f, "slices:")?;
            for s in self.slices.iter() {
                write!(f, " {},", hex::encode_upper(s.as_slice()))?;
            }
            write!(f, " }}")
        }
    }
}

impl From<ArcSlice> for RBuf {
    fn from(slice: ArcSlice) -> RBuf {
        RBuf {
            zero: Some(slice),
            slices: vec![],
            pos: (0, 0),
            #[cfg(feature = "zero-copy")]
            shm_buf: None,
        }
    }
}

impl From<Vec<u8>> for RBuf {
    fn from(buf: Vec<u8>) -> RBuf {
        let len = buf.len();
        RBuf::from(ArcSlice::new(buf.into(), 0, len))
    }
}

impl From<&[u8]> for RBuf {
    fn from(slice: &[u8]) -> RBuf {
        RBuf::from(slice.to_vec())
    }
}

impl From<Vec<ArcSlice>> for RBuf {
    fn from(mut slices: Vec<ArcSlice>) -> RBuf {
        RBuf {
            zero: Some(slices.remove(0)),
            slices,
            pos: (0, 0),
            #[cfg(feature = "zero-copy")]
            shm_buf: None,
        }
    }
}

impl<'a> From<Vec<IoSlice<'a>>> for RBuf {
    fn from(slices: Vec<IoSlice>) -> RBuf {
        let v: Vec<ArcSlice> = slices.iter().map(ArcSlice::from).collect();
        RBuf::from(v)
    }
}

impl From<&super::WBuf> for RBuf {
    fn from(wbuf: &super::WBuf) -> RBuf {
        RBuf::from(wbuf.as_arcslices())
    }
}

impl From<super::WBuf> for RBuf {
    fn from(wbuf: super::WBuf) -> RBuf {
        Self::from(&wbuf)
    }
}

impl PartialEq for RBuf {
    fn eq(&self, other: &Self) -> bool {
        if self.len() != other.len() {
            false
        } else {
            let mut b1 = self.clone();
            b1.reset_pos();
            let mut b2 = other.clone();
            b2.reset_pos();
            for _ in 0..b1.len() {
                if b1.read().unwrap() != b2.read().unwrap() {
                    return false;
                }
            }
            true
        }
    }
}

#[cfg(feature = "zero-copy")]
impl From<Arc<SharedMemoryBuf>> for RBuf {
    fn from(smb: Arc<SharedMemoryBuf>) -> RBuf {
        let bs = bincode::serialize(&smb.info).unwrap();
        let len = bs.len();
        let slice = ArcSlice::new(bs.into(), 0, len);
        RBuf {
            zero: Some(slice),
            slices: vec![],
            pos: (0, 0),
            #[cfg(feature = "zero-copy")]
            shm_buf: Some(smb),
        }
    }
}

#[cfg(feature = "zero-copy")]
impl From<SharedMemoryBuf> for RBuf {
    fn from(smb: SharedMemoryBuf) -> RBuf {
        RBuf::from(Arc::new(smb))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rbuf() {
        let v1 = ArcSlice::from(vec![0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
        let v2 = ArcSlice::from(vec![10u8, 11, 12, 13, 14, 15, 16, 17, 18, 19]);
        let v3 = ArcSlice::from(vec![20u8, 21, 22, 23, 24, 25, 26, 27, 28, 29]);

        // test a 1st buffer
        let mut buf1 = RBuf::new();
        assert!(buf1.is_empty());
        assert!(!buf1.can_read());
        assert_eq!(0, buf1.get_pos());
        assert_eq!(0, buf1.readable());
        assert_eq!(0, buf1.len());
        assert_eq!(0, buf1.as_ioslices().len());

        buf1.add_slice(v1.clone());
        println!("[01] {:?}", buf1);
        assert!(!buf1.is_empty());
        assert!(buf1.can_read());
        assert_eq!(0, buf1.get_pos());
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
        assert_eq!(0, buf1.get_pos());
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
        assert_eq!(0, buf1.get_pos());
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
        for i in 0..buf1.len() - 1 {
            assert!(buf1.set_pos(i));
            assert_eq!(i, buf1.get_pos());
            assert_eq!(i as u8, buf1.read().unwrap());
        }

        // test read_bytes
        buf1.reset_pos();
        println!("[05] {:?}", buf1);
        let mut bytes = [0u8; 3];
        for i in 0..10 {
            assert!(buf1.read_bytes(&mut bytes));
            println!(
                "[05][{}] {:?} Bytes: {:?}",
                i,
                buf1,
                hex::encode_upper(bytes)
            );
            assert_eq!([i * 3 as u8, i * 3 + 1, i * 3 + 2], bytes);
        }

        // test other buffers sharing the same vecs
        let mut buf2 = RBuf::from(v1.clone());
        buf2.add_slice(v2.clone());
        println!("[06] {:?}", buf1);
        assert!(!buf2.is_empty());
        assert!(buf2.can_read());
        assert_eq!(0, buf2.get_pos());
        assert_eq!(20, buf2.readable());
        assert_eq!(20, buf2.len());
        assert_eq!(2, buf2.as_ioslices().len());
        for i in 0..buf2.len() - 1 {
            assert_eq!(i as u8, buf2.read().unwrap());
        }

        let mut buf3 = RBuf::from(v1.clone());
        println!("[07] {:?}", buf1);
        assert!(!buf3.is_empty());
        assert!(buf3.can_read());
        assert_eq!(0, buf3.get_pos());
        assert_eq!(10, buf3.readable());
        assert_eq!(10, buf3.len());
        assert_eq!(1, buf3.as_ioslices().len());
        for i in 0..buf3.len() - 1 {
            assert_eq!(i as u8, buf3.read().unwrap());
        }

        // test read_into_rbuf
        buf1.reset_pos();
        println!("[08] {:?}", buf1);
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
        println!("[09] {:?}", buf1);
        let mut dest = RBuf::new();
        assert!(buf1.drain_into_rbuf(&mut dest));
        assert_eq!(buf1.readable(), 0);
        assert_eq!(buf1.len(), dest.readable());
    }
}
