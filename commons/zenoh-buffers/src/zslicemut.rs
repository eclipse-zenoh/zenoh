//
// Copyright (c) 2023 ZettaScale Technology
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

use core::ops::{Deref, DerefMut, Range};

/*************************************/
/*             ZSLICEMUT             */
/*************************************/
pub struct ZSliceMut<'a> {
    slice: &'a mut [u8],
    start: &'a mut usize,
    end: &'a mut usize,
}

impl<'a> ZSliceMut<'a> {
    pub(crate) fn new(slice: &'a mut [u8], start: &'a mut usize, end: &'a mut usize) -> Self {
        Self { slice, start, end }
    }

    pub fn range(&self) -> Range<usize> {
        *self.start..*self.end
    }

    pub fn set_range(&mut self, range: Range<usize>) -> bool {
        match self.slice.len() >= range.end {
            true => {
                *self.start = range.start;
                *self.end = range.end;
                true
            }
            false => false,
        }
    }

    pub fn available_len(&self) -> usize {
        self.slice.len()
    }

    pub fn len(&self) -> usize {
        *self.end - *self.start
    }

    #[inline]
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        *self.end == *self.start
    }
}

// TODO: add deallocate locking mechanism here
impl<'a> Drop for ZSliceMut<'a> {
    fn drop(&mut self) {}
}

impl<'a> Deref for ZSliceMut<'a> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        crate::unsafe_slice!(self.slice, *self.start..*self.end)
    }
}

impl<'a> DerefMut for ZSliceMut<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        crate::unsafe_slice_mut!(self.slice, *self.start..*self.end)
    }
}

impl<'a> AsRef<[u8]> for ZSliceMut<'a> {
    fn as_ref(&self) -> &[u8] {
        self
    }
}

impl<'a> AsMut<[u8]> for ZSliceMut<'a> {
    fn as_mut(&mut self) -> &mut [u8] {
        self
    }
}
