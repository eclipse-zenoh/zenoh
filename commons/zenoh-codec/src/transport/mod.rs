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
mod close;
mod init;
mod join;
mod open;

use crate::*;
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{
    common::{imsg, Attachment},
    transport::*,
};

// TransportMessage
impl<W> WCodec<&mut W, &TransportMessage> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &TransportMessage) -> Self::Output {
        if let Some(a) = x.attachment.as_ref() {
            zcwrite!(self, writer, a)?;
        }
        match &x.body {
            TransportBody::InitSyn(b) => zcwrite!(self, writer, b),
            TransportBody::InitAck(b) => zcwrite!(self, writer, b),
            TransportBody::OpenSyn(b) => zcwrite!(self, writer, b),
            TransportBody::OpenAck(b) => zcwrite!(self, writer, b),
            TransportBody::Join(b) => zcwrite!(self, writer, b),
            TransportBody::Close(b) => zcwrite!(self, writer, b),
        }
    }
}

impl<R> RCodec<&mut R, TransportMessage> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<TransportMessage, Self::Error> {
        let mut codec = Zenoh060RCodec {
            header: zcread!(self, reader)?,
            ..Default::default()
        };
        let mut attachment: Option<Attachment> = None;
        if imsg::mid(codec.header) == tmsg::id::ATTACHMENT {
            let a: Attachment = zcread!(codec, reader)?;
            attachment = Some(a);
            codec.header = zcread!(self, reader)?;
        }
        let body = match imsg::mid(codec.header) {
            tmsg::id::INIT => {
                if !imsg::has_flag(codec.header, tmsg::flag::A) {
                    TransportBody::InitSyn(zcread!(codec, reader)?)
                } else {
                    TransportBody::InitAck(zcread!(codec, reader)?)
                }
            }
            tmsg::id::OPEN => {
                if !imsg::has_flag(codec.header, tmsg::flag::A) {
                    TransportBody::OpenSyn(zcread!(codec, reader)?)
                } else {
                    TransportBody::OpenAck(zcread!(codec, reader)?)
                }
            }
            tmsg::id::JOIN => TransportBody::Join(zcread!(codec, reader)?),
            tmsg::id::CLOSE => TransportBody::Close(zcread!(codec, reader)?),
            _ => return Err(DidntRead),
        };

        Ok(TransportMessage { body, attachment })
    }
}
