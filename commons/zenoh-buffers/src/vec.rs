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
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
use core::{mem, num::NonZeroUsize, option};

use crate::{
    buffer::{Buffer, SplitBuffer},
    reader::HasReader,
    writer::{BacktrackableWriter, DidntWrite, HasWriter, Writer},
};

/// Allocate a vector with a given capacity and sets the length to that capacity.
#[must_use]
pub fn uninit(capacity: usize) -> Vec<u8> {
    let mut vbuf = Vec::with_capacity(capacity);
    // SAFETY: this operation is safe since we are setting the length equal to the allocated capacity.
    #[allow(clippy::uninit_vec)]
    unsafe {
        vbuf.set_len(capacity);
    }
    vbuf
}

// Buffer
impl Buffer for Vec<u8> {
    fn len(&self) -> usize {
        Vec::len(self)
    }
}

impl Buffer for &Vec<u8> {
    fn len(&self) -> usize {
        Vec::len(self)
    }
}

impl Buffer for &mut Vec<u8> {
    fn len(&self) -> usize {
        Vec::len(self)
    }
}

// SplitBuffer
impl SplitBuffer for Vec<u8> {
    type Slices<'a> = option::IntoIter<&'a [u8]>;

    fn slices(&self) -> Self::Slices<'_> {
        Some(self.as_slice()).into_iter()
    }
}

// Writer
impl HasWriter for &mut Vec<u8> {
    type Writer = Self;

    fn writer(self) -> Self::Writer {
        self
    }
}

impl Writer for Vec<u8> {
    fn write(&mut self, bytes: &[u8]) -> Result<NonZeroUsize, DidntWrite> {
        if bytes.is_empty() {
            return Err(DidntWrite);
        }
        self.extend_from_slice(bytes);
        // SAFETY: this operation is safe since we early return in case bytes is empty
        Ok(unsafe { NonZeroUsize::new_unchecked(bytes.len()) })
    }

    fn write_exact(&mut self, bytes: &[u8]) -> Result<(), DidntWrite> {
        self.write(bytes).map(|_| ())
    }

    fn remaining(&self) -> usize {
        usize::MAX
    }

    fn write_u8(&mut self, byte: u8) -> Result<(), DidntWrite> {
        self.push(byte);
        Ok(())
    }

    unsafe fn with_slot<F>(&mut self, mut len: usize, write: F) -> Result<NonZeroUsize, DidntWrite>
    where
        F: FnOnce(&mut [u8]) -> usize,
    {
        self.reserve(len);

        // SAFETY: we already reserved len elements on the vector.
        let s = crate::unsafe_slice_mut!(self.spare_capacity_mut(), ..len);
        // SAFETY: converting MaybeUninit<u8> into [u8] is safe because we are going to write on it.
        //         The returned len tells us how many bytes have been written so as to update the len accordingly.
        len = unsafe { write(&mut *(s as *mut [mem::MaybeUninit<u8>] as *mut [u8])) };
        // SAFETY: we already reserved len elements on the vector.
        unsafe { self.set_len(self.len() + len) };

        NonZeroUsize::new(len).ok_or(DidntWrite)
    }
}

impl BacktrackableWriter for Vec<u8> {
    type Mark = usize;

    fn mark(&mut self) -> Self::Mark {
        self.len()
    }

    fn rewind(&mut self, mark: Self::Mark) -> bool {
        self.truncate(mark);
        true
    }
}

// Reader
impl<'a> HasReader for &'a Vec<u8> {
    type Reader = &'a [u8];

    fn reader(self) -> Self::Reader {
        self
    }
}
