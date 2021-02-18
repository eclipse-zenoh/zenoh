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
use std::fmt;
use std::io::IoSlice;

#[derive(Clone, Default)]
pub struct RBuf {
    slices: Vec<ArcSlice>,
    pos: (usize, usize),
    #[cfg(feature = "zero-copy")]
    shm_buf: Option<SharedMemoryBuf>,
}

impl RBuf {
    pub fn new() -> RBuf {
        let slices = Vec::with_capacity(32);
        RBuf {
            slices,
            pos: (0, 0),
            #[cfg(feature = "zero-copy")]
            shm_buf: None,
        }
    }

    pub fn empty() -> RBuf {
        let slices = Vec::with_capacity(0);
        RBuf {
            slices,
            pos: (0, 0),
            #[cfg(feature = "zero-copy")]
            shm_buf: None,
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    pub fn add_slice(&mut self, slice: ArcSlice) {
        self.slices.push(slice);
    }

    #[inline]
    pub fn get_slices(&self) -> &[ArcSlice] {
        &self.slices[..]
    }

    #[inline]
    pub fn drain_slices(self) -> Vec<ArcSlice> {
        self.slices
    }

    pub fn as_ioslices(&self) -> Vec<IoSlice> {
        let mut result = Vec::with_capacity(self.slices.len());
        for s in &self.slices {
            result.push(s.as_ioslice());
        }
        result
    }

    pub fn len(&self) -> usize {
        let mut l = 0;
        for s in &self.slices {
            l += s.len();
        }
        l
    }

    pub fn reset_pos(&mut self) {
        self.pos = (0, 0);
    }

    fn skip_bytes_no_check(&mut self, n: usize) {
        if n > 0 {
            if self.pos.1 + n < self.current_slice().len() {
                self.pos.1 += n;
            } else {
                let remaining = self.current_slice().len() - self.pos.1;
                self.pos = (self.pos.0 + 1, 0);
                self.skip_bytes_no_check(n - remaining)
            }
        }
    }

    pub fn skip_bytes(&mut self, n: usize) -> bool {
        let remaining = self.readable();
        if n <= remaining {
            self.skip_bytes_no_check(n);
            return true;
        }
        false
    }

    #[inline]
    pub fn set_pos(&mut self, index: usize) -> bool {
        self.reset_pos();
        self.skip_bytes(index)
    }

    pub fn get_pos(&self) -> usize {
        let mut result = self.pos.1;
        if self.pos.0 > 0 {
            for i in 0..self.pos.0 {
                result += self.slices[i].len();
            }
        }
        result
    }

    #[inline]
    pub fn clear(&mut self) {
        self.slices.clear();
        self.pos.0 = 0;
    }

    #[inline]
    fn current_slice(&self) -> &ArcSlice {
        &self.slices[self.pos.0]
    }

    #[inline]
    pub fn can_read(&self) -> bool {
        self.pos.0 < self.slices.len()
            && (self.pos.0 < self.slices.len() - 1 || self.pos.1 < self.current_slice().len())
    }

    pub fn readable(&self) -> usize {
        if !self.can_read() {
            0
        } else {
            let mut result = self.current_slice().len() - self.pos.1;
            for s in &self.slices[self.pos.0 + 1..] {
                result += s.len()
            }
            result
        }
    }

    pub fn read(&mut self) -> Option<u8> {
        if self.can_read() {
            let b = self.current_slice()[self.pos.1];
            self.skip_bytes_no_check(1);
            return Some(b);
        }
        None
    }

    // same than read() but not moving read position (allow not mutable self)
    pub fn get(&self) -> Option<u8> {
        if self.can_read() {
            let b = self.current_slice()[self.pos.1];
            Some(b)
        } else {
            None
        }
    }

    pub fn read_bytes(&mut self, bs: &mut [u8]) -> bool {
        if !self.get_bytes(bs) {
            return false;
        };
        self.skip_bytes_no_check(bs.len());
        true
    }

    fn get_bytes_no_check(&self, slicepos: (usize, usize), bs: &mut [u8]) {
        let len = bs.len();
        if len > 0 {
            let rem_in_current = self.slices[slicepos.0].len() - slicepos.1;
            let to_read = std::cmp::min(rem_in_current, len);
            bs[0..to_read].copy_from_slice(
                self.slices[slicepos.0].get_sub_slice(slicepos.1, slicepos.1 + to_read),
            );
            self.get_bytes_no_check((slicepos.0 + 1, 0), &mut bs[to_read..])
        }
    }

    // same than read_bytes() but not moving read position (allow not mutable self)
    pub fn get_bytes(&self, bs: &mut [u8]) -> bool {
        let len = bs.len();
        let remaining = self.readable();
        if len > remaining {
            return false;
        }
        self.get_bytes_no_check(self.pos, bs);
        true
    }

    pub fn read_vec(&mut self) -> Vec<u8> {
        let readable = self.readable();
        let mut vec = vec![0u8; readable];
        self.get_bytes_no_check(self.pos, &mut vec);
        self.skip_bytes_no_check(readable);
        vec
    }

    // same than read_vec() but not moving read position (allow not mutable self)
    pub fn get_vec(&self) -> Vec<u8> {
        let mut vec = vec![0u8; self.readable()];
        self.get_bytes_no_check(self.pos, &mut vec);
        vec
    }

    // returns a Vec<u8> containing a copy of RBuf content (not considering read position)
    pub fn to_vec(&self) -> Vec<u8> {
        let mut vec = vec![0u8; self.len()];
        self.get_bytes_no_check((0, 0), &mut vec[..]);
        vec
    }

    // Read 'len' bytes from 'self' and add those to 'dest'
    // This is 0-copy, only ArcSlices from 'self' are added to 'dest', without cloning the original buffer.
    fn read_into_rbuf_no_check(&mut self, dest: &mut RBuf, len: usize) {
        let mut to_copy = len;
        while to_copy > 0 {
            let remain_in_slice = self.current_slice().len() - self.pos.1;
            let l = to_copy.min(remain_in_slice);
            dest.add_slice(
                self.current_slice()
                    .new_sub_slice(self.pos.1, self.pos.1 + l),
            );
            self.skip_bytes_no_check(l);
            to_copy -= l;
        }
    }

    // Read 'len' bytes from 'self' and add those to 'dest'
    pub fn read_into_rbuf(&mut self, dest: &mut RBuf, len: usize) -> bool {
        if self.readable() >= len {
            self.read_into_rbuf_no_check(dest, len);
            return true;
        }
        false
    }

    // Read all the bytes from 'self' and add those to 'dest'
    pub fn drain_into_rbuf(&mut self, dest: &mut RBuf) {
        self.read_into_rbuf_no_check(dest, self.readable());
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
            self.slices.clear();
            self.add_slice(shm.into());
        }
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
        if self.slices.is_empty() {
            write!(f, "slices: none }}")
        } else {
            write!(f, "slices:")?;
            for s in &self.slices {
                write!(f, " {},", hex::encode_upper(s.as_slice()))?;
            }
            write!(f, " }}")
        }
    }
}

impl From<ArcSlice> for RBuf {
    fn from(slice: ArcSlice) -> RBuf {
        RBuf {
            slices: vec![slice],
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
    fn from(slices: Vec<ArcSlice>) -> RBuf {
        RBuf {
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
impl From<SharedMemoryBuf> for RBuf {
    fn from(smb: SharedMemoryBuf) -> RBuf {
        let bs = bincode::serialize(&smb.info).unwrap();
        let len = bs.len();
        let slice = ArcSlice::new(bs.into(), 0, len);
        RBuf {
            slices: vec![slice],
            pos: (0, 0),
            #[cfg(feature = "zero-copy")]
            shm_buf: Some(smb),
        }
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
        let mut bytes = [0u8; 3];
        for i in 0..10 {
            assert!(buf1.read_bytes(&mut bytes));
            assert_eq!([i * 3 as u8, i * 3 + 1, i * 3 + 2], bytes);
        }

        // test other buffers sharing the same vecs
        let mut buf2 = RBuf::from(v1.clone());
        buf2.add_slice(v2.clone());
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
    }
}
