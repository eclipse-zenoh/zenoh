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
use super::ArcSlice;
use async_std::sync::Arc;
use std::fmt;
use std::io;
use std::io::IoSlice;
use std::ops::Bound::{Excluded, Included, Unbounded};
use std::ops::RangeBounds;

// Notes:
//  - Wbuf has 2 flavors:
//    - contigous:
//      - it is a Vec<u8> which is contigous in memory
//      - it is initialized with a fixed capacity and won't be extended
//      - if a write exceeds capacity, 'false' is returned
//      - the writing of ArcSlices makes a copy of the buffer into WBuf
//    - non-contigous:
//      - it manages a list of slices which could be:
//          - either ArcSlices, passed by the user => 0-copy
//          - either slices of an internal Vec<u8> used for serialization
//      - the internal Vec<u8> is initialized with the specified capacity but can be expended
//      - user can get the WBuf content as a list of IoSlices
//      - user can request the copy any sub-part of WBuf into a slice using copy_into_slice()

#[derive(Clone)]
enum Slice {
    External(ArcSlice),
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

#[derive(Clone)]
pub struct WBuf {
    slices: Vec<Slice>,
    buf: Vec<u8>,
    contiguous: bool,
    capacity: usize,
    copy_pos: (usize, usize),  // (index in slices, index in the slice)
    mark: (Vec<Slice>, usize), // (backup of slices, len of buf)
}

impl WBuf {
    pub fn new(capacity: usize, contiguous: bool) -> WBuf {
        let buf = Vec::with_capacity(capacity);
        let slices = [Slice::Internal(0, None)].to_vec();
        WBuf {
            mark: (slices.clone(), 0),
            slices,
            buf,
            contiguous,
            capacity,
            copy_pos: (0, 0),
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
        self.buf.is_empty() && self.slices.iter().find(|s| s.is_external()).is_none()
    }

    pub fn clear(&mut self) {
        self.buf.clear();
        self.slices.clear();
        self.slices.push(Slice::Internal(0, None));
        self.copy_pos = (0, 0);
        self.mark = (self.slices.clone(), 0);
    }

    pub fn as_arcslices(&self) -> Vec<ArcSlice> {
        let arc_buf = Arc::new(self.buf.clone());
        if self.contiguous {
            if !self.buf.is_empty() {
                [ArcSlice::from(arc_buf)].to_vec()
            } else {
                [].to_vec()
            }
        } else {
            self.slices
                .iter()
                .map(|s| match s {
                    Slice::External(arcs) => arcs.clone(),
                    Slice::Internal(start, Some(end)) => {
                        ArcSlice::new(arc_buf.clone().into(), *start, *end)
                    }
                    Slice::Internal(start, None) => {
                        ArcSlice::new(arc_buf.clone().into(), *start, arc_buf.len())
                    }
                })
                .filter(|s| !s.is_empty())
                .collect()
        }
    }

    pub fn as_ioslices(&self) -> Vec<IoSlice> {
        if self.contiguous {
            if !self.buf.is_empty() {
                [IoSlice::new(&self.buf[..])].to_vec()
            } else {
                [].to_vec()
            }
        } else {
            self.slices
                .iter()
                .map(|s| match s {
                    Slice::External(arcs) => arcs.as_ioslice(),
                    Slice::Internal(start, Some(end)) => IoSlice::new(&self.buf[*start..*end]),
                    Slice::Internal(start, None) => IoSlice::new(&self.buf[*start..]),
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
            panic!("Cannot return 1st wlice of WBuf as mutable: it's an external ArcSlice");
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
            panic!("Cannot return 1st wlice of WBuf as mutable: it's an external ArcSlice");
        }
    }

    fn get_slice_to_copy(&self) -> &[u8] {
        match self.slices.get(self.copy_pos.0) {
            Some(Slice::External(ref s)) => s.as_slice(),
            Some(Slice::Internal(start, Some(end))) => &self.buf[*start..*end],
            Some(Slice::Internal(start, None)) => &self.buf[*start..],
            None => panic!("Shouln't happen: copy_pos.0 is out of bound in {:?}", self),
        }
    }

    pub fn copy_into_slice(&mut self, dest: &mut [u8]) {
        if self.copy_pos.0 >= self.slices.len() {
            panic!("Not enough bytes to copy into dest");
        }
        let src = self.get_slice_to_copy();
        let dest_len = dest.len();
        if src.len() - self.copy_pos.1 >= dest_len {
            // Copy a sub-part of src into dest
            let end_pos = self.copy_pos.1 + dest_len;
            dest.copy_from_slice(&src[self.copy_pos.1..end_pos]);
            // Move copy_pos
            if end_pos < src.len() {
                self.copy_pos.1 = end_pos
            } else {
                self.copy_pos = (self.copy_pos.0 + 1, 0);
            }
        } else {
            // Copy the remaining of src into dest
            let copy_len = src.len() - self.copy_pos.1;
            dest[..copy_len].copy_from_slice(&src[self.copy_pos.1..]);
            // Move copy_pos to next slice and recurse
            self.copy_pos = (self.copy_pos.0 + 1, 0);
            self.copy_into_slice(&mut dest[copy_len..]);
        }
    }

    pub fn copy_into_wbuf(&mut self, dest: &mut WBuf, dest_len: usize) {
        if self.copy_pos.0 >= self.slices.len() {
            panic!("Not enough bytes to copy into dest");
        }
        let src = self.get_slice_to_copy();
        if src.len() - self.copy_pos.1 >= dest_len {
            // Copy a sub-part of src into dest
            let end_pos = self.copy_pos.1 + dest_len;
            if !(dest.write_bytes(&src[self.copy_pos.1..end_pos])) {
                panic!("Failed to copy bytes into wbuf: destination is probably not big enough");
            };
            // Move copy_pos
            if end_pos < src.len() {
                self.copy_pos.1 = end_pos
            } else {
                self.copy_pos = (self.copy_pos.0 + 1, 0);
            }
        } else {
            // Copy the remaining of src into dest
            let copy_len = src.len() - self.copy_pos.1;
            if !(dest.write_bytes(&src[self.copy_pos.1..])) {
                panic!("Failed to copy bytes into wbuf: destination is probably not big enough");
            };
            // Move copy_pos to next slice and recurse
            self.copy_pos = (self.copy_pos.0 + 1, 0);
            self.copy_into_wbuf(dest, dest_len - copy_len);
        }
    }

    #[inline]
    pub fn mark(&mut self) {
        self.mark = (self.slices.clone(), self.buf.len());
    }

    #[inline]
    pub fn revert(&mut self) {
        // restaure slices and truncate buf to saved len
        self.slices = self.mark.0.clone();
        self.buf.truncate(self.mark.1);
    }

    #[inline]
    fn can_write_in_buf(&self, size: usize) -> bool {
        // We can write in buf if
        //   - non contiguous
        //   - OR: writing won't exceed buf capacity
        !self.contiguous || self.buf.len() + size <= self.buf.capacity()
    }

    pub fn write(&mut self, b: u8) -> bool {
        if self.can_write_in_buf(1) {
            self.buf.push(b);
            true
        } else {
            false
        }
    }

    // NOTE: this is different from write_slice() as this makes a copy of bytes into WBuf.
    pub fn write_bytes(&mut self, s: &[u8]) -> bool {
        if self.can_write_in_buf(s.len()) {
            self.buf.extend_from_slice(s);
            true
        } else {
            false
        }
    }

    // NOTE: if not-contiguous, this is 0-copy (the slice is just added to slices list)
    //       otherwise, it's a copy into buf, if doesn't exceed the capacity.
    pub fn write_slice(&mut self, slice: ArcSlice) -> bool {
        if !self.contiguous {
            // If last slice was an internal without end, set it
            if let Some(&mut Slice::Internal(start, None)) = self.slices.last_mut() {
                self.slices.pop();
                self.slices
                    .push(Slice::Internal(start, Some(self.buf.len())));
            }
            // Push the ArcSlice in slices list
            self.slices.push(Slice::External(slice));
            // Push a new internal slice ready for future writes
            self.slices.push(Slice::Internal(self.buf.len(), None));
            true
        } else if self.buf.len() + slice.len() <= self.buf.capacity() {
            // Copy the ArcSlice into buf
            self.buf.extend_from_slice(slice.as_slice());
            true
        } else {
            false
        }
    }
}

impl io::Write for WBuf {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.write_bytes(buf) {
            Ok(buf.len())
        } else {
            Ok(0)
        }
    }

    #[inline]
    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        if self.write_bytes(buf) {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "failed to write whole buffer",
            ))
        }
    }

