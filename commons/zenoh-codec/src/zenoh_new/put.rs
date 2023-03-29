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
use crate::{
    common::extension, LCodec, RCodec, WCodec, Zenoh080, Zenoh080Bounded, Zenoh080Header,
    Zenoh080Length,
};
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
    ZBuf,
};
use zenoh_protocol::{
    common::{iext, imsg, ZExtZBufHeader},
    core::{Encoding, ZenohId},
    zenoh_new::{
        id,
        put::{ext, flag, Put},
    },
};

impl LCodec<&ext::SourceInfoType> for Zenoh080 {
    fn w_len(self, x: &ext::SourceInfoType) -> usize {
        1 + self.w_len(&x.zid) + self.w_len(x.eid) + self.w_len(x.sn)
    }
}

impl<W> WCodec<(&ext::SourceInfoType, bool), &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: (&ext::SourceInfoType, bool)) -> Self::Output {
        let (x, more) = x;
        let header: ZExtZBufHeader<{ ext::SourceInfo::ID }> = ZExtZBufHeader::new(self.w_len(x));
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

impl<R> RCodec<(ext::SourceInfoType, bool), &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(ext::SourceInfoType, bool), Self::Error> {
        let (_, more): (ZExtZBufHeader<{ ext::SourceInfo::ID }>, bool) = self.read(&mut *reader)?;

        let flags: u8 = self.codec.read(&mut *reader)?;
        let length = 1 + ((flags >> 4) as usize);

        let lodec = Zenoh080Length::new(length);
        let zid: ZenohId = lodec.read(&mut *reader)?;

        let eid: u32 = self.codec.read(&mut *reader)?;
        let sn: u32 = self.codec.read(&mut *reader)?;

        Ok((ext::SourceInfoType { zid, eid, sn }, more))
    }
}

impl<W> WCodec<&Put, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Put) -> Self::Output {
        // Header
        let mut header = id::PUT;
        if x.timestamp.is_some() {
            header |= flag::T;
        }
        if x.encoding != Encoding::default() {
            header |= flag::E;
        }
        let mut n_exts = (x.ext_sinfo.is_some()) as u8;
        if n_exts != 0 {
            header |= flag::Z;
        }
        self.write(&mut *writer, header)?;

        // Body
        if let Some(ts) = x.timestamp.as_ref() {
            self.write(&mut *writer, ts)?;
        }
        if x.encoding != Encoding::default() {
            self.write(&mut *writer, &x.encoding)?;
        }

        // Extensions
        if let Some(sinfo) = x.ext_sinfo.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (sinfo, n_exts != 0))?;
        }

        // Payload
        let bodec = Zenoh080Bounded::<u32>::new();
        bodec.write(&mut *writer, &x.payload)?;

        Ok(())
    }
}

impl<R> RCodec<Put, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Put, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);
        codec.read(reader)
    }
}

impl<R> RCodec<Put, &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Put, Self::Error> {
        if imsg::mid(self.header) != id::PUT {
            return Err(DidntRead);
        }

        // Body
        let mut timestamp: Option<uhlc::Timestamp> = None;
        if imsg::has_flag(self.header, flag::T) {
            timestamp = Some(self.codec.read(&mut *reader)?);
        }

        let mut encoding = Encoding::default();
        if imsg::has_flag(self.header, flag::E) {
            encoding = self.codec.read(&mut *reader)?;
        }

        // Extensions
        let mut ext_sinfo: Option<ext::SourceInfoType> = None;

        let mut has_ext = imsg::has_flag(self.header, flag::Z);
        while has_ext {
            let ext: u8 = self.codec.read(&mut *reader)?;
            let eodec = Zenoh080Header::new(ext);
            match iext::eid(ext) {
                ext::SourceInfo::ID => {
                    let (s, ext): (ext::SourceInfoType, bool) = eodec.read(&mut *reader)?;
                    ext_sinfo = Some(s);
                    has_ext = ext;
                }
                _ => {
                    has_ext = extension::skip(reader, "Put", ext)?;
                }
            }
        }

        // Payload
        let bodec = Zenoh080Bounded::<u32>::new();
        let payload: ZBuf = bodec.read(&mut *reader)?;

        Ok(Put {
            timestamp,
            encoding,
            ext_sinfo,
            payload,
        })
    }
}
