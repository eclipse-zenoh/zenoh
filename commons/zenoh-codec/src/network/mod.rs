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
mod declare;
mod pull;
mod push;
mod request;

use crate::{LCodec, RCodec, WCodec, Zenoh080, Zenoh080Header};
use zenoh_buffers::{
    reader::{BacktrackableReader, DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{
    common::{imsg, ZExtZ64, ZExtZBufHeader},
    network::*,
    network::{NetworkBody, NetworkMessage},
};

// NetworkMessage
impl<W> WCodec<&NetworkMessage, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &NetworkMessage) -> Self::Output {
        match &x.body {
            NetworkBody::Push(b) => self.write(&mut *writer, b),
            _ => Ok(()), //TODO
        }
    }
}

impl<R> RCodec<NetworkMessage, &mut R> for Zenoh080
where
    R: Reader + BacktrackableReader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<NetworkMessage, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;

        let codec = Zenoh080Header::new(header);
        let body = match imsg::mid(codec.header) {
            id::PUSH => NetworkBody::Push(codec.read(&mut *reader)?),
            // id::FRAGMENT => NetworkBody::Fragment(codec.read(&mut *reader)?),
            // id::KEEP_ALIVE => NetworkBody::KeepAlive(codec.read(&mut *reader)?),
            // id::INIT => {
            //     if !imsg::has_flag(codec.header, zenoh_protocol::transport::init::flag::A) {
            //         NetworkBody::InitSyn(codec.read(&mut *reader)?)
            //     } else {
            //         NetworkBody::InitAck(codec.read(&mut *reader)?)
            //     }
            // }
            // id::OPEN => {
            //     if !imsg::has_flag(codec.header, zenoh_protocol::transport::open::flag::A) {
            //         NetworkBody::OpenSyn(codec.read(&mut *reader)?)
            //     } else {
            //         NetworkBody::OpenAck(codec.read(&mut *reader)?)
            //     }
            // }
            // // id::JOIN => NetworkBody::Join(codec.read(&mut *reader)?),
            // id::CLOSE => NetworkBody::Close(codec.read(&mut *reader)?),
            _ => return Err(DidntRead),
        };

        Ok(NetworkMessage { body })
    }
}

// Extensions: QoS
crate::impl_zextz64!(ext::QoS, ext::QOS);

// Extensions: Timestamp
impl<W> WCodec<(&ext::Timestamp, bool), &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: (&ext::Timestamp, bool)) -> Self::Output {
        let (tstamp, more) = x;
        let header: ZExtZBufHeader<{ ext::TSTAMP }> =
            ZExtZBufHeader::new(self.w_len(&tstamp.timestamp));
        self.write(&mut *writer, (&header, more))?;
        self.write(&mut *writer, &tstamp.timestamp)
    }
}

impl<R> RCodec<(ext::Timestamp, bool), &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(ext::Timestamp, bool), Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);
        codec.read(reader)
    }
}

impl<R> RCodec<(ext::Timestamp, bool), &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(ext::Timestamp, bool), Self::Error> {
        let codec = Zenoh080::new();
        let (_, more): (ZExtZBufHeader<{ ext::TSTAMP }>, bool) = self.read(&mut *reader)?;
        let timestamp: uhlc::Timestamp = codec.read(&mut *reader)?;
        Ok((ext::Timestamp { timestamp }, more))
    }
}
