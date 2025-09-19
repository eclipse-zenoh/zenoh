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
use core::{
    marker::PhantomData,
    mem,
    num::NonZeroUsize,
    option,
    slice::{self},
};

use crate::{
    buffer::{Buffer, SplitBuffer},
    reader::{BacktrackableReader, DidntRead, DidntSiphon, HasReader, Reader, SiphonableReader},
    writer::{BacktrackableWriter, DidntWrite, HasWriter, Writer},
    ZSlice,
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
    type Slices<'a>
        = option::IntoIter<&'a [u8]>
    where
        'b: 'a;

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
        let Some(len) = NonZeroUsize::new(bytes.len().min(self.len())) else {
            return Err(DidntWrite);
        };
        let (to_write, remain) = mem::take(self).split_at_mut(len.get());
        to_write.copy_from_slice(&bytes[..len.get()]);
        *self = remain;
        Ok(len)
    }

    fn write_exact(&mut self, bytes: &[u8]) -> Result<(), DidntWrite> {
        let len = bytes.len();
        if self.len() < len {
            return Err(DidntWrite);
        }
        let _ = self.write(bytes);
        Ok(())
    }

    fn remaining(&self) -> usize {
        self.len()
    }

    unsafe fn with_slot<F>(&mut self, len: usize, write: F) -> Result<NonZeroUsize, DidntWrite>
    where
        F: FnOnce(&mut [u8]) -> usize,
    {
        if len > self.len() {
            return Err(DidntWrite);
        }
        let written = write(&mut self[..len]);
        // SAFETY: `written` < `len` is guaranteed by function contract
        *self = unsafe { mem::take(self).get_unchecked_mut(written..) };
        NonZeroUsize::new(written).ok_or(DidntWrite)
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
impl HasReader for &[u8] {
    type Reader = Self;

    fn reader(self) -> Self::Reader {
        self
    }
}

impl Reader for &[u8] {
    fn read(&mut self, into: &mut [u8]) -> Result<NonZeroUsize, DidntRead> {
        let Some(len) = NonZeroUsize::new(self.len().min(into.len())) else {
            return Err(DidntRead);
        };
        let (to_write, remain) = self.split_at(len.get());
        into[..len.get()].copy_from_slice(to_write);
        *self = remain;
        Ok(len)
    }

    fn read_exact(&mut self, into: &mut [u8]) -> Result<(), DidntRead> {
        let len = into.len();
        if self.len() < len {
            return Err(DidntRead);
        }
        let (to_write, remain) = self.split_at(len);
        into[..len].copy_from_slice(to_write);
        *self = remain;
        Ok(())
    }

    fn remaining(&self) -> usize {
        self.len()
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

    fn read_u8(&mut self) -> Result<u8, DidntRead> {
        let mut buf = [0; 1];
        self.read(&mut buf)?;
        Ok(buf[0])
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

impl SiphonableReader for &[u8] {
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
