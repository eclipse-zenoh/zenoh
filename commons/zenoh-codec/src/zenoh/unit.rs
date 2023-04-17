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
use crate::{RCodec, WCodec, Zenoh060, Zenoh060Header, Zenoh060HeaderReplyContext};
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{
    common::imsg,
    core::CongestionControl,
    zenoh::{zmsg, Unit},
};

impl<W> WCodec<&Unit, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Unit) -> Self::Output {
        if let Some(reply_context) = x.reply_context.as_ref() {
            self.write(&mut *writer, reply_context)?;
        }

        // Header
        let mut header = zmsg::id::UNIT;
        if x.congestion_control == CongestionControl::Drop {
            header |= zmsg::flag::D;
        }
        self.write(&mut *writer, header)?;

        // Body
        Ok(())
    }
}

impl<R> RCodec<Unit, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Unit, Self::Error> {
        let mut codec = Zenoh060HeaderReplyContext {
            header: self.read(&mut *reader)?,
            ..Default::default()
        };
        if imsg::mid(codec.header) == zmsg::id::REPLY_CONTEXT {
            let hodec = Zenoh060Header {
                header: codec.header,
                ..Default::default()
            };
            codec.reply_context = Some(hodec.read(&mut *reader)?);
            codec.header = self.read(&mut *reader)?;
        }
        codec.read(reader)
    }
}

impl<R> RCodec<Unit, &mut R> for Zenoh060HeaderReplyContext
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, _reader: &mut R) -> Result<Unit, Self::Error> {
        if imsg::mid(self.header) != zmsg::id::UNIT {
            return Err(DidntRead);
        }

        let congestion_control = if imsg::has_flag(self.header, zmsg::flag::D) {
            CongestionControl::Drop
        } else {
            CongestionControl::Block
        };
        Ok(Unit {
            congestion_control,
            reply_context: self.reply_context,
        })
    }
}
