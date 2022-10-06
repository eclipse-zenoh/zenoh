//
// Copyright (c) 2022 ZettaScale Technology
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
use std::borrow::Cow;

pub mod buffer {
    pub trait ConstructibleBuffer {
        /// Constructs a split buffer that may accept `slice_capacity` segments without allocating.
        /// It may also accept receiving cached writes for `cache_capacity` bytes before needing to reallocate its cache.
        fn with_capacities(slice_capacity: usize, cache_capacity: usize) -> Self;
    }
    pub trait BoundedBuffer {
        /// Indicates how many bytes can still be written to the buffer before it may reject further writes.
        fn remaining_capacity(&self) -> usize;
    }

    pub trait Indexable {
        type Index;
        /// Returns an index-like mark that may be used for future operations at the current buffer-end
        fn get_index(&self) -> Self::Index;
        /// Replaces the data following `from` with the contents of `with`.
        ///
        /// This is an exact size operation, to avoid invalidating other marks that may have been made.
        fn replace(&mut self, from: &Self::Index, with: &[u8]) -> bool;
    }

    #[derive(Debug, Clone, Copy)]
    pub struct DidntWrite;
    pub trait Buffer {
        fn write(&mut self, bytes: &[u8]) -> Result<(), DidntWrite>;
        fn write_byte(&mut self, byte: u8) -> Result<(), DidntWrite>;
        fn append(&mut self, slice: crate::zslice::ZSlice) -> Result<(), DidntWrite> {
            self.write(slice.as_slice())
        }
    }
}

pub mod writer {
    pub trait Writer {
        type Buffer;
    }
    pub trait BacktrackableWriter {
        fn mark(&mut self);
        fn revert(&mut self) -> bool;
    }
    pub trait HasWriter {
        type Writer: Writer;
        /// Returns the most appropriate writer for `self`
        fn writer(self) -> Self::Writer;
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

pub mod reader {
    use crate::ZSlice;
    pub trait Reader {
        fn read(&mut self, into: &mut [u8]) -> usize;
        #[must_use = "returns true upon success"]
        fn read_exact(&mut self, into: &mut [u8]) -> bool;
        fn read_byte(&mut self) -> Option<u8> {
            let mut byte = 0;
            (self.read(std::slice::from_mut(&mut byte)) != 0).then_some(byte)
        }
        fn remaining(&self) -> usize;
        fn can_read(&self) -> bool {
            self.remaining() != 0
        }
        /// Reads the current ZSlice up to len, updating the reader by the size of the returned ZSlice
        fn read_zslice(&mut self, len: usize) -> Option<ZSlice> {
            // We'll be truncating the vector immediately, and u8 is Copy and therefore doesn't have `Drop`
            let mut buffer: Vec<u8> = Vec::with_capacity(len);
            let b = buffer.spare_capacity_mut();
            unsafe {
                let len = self.read(std::mem::transmute(b));
                if len == 0 {
                    return None;
                }
                buffer.set_len(len);
            }
            Some(buffer.into())
        }
    }
    pub trait HasReader {
        type Reader: Reader;
        /// Returns the most appropriate reader for `self`
        fn reader(self) -> Self::Reader;
    }
}
