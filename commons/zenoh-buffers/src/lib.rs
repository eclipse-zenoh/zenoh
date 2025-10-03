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
//! [Click here for Zenoh's documentation](https://docs.rs/zenoh/latest/zenoh)
//!
//! Provide different buffer implementations used for serialization and deserialization.
#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;

mod bbuf;
mod slice;
pub mod vec;
mod zbuf;
mod zslice;

pub use bbuf::*;
pub use zbuf::*;
pub use zslice::*;

// SAFETY: this crate operates on eventually initialized slices for read and write. Because of that, internal buffers
//         implementation keeps track of various slices indexes. Boundaries checks are performed by individual
//         implementations every time they need to access a slices. This means, that accessing a slice with [<range>]
//         syntax after having already verified the indexes will force the compiler to verify again the slice
//         boundaries. In case of access violation the program will panic. However, it is desirable to avoid redundant
//         checks for performance reasons. Nevertheless, it is desirable to keep those checks for testing and debugging
//         purposes. Hence, the macros below will allow to switch boundaries check in case of test and to avoid them in
//         all the other cases.
#[cfg(any(test, feature = "test"))]
#[macro_export]
macro_rules! unsafe_slice {
    ($s:expr,$r:expr) => {
        &$s[$r]
    };
}

#[cfg(any(test, feature = "test"))]
#[macro_export]
macro_rules! unsafe_slice_mut {
    ($s:expr,$r:expr) => {
        &mut $s[$r]
    };
}

#[cfg(all(not(test), not(feature = "test")))]
#[macro_export]
macro_rules! unsafe_slice {
    ($s:expr,$r:expr) => {{
        let slice = &*$s;
        let index = $r;
        unsafe { slice.get_unchecked(index) }
    }};
}

#[cfg(all(not(test), not(feature = "test")))]
#[macro_export]
macro_rules! unsafe_slice_mut {
    ($s:expr,$r:expr) => {{
        let slice = &mut *$s;
        let index = $r;
        unsafe { slice.get_unchecked_mut(index) }
    }};
}

pub mod buffer {
    use alloc::{borrow::Cow, vec::Vec};

    pub trait Buffer {
        /// Returns the number of bytes in the buffer.
        fn len(&self) -> usize;

        /// Returns `true` if the buffer has a length of 0.
        fn is_empty(&self) -> bool {
            self.len() == 0
        }
    }

    /// A trait for buffers that can be composed of multiple non contiguous slices.
    pub trait SplitBuffer: Buffer {
        type Slices<'a>: Iterator<Item = &'a [u8]> + ExactSizeIterator
        where
            Self: 'a;

        /// Gets all the slices of this buffer.
        fn slices(&self) -> Self::Slices<'_>;

        /// Returns all the bytes of this buffer in a conitguous slice.
        /// This may require allocation and copy if the original buffer
        /// is not contiguous.
        fn contiguous(&self) -> Cow<'_, [u8]> {
            let mut slices = self.slices();
            match slices.len() {
                0 => Cow::Borrowed(b""),
                1 => {
                    // SAFETY: unwrap here is safe because we have explicitly checked
                    //         the iterator has 1 element.
                    Cow::Borrowed(unsafe { slices.next().unwrap_unchecked() })
                }
                _ => Cow::Owned(slices.fold(Vec::with_capacity(self.len()), |mut acc, it| {
                    acc.extend_from_slice(it);
                    acc
                })),
            }
        }
    }
}

pub mod writer {
    use core::num::NonZeroUsize;

