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
use crate::{RCodec, WCodec, Zenoh060};
use alloc::string::String;
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::core::{Encoding, ZInt};

impl<W> WCodec<&Encoding, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Encoding) -> Self::Output {
        self.write(&mut *writer, u8::from(*x.prefix()))?;
        self.write(&mut *writer, x.suffix())?;
        Ok(())
    }
}

impl<R> RCodec<Encoding, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Encoding, Self::Error> {
        let prefix: ZInt = self.read(&mut *reader)?;
        let suffix: String = self.read(&mut *reader)?;
        let encoding = Encoding::new(prefix, suffix).ok_or(DidntRead)?;
        Ok(encoding)
    }
}
