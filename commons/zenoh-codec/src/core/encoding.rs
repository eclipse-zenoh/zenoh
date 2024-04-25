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

use crate::{LCodec, RCodec, WCodec, Zenoh080, Zenoh080Bounded};
use alloc::string::String;
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::core::Encoding;

impl LCodec<&Encoding> for Zenoh080 {
    fn w_len(self, x: &Encoding) -> usize {
        1 + self.w_len(x.suffix())
    }
}

impl<W> WCodec<&Encoding, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Encoding) -> Self::Output {
        let zodec = Zenoh080Bounded::<u8>::new();
        zodec.write(&mut *writer, *x.prefix() as u8)?;
        zodec.write(&mut *writer, x.suffix())?;
        Ok(())
    }
}

impl<R> RCodec<Encoding, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Encoding, Self::Error> {
        let zodec = Zenoh080Bounded::<u8>::new();
        let prefix: u8 = zodec.read(&mut *reader)?;
        let suffix: String = zodec.read(&mut *reader)?;
        let encoding = Encoding::new(prefix, suffix).map_err(|_| DidntRead)?;
        Ok(encoding)
    }
}
