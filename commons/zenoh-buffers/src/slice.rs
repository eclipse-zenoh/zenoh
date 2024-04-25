// Copyright (c) 2024 ZettaScale Technology
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

use crate::{
    buffer::{Buffer, SplitBuffer},
    reader::{BacktrackableReader, DidntRead, DidntSiphon, HasReader, Reader, SiphonableReader},
    writer::{BacktrackableWriter, DidntWrite, HasWriter, Writer},
    ZSlice,
};
use core::{
    marker::PhantomData,
    mem,
    num::NonZeroUsize,
    option,
    slice::{self},
};

// Buffer
impl Buffer for &[u8] {
    #[inline(always)]
    fn len(&self) -> usize {
        <[u8]>::len(self)
    }
}

impl Buffer for &mut [u8] {
    #[inline(always)]
    fn len(&self) -> usize {
        <[u8]>::len(self)
    }
}

// SplitBuffer
impl<'b> SplitBuffer for &'b [u8] {
    type Slices<'a> = option::IntoIter<&'a [u8]> where 'b: 'a;

    fn slices(&self) -> Self::Slices<'_> {
        Some(*self).into_iter()
    }
}

// Writer
impl HasWriter for &mut [u8] {
    type Writer = Self;

    fn writer(self) -> Self::Writer {
        self
    }
}

impl Writer for &mut [u8] {
    fn write(&mut self, bytes: &[u8]) -> Result<NonZeroUsize, DidntWrite> {
        let len = bytes.len().min(self.len());
        if len == 0 {
            return Err(DidntWrite);
        }

        // SAFETY: len is guaranteed to be the minimum between lhs and rhs length.
        //         We early return if length is 0.
        let lhs = crate::unsafe_slice_mut!(self, ..len);
        let rhs = crate::unsafe_slice!(bytes, ..len);
        lhs.copy_from_slice(rhs);

        // SAFETY: len is guaranteed to be the minimum between lhs and rhs length.
        let lhs = crate::unsafe_slice_mut!(self, len..);
        // SAFETY: this doesn't compile with simple assignment because the compiler
        // doesn't believe that the subslice has the same lifetime as the original slice,
        // so we transmute to assure it that it does.
        *self = unsafe { mem::transmute(lhs) };

        // SAFETY: this operation is safe since we check if len is non-zero.
        Ok(unsafe { NonZeroUsize::new_unchecked(len) })
    }

    fn write_exact(&mut self, bytes: &[u8]) -> Result<(), DidntWrite> {
        let len = bytes.len();
        if self.len() < len {
            return Err(DidntWrite);
        }

        // SAFETY: len is guaranteed to be the smaller than lhs length.
        let lhs = crate::unsafe_slice_mut!(self, ..len);
        let rhs = crate::unsafe_slice!(bytes, ..len);
        lhs.copy_from_slice(rhs);

        // SAFETY: len is guaranteed to be the minimum between lhs and rhs length.
        let lhs = crate::unsafe_slice_mut!(self, len..);
        // SAFETY: this doesn't compile with simple assignment because the compiler
        // doesn't believe that the subslice has the same lifetime as the original slice,
        // so we transmute to assure it that it does.
        *self = unsafe { mem::transmute(lhs) };

        Ok(())
    }

    fn remaining(&self) -> usize {
        self.len()
    }

    fn with_slot<F>(&mut self, mut len: usize, f: F) -> Result<NonZeroUsize, DidntWrite>
    where
        F: FnOnce(&mut [u8]) -> usize,
    {
        if len > self.len() {
            return Err(DidntWrite);
        }
        // SAFETY: we early return in case len is greater than slice.len().
        let s = crate::unsafe_slice_mut!(self, ..len);
        len = f(s);
        // SAFETY: we early return in case len is greater than slice.len().
        let s = crate::unsafe_slice_mut!(self, len..);
        // SAFETY: this doesn't compile with simple assignment because the compiler
        // doesn't believe that the subslice has the same lifetime as the original slice,
        // so we transmute to assure it that it does.
        *self = unsafe { mem::transmute(s) };

        NonZeroUsize::new(len).ok_or(DidntWrite)
    }
}

