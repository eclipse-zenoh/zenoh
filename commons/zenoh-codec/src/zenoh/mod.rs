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
mod data;
mod routing;

use crate::*;
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{
    common::{imsg, Attachment},
    zenoh::{ZenohBody, ZenohMessage},
};

impl<W> WCodec<&mut W, &ZenohMessage> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &ZenohMessage) -> Self::Output {
        if let Some(a) = x.attachment.as_ref() {
            self.write(&mut *writer, a)?;
        }

        if let Some(r) = x.routing_context.as_ref() {
            self.write(&mut *writer, r)?;
        }

        match &x.body {
            ZenohBody::Data(d) => self.write(&mut *writer, d),
        }
    }
}

impl<'a, R> RCodec<&'a mut R, ZenohMessage> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ZenohMessage, Self::Error> {
        // let mut codec = Zenoh060Header {
        //     header: self.read(&mut *reader)?,
        //     ..Default::default()
        // };

        // let attachment = if imsg::mid(codec.header) == imsg::id::ATTACHMENT {
        //     let a: Attachment = codec.read(&mut *reader)?;
        //     codec.header = self.read(&mut *reader)?;
        //     Some(a)
        // } else {
        //     None
        // };
        // let body = match imsg::mid(codec.header) {
        //     imsg::id::SCOUT => ScoutingBody::Scout(codec.read(&mut *reader)?),
        //     imsg::id::HELLO => ScoutingBody::Hello(codec.read(&mut *reader)?),
        //     _ => return Err(DidntRead),
        // };
        // Ok(ZenohMessage { body, attachment })
        Err(DidntRead)
    }
}
