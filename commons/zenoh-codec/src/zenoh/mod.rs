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
pub mod del;
pub mod err;
pub mod put;
pub mod query;
pub mod reply;
pub mod shm;

use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
#[cfg(feature = "shared-memory")]
use zenoh_protocol::common::ZExtUnit;
use zenoh_protocol::{
    common::{imsg, ZExtZBufHeader},
    core::{Encoding, EntityGlobalIdProto, EntityId, ZenohIdProto},
    zenoh::{ext, id, PushBody, RequestBody, ResponseBody},
};

use crate::{
    zenoh::shm::{ZBufRCodec, ZBufWCodec},
    LCodec, RCodec, WCodec, Zenoh080, Zenoh080Header, Zenoh080Length,
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
            _ => return Err(DidntRead),
        };

        Ok(body)
    }
}

// Extension: SourceInfo
impl<const ID: u8> LCodec<&ext::SourceInfoType<{ ID }>> for Zenoh080 {
    fn w_len(self, x: &ext::SourceInfoType<{ ID }>) -> usize {
        let ext::SourceInfoType { id, sn } = x;

        1 + self.w_len(&id.zid) + self.w_len(id.eid) + self.w_len(*sn)
    }
}

impl<W, const ID: u8> WCodec<(&ext::SourceInfoType<{ ID }>, bool), &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: (&ext::SourceInfoType<{ ID }>, bool)) -> Self::Output {
        let (x, more) = x;
        let ext::SourceInfoType { id, sn } = x;

        let header: ZExtZBufHeader<{ ID }> = ZExtZBufHeader::new(self.w_len(x));
        self.write(&mut *writer, (&header, more))?;

        let flags: u8 = (id.zid.size() as u8 - 1) << 4;
        self.write(&mut *writer, flags)?;

        let lodec = Zenoh080Length::new(id.zid.size());
        lodec.write(&mut *writer, &id.zid)?;

        self.write(&mut *writer, id.eid)?;
        self.write(&mut *writer, sn)?;
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
        let zid: ZenohIdProto = lodec.read(&mut *reader)?;

        let eid: EntityId = self.codec.read(&mut *reader)?;
        let sn: u32 = self.codec.read(&mut *reader)?;

        Ok((
            ext::SourceInfoType {
                id: EntityGlobalIdProto { zid, eid },
                sn,
            },
            more,
        ))
    }
}

// Extension: Shm
#[cfg(feature = "shared-memory")]
impl<W, const ID: u8> WCodec<(&ext::ShmType<{ ID }>, bool), &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: (&ext::ShmType<{ ID }>, bool)) -> Self::Output {
        let (x, more) = x;
        let ext::ShmType = x;

        let header: ZExtUnit<{ ID }> = ZExtUnit::new();
        self.write(&mut *writer, (&header, more))?;
        Ok(())
    }
}

#[cfg(feature = "shared-memory")]
impl<R, const ID: u8> RCodec<(ext::ShmType<{ ID }>, bool), &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(ext::ShmType<{ ID }>, bool), Self::Error> {
        let (_, more): (ZExtUnit<{ ID }>, bool) = self.read(&mut *reader)?;
        Ok((ext::ShmType, more))
    }
}

// Extension ValueType
impl<W, const VID: u8, ZBufW: ZBufWCodec> WCodec<(&ext::ValueType<{ VID }>, bool, ZBufW), &mut W>
    for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: (&ext::ValueType<{ VID }>, bool, ZBufW)) -> Self::Output {
        let (x, more, _) = x;
        let ext::ValueType { encoding, payload } = x;

        // Compute extension length
        let mut len = self.w_len(encoding);

        ZBufW::calc_wlen(payload, &mut len);

        // Write ZExtBuf header
        let header: ZExtZBufHeader<{ VID }> = ZExtZBufHeader::new(len);
        self.write(&mut *writer, (&header, more))?;

        // Write encoding
        self.write(&mut *writer, encoding)?;

        // Write payload
        ZBufW::write_zbuf_nolen(writer, payload)
    }
}

impl<R, const VID: u8, ZBufR: ZBufRCodec> RCodec<(ext::ValueType<{ VID }>, bool, ZBufR), &mut R>
    for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(
        #[allow(unused_mut)] mut self,
        reader: &mut R,
    ) -> Result<(ext::ValueType<{ VID }>, bool, ZBufR), Self::Error> {
        let (header, more): (ZExtZBufHeader<{ VID }>, bool) = self.read(&mut *reader)?;

        // Read encoding
        let start = reader.remaining();
        let encoding: Encoding = self.codec.read(&mut *reader)?;
        let end = reader.remaining();

        // Calculate how many bytes are left in the payload
        let len = header.len - (start - end);

        let payload = ZBufR::read_zbuf_nolen(reader, len)?;

        Ok((ext::ValueType { encoding, payload }, more, ZBufR::default()))
    }
}

// Extension: Attachment
impl<W, const ID: u8, ZBufW: ZBufWCodec> WCodec<(&ext::AttachmentType<{ ID }>, bool, ZBufW), &mut W>
    for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: (&ext::AttachmentType<{ ID }>, bool, ZBufW)) -> Self::Output {
        let (x, more, _zbuf_wcodec) = x;
        let ext::AttachmentType { buffer } = x;

        let header: ZExtZBufHeader<{ ID }> = ZExtZBufHeader::new(self.w_len(buffer));
        self.write(&mut *writer, (&header, more))?;

        ZBufW::write_zbuf_nolen(writer, buffer)
    }
}

impl<R, const ID: u8, ZBufR: ZBufRCodec> RCodec<(ext::AttachmentType<{ ID }>, bool, ZBufR), &mut R>
    for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(
        self,
        reader: &mut R,
    ) -> Result<(ext::AttachmentType<{ ID }>, bool, ZBufR), Self::Error> {
        let (h, more): (ZExtZBufHeader<{ ID }>, bool) = self.read(&mut *reader)?;

        let buffer = ZBufR::read_zbuf_nolen(reader, h.len)?;

        Ok((ext::AttachmentType { buffer }, more, ZBufR::default()))
    }
}
