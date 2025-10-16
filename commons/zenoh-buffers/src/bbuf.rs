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
use alloc::boxed::Box;
use alloc::sync::Arc;
use core::{fmt, num::NonZeroUsize, option};

use crate::{
    buffer::{Buffer, SplitBuffer},
    reader::HasReader,
    vec,
    writer::{BacktrackableWriter, DidntWrite, HasWriter, Writer},
    ZSlice,
};

#[derive(Clone, PartialEq, Eq)]
pub struct BBuf {
    buffer: Box<[u8]>,
    len: usize,
}

impl BBuf {
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buffer: vec::uninit(capacity).into_boxed_slice(),
            len: 0,
        }
    }

    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.buffer.len()
    }

    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        // SAFETY: self.len is ensured by the writer to be smaller than buffer length.
        crate::unsafe_slice!(self.buffer, ..self.len)
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        // SAFETY: self.len is ensured by the writer to be smaller than buffer length.
        crate::unsafe_slice_mut!(self.buffer, ..self.len)
    }

    pub fn clear(&mut self) {
        self.len = 0;
    }

    fn as_writable_slice(&mut self) -> &mut [u8] {
        // SAFETY: self.len is ensured by the writer to be smaller than buffer length.
        crate::unsafe_slice_mut!(self.buffer, self.len..)
    }
}

impl fmt::Debug for BBuf {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:02x?}", self.as_slice())
    }
}

// Buffer
impl Buffer for BBuf {
    fn len(&self) -> usize {
        self.len
    }
}

impl Buffer for &BBuf {
    fn len(&self) -> usize {
        self.len
    }
}

impl Buffer for &mut BBuf {
    fn len(&self) -> usize {
        self.len
    }
}

// SplitBuffer
impl SplitBuffer for BBuf {
    type Slices<'a> = option::IntoIter<&'a [u8]>;

    fn slices(&self) -> Self::Slices<'_> {
        Some(self.as_slice()).into_iter()
    }
}

// Writer
impl HasWriter for &mut BBuf {
    type Writer = Self;

    fn writer(self) -> Self::Writer {
        self
    }
}

impl Writer for BBuf {
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

    unsafe fn with_slot<F>(&mut self, len: usize, write: F) -> Result<NonZeroUsize, DidntWrite>
    where
        F: FnOnce(&mut [u8]) -> usize,
    {
        if self.remaining() < len {
            return Err(DidntWrite);
        }

        // SAFETY: self.remaining() >= len
        let written = write(unsafe { self.as_writable_slice().get_unchecked_mut(..len) });
        self.len += written;

        NonZeroUsize::new(written).ok_or(DidntWrite)
    }
}

impl BacktrackableWriter for BBuf {
    type Mark = usize;

    fn mark(&mut self) -> Self::Mark {
        self.len
    }

    fn rewind(&mut self, mark: Self::Mark) -> bool {
        self.len = mark;
        true
    }
}

#[cfg(feature = "std")]
impl std::io::Write for BBuf {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match <Self as Writer>::write(self, buf) {
            Ok(n) => Ok(n.get()),
            Err(_) => Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "UnexpectedEof",
            )),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

// Reader
impl<'a> HasReader for &'a BBuf {
    type Reader = &'a [u8];

    fn reader(self) -> Self::Reader {
        self.as_slice()
    }
}

// From impls
impl From<BBuf> for ZSlice {
    fn from(value: BBuf) -> Self {
        // SAFETY: buffer length is ensured to be lesser than its capacity
        unsafe { ZSlice::new(Arc::new(value.buffer), 0, value.len).unwrap_unchecked() }
    }
}

#[cfg(feature = "test")]
impl BBuf {
    #[doc(hidden)]
    pub fn rand(len: usize) -> Self {
        #[cfg(not(feature = "std"))]
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
