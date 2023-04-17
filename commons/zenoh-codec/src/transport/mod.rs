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
mod close;
mod frame;
mod init;
mod join;
mod keepalive;
mod open;

use crate::{RCodec, WCodec, Zenoh060, Zenoh060Header};
use zenoh_buffers::{
    reader::{BacktrackableReader, DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{
    common::{imsg, Attachment},
    transport::*,
};

// TransportMessage
impl<W> WCodec<&TransportMessage, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &TransportMessage) -> Self::Output {
        if let Some(a) = x.attachment.as_ref() {
            self.write(&mut *writer, a)?;
        }
        match &x.body {
            TransportBody::InitSyn(b) => self.write(&mut *writer, b),
            TransportBody::InitAck(b) => self.write(&mut *writer, b),
            TransportBody::OpenSyn(b) => self.write(&mut *writer, b),
            TransportBody::OpenAck(b) => self.write(&mut *writer, b),
            TransportBody::Join(b) => self.write(&mut *writer, b),
            TransportBody::Close(b) => self.write(&mut *writer, b),
            TransportBody::KeepAlive(b) => self.write(&mut *writer, b),
            TransportBody::Frame(b) => self.write(&mut *writer, b),
        }
    }
}

impl<R> RCodec<TransportMessage, &mut R> for Zenoh060
where
    R: Reader + BacktrackableReader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<TransportMessage, Self::Error> {
        let mut codec = Zenoh060Header {
            header: self.read(&mut *reader)?,
            ..Default::default()
        };
        let mut attachment: Option<Attachment> = None;
        if imsg::mid(codec.header) == tmsg::id::ATTACHMENT {
            let a: Attachment = codec.read(&mut *reader)?;
            attachment = Some(a);
            codec.header = self.read(&mut *reader)?;
        }
        let body = match imsg::mid(codec.header) {
            tmsg::id::INIT => {
                if !imsg::has_flag(codec.header, tmsg::flag::A) {
                    TransportBody::InitSyn(codec.read(&mut *reader)?)
                } else {
                    TransportBody::InitAck(codec.read(&mut *reader)?)
                }
            }
            tmsg::id::OPEN => {
                if !imsg::has_flag(codec.header, tmsg::flag::A) {
                    TransportBody::OpenSyn(codec.read(&mut *reader)?)
                } else {
                    TransportBody::OpenAck(codec.read(&mut *reader)?)
                }
            }
            tmsg::id::JOIN => TransportBody::Join(codec.read(&mut *reader)?),
            tmsg::id::CLOSE => TransportBody::Close(codec.read(&mut *reader)?),
            tmsg::id::KEEP_ALIVE => TransportBody::KeepAlive(codec.read(&mut *reader)?),
            tmsg::id::PRIORITY | tmsg::id::FRAME => TransportBody::Frame(codec.read(&mut *reader)?),
            _ => return Err(DidntRead),
        };

        Ok(TransportMessage { body, attachment })
    }
}
