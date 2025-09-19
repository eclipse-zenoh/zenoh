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
pub mod batch;
mod close;
mod fragment;
pub mod frame;
mod init;
mod join;
mod keepalive;
mod oam;
mod open;

use zenoh_buffers::{
    reader::{BacktrackableReader, DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{
    common::{imsg, ZExtZ64},
    network::NetworkMessage,
    transport::*,
};

use crate::{RCodec, WCodec, Zenoh080, Zenoh080Header};

// TransportMessageLowLatency
impl<W> WCodec<TransportMessageLowLatencyRef<'_>, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: TransportMessageLowLatencyRef) -> Self::Output {
        let TransportMessageLowLatencyRef { body } = x;

        match body {
            TransportBodyLowLatencyRef::Network(b) => self.write(&mut *writer, b),
            TransportBodyLowLatencyRef::KeepAlive(b) => self.write(&mut *writer, &b),
            TransportBodyLowLatencyRef::Close(b) => self.write(&mut *writer, &b),
        }
    }
}

impl<R> RCodec<TransportMessageLowLatency, &mut R> for Zenoh080
where
    R: Reader + BacktrackableReader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<TransportMessageLowLatency, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;

        let codec = Zenoh080Header::new(header);
        let body = match imsg::mid(codec.header) {
            id::KEEP_ALIVE => TransportBodyLowLatency::KeepAlive(codec.read(&mut *reader)?),
            id::CLOSE => TransportBodyLowLatency::Close(codec.read(&mut *reader)?),
            _ => {
                let nw: NetworkMessage = codec.read(&mut *reader)?;
                TransportBodyLowLatency::Network(nw)
            }
        };

        Ok(TransportMessageLowLatency { body })
    }
}

// TransportMessage
impl<W> WCodec<&TransportMessage, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &TransportMessage) -> Self::Output {
        let TransportMessage { body, .. } = x;

        match body {
            TransportBody::Frame(b) => self.write(&mut *writer, b),
            TransportBody::Fragment(b) => self.write(&mut *writer, b),
            TransportBody::KeepAlive(b) => self.write(&mut *writer, b),
            TransportBody::InitSyn(b) => self.write(&mut *writer, b),
            TransportBody::InitAck(b) => self.write(&mut *writer, b),
            TransportBody::OpenSyn(b) => self.write(&mut *writer, b),
            TransportBody::OpenAck(b) => self.write(&mut *writer, b),
            TransportBody::Close(b) => self.write(&mut *writer, b),
            TransportBody::OAM(b) => self.write(&mut *writer, b),
            TransportBody::Join(b) => self.write(&mut *writer, b),
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
            id::FRAME => TransportBody::Frame(codec.read(&mut *reader)?),
            id::FRAGMENT => TransportBody::Fragment(codec.read(&mut *reader)?),
            id::KEEP_ALIVE => TransportBody::KeepAlive(codec.read(&mut *reader)?),
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
            id::CLOSE => TransportBody::Close(codec.read(&mut *reader)?),
            id::OAM => TransportBody::OAM(codec.read(&mut *reader)?),
            id::JOIN => TransportBody::Join(codec.read(&mut *reader)?),
            _ => return Err(DidntRead),
        };

        Ok(body.into())
    }
}

// Extensions: QoS
impl<W, const ID: u8> WCodec<(ext::QoSType<{ ID }>, bool), &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: (ext::QoSType<{ ID }>, bool)) -> Self::Output {
        let (x, more) = x;
        let ext: ZExtZ64<{ ID }> = x.into();

        self.write(&mut *writer, (&ext, more))
    }
}

impl<R, const ID: u8> RCodec<(ext::QoSType<{ ID }>, bool), &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(ext::QoSType<{ ID }>, bool), Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);
        codec.read(reader)
    }
}

impl<R, const ID: u8> RCodec<(ext::QoSType<{ ID }>, bool), &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(ext::QoSType<{ ID }>, bool), Self::Error> {
        let (ext, more): (ZExtZ64<{ ID }>, bool) = self.read(&mut *reader)?;
        Ok((ext.into(), more))
    }
}

// Extensions: Patch
impl<W, const ID: u8> WCodec<(ext::PatchType<{ ID }>, bool), &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: (ext::PatchType<{ ID }>, bool)) -> Self::Output {
        let (x, more) = x;
        let ext: ZExtZ64<{ ID }> = x.into();

        self.write(&mut *writer, (&ext, more))
    }
}

impl<R, const ID: u8> RCodec<(ext::PatchType<{ ID }>, bool), &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(ext::PatchType<{ ID }>, bool), Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);
        codec.read(reader)
    }
}

impl<R, const ID: u8> RCodec<(ext::PatchType<{ ID }>, bool), &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(ext::PatchType<{ ID }>, bool), Self::Error> {
        let (ext, more): (ZExtZ64<{ ID }>, bool) = self.read(&mut *reader)?;
        Ok((ext.into(), more))
    }
}
