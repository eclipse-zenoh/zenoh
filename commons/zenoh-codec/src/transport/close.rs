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
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{
    common::imsg,
    transport::{
        close::{flag, Close},
        id,
    },
};

use crate::{common::extension, RCodec, WCodec, Zenoh080, Zenoh080Header};

impl<W> WCodec<&Close, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Close) -> Self::Output {
        let Close { reason, session } = x;

        // Header
        let mut header = id::CLOSE;
        if *session {
            header |= flag::S;
        }
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, reason)?;

        Ok(())
    }
}

impl<R> RCodec<Close, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Close, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);

        codec.read(reader)
    }
}

impl<R> RCodec<Close, &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Close, Self::Error> {
        if imsg::mid(self.header) != id::CLOSE {
            return Err(DidntRead);
        }
        let session = imsg::has_flag(self.header, flag::S);

        // Body
        let reason: u8 = self.codec.read(&mut *reader)?;

        // Extensions
        let has_ext = imsg::has_flag(self.header, flag::Z);
        if has_ext {
            extension::skip_all(reader, "Close")?;
        }

        Ok(Close { reason, session })
    }
}
