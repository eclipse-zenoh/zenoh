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
mod oam;
mod push;
mod request;
mod response;

use crate::{LCodec, RCodec, WCodec, Zenoh080, Zenoh080Header};
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{
    common::{imsg, ZExtZ64, ZExtZBufHeader},
    network::*,
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
            NetworkBody::Request(b) => self.write(&mut *writer, b),
            NetworkBody::Response(b) => self.write(&mut *writer, b),
            NetworkBody::ResponseFinal(b) => self.write(&mut *writer, b),
            NetworkBody::Declare(b) => self.write(&mut *writer, b),
            NetworkBody::OAM(b) => self.write(&mut *writer, b),
        }
    }
}

impl<R> RCodec<NetworkMessage, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<NetworkMessage, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;

        let codec = Zenoh080Header::new(header);
        let body = match imsg::mid(codec.header) {
            id::PUSH => NetworkBody::Push(codec.read(&mut *reader)?),
            id::REQUEST => NetworkBody::Request(codec.read(&mut *reader)?),
            id::RESPONSE => NetworkBody::Response(codec.read(&mut *reader)?),
            id::RESPONSE_FINAL => NetworkBody::ResponseFinal(codec.read(&mut *reader)?),
            id::DECLARE => NetworkBody::Declare(codec.read(&mut *reader)?),
            id::OAM => NetworkBody::OAM(codec.read(&mut *reader)?),
            _ => return Err(DidntRead),
        };

        Ok(NetworkMessage { body })
    }
}

// Extensions: QoS
crate::impl_zextz64!(ext::QoSType, ext::QoS::ID);

// Extensions: Timestamp
impl<W> WCodec<(&ext::TimestampType, bool), &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: (&ext::TimestampType, bool)) -> Self::Output {
        let (tstamp, more) = x;
        let header: ZExtZBufHeader<{ ext::Timestamp::ID }> =
            ZExtZBufHeader::new(self.w_len(&tstamp.timestamp));
        self.write(&mut *writer, (&header, more))?;
        self.write(&mut *writer, &tstamp.timestamp)
    }
}

impl<R> RCodec<(ext::TimestampType, bool), &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(ext::TimestampType, bool), Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);
        codec.read(reader)
    }
}

impl<R> RCodec<(ext::TimestampType, bool), &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(ext::TimestampType, bool), Self::Error> {
        let (_, more): (ZExtZBufHeader<{ ext::Timestamp::ID }>, bool) = self.read(&mut *reader)?;
        let timestamp: uhlc::Timestamp = self.codec.read(&mut *reader)?;
        Ok((ext::TimestampType { timestamp }, more))
    }
}
