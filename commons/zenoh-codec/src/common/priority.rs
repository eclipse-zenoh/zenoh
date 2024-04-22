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

use crate::{RCodec, WCodec, Zenoh080, Zenoh080Header};
use core::convert::TryInto;
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{common::imsg, core::Priority};

impl<W> WCodec<&Priority, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Priority) -> Self::Output {
        // Header
        let header = imsg::id::PRIORITY | ((*x as u8) << imsg::HEADER_BITS);
        self.write(&mut *writer, header)?;
        Ok(())
    }
}

impl<R> RCodec<Priority, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Priority, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);

        codec.read(reader)
    }
}

impl<R> RCodec<Priority, &mut R> for Zenoh080Header
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
