use crate::traits::{
    buffer::Buffer,
    buffer::DidntWrite,
    reader::{HasReader, Reader},
};
impl Buffer for Vec<u8> {
    fn write(&mut self, bytes: &[u8]) -> Result<(), DidntWrite> {
        self.extend_from_slice(bytes);
        Ok(())
    }
    fn write_byte(&mut self, byte: u8) -> Result<(), DidntWrite> {
        self.push(byte);
        Ok(())
    }
}
impl Reader for &[u8] {
    fn can_read(&self) -> bool {
        !self.is_empty()
    }
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
            true
        }
    }
    fn read_byte(&mut self) -> Option<u8> {
        if self.is_empty() {
            None
        } else {
            let ret = self[0];
            *self = &self[1..];
            Some(ret)
        }
    }
    fn remaining(&self) -> usize {
        self.len()
    }
}
impl HasReader for &[u8] {
    type Reader = Self;
    fn reader(self) -> Self::Reader {
        self
    }
}
