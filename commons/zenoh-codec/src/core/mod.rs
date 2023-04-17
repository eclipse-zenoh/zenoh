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
mod encoding;
mod endpoint;
mod keyexpr;
mod locator;
mod property;
mod timestamp;
mod zbuf;
mod zenohid;
mod zint;
mod zslice;

use crate::{RCodec, WCodec, Zenoh060};
use alloc::{string::String, vec::Vec};
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};

// u8
impl<W> WCodec<u8, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: u8) -> Self::Output {
        writer.write_u8(x)
    }
}

impl<W> WCodec<&u8, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &u8) -> Self::Output {
        self.write(writer, *x)
    }
}

impl<R> RCodec<u8, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<u8, Self::Error> {
        reader.read_u8()
    }
}

// &[u8] / Vec<u8>
impl<W> WCodec<&[u8], &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &[u8]) -> Self::Output {
        self.write(&mut *writer, x.len())?;
        if x.is_empty() {
            Ok(())
        } else {
            writer.write_exact(x)
        }
    }
}

impl<R> RCodec<Vec<u8>, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    #[allow(clippy::uninit_vec)]
    fn read(self, reader: &mut R) -> Result<Vec<u8>, Self::Error> {
        let len: usize = self.read(&mut *reader)?;
        let mut buff = zenoh_buffers::vec::uninit(len);
        if len != 0 {
            reader.read_exact(&mut buff[..])?;
        }
        Ok(buff)
    }
}

// &str / String
impl<W> WCodec<&str, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &str) -> Self::Output {
        self.write(&mut *writer, x.as_bytes())
    }
}

impl<W> WCodec<&String, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &String) -> Self::Output {
        self.write(&mut *writer, x.as_str())
    }
}

impl<R> RCodec<String, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<String, Self::Error> {
        let vec: Vec<u8> = self.read(&mut *reader)?;
        String::from_utf8(vec).map_err(|_| DidntRead)
    }
}
