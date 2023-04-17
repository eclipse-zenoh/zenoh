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
use crate::{RCodec, WCodec, Zenoh060, Zenoh060Header};
use core::convert::TryInto;
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{common::imsg, core::Priority, transport::tmsg};

impl<W> WCodec<&Priority, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Priority) -> Self::Output {
        // Header
        let header = tmsg::id::PRIORITY | ((*x as u8) << imsg::HEADER_BITS);
        self.write(&mut *writer, header)?;
        Ok(())
    }
}

impl<R> RCodec<Priority, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Priority, Self::Error> {
        let codec = Zenoh060Header {
            header: self.read(&mut *reader)?,
            ..Default::default()
        };
        codec.read(reader)
    }
}

impl<R> RCodec<Priority, &mut R> for Zenoh060Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, _reader: &mut R) -> Result<Priority, Self::Error> {
        if imsg::mid(self.header) != imsg::id::PRIORITY {
            return Err(DidntRead);
        }

        let priority: Priority = (imsg::flags(self.header) >> imsg::HEADER_BITS)
            .try_into()
            .map_err(|_| DidntRead)?;
        Ok(priority)
    }
}
