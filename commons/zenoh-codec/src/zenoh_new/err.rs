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
use crate::{common::extension, LCodec, RCodec, WCodec, Zenoh080, Zenoh080Header};
use alloc::vec::Vec;
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
    SplitBuffer, ZBuf,
};
use zenoh_protocol::{
    common::{iext, imsg, ZExtZBufHeader},
    core::Encoding,
    zenoh_new::{
        err::{ext, flag, Err},
        id,
    },
};

impl LCodec<&ext::ErrBodyType> for Zenoh080 {
    fn w_len(self, x: &ext::ErrBodyType) -> usize {
        self.w_len(&x.encoding) + x.payload.len()
    }
}

impl<W> WCodec<(&ext::ErrBodyType, bool), &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: (&ext::ErrBodyType, bool)) -> Self::Output {
        let (x, more) = x;
        let header: ZExtZBufHeader<{ ext::ErrBody::ID }> = ZExtZBufHeader::new(self.w_len(x));
        self.write(&mut *writer, (&header, more))?;

        self.write(&mut *writer, &x.encoding)?;
        // Don't write the length since it is already included in the header
        for s in x.payload.zslices() {
            writer.write_zslice(s)?;
        }

        Ok(())
    }
}

impl<R> RCodec<(ext::ErrBodyType, bool), &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(ext::ErrBodyType, bool), Self::Error> {
        let (header, more): (ZExtZBufHeader<{ ext::ErrBody::ID }>, bool) =
            self.read(&mut *reader)?;

        let start = reader.remaining();
        let encoding: Encoding = self.codec.read(&mut *reader)?;
        let end = reader.remaining();
        // Calculate how many bytes are left in the payload
        let len = header.len - (start - end);
        let mut payload = ZBuf::empty();
        reader.read_zslices(len, |s| payload.push_zslice(s))?;

        Ok((ext::ErrBodyType { encoding, payload }, more))
    }
}

impl<W> WCodec<&Err, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Err) -> Self::Output {
        // Header
        let mut header = id::ERR;
        if x.timestamp.is_some() {
            header |= flag::T;
        }
        if x.is_infrastructure {
            header |= flag::I;
        }
        let mut n_exts = (x.ext_sinfo.is_some() as u8)
            + (x.ext_body.is_some() as u8)
            + (x.ext_unknown.len() as u8);
        if n_exts != 0 {
            header |= flag::Z;
        }
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, x.code)?;
        if let Some(ts) = x.timestamp.as_ref() {
            self.write(&mut *writer, ts)?;
        }

        // Extensions
        if let Some(sinfo) = x.ext_sinfo.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (sinfo, n_exts != 0))?;
        }
        if let Some(body) = x.ext_body.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (body, n_exts != 0))?;
        }
        for u in x.ext_unknown.iter() {
            n_exts -= 1;
            self.write(&mut *writer, (u, n_exts != 0))?;
        }

        Ok(())
    }
}

impl<R> RCodec<Err, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Err, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);
        codec.read(reader)
    }
}

impl<R> RCodec<Err, &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Err, Self::Error> {
        if imsg::mid(self.header) != id::ERR {
            return Err(DidntRead);
        }

        // Body
        let code: u16 = self.codec.read(&mut *reader)?;
        let is_infrastructure = imsg::has_flag(self.header, flag::I);
        let mut timestamp: Option<uhlc::Timestamp> = None;
        if imsg::has_flag(self.header, flag::T) {
            timestamp = Some(self.codec.read(&mut *reader)?);
        }

        // Extensions
        let mut ext_sinfo: Option<ext::SourceInfoType> = None;
        let mut ext_body: Option<ext::ErrBodyType> = None;
        let mut ext_unknown = Vec::new();

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
                ext::ErrBody::ID => {
                    let (s, ext): (ext::ErrBodyType, bool) = eodec.read(&mut *reader)?;
                    ext_body = Some(s);
                    has_ext = ext;
                }
                _ => {
                    let (u, ext) = extension::read(reader, "Err", ext)?;
                    ext_unknown.push(u);
                    has_ext = ext;
                }
            }
        }

        Ok(Err {
            code,
            is_infrastructure,
            timestamp,
            ext_sinfo,
            ext_body,
            ext_unknown,
        })
    }
}
