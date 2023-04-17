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

//! ⚠️ WARNING ⚠️
//!
//! This crate is intended for Zenoh's internal use.
//!
//! [Click here for Zenoh's documentation](../zenoh/index.html)
//!
//! Provide different buffer implementations used for serialization and deserialization.
#![no_std]
extern crate alloc;

mod bbuf;
mod slice;
pub mod vec;
mod zbuf;
mod zslice;

use alloc::{borrow::Cow, vec::Vec};
pub use bbuf::*;
pub use zbuf::*;
pub use zslice::*;

pub mod writer {
    use crate::ZSlice;
    use core::num::NonZeroUsize;

    #[derive(Debug, Clone, Copy)]
    pub struct DidntWrite;

    pub trait Writer {
        fn write(&mut self, bytes: &[u8]) -> Result<NonZeroUsize, DidntWrite>;
        fn write_exact(&mut self, bytes: &[u8]) -> Result<(), DidntWrite>;
        fn remaining(&self) -> usize;

        fn write_u8(&mut self, byte: u8) -> Result<(), DidntWrite> {
            self.write_exact(core::slice::from_ref(&byte))
        }
        fn write_zslice(&mut self, slice: &ZSlice) -> Result<(), DidntWrite> {
            self.write_exact(slice.as_slice())
        }
        fn can_write(&self) -> bool {
            self.remaining() != 0
        }
        /// Provides a buffer of exactly `len` uninitialized bytes to `f` to allow in-place writing.
        /// `f` must return the number of bytes it actually wrote.
        fn with_slot<F>(&mut self, len: usize, f: F) -> Result<NonZeroUsize, DidntWrite>
        where
            F: FnOnce(&mut [u8]) -> usize;
    }
    pub trait BacktrackableWriter: Writer {
        type Mark;

        fn mark(&mut self) -> Self::Mark;
        fn rewind(&mut self, mark: Self::Mark) -> bool;
    }

    pub trait HasWriter {
        type Writer: Writer;

        /// Returns the most appropriate writer for `self`
        fn writer(self) -> Self::Writer;
    }
}

pub mod reader {
    use crate::ZSlice;
    use core::num::NonZeroUsize;

    #[derive(Debug, Clone, Copy)]
    pub struct DidntRead;

    pub trait Reader {
        fn read(&mut self, into: &mut [u8]) -> Result<NonZeroUsize, DidntRead>;
        fn read_exact(&mut self, into: &mut [u8]) -> Result<(), DidntRead>;
        fn remaining(&self) -> usize;

        /// Returns an iterator of ZSlices such that the sum of their length is _exactly_ `len`.
        fn read_zslices<F: FnMut(ZSlice)>(
            &mut self,
            len: usize,
            for_each_slice: F,
        ) -> Result<(), DidntRead>;

        /// Reads exactly `len` bytes, returning them as a single ZSlice.
        fn read_zslice(&mut self, len: usize) -> Result<ZSlice, DidntRead>;

        fn read_u8(&mut self) -> Result<u8, DidntRead> {
            let mut byte = 0;
            let read = self.read(core::slice::from_mut(&mut byte))?;
            if read.get() == 1 {
                Ok(byte)
            } else {
                Err(DidntRead)
            }
        }

        fn can_read(&self) -> bool {
            self.remaining() != 0
        }
    }

    pub trait BacktrackableReader: Reader {
        type Mark;

        fn mark(&mut self) -> Self::Mark;
        fn rewind(&mut self, mark: Self::Mark) -> bool;
    }

    #[derive(Debug, Clone, Copy)]
    pub struct DidntSiphon;

    pub trait SiphonableReader: Reader {
        fn siphon<W>(&mut self, writer: W) -> Result<NonZeroUsize, DidntSiphon>
        where
            W: crate::writer::Writer;
    }

    pub trait HasReader {
        type Reader: Reader;

        /// Returns the most appropriate reader for `self`
        fn reader(self) -> Self::Reader;
    }
}

/// A trait for buffers that can be composed of multiple non contiguous slices.
pub trait SplitBuffer<'a> {
    type Slices: Iterator<Item = &'a [u8]> + ExactSizeIterator;

    /// Gets all the slices of this buffer.
    fn slices(&'a self) -> Self::Slices;

    /// Returns `true` if the buffer has a length of 0.
    fn is_empty(&'a self) -> bool {
        self.slices().all(|s| s.is_empty())
    }

    /// Returns the number of bytes in the buffer.
    fn len(&'a self) -> usize {
        self.slices().fold(0, |acc, it| acc + it.len())
    }

    /// Returns all the bytes of this buffer in a conitguous slice.
    /// This may require allocation and copy if the original buffer
    /// is not contiguous.
    fn contiguous(&'a self) -> Cow<'a, [u8]> {
        let mut slices = self.slices();
        match slices.len() {
            0 => Cow::Borrowed(b""),
            1 => Cow::Borrowed(slices.next().unwrap()),
            _ => Cow::Owned(slices.fold(Vec::new(), |mut acc, it| {
                acc.extend(it);
                acc
            })),
        }
    }
}
