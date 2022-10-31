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
use crate::*;
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
    ZBuf,
};
use zenoh_protocol::{
    common::{imsg, Attachment},
    transport::tmsg,
};

impl<W> WCodec<&mut W, &Attachment> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Attachment) -> Self::Output {
        // Header
        let header = tmsg::id::ATTACHMENT;
        // #[cfg(feature = "shared-memory")]
        // if self.buffer.has_shminfo() {
        //     header |= tmsg::flag::Z;
        // }
        self.write(&mut *writer, header)?;

        // #[cfg(feature = "shared-memory")]
        // {
        //     // self.write_zbuf(&attachment.buffer, attachment.buffer.has_shminfo())
        //     self.write_zbuf(&attachment.buffer, false)
        // }

        // #[cfg(not(feature = "shared-memory"))]
        // {
        //     self.write_zbuf(&attachment.buffer)
        // }

        // Body
        self.write(&mut *writer, &x.buffer)
    }
}

impl<R> RCodec<&mut R, Attachment> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Attachment, Self::Error> {
        let codec = Zenoh060RCodec {
            header: self.read(&mut *reader)?,
            ..Default::default()
        };
        codec.read(reader)
    }
}

impl<R> RCodec<&mut R, Attachment> for Zenoh060RCodec
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Attachment, Self::Error> {
        if imsg::mid(self.header) != imsg::id::ATTACHMENT {
            return Err(DidntRead);
        }

        // #[cfg(feature = "shared-memory")]
        // {
        //     let buffer = self.read_zbuf(imsg::has_flag(header, tmsg::flag::Z))?;
        //     Some(Attachment { buffer })
        // }

        // #[cfg(not(feature = "shared-memory"))]
        // {
        //     let buffer = self.read_zbuf()?;
        //     Some(Attachment { buffer })
        // }

        let buffer: ZBuf = self.codec.read(&mut *reader)?;
        Ok(Attachment { buffer })
    }
}