    #[inline]
    fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        let mut nwritten = 0;
        for buf in bufs {
            if self.write_slice(buf.into()) {
                nwritten += buf.len();
            } else {
                break;
            }
        }
        Ok(nwritten)
    }

    #[inline]
    fn is_write_vectored(&self) -> bool {
        true
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! to_vec_vec {
        ($wbuf:ident) => {
            $wbuf
                .as_ioslices()
                .iter()
                .map(|s| s.to_vec())
                .collect::<Vec<Vec<u8>>>()
        };
    }

    #[test]
    fn wbuf_contiguous_new() {
        let buf = WBuf::new(10, true);
        assert_eq!(buf.len(), 0);
        assert!(buf.is_empty());
        assert_eq!(buf.capacity(), 10);
        assert!(to_vec_vec!(buf).is_empty());
    }

    #[test]
    fn wbuf_contiguous_write() {
        let mut buf = WBuf::new(2, true);
        assert!(buf.write(0));
        assert_eq!(buf.len(), 1);
        assert!(!buf.is_empty());
        assert_eq!(buf.capacity(), 2);
        assert_eq!(to_vec_vec!(buf), [[0]]);

        assert!(buf.write(1));
        assert_eq!(buf.len(), 2);
        assert!(!buf.is_empty());
        assert_eq!(buf.capacity(), 2);
        assert_eq!(to_vec_vec!(buf), [[0, 1]]);

        assert!(!buf.write(2));
        assert_eq!(buf.len(), 2);
        assert!(!buf.is_empty());
        assert_eq!(buf.capacity(), 2);
        assert_eq!(to_vec_vec!(buf), [[0, 1]]);

        // also test clear()
        buf.clear();
        assert_eq!(buf.len(), 0);
        assert!(buf.is_empty());
        assert_eq!(buf.capacity(), 2);
        assert!(to_vec_vec!(buf).is_empty());
    }

    #[test]
    fn wbuf_contiguous_write_bytes() {
        let mut buf = WBuf::new(6, true);
        assert!(buf.write_bytes(&[0, 1, 2]));
        assert_eq!(buf.len(), 3);
        assert_eq!(buf.capacity(), 6);
        assert_eq!(to_vec_vec!(buf), [[0, 1, 2]]);

        assert!(buf.write_bytes(&[3, 4]));
        assert_eq!(buf.len(), 5);
        assert_eq!(buf.capacity(), 6);
        assert_eq!(to_vec_vec!(buf), [[0, 1, 2, 3, 4]]);

        assert!(!buf.write_bytes(&[5, 6]));
        assert_eq!(buf.len(), 5);
        assert_eq!(buf.capacity(), 6);
        assert_eq!(to_vec_vec!(buf), [[0, 1, 2, 3, 4]]);

        assert!(buf.write_bytes(&[5]));
        assert_eq!(buf.len(), 6);
        assert_eq!(buf.capacity(), 6);
        assert_eq!(to_vec_vec!(buf), [[0, 1, 2, 3, 4, 5]]);

        // also test clear()
        buf.clear();
        assert_eq!(buf.len(), 0);
        assert!(buf.is_empty());
        assert_eq!(buf.capacity(), 6);
        assert!(to_vec_vec!(buf).is_empty());
    }

    #[test]
    fn wbuf_contiguous_write_slice() {
        let mut buf = WBuf::new(6, true);
        assert!(buf.write_slice(ArcSlice::from(&[0u8, 1, 2] as &[u8])));
        assert_eq!(buf.len(), 3);
        assert_eq!(buf.capacity(), 6);
        assert_eq!(to_vec_vec!(buf), [[0, 1, 2]]);

        assert!(buf.write_slice(ArcSlice::from(&[3u8, 4] as &[u8])));
        assert_eq!(buf.len(), 5);
        assert_eq!(buf.capacity(), 6);
        assert_eq!(to_vec_vec!(buf), [[0, 1, 2, 3, 4]]);

        assert!(!buf.write_slice(ArcSlice::from(&[5u8, 6] as &[u8])));
        assert_eq!(buf.len(), 5);
        assert_eq!(buf.capacity(), 6);
        assert_eq!(to_vec_vec!(buf), [[0, 1, 2, 3, 4]]);

        assert!(buf.write_slice(ArcSlice::from(&[5u8] as &[u8])));
        assert_eq!(buf.len(), 6);
        assert_eq!(buf.capacity(), 6);
        assert_eq!(to_vec_vec!(buf), [[0, 1, 2, 3, 4, 5]]);
    }

    #[test]
    fn wbuf_contiguous_get_first_slice_mut() {
        let mut buf = WBuf::new(10, true);
        // reserve 2 bytes writing 0x00
        assert!(buf.write(0));
        assert!(buf.write(0));

        // write a payload
        assert!(buf.write_bytes(&[1, 2, 3, 4, 5]));

        // prepend size in 2 bytes
        let prefix: &mut [u8] = buf.get_first_slice_mut(..2);
        prefix[0] = 5;
        prefix[1] = 0;

        assert_eq!(to_vec_vec!(buf), [[5, 0, 1, 2, 3, 4, 5]]);
    }

    #[test]
    fn wbuf_contiguous_mark_reset() {
        let mut buf = WBuf::new(6, true);
        assert!(buf.write_bytes(&[0, 1, 2]));
        buf.revert();
        assert!(to_vec_vec!(buf).is_empty());

        assert!(buf.write_bytes(&[0, 1, 2]));
        buf.mark();
        assert!(buf.write_bytes(&[3, 4]));
        buf.revert();
        assert_eq!(to_vec_vec!(buf), [[0, 1, 2]]);

        assert!(buf.write_bytes(&[3, 4]));
        assert!(!buf.write_bytes(&[5, 6]));
        buf.revert();
        assert_eq!(to_vec_vec!(buf), [[0, 1, 2]]);

        assert!(buf.write_bytes(&[3, 4]));
        buf.mark();
        assert!(!buf.write_bytes(&[5, 6]));
        buf.revert();
        assert_eq!(to_vec_vec!(buf), [[0, 1, 2, 3, 4]]);
    }

    #[test]
    fn wbuf_contiguous_copy_into_slice() {
        let mut buf = WBuf::new(6, true);
        assert!(buf.write_slice(ArcSlice::from(&[0u8, 1, 2, 3, 4, 5] as &[u8])));

        let mut copy = vec![0; 10];
        buf.copy_into_slice(&mut copy[0..3]);
        assert_eq!(copy, &[0, 1, 2, 0, 0, 0, 0, 0, 0, 0]);
        buf.copy_into_slice(&mut copy[3..6]);
        assert_eq!(copy, &[0, 1, 2, 3, 4, 5, 0, 0, 0, 0]);
    }

    #[test]
    fn wbuf_contiguous_copy_into_wbuf() {
        let mut buf = WBuf::new(6, true);
        assert!(buf.write_slice(ArcSlice::from(&[0u8, 1, 2, 3, 4, 5] as &[u8])));

        let mut copy = WBuf::new(10, true);
        buf.copy_into_wbuf(&mut copy, 3);
        assert_eq!(to_vec_vec!(copy), &[[0, 1, 2,]]);
        buf.copy_into_wbuf(&mut copy, 3);
        assert_eq!(to_vec_vec!(copy), &[[0, 1, 2, 3, 4, 5]]);
    }

    #[test]
    fn wbuf_noncontiguous_new() {
        let buf = WBuf::new(10, false);
        assert_eq!(buf.len(), 0);
        assert!(buf.is_empty());
        assert_eq!(buf.capacity(), 10);
        assert!(to_vec_vec!(buf).is_empty());
    }

    #[test]
    fn wbuf_noncontiguous_write() {
        let mut buf = WBuf::new(2, false);
        assert!(buf.write(0));
        assert_eq!(buf.len(), 1);
        assert!(!buf.is_empty());
        assert_eq!(buf.capacity(), 2);
        assert_eq!(to_vec_vec!(buf), [[0]]);

        assert!(buf.write(1));
        assert_eq!(buf.len(), 2);
        assert!(!buf.is_empty());
        assert_eq!(buf.capacity(), 2);
        assert_eq!(to_vec_vec!(buf), [[0, 1]]);

        assert!(buf.write(2));
        assert_eq!(buf.len(), 3);
        assert!(!buf.is_empty());
        assert!(buf.capacity() > 2);
        assert_eq!(to_vec_vec!(buf), [[0, 1, 2]]);

        // also test clear()
        buf.clear();
        assert_eq!(buf.len(), 0);
        assert!(buf.is_empty());
        assert!(buf.capacity() > 2);
        assert!(to_vec_vec!(buf).is_empty());
    }

    #[test]
    fn wbuf_noncontiguous_write_bytes() {
        let mut buf = WBuf::new(6, false);
        assert!(buf.write_bytes(&[0, 1, 2]));
        assert_eq!(buf.len(), 3);
        assert_eq!(buf.capacity(), 6);
        assert_eq!(to_vec_vec!(buf), [[0, 1, 2]]);

        assert!(buf.write_bytes(&[3, 4]));
        assert_eq!(buf.len(), 5);
        assert_eq!(buf.capacity(), 6);
        assert_eq!(to_vec_vec!(buf), [[0, 1, 2, 3, 4]]);

        assert!(buf.write_bytes(&[5, 6]));
        assert_eq!(buf.len(), 7);
        assert!(buf.capacity() > 6);
        assert_eq!(to_vec_vec!(buf), [[0, 1, 2, 3, 4, 5, 6]]);

        // also test clear()
        buf.clear();
        assert_eq!(buf.len(), 0);
        assert!(buf.is_empty());
        assert!(buf.capacity() > 6);
        assert!(to_vec_vec!(buf).is_empty());
    }

    #[test]
    fn wbuf_noncontiguous_write_slice() {
        let mut buf = WBuf::new(6, false);
        assert!(buf.write_slice(ArcSlice::from(&[0u8, 1, 2] as &[u8])));
        assert_eq!(buf.len(), 3);
        assert_eq!(buf.capacity(), 6);
        assert_eq!(to_vec_vec!(buf), [[0, 1, 2]]);

        assert!(buf.write_slice(ArcSlice::from(&[3u8, 4] as &[u8])));
        assert_eq!(buf.len(), 5);
        assert_eq!(buf.capacity(), 6);
        assert_eq!(to_vec_vec!(buf), [vec![0, 1, 2], vec![3, 4]]);

        assert!(buf.write_slice(ArcSlice::from(&[5u8, 6] as &[u8])));
        assert_eq!(buf.len(), 7);
        assert_eq!(buf.capacity(), 6);
        assert_eq!(to_vec_vec!(buf), [vec![0, 1, 2], vec![3, 4], vec![5, 6]]);

        assert!(buf.write_slice(ArcSlice::from(&[7u8] as &[u8])));
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
        assert!(buf.write(0));
        assert!(buf.write(1));
        assert_eq!(to_vec_vec!(buf), [[0, 1]]);

        assert!(buf.write_slice(ArcSlice::from(&[2u8, 3, 4] as &[u8])));
        assert_eq!(to_vec_vec!(buf), [vec![0, 1], vec![2, 3, 4]]);

        assert!(buf.write(5));
        assert!(buf.write_bytes(&[6, 7]));
        assert!(buf.write(8));
        assert_eq!(
            to_vec_vec!(buf),
            [vec![0, 1], vec![2, 3, 4], vec![5, 6, 7, 8]]
        );

        assert!(buf.write_slice(ArcSlice::from(&[9u8, 10, 11] as &[u8])));
        assert_eq!(
            to_vec_vec!(buf),
            [vec![0, 1], vec![2, 3, 4], vec![5, 6, 7, 8], vec![9, 10, 11]]
        );
    }

    #[test]
    fn wbuf_noncontiguous_get_first_slice_mut() {
        let mut buf = WBuf::new(10, false);
        // reserve 2 bytes writing 0x00
        assert!(buf.write(0));
        assert!(buf.write(0));

        // write some bytes
        assert!(buf.write_bytes(&[1, 2, 3, 4, 5]));
        // add an ArcSlice
        assert!(buf.write_slice(ArcSlice::from(&[6u8, 7, 8, 9, 10] as &[u8])));

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
        let mut buf = WBuf::new(6, false);
        assert!(buf.write(0));
        assert!(buf.write(1));
        buf.revert();
        assert!(to_vec_vec!(buf).is_empty());

        assert!(buf.write_bytes(&[0, 1]));
        buf.revert();
        assert!(to_vec_vec!(buf).is_empty());

        assert!(buf.write_slice(ArcSlice::from(&[0u8, 1] as &[u8])));
        buf.revert();
        assert!(to_vec_vec!(buf).is_empty());

        assert!(buf.write_bytes(&[0, 1, 2]));
        buf.mark();
        assert!(buf.write_bytes(&[3, 4]));
        assert!(buf.write_slice(ArcSlice::from(&[5u8, 6] as &[u8])));
        assert!(buf.write(7));
        assert!(buf.write_slice(ArcSlice::from(&[8u8, 9] as &[u8])));
        buf.revert();
        assert_eq!(to_vec_vec!(buf), [[0, 1, 2]]);

        assert!(buf.write_bytes(&[3, 4]));
        buf.mark();
        assert!(buf.write_slice(ArcSlice::from(&[5u8, 6] as &[u8])));
        assert!(buf.write(7));
        assert!(buf.write_slice(ArcSlice::from(&[8u8, 9] as &[u8])));
        buf.revert();
        assert_eq!(to_vec_vec!(buf), [[0, 1, 2, 3, 4]]);

        assert!(buf.write_slice(ArcSlice::from(&[5u8, 6] as &[u8])));
        buf.mark();
        assert!(buf.write(7));
        assert!(buf.write_slice(ArcSlice::from(&[8u8, 9] as &[u8])));
        buf.revert();
        assert_eq!(to_vec_vec!(buf), [vec![0, 1, 2, 3, 4], vec![5, 6]]);

        assert!(buf.write(7));
        buf.mark();
        assert!(buf.write_slice(ArcSlice::from(&[8u8, 9] as &[u8])));
        buf.revert();
        assert_eq!(to_vec_vec!(buf), [vec![0, 1, 2, 3, 4], vec![5, 6], vec![7]]);

        assert!(buf.write_slice(ArcSlice::from(&[8u8, 9] as &[u8])));
        buf.mark();
        assert!(buf.write_slice(ArcSlice::from(&[10u8, 11] as &[u8])));
        buf.revert();
        assert_eq!(
            to_vec_vec!(buf),
            [vec![0, 1, 2, 3, 4], vec![5, 6], vec![7], vec![8, 9]]
        );
    }

    #[test]
    fn wbuf_noncontiguous_copy_into_slice() {
        let mut buf = WBuf::new(6, false);
        assert!(buf.write(0));
        assert!(buf.write(1));
        assert!(buf.write_slice(ArcSlice::from(&[2u8, 3, 4] as &[u8])));
        assert!(buf.write(5));
        assert!(buf.write_bytes(&[6, 7]));
        assert!(buf.write(8));
        assert!(buf.write_slice(ArcSlice::from(&[9u8, 10, 11] as &[u8])));
        assert_eq!(
            to_vec_vec!(buf),
            [vec![0, 1], vec![2, 3, 4], vec![5, 6, 7, 8], vec![9, 10, 11]]
        );

        let mut copy = vec![0; 12];
        buf.copy_into_slice(&mut copy[0..1]);
        assert_eq!(copy, &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        buf.copy_into_slice(&mut copy[1..6]);
        assert_eq!(copy, &[0, 1, 2, 3, 4, 5, 0, 0, 0, 0, 0, 0]);
        buf.copy_into_slice(&mut copy[6..8]);
        assert_eq!(copy, &[0, 1, 2, 3, 4, 5, 6, 7, 0, 0, 0, 0]);
        buf.copy_into_slice(&mut copy[8..12]);
        assert_eq!(copy, &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]);
    }

    #[test]
    fn wbuf_noncontiguous_copy_into_wbuf() {
        let mut buf = WBuf::new(6, false);
        assert!(buf.write(0));
        assert!(buf.write(1));
        assert!(buf.write_slice(ArcSlice::from(&[2u8, 3, 4] as &[u8])));
        assert!(buf.write(5));
        assert!(buf.write_bytes(&[6, 7]));
        assert!(buf.write(8));
        assert!(buf.write_slice(ArcSlice::from(&[9u8, 10, 11] as &[u8])));
        assert_eq!(
            to_vec_vec!(buf),
            [vec![0, 1], vec![2, 3, 4], vec![5, 6, 7, 8], vec![9, 10, 11]]
        );

        let mut copy = WBuf::new(12, true);
        buf.copy_into_wbuf(&mut copy, 1);
        assert_eq!(to_vec_vec!(copy), &[[0]]);
        buf.copy_into_wbuf(&mut copy, 5);
        assert_eq!(to_vec_vec!(copy), &[[0, 1, 2, 3, 4, 5]]);
        buf.copy_into_wbuf(&mut copy, 2);
        assert_eq!(to_vec_vec!(copy), &[[0, 1, 2, 3, 4, 5, 6, 7]]);
        buf.copy_into_wbuf(&mut copy, 4);
        assert_eq!(to_vec_vec!(copy), &[[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]]);
    }
}
