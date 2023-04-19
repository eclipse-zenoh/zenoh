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
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
    ZBuf,
};
use zenoh_protocol::{
    common::{imsg, Attachment},
    transport::tmsg,
};
#[cfg(feature = "shared-memory")]
use {crate::Zenoh060Condition, core::any::TypeId, zenoh_shm::SharedMemoryBufInfoSerialized};

impl<W> WCodec<&Attachment, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Attachment) -> Self::Output {
        // Header
        #[allow(unused_mut)] // mut required with #[cfg(feature = "shared-memory")]
        let mut header = tmsg::id::ATTACHMENT;

        #[cfg(feature = "shared-memory")]
        {
            let has_shminfo = x
                .buffer
                .zslices()
                .any(|s| s.buf.as_any().type_id() == TypeId::of::<SharedMemoryBufInfoSerialized>());
            if has_shminfo {
                header |= tmsg::flag::Z;
            }
        }

        self.write(&mut *writer, header)?;

        // Body
        #[cfg(feature = "shared-memory")]
        {
            let codec = Zenoh060Condition::new(imsg::has_flag(header, tmsg::flag::Z));
            codec.write(&mut *writer, &x.buffer)
        }
        #[cfg(not(feature = "shared-memory"))]
        {
            self.write(&mut *writer, &x.buffer)
        }
    }
}

impl<R> RCodec<Attachment, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Attachment, Self::Error> {
        let codec = Zenoh060Header {
            header: self.read(&mut *reader)?,
            ..Default::default()
        };
        codec.read(reader)
    }
}

impl<R> RCodec<Attachment, &mut R> for Zenoh060Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Attachment, Self::Error> {
        if imsg::mid(self.header) != imsg::id::ATTACHMENT {
            return Err(DidntRead);
        }

        let buffer: ZBuf = {
            #[cfg(feature = "shared-memory")]
            {
                let codec = Zenoh060Condition::new(imsg::has_flag(self.header, tmsg::flag::Z));
                codec.read(&mut *reader)?
            }
            #[cfg(not(feature = "shared-memory"))]
            {
                self.codec.read(&mut *reader)?
            }
        };

        Ok(Attachment { buffer })
    }
}
