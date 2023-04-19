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
    transport::{tmsg, Close},
};

impl<W> WCodec<&Close, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Close) -> Self::Output {
        // Header
        let mut header = tmsg::id::CLOSE;
        if x.zid.is_some() {
            header |= tmsg::flag::I;
        }
        if x.link_only {
            header |= tmsg::flag::K;
        }
        self.write(&mut *writer, header)?;

        // Body
        if let Some(p) = x.zid.as_ref() {
            self.write(&mut *writer, p)?;
        }
        self.write(&mut *writer, x.reason)?;
        Ok(())
    }
}

impl<R> RCodec<Close, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Close, Self::Error> {
        let codec = Zenoh060Header {
            header: self.read(&mut *reader)?,
            ..Default::default()
        };
        codec.read(reader)
    }
}

impl<R> RCodec<Close, &mut R> for Zenoh060Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Close, Self::Error> {
        if imsg::mid(self.header) != tmsg::id::CLOSE {
            return Err(DidntRead);
        }

        let link_only = imsg::has_flag(self.header, tmsg::flag::K);
        let zid = if imsg::has_flag(self.header, tmsg::flag::I) {
            let zid: ZenohId = self.codec.read(&mut *reader)?;
            Some(zid)
        } else {
            None
        };
        let reason: u8 = self.codec.read(&mut *reader)?;

        Ok(Close {
            zid,
            reason,
            link_only,
        })
    }
}
