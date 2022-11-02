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
};
use zenoh_protocol::{
    common::imsg,
    core::ZenohId,
    transport::{tmsg, Close},
};

impl<W> WCodec<&mut W, &Close> for Zenoh060
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
        zcwrite!(self, writer, header)?;

        // Body
        if let Some(p) = x.zid.as_ref() {
            zcwrite!(self, writer, p)?;
        }
        zcwrite!(self, writer, x.reason)?;
        Ok(())
    }
}

impl<R> RCodec<&mut R, Close> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Close, Self::Error> {
        let codec = Zenoh060RCodec {
            header: zcread!(self, reader)?,
            ..Default::default()
        };
        codec.read(reader)
    }
}

impl<R> RCodec<&mut R, Close> for Zenoh060RCodec
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Close, Self::Error> {
        if imsg::mid(self.header) != imsg::id::CLOSE {
            return Err(DidntRead);
        }

        let link_only = imsg::has_flag(self.header, tmsg::flag::K);
        let zid = if imsg::has_flag(self.header, tmsg::flag::I) {
            let zid: ZenohId = zcread!(self.codec, reader)?;
            Some(zid)
        } else {
            None
        };
        let reason: u8 = zcread!(self.codec, reader)?;

        Ok(Close {
            zid,
            reason,
            link_only,
        })
    }
}
