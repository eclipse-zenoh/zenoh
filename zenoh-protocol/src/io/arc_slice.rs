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
use async_std::sync::Arc;
use std::fmt;
use std::io::IoSlice;

#[derive(Clone)]
pub struct ArcSlice {
    buf: Arc<Vec<u8>>,
    start: usize,
    end: usize,
}

impl ArcSlice {
    pub fn new(buf: Arc<Vec<u8>>, start: usize, end: usize) -> ArcSlice {
        assert!(end <= buf.len());
        ArcSlice { buf, start, end }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        &self.buf[self.start..self.end]
    }

    #[inline]
    pub fn as_ioslice(&self) -> IoSlice {
        IoSlice::new(self.as_slice())
    }

    pub fn get_sub_slice(&self, start: usize, end: usize) -> &[u8] {
        assert!(end <= self.len());
        &self.buf[self.start + start..self.start + end]
    }

    pub fn new_sub_slice(&self, start: usize, end: usize) -> ArcSlice {
        assert!(end <= self.len());
        ArcSlice {
            buf: self.buf.clone(),
            start: self.start + start,
            end: self.start + end,
        }
    }
}

impl std::ops::Index<usize> for ArcSlice {
    type Output = u8;

    fn index(&self, index: usize) -> &Self::Output {
        std::ops::Index::index(&**self.buf, index + self.start)
    }
}

impl fmt::Display for ArcSlice {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:02x?}", self.as_slice())
    }
}

impl fmt::Debug for ArcSlice {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "ArcSlice{{ start: {}, end:{}, buf:\n {:02x?} \n}}",
            self.start,
            self.end,
            &self.buf[..]
        )
    }
}

impl From<Arc<Vec<u8>>> for ArcSlice {
    fn from(buf: Arc<Vec<u8>>) -> ArcSlice {
        let len = buf.len();
        ArcSlice::new(buf, 0, len)
    }
}

impl From<Vec<u8>> for ArcSlice {
    fn from(buf: Vec<u8>) -> ArcSlice {
        ArcSlice::from(Arc::new(buf))
    }
}

impl From<&[u8]> for ArcSlice {
    fn from(buf: &[u8]) -> ArcSlice {
        ArcSlice::from(buf.to_vec())
    }
}

impl<'a> From<&IoSlice<'a>> for ArcSlice {
    fn from(buf: &IoSlice) -> ArcSlice {
        ArcSlice::from(buf.to_vec())
    }
}

impl PartialEq for ArcSlice {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl Eq for ArcSlice {}
