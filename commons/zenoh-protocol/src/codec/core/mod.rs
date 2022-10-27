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
mod attachment;
mod locator;
mod zbuf;
mod zenohid;
mod zint;

use crate::codec::*;
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};

// u8
impl<W> WCodec<&mut W, u8> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: u8) -> Self::Output {
        writer.write_u8(x)
    }
}

impl<R> RCodec<&mut R, u8> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<u8, Self::Error> {
        reader.read_u8()
    }
}

// &[u8]
impl<W> WCodec<&mut W, &[u8]> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &[u8]) -> Self::Output {
        self.write(&mut *writer, x.len())?;
        writer.write_exact(x)
    }
}

impl<R> RCodec<&mut R, Vec<u8>> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    #[allow(clippy::uninit_vec)]
    fn read(self, reader: &mut R) -> Result<Vec<u8>, Self::Error> {
        let len: usize = self.read(&mut *reader)?;

        let mut buff = Vec::with_capacity(len);
        unsafe {
            buff.set_len(len);
        }
        reader.read_exact(&mut buff[..])?;
        Ok(buff)
    }
}

// &str / String
impl<W> WCodec<&mut W, &str> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &str) -> Self::Output {
        self.write(&mut *writer, x.as_bytes())
    }
}

impl<R> RCodec<&mut R, String> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<String, Self::Error> {
        let vec: Vec<u8> = self.read(&mut *reader)?;
        String::from_utf8(vec).map_err(|_| DidntRead)
    }
}
