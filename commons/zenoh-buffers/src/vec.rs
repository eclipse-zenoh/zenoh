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
use crate::{
    reader::HasReader,
    writer::{BacktrackableWriter, DidntWrite, HasWriter, Writer},
};
use alloc::vec::Vec;
use core::{mem, num::NonZeroUsize};

/// Allocate a vector with a given capacity and sets the length to that capacity.
pub fn uninit(capacity: usize) -> Vec<u8> {
    let mut vbuf = Vec::with_capacity(capacity);
    #[allow(clippy::uninit_vec)]
    unsafe {
        vbuf.set_len(capacity);
    }
    vbuf
}

// Writer
impl<'a> HasWriter for &'a mut Vec<u8> {
    type Writer = Self;

    fn writer(self) -> Self::Writer {
        self
    }
}

impl Writer for &mut Vec<u8> {
    fn write(&mut self, bytes: &[u8]) -> Result<NonZeroUsize, DidntWrite> {
        if bytes.is_empty() {
            return Err(DidntWrite);
        }
        self.extend_from_slice(bytes);
        // Safety: this operation is safe since we early return if bytes is empty
        Ok(unsafe { NonZeroUsize::new_unchecked(bytes.len()) })
    }

    fn write_exact(&mut self, bytes: &[u8]) -> Result<(), DidntWrite> {
        self.write(bytes).map(|_| ())
    }

    fn write_u8(&mut self, byte: u8) -> Result<(), DidntWrite> {
        self.push(byte);
        Ok(())
    }

    fn remaining(&self) -> usize {
        usize::MAX
    }

    fn with_slot<F>(&mut self, mut len: usize, f: F) -> Result<NonZeroUsize, DidntWrite>
    where
        F: FnOnce(&mut [u8]) -> usize,
    {
        self.reserve(len);
        unsafe {
            len = f(mem::transmute(&mut self.spare_capacity_mut()[..len]));
            self.set_len(self.len() + len);
        }
        NonZeroUsize::new(len).ok_or(DidntWrite)
    }
}

impl BacktrackableWriter for &mut Vec<u8> {
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