    use crate::ZSlice;

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
        /// Provides a buffer of exactly `len` uninitialized bytes to `write` to allow in-place writing.
        /// `write` must return the number of bytes it actually wrote.
        ///
        /// # Safety
        ///
        /// Caller must ensure that `write` return an integer lesser than or equal to the length of
        /// the slice passed in argument
        unsafe fn with_slot<F>(&mut self, len: usize, write: F) -> Result<NonZeroUsize, DidntWrite>
        where
            F: FnOnce(&mut [u8]) -> usize;
    }

    impl<W: Writer + ?Sized> Writer for &mut W {
        fn write(&mut self, bytes: &[u8]) -> Result<NonZeroUsize, DidntWrite> {
            (**self).write(bytes)
        }
        fn write_exact(&mut self, bytes: &[u8]) -> Result<(), DidntWrite> {
            (**self).write_exact(bytes)
        }
        fn remaining(&self) -> usize {
            (**self).remaining()
        }
        fn write_u8(&mut self, byte: u8) -> Result<(), DidntWrite> {
            (**self).write_u8(byte)
        }
        fn write_zslice(&mut self, slice: &ZSlice) -> Result<(), DidntWrite> {
            (**self).write_zslice(slice)
        }
        fn can_write(&self) -> bool {
            (**self).can_write()
        }
        unsafe fn with_slot<F>(&mut self, len: usize, write: F) -> Result<NonZeroUsize, DidntWrite>
        where
            F: FnOnce(&mut [u8]) -> usize,
        {
            // SAFETY: same precondition
            unsafe { (**self).with_slot(len, write) }
        }
    }

    pub trait BacktrackableWriter: Writer {
        type Mark;

        fn mark(&mut self) -> Self::Mark;
        fn rewind(&mut self, mark: Self::Mark) -> bool;
    }

    impl<W: BacktrackableWriter + ?Sized> BacktrackableWriter for &mut W {
        type Mark = W::Mark;
        fn mark(&mut self) -> Self::Mark {
            (**self).mark()
        }
        fn rewind(&mut self, mark: Self::Mark) -> bool {
            (**self).rewind(mark)
        }
    }

    pub trait HasWriter {
        type Writer: Writer;

        /// Returns the most appropriate writer for `self`
        fn writer(self) -> Self::Writer;
    }
}

pub mod reader {
    use core::num::NonZeroUsize;

    use crate::ZSlice;

    #[derive(Debug, Clone, Copy)]
    pub struct DidntRead;

    pub trait Reader {
        fn read(&mut self, into: &mut [u8]) -> Result<NonZeroUsize, DidntRead>;
        fn read_exact(&mut self, into: &mut [u8]) -> Result<(), DidntRead>;
        fn remaining(&self) -> usize;

        /// Returns an iterator of `ZSlices` such that the sum of their length is _exactly_ `len`.
        fn read_zslices<F: FnMut(ZSlice)>(
            &mut self,
            len: usize,
            for_each_slice: F,
        ) -> Result<(), DidntRead>;

        /// Reads exactly `len` bytes, returning them as a single `ZSlice`.
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

    impl<R: Reader + ?Sized> Reader for &mut R {
        fn read(&mut self, into: &mut [u8]) -> Result<NonZeroUsize, DidntRead> {
            (**self).read(into)
        }
        fn read_exact(&mut self, into: &mut [u8]) -> Result<(), DidntRead> {
            (**self).read_exact(into)
        }
        fn remaining(&self) -> usize {
            (**self).remaining()
        }
        fn read_zslices<F: FnMut(ZSlice)>(
            &mut self,
            len: usize,
            for_each_slice: F,
        ) -> Result<(), DidntRead> {
            (**self).read_zslices(len, for_each_slice)
        }
        fn read_zslice(&mut self, len: usize) -> Result<ZSlice, DidntRead> {
            (**self).read_zslice(len)
        }
        fn read_u8(&mut self) -> Result<u8, DidntRead> {
            (**self).read_u8()
        }
        fn can_read(&self) -> bool {
            (**self).can_read()
        }
    }

    pub trait BacktrackableReader: Reader {
        type Mark;

        fn mark(&mut self) -> Self::Mark;
        fn rewind(&mut self, mark: Self::Mark) -> bool;
    }

    impl<R: BacktrackableReader + ?Sized> BacktrackableReader for &mut R {
        type Mark = R::Mark;
        fn mark(&mut self) -> Self::Mark {
            (**self).mark()
        }
        fn rewind(&mut self, mark: Self::Mark) -> bool {
            (**self).rewind(mark)
        }
    }

    pub trait AdvanceableReader: Reader {
        fn skip(&mut self, offset: usize) -> Result<(), DidntRead>;
        fn backtrack(&mut self, offset: usize) -> Result<(), DidntRead>;
        fn advance(&mut self, offset: isize) -> Result<(), DidntRead> {
            if offset > 0 {
                self.skip(offset as usize)
            } else {
                self.backtrack((-offset) as usize)
            }
        }
    }

    #[derive(Debug, Clone, Copy)]
    pub struct DidntSiphon;

    pub trait SiphonableReader: Reader {
        fn siphon<W>(&mut self, writer: &mut W) -> Result<NonZeroUsize, DidntSiphon>
        where
            W: crate::writer::Writer;
    }

    pub trait HasReader {
        type Reader: Reader;

        /// Returns the most appropriate reader for `self`
        fn reader(self) -> Self::Reader;
    }
}
