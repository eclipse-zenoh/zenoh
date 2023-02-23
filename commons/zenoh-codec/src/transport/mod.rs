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
mod fragment;
mod frame;
mod init;
mod join;
mod keepalive;
mod open;

use crate::{RCodec, WCodec, Zenoh080, Zenoh080Header};
use zenoh_buffers::{
    reader::{BacktrackableReader, DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{common::imsg, transport::*};

// TransportMessage
impl<W> WCodec<&TransportMessage, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &TransportMessage) -> Self::Output {
        match &x.body {
            TransportBody::InitSyn(b) => self.write(&mut *writer, b),
            TransportBody::InitAck(b) => self.write(&mut *writer, b),
            TransportBody::OpenSyn(b) => self.write(&mut *writer, b),
            TransportBody::OpenAck(b) => self.write(&mut *writer, b),
            TransportBody::Join(b) => self.write(&mut *writer, b),
            TransportBody::Close(b) => self.write(&mut *writer, b),
            TransportBody::KeepAlive(b) => self.write(&mut *writer, b),
            TransportBody::Frame(b) => self.write(&mut *writer, b),
            TransportBody::Fragment(b) => self.write(&mut *writer, b),
        }
    }
}

impl<R> RCodec<TransportMessage, &mut R> for Zenoh080
where
    R: Reader + BacktrackableReader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<TransportMessage, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;

        let codec = Zenoh080Header::new(header);
        let body = match imsg::mid(codec.header) {
            id::INIT => {
                if !imsg::has_flag(codec.header, zenoh_protocol::transport::init::flag::A) {
                    TransportBody::InitSyn(codec.read(&mut *reader)?)
                } else {
                    TransportBody::InitAck(codec.read(&mut *reader)?)
                }
            }
            id::OPEN => {
                if !imsg::has_flag(codec.header, zenoh_protocol::transport::open::flag::A) {
                    TransportBody::OpenSyn(codec.read(&mut *reader)?)
                } else {
                    TransportBody::OpenAck(codec.read(&mut *reader)?)
                }
            }
            id::JOIN => TransportBody::Join(codec.read(&mut *reader)?),
            id::CLOSE => TransportBody::Close(codec.read(&mut *reader)?),
            id::KEEP_ALIVE => TransportBody::KeepAlive(codec.read(&mut *reader)?),
            id::FRAME => TransportBody::Frame(codec.read(&mut *reader)?),
            id::FRAGMENT => TransportBody::Fragment(codec.read(&mut *reader)?),
            _ => return Err(DidntRead),
        };

        Ok(TransportMessage { body })
    }
}
