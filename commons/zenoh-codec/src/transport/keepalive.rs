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
};
use zenoh_protocol::{
    common::imsg,
    core::ZenohId,
    transport::{tmsg, KeepAlive},
};

impl<W> WCodec<&KeepAlive, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &KeepAlive) -> Self::Output {
        // Header
        let mut header = tmsg::id::KEEP_ALIVE;
        if x.zid.is_some() {
            header |= tmsg::flag::I;
        }
        self.write(&mut *writer, header)?;

        // Body
        if let Some(p) = x.zid.as_ref() {
            self.write(&mut *writer, p)?;
        }
        Ok(())
    }
}

impl<R> RCodec<KeepAlive, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<KeepAlive, Self::Error> {
        let codec = Zenoh060Header {
            header: self.read(&mut *reader)?,
            ..Default::default()
        };
        codec.read(reader)
    }
}

impl<R> RCodec<KeepAlive, &mut R> for Zenoh060Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<KeepAlive, Self::Error> {
        if imsg::mid(self.header) != tmsg::id::KEEP_ALIVE {
            return Err(DidntRead);
        }

        let zid = if imsg::has_flag(self.header, tmsg::flag::I) {
            let zid: ZenohId = self.codec.read(&mut *reader)?;
            Some(zid)
        } else {
            None
        };

        Ok(KeepAlive { zid })
    }
}