pub struct SliceMark<'s> {
    ptr: *const u8,
    len: usize,
    // Enforce lifetime for the pointer
    _phantom: PhantomData<&'s u8>,
}

impl<'s> BacktrackableWriter for &'s mut [u8] {
    type Mark = SliceMark<'s>;

    fn mark(&mut self) -> Self::Mark {
        SliceMark {
            ptr: self.as_ptr(),
            len: self.len(),
            _phantom: PhantomData,
        }
    }

    fn rewind(&mut self, mark: Self::Mark) -> bool {
        // SAFETY: SliceMark's lifetime is bound to the slice's lifetime
        *self = unsafe { slice::from_raw_parts_mut(mark.ptr as *mut u8, mark.len) };
        true
    }
}

// Reader
impl<'a> HasReader for &'a [u8] {
    type Reader = Self;

    fn reader(self) -> Self::Reader {
        self
    }
}

impl Reader for &[u8] {
    fn read(&mut self, into: &mut [u8]) -> Result<NonZeroUsize, DidntRead> {
        let len = self.len().min(into.len());
        // SAFETY: len is guaranteed to be the smaller than lhs length.
        let lhs = crate::unsafe_slice_mut!(into, ..len);
        let rhs = crate::unsafe_slice!(self, ..len);
        lhs.copy_from_slice(rhs);
        // SAFETY: len is guaranteed to be smaller than slice.len().
        *self = crate::unsafe_slice!(self, len..);
        NonZeroUsize::new(len).ok_or(DidntRead)
    }

    fn read_exact(&mut self, into: &mut [u8]) -> Result<(), DidntRead> {
        let len = into.len();
        if self.len() < len {
            return Err(DidntRead);
        }
        // SAFETY: len is guaranteed to be the smaller than lhs length.
        let lhs = crate::unsafe_slice_mut!(into, ..len);
        let rhs = crate::unsafe_slice!(self, ..len);
        lhs.copy_from_slice(rhs);
        // SAFETY: len is guaranteed to be smaller than slice.len().
        *self = crate::unsafe_slice!(self, len..);
        Ok(())
    }

    fn read_u8(&mut self) -> Result<u8, DidntRead> {
        if !self.can_read() {
            return Err(DidntRead);
        }
        // SAFETY: we early return in case the slice is empty.
        //         Therefore, there is at least one element in the slice.
        let ret = *crate::unsafe_slice!(self, 0);
        *self = crate::unsafe_slice!(self, 1..);
        Ok(ret)
    }

    fn read_zslices<F: FnMut(ZSlice)>(&mut self, len: usize, mut f: F) -> Result<(), DidntRead> {
        let zslice = self.read_zslice(len)?;
        f(zslice);
        Ok(())
    }

    fn read_zslice(&mut self, len: usize) -> Result<ZSlice, DidntRead> {
        // SAFETY: the buffer is initialized by the `read_exact()` function. Should the `read_exact()`
        // function fail, the `read_zslice()` will fail as well and return None. It is hence guaranteed
        // that any `ZSlice` returned by `read_zslice()` points to a fully initialized buffer.
        let mut buffer = crate::vec::uninit(len);
        self.read_exact(&mut buffer)?;
        Ok(buffer.into())
    }

    fn remaining(&self) -> usize {
        self.len()
    }

    fn can_read(&self) -> bool {
        !self.is_empty()
    }
}

impl<'a> BacktrackableReader for &'a [u8] {
    type Mark = &'a [u8];

    fn mark(&mut self) -> Self::Mark {
        self
    }

    fn rewind(&mut self, mark: Self::Mark) -> bool {
        *self = mark;
        true
    }
}

impl<'a> SiphonableReader for &'a [u8] {
    fn siphon<W>(&mut self, writer: &mut W) -> Result<NonZeroUsize, DidntSiphon>
    where
        W: Writer,
    {
        let res = writer.write(self).map_err(|_| DidntSiphon);
        if let Ok(len) = res {
            // SAFETY: len is returned from the writer, therefore it means
            //         len amount of bytes have been written to the slice.
            *self = crate::unsafe_slice!(self, len.get()..);
        }
        res
    }
}
