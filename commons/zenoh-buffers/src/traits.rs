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
}

pub mod writer {
    #[derive(Debug, Clone, Copy)]
    pub struct DidntWrite;

    pub trait Writer {
        fn write(&mut self, bytes: &[u8]) -> Result<usize, DidntWrite>;
        fn write_exact(&mut self, bytes: &[u8]) -> Result<(), DidntWrite>;
        fn remaining(&self) -> usize;

        fn write_u8(&mut self, byte: u8) -> Result<(), DidntWrite> {
            self.write_exact(std::slice::from_ref(&byte))
        }
        fn write_zslice(&mut self, slice: crate::zslice::ZSlice) -> Result<(), DidntWrite> {
            self.write_exact(slice.as_slice())
        }
        fn can_write(&self) -> bool {
            self.remaining() != 0
        }
        /// Provides a buffer of exactly `len` uninitialized bytes to `f` to allow in-place writing.
        /// `f` must return the number of bytes it actually wrote.
        fn with_slot<F: FnOnce(&mut [u8]) -> usize>(
            &mut self,
            len: usize,
            f: F,
        ) -> Result<usize, DidntWrite>;
    }

    pub trait ReserveWriter<'a, const LEN: usize> {
        // type Reserver
        // where
        //     (Reserver, &mut Self): Writer;
        type Mark: 'a;

        fn reserve<
            Len,
            F: FnOnce(Reservation<'a, Self::Mark, Len>) -> Reservation<'a, Self::Mark, typenum::Z0>,
        >(
            &mut self,
        );
    }
    pub struct Reservation<'a, Writer, Len> {
        len: std::marker::PhantomData<Len>,
        marker: std::marker::PhantomData<fn(&'a Writer) -> &'a Writer>,
    }

    pub trait BacktrackableWriter {
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

    pub trait Reader {
        fn read(&mut self, into: &mut [u8]) -> usize;
        #[must_use = "returns true upon success"]
        fn read_exact(&mut self, into: &mut [u8]) -> bool;
        fn remaining(&self) -> usize;

        fn read_u8(&mut self) -> Option<u8> {
            let mut byte = 0;
            (self.read(std::slice::from_mut(&mut byte)) != 0).then_some(byte)
        }
        type ZSliceIterator: IntoIterator<Item = ZSlice>;
        /// Returns an iterator of ZSlices such that the sum of their length is _exactly_ `len`.
        fn read_zslices(&mut self, len: usize) -> Self::ZSliceIterator;
        fn can_read(&self) -> bool {
            self.remaining() != 0
        }
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
