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

use crate::{
    LCodec, RCodec, WCodec, Zenoh080, Zenoh080Header, Zenoh080Length, Zenoh080Reliability,
};
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{
    common::{imsg, ZExtZ64, ZExtZBufHeader},
    core::{Reliability, ZenohId},
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
        let codec = Zenoh080Reliability::new(Reliability::default());
        codec.read(reader)
    }
}

impl<R> RCodec<NetworkMessage, &mut R> for Zenoh080Reliability
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<NetworkMessage, Self::Error> {
        let header: u8 = self.codec.read(&mut *reader)?;

        let codec = Zenoh080Header::new(header);
        codec.read(&mut *reader)
    }
}

impl<R> RCodec<NetworkMessage, &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<NetworkMessage, Self::Error> {
        let body = match imsg::mid(self.header) {
            id::PUSH => NetworkBody::Push(self.read(&mut *reader)?),
            id::REQUEST => NetworkBody::Request(self.read(&mut *reader)?),
            id::RESPONSE => NetworkBody::Response(self.read(&mut *reader)?),
            id::RESPONSE_FINAL => NetworkBody::ResponseFinal(self.read(&mut *reader)?),
            id::DECLARE => NetworkBody::Declare(self.read(&mut *reader)?),
            id::OAM => NetworkBody::OAM(self.read(&mut *reader)?),
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

// Extensions: Timestamp
impl<W, const ID: u8> WCodec<(&ext::TimestampType<{ ID }>, bool), &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: (&ext::TimestampType<{ ID }>, bool)) -> Self::Output {
        let (tstamp, more) = x;
        let header: ZExtZBufHeader<{ ID }> = ZExtZBufHeader::new(self.w_len(&tstamp.timestamp));
        self.write(&mut *writer, (&header, more))?;
        self.write(&mut *writer, &tstamp.timestamp)
    }
}

impl<R, const ID: u8> RCodec<(ext::TimestampType<{ ID }>, bool), &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(ext::TimestampType<{ ID }>, bool), Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);
        codec.read(reader)
    }
}

impl<R, const ID: u8> RCodec<(ext::TimestampType<{ ID }>, bool), &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(ext::TimestampType<{ ID }>, bool), Self::Error> {
        let (_, more): (ZExtZBufHeader<{ ID }>, bool) = self.read(&mut *reader)?;
        let timestamp: uhlc::Timestamp = self.codec.read(&mut *reader)?;
        Ok((ext::TimestampType { timestamp }, more))
    }
}

// Extensions: NodeId
impl<W, const ID: u8> WCodec<(ext::NodeIdType<{ ID }>, bool), &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: (ext::NodeIdType<{ ID }>, bool)) -> Self::Output {
        let (x, more) = x;
        let ext: ZExtZ64<{ ID }> = x.into();
        self.write(&mut *writer, (&ext, more))
    }
}

impl<R, const ID: u8> RCodec<(ext::NodeIdType<{ ID }>, bool), &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(ext::NodeIdType<{ ID }>, bool), Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);
        codec.read(reader)
    }
}

impl<R, const ID: u8> RCodec<(ext::NodeIdType<{ ID }>, bool), &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(ext::NodeIdType<{ ID }>, bool), Self::Error> {
        let (ext, more): (ZExtZ64<{ ID }>, bool) = self.read(&mut *reader)?;
        Ok((ext.into(), more))
    }
}

// Extension: EntityId
impl<const ID: u8> LCodec<&ext::EntityIdType<{ ID }>> for Zenoh080 {
    fn w_len(self, x: &ext::EntityIdType<{ ID }>) -> usize {
        1 + self.w_len(&x.zid) + self.w_len(x.eid)
    }
}

impl<W, const ID: u8> WCodec<(&ext::EntityIdType<{ ID }>, bool), &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: (&ext::EntityIdType<{ ID }>, bool)) -> Self::Output {
        let (x, more) = x;
        let header: ZExtZBufHeader<{ ID }> = ZExtZBufHeader::new(self.w_len(x));
        self.write(&mut *writer, (&header, more))?;

        let flags: u8 = (x.zid.size() as u8 - 1) << 4;
        self.write(&mut *writer, flags)?;

        let lodec = Zenoh080Length::new(x.zid.size());
        lodec.write(&mut *writer, &x.zid)?;

        self.write(&mut *writer, x.eid)?;
        Ok(())
    }
}

impl<R, const ID: u8> RCodec<(ext::EntityIdType<{ ID }>, bool), &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(ext::EntityIdType<{ ID }>, bool), Self::Error> {
        let (_, more): (ZExtZBufHeader<{ ID }>, bool) = self.read(&mut *reader)?;

        let flags: u8 = self.codec.read(&mut *reader)?;
        let length = 1 + ((flags >> 4) as usize);

        let lodec = Zenoh080Length::new(length);
        let zid: ZenohId = lodec.read(&mut *reader)?;

        let eid: u32 = self.codec.read(&mut *reader)?;

        Ok((ext::EntityIdType { zid, eid }, more))
    }
}
