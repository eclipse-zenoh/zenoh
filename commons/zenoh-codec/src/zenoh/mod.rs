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

#[cfg(not(feature = "shared-memory"))]
use crate::Zenoh080Bounded;
#[cfg(feature = "shared-memory")]
use crate::Zenoh080Sliced;
use crate::{LCodec, RCodec, WCodec, Zenoh080, Zenoh080Header, Zenoh080Length};
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
    ZBuf,
};
#[cfg(feature = "shared-memory")]
use zenoh_protocol::common::{iext, ZExtUnit};
use zenoh_protocol::{
    common::{imsg, ZExtZBufHeader},
    core::{Encoding, ZenohId},
    zenoh::{ext, id, PushBody, RequestBody, ResponseBody},
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

// Extension: Shm
#[cfg(feature = "shared-memory")]
impl<W, const ID: u8> WCodec<(&ext::ShmType<{ ID }>, bool), &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: (&ext::ShmType<{ ID }>, bool)) -> Self::Output {
        let (_, more) = x;
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
impl<W, const VID: u8, const SID: u8> WCodec<(&ext::ValueType<{ VID }, { SID }>, bool), &mut W>
    for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: (&ext::ValueType<{ VID }, { SID }>, bool)) -> Self::Output {
        let (x, more) = x;

        #[cfg(feature = "shared-memory")] // Write Shm extension if present
        if let Some(eshm) = x.ext_shm.as_ref() {
            self.write(&mut *writer, (eshm, true))?;
        }

        // Compute extension length
        let mut len = self.w_len(&x.encoding);

        #[cfg(feature = "shared-memory")]
        {
            let codec = Zenoh080Sliced::<u32>::new(x.ext_shm.is_some());
            len += codec.w_len(&x.payload);
        }

        #[cfg(not(feature = "shared-memory"))]
        {
            let codec = Zenoh080Bounded::<u32>::new();
            len += codec.w_len(&x.payload);
        }

        // Write ZExtBuf header
        let header: ZExtZBufHeader<{ VID }> = ZExtZBufHeader::new(len);
        self.write(&mut *writer, (&header, more))?;

        // Write encoding
        self.write(&mut *writer, &x.encoding)?;

        // Write payload
        fn write<W>(writer: &mut W, payload: &ZBuf) -> Result<(), DidntWrite>
        where
            W: Writer,
        {
            // Don't write the length since it is already included in the header
            for s in payload.zslices() {
                writer.write_zslice(s)?;
            }
            Ok(())
        }

        #[cfg(feature = "shared-memory")]
        {
            if x.ext_shm.is_some() {
                let codec = Zenoh080Sliced::<u32>::new(true);
                codec.write(&mut *writer, &x.payload)?;
            } else {
                write(&mut *writer, &x.payload)?;
            }
        }

        #[cfg(not(feature = "shared-memory"))]
        {
            write(&mut *writer, &x.payload)?;
        }

        Ok(())
    }
}

impl<R, const VID: u8, const SID: u8> RCodec<(ext::ValueType<{ VID }, { SID }>, bool), &mut R>
    for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(
        #[allow(unused_mut)] mut self,
        reader: &mut R,
    ) -> Result<(ext::ValueType<{ VID }, { SID }>, bool), Self::Error> {
        #[cfg(feature = "shared-memory")]
        let ext_shm = if iext::eid(self.header) == SID {
            self.header = self.codec.read(&mut *reader)?;
            Some(ext::ShmType)
        } else {
            None
        };
        let (header, more): (ZExtZBufHeader<{ VID }>, bool) = self.read(&mut *reader)?;

        // Read encoding
        let start = reader.remaining();
        let encoding: Encoding = self.codec.read(&mut *reader)?;
        let end = reader.remaining();

        // Read payload
        fn read<R>(reader: &mut R, len: usize) -> Result<ZBuf, DidntRead>
        where
            R: Reader,
        {
            let mut payload = ZBuf::empty();
            reader.read_zslices(len, |s| payload.push_zslice(s))?;
            Ok(payload)
        }

        // Calculate how many bytes are left in the payload
        let len = header.len - (start - end);

        let payload: ZBuf = {
            #[cfg(feature = "shared-memory")]
            {
                if ext_shm.is_some() {
                    let codec = Zenoh080Sliced::<u32>::new(true);
                    let payload: ZBuf = codec.read(&mut *reader)?;
                    payload
                } else {
                    read(&mut *reader, len)?
                }
            }

            #[cfg(not(feature = "shared-memory"))]
            {
                read(&mut *reader, len)?
            }
        };

        Ok((
            ext::ValueType {
                #[cfg(feature = "shared-memory")]
                ext_shm,
                encoding,
                payload,
            },
            more,
        ))
    }
}
