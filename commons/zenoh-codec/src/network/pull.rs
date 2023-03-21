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
use crate::{RCodec, WCodec, Zenoh080, Zenoh080Header};
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{
    common::{imsg, ZExtUnknown},
    network::{id, push::flag, Pull},
    zenoh::PullId,
};

impl<W> WCodec<&Pull, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Pull) -> Self::Output {
        // Header
        let header = id::PULL;
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, x.target)?;
        self.write(&mut *writer, x.id)?;

        Ok(())
    }
}

impl<R> RCodec<Pull, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Pull, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);
        codec.read(reader)
    }
}

impl<R> RCodec<Pull, &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Pull, Self::Error> {
        if imsg::mid(self.header) != id::PULL {
            return Err(DidntRead);
        }

        // Body
        let target: u8 = self.codec.read(&mut *reader)?;
        let id: PullId = self.codec.read(&mut *reader)?;

        // Extensions
        let mut has_ext = imsg::has_flag(self.header, flag::Z);
        while has_ext {
            let (_, ext): (ZExtUnknown, bool) = self.codec.read(&mut *reader)?;
            has_ext = ext;
        }

        Ok(Pull { target, id })
    }
}
