use crate::{
    traits::{
        reader::{HasReader, Reader},
        writer::{DidntWrite, Writer},
    },
    ZSlice,
};

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

impl Reader for &[u8] {
    fn read(&mut self, into: &mut [u8]) -> usize {
        let len = self.len().min(into.len());
        into[..len].copy_from_slice(&self[..len]);
        *self = &self[len..];
        len
    }

    fn read_exact(&mut self, into: &mut [u8]) -> bool {
        let len = into.len();
        if self.len() < len {
            false
        } else {
            into.copy_from_slice(&self[..len]);
            *self = &self[len..];
            true
        }
    }

    fn read_u8(&mut self) -> Option<u8> {
        if self.can_read() {
            let ret = self[0];
            *self = &self[1..];
            Some(ret)
        } else {
            None
        }
    }

    type ZSliceIterator = std::option::IntoIter<ZSlice>;
    fn read_zslices(&mut self, len: usize) -> Self::ZSliceIterator {
        self.read_zslice(len).into_iter()
    }
    #[allow(clippy::uninit_vec)]
    // SAFETY: the buffer is initialized by the `read_exact()` function. Should the `read_exact()`
    // function fail, the `read_zslice()` will fail as well and return None. It is hence guaranteed
    // that any `ZSlice` returned by `read_zslice()` points to a fully initialized buffer.
    // Therefore, it is safe to suppress the `clippy::uninit_vec` lint.
    fn read_zslice(&mut self, len: usize) -> Option<ZSlice> {
        // We'll be truncating the vector immediately, and u8 is Copy and therefore doesn't have `Drop`
        let mut buffer: Vec<u8> = Vec::with_capacity(len);
        unsafe {
            buffer.set_len(len);
        }
        self.read_exact(&mut buffer).then_some(buffer.into())
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
