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
pub mod ack;
pub mod del;
pub mod err;
pub mod pull;
pub mod put;
pub mod query;
pub mod reply;

use crate::{LCodec, RCodec, WCodec, Zenoh080, Zenoh080Header, Zenoh080Length};
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{
    common::{imsg, ZExtZBufHeader},
    core::ZenohId,
    zenoh_new::{ext, id, PushBody, RequestBody, ResponseBody},
};

// Push
impl<W> WCodec<&PushBody, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &PushBody) -> Self::Output {
        match x {
            PushBody::Put(b) => self.write(&mut *writer, b),
            PushBody::Del(b) => self.write(&mut *writer, b),
        }
    }
}

impl<R> RCodec<PushBody, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<PushBody, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;

        let codec = Zenoh080Header::new(header);
        let body = match imsg::mid(codec.header) {
            id::PUT => PushBody::Put(codec.read(&mut *reader)?),
            id::DEL => PushBody::Del(codec.read(&mut *reader)?),
            _ => return Err(DidntRead),
        };

        Ok(body)
    }
}

// Request
impl<W> WCodec<&RequestBody, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &RequestBody) -> Self::Output {
        match x {
            RequestBody::Query(b) => self.write(&mut *writer, b),
            RequestBody::Put(b) => self.write(&mut *writer, b),
            RequestBody::Del(b) => self.write(&mut *writer, b),
            RequestBody::Pull(b) => self.write(&mut *writer, b),
        }
    }
}

impl<R> RCodec<RequestBody, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<RequestBody, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;

        let codec = Zenoh080Header::new(header);
        let body = match imsg::mid(codec.header) {
            id::QUERY => RequestBody::Query(codec.read(&mut *reader)?),
            id::PUT => RequestBody::Put(codec.read(&mut *reader)?),
            id::DEL => RequestBody::Del(codec.read(&mut *reader)?),
            id::PULL => RequestBody::Pull(codec.read(&mut *reader)?),
            _ => return Err(DidntRead),
        };

        Ok(body)
    }
}

// Response
impl<W> WCodec<&ResponseBody, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &ResponseBody) -> Self::Output {
        match x {
            ResponseBody::Reply(b) => self.write(&mut *writer, b),
            ResponseBody::Err(b) => self.write(&mut *writer, b),
            ResponseBody::Ack(b) => self.write(&mut *writer, b),
            ResponseBody::Put(b) => self.write(&mut *writer, b),
        }
    }
}

impl<R> RCodec<ResponseBody, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ResponseBody, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;

        let codec = Zenoh080Header::new(header);
        let body = match imsg::mid(codec.header) {
            id::REPLY => ResponseBody::Reply(codec.read(&mut *reader)?),
            id::ERR => ResponseBody::Err(codec.read(&mut *reader)?),
            id::ACK => ResponseBody::Ack(codec.read(&mut *reader)?),
            id::PUT => ResponseBody::Put(codec.read(&mut *reader)?),
            _ => return Err(DidntRead),
        };

        Ok(body)
    }
}

// Extension: SourceInfo
impl<const ID: u8> LCodec<&ext::SourceInfoType<{ ID }>> for Zenoh080 {
    fn w_len(self, x: &ext::SourceInfoType<{ ID }>) -> usize {
        1 + self.w_len(&x.zid) + self.w_len(x.eid) + self.w_len(x.sn)
    }
}

impl<W, const ID: u8> WCodec<(&ext::SourceInfoType<{ ID }>, bool), &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: (&ext::SourceInfoType<{ ID }>, bool)) -> Self::Output {
        let (x, more) = x;
        let header: ZExtZBufHeader<{ ID }> = ZExtZBufHeader::new(self.w_len(x));
        self.write(&mut *writer, (&header, more))?;

        let flags: u8 = (x.zid.size() as u8 - 1) << 4;
        self.write(&mut *writer, flags)?;

        let lodec = Zenoh080Length::new(x.zid.size());
        lodec.write(&mut *writer, &x.zid)?;

        self.write(&mut *writer, x.eid)?;
        self.write(&mut *writer, x.sn)?;
        Ok(())
    }
}

impl<R, const ID: u8> RCodec<(ext::SourceInfoType<{ ID }>, bool), &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(ext::SourceInfoType<{ ID }>, bool), Self::Error> {
        let (_, more): (ZExtZBufHeader<{ ID }>, bool) = self.read(&mut *reader)?;

        let flags: u8 = self.codec.read(&mut *reader)?;
        let length = 1 + ((flags >> 4) as usize);

        let lodec = Zenoh080Length::new(length);
        let zid: ZenohId = lodec.read(&mut *reader)?;

        let eid: u32 = self.codec.read(&mut *reader)?;
        let sn: u32 = self.codec.read(&mut *reader)?;

        Ok((ext::SourceInfoType { zid, eid, sn }, more))
    }
}
