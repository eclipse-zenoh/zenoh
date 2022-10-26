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
use crate::{
    reader::{DidntRead, HasReader, Reader},
    writer::{BacktrackableWriter, DidntWrite, Writer},
    ZSlice,
};
use std::marker::PhantomData;

impl Writer for Vec<u8> {
    fn write(&mut self, bytes: &[u8]) -> Result<usize, DidntWrite> {
        self.extend_from_slice(bytes);
        Ok(bytes.len())
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

    fn with_slot<F: FnOnce(&mut [u8]) -> usize>(
        &mut self,
        mut len: usize,
        f: F,
    ) -> Result<(), DidntWrite> {
        self.reserve(len);
        unsafe {
            len = f(std::mem::transmute(&mut self.spare_capacity_mut()[..len]));
            self.set_len(self.len() + len);
        }
        Ok(())
    }
}

impl Writer for &mut [u8] {
    fn write(&mut self, bytes: &[u8]) -> Result<usize, DidntWrite> {
        let len = bytes.len().min(self.len());
        self[..len].copy_from_slice(&bytes[..len]);
        // Safety: this doesn't compile with simple assignment because the compiler
        // doesn't believe that the subslice has the same lifetime as the original slice,
        // so we transmute to assure it that it does.
        *self = unsafe { std::mem::transmute(&mut self[len..]) };
        Ok(len)
    }

    fn write_exact(&mut self, bytes: &[u8]) -> Result<(), DidntWrite> {
        let len = bytes.len();
        if self.len() < len {
            Err(DidntWrite)
        } else {
            self[..len].copy_from_slice(&bytes[..len]);
            // Safety: this doesn't compile with simple assignment because the compiler
            // doesn't believe that the subslice has the same lifetime as the original slice,
            // so we transmute to assure it that it does.
            *self = unsafe { std::mem::transmute(&mut self[len..]) };
            Ok(())
        }
    }

    fn remaining(&self) -> usize {
        self.len()
    }

    fn with_slot<F: FnOnce(&mut [u8]) -> usize>(
        &mut self,
        mut len: usize,
        f: F,
    ) -> Result<(), DidntWrite> {
        if len > self.len() {
            return Err(DidntWrite);
        }
        len = f(&mut self[..len]);
        // Safety: this doesn't compile with simple assignment because the compiler
        // doesn't believe that the subslice has the same lifetime as the original slice,
        // so we transmute to assure it that it does.
        *self = unsafe { std::mem::transmute(&mut self[len..]) };
        Ok(())
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

pub struct SliceMark<'s> {
    ptr: *const u8,
    len: usize,
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
        *self = unsafe { std::slice::from_raw_parts_mut(mark.ptr as *mut u8, mark.len) };
        true
    }
}

impl Reader for &[u8] {
    fn read(&mut self, into: &mut [u8]) -> Result<usize, DidntRead> {
        let len = self.len().min(into.len());
        into[..len].copy_from_slice(&self[..len]);
        *self = &self[len..];
        Ok(len)
    }

    fn read_exact(&mut self, into: &mut [u8]) -> Result<(), DidntRead> {
        let len = into.len();
        if self.len() < len {
            Err(DidntRead)
        } else {
            into.copy_from_slice(&self[..len]);
            *self = &self[len..];
            Ok(())
        }
    }

    fn read_u8(&mut self) -> Result<u8, DidntRead> {
        if self.can_read() {
            let ret = self[0];
            *self = &self[1..];
            Ok(ret)
        } else {
            Err(DidntRead)
        }
    }

    type ZSliceIterator = std::option::IntoIter<ZSlice>;
    fn read_zslices(&mut self, len: usize) -> Result<Self::ZSliceIterator, DidntRead> {
        let zslice = self.read_zslice(len)?;
        Ok(Some(zslice).into_iter())
    }

    #[allow(clippy::uninit_vec)]
    // SAFETY: the buffer is initialized by the `read_exact()` function. Should the `read_exact()`
    // function fail, the `read_zslice()` will fail as well and return None. It is hence guaranteed
    // that any `ZSlice` returned by `read_zslice()` points to a fully initialized buffer.
    // Therefore, it is safe to suppress the `clippy::uninit_vec` lint.
    fn read_zslice(&mut self, len: usize) -> Result<ZSlice, DidntRead> {
        // We'll be truncating the vector immediately, and u8 is Copy and therefore doesn't have `Drop`
        let mut buffer: Vec<u8> = Vec::with_capacity(len);
        unsafe {
            buffer.set_len(len);
        }
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

impl HasReader for &[u8] {
    type Reader = Self;

    fn reader(self) -> Self::Reader {
        self
    }
}
