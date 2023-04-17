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
    vec,
    writer::{BacktrackableWriter, DidntWrite, HasWriter, Writer},
};
use alloc::boxed::Box;
use core::num::NonZeroUsize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BBuf {
    buffer: Box<[u8]>,
    len: usize,
}

impl BBuf {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buffer: vec::uninit(capacity).into_boxed_slice(),
            len: 0,
        }
    }

    pub fn capacity(&self) -> usize {
        self.buffer.len()
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.buffer[..self.len]
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.buffer[..self.len]
    }

    pub fn clear(&mut self) {
        self.len = 0;
    }

    fn as_writable_slice(&mut self) -> &mut [u8] {
        &mut self.buffer[self.len..]
    }
}

// Writer
impl HasWriter for &mut BBuf {
    type Writer = Self;

    fn writer(self) -> Self::Writer {
        self
    }
}

impl Writer for &mut BBuf {
    fn write(&mut self, bytes: &[u8]) -> Result<NonZeroUsize, DidntWrite> {
        let mut writer = self.as_writable_slice().writer();
        let len = writer.write(bytes)?;
        self.len += len.get();
        Ok(len)
    }

    fn write_exact(&mut self, bytes: &[u8]) -> Result<(), DidntWrite> {
        let mut writer = self.as_writable_slice().writer();
        writer.write_exact(bytes)?;
        self.len += bytes.len();
        Ok(())
    }

    fn remaining(&self) -> usize {
        self.capacity() - self.len()
    }

    fn with_slot<F>(&mut self, len: usize, f: F) -> Result<NonZeroUsize, DidntWrite>
    where
        F: FnOnce(&mut [u8]) -> usize,
    {
        if self.remaining() < len {
            return Err(DidntWrite);
        }

        let written = f(self.as_writable_slice());
        self.len += written;

        NonZeroUsize::new(written).ok_or(DidntWrite)
    }
}

impl BacktrackableWriter for &mut BBuf {
    type Mark = usize;

    fn mark(&mut self) -> Self::Mark {
        self.len
    }

    fn rewind(&mut self, mark: Self::Mark) -> bool {
        self.len = mark;
        true
    }
}

// Reader
impl<'a> HasReader for &'a BBuf {
    type Reader = &'a [u8];

    fn reader(self) -> Self::Reader {
        self.as_slice()
    }
}

#[cfg(feature = "test")]
impl BBuf {
    pub fn rand(len: usize) -> Self {
        use alloc::vec::Vec;
        use rand::Rng;

        let mut rng = rand::thread_rng();
        let buffer = (0..len)
            .map(|_| rng.gen())
            .collect::<Vec<u8>>()
            .into_boxed_slice();

        Self { buffer, len }
    }
}
