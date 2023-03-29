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
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
    SplitBuffer, ZBuf,
};
use zenoh_protocol::{
    common::{iext, imsg, ZExtZBufHeader},
    core::Encoding,
    zenoh_new::{
        id,
        query::{ext, flag, Query},
        ConsolidationMode,
    },
};

impl LCodec<&ext::QueryBodyType> for Zenoh080 {
    fn w_len(self, x: &ext::QueryBodyType) -> usize {
        self.w_len(&x.encoding) + x.payload.len()
    }
}

impl<W> WCodec<(&ext::QueryBodyType, bool), &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: (&ext::QueryBodyType, bool)) -> Self::Output {
        let (x, more) = x;
        let header: ZExtZBufHeader<{ ext::QueryBody::ID }> = ZExtZBufHeader::new(self.w_len(x));
        self.write(&mut *writer, (&header, more))?;

        self.write(&mut *writer, &x.encoding)?;
        // Don't write the length since it is already included in the header
        for s in x.payload.zslices() {
            writer.write_zslice(s)?;
        }

        Ok(())
    }
}

impl<R> RCodec<(ext::QueryBodyType, bool), &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(ext::QueryBodyType, bool), Self::Error> {
        let (header, more): (ZExtZBufHeader<{ ext::QueryBody::ID }>, bool) =
            self.read(&mut *reader)?;

        let start = reader.remaining();
        let encoding: Encoding = self.codec.read(&mut *reader)?;
        let end = reader.remaining();
        // Calculate how many bytes are left in the payload
        let len = header.len - (start - end);
        let mut payload = ZBuf::empty();
        reader.read_zslices(len, |s| payload.push_zslice(s))?;

        Ok((ext::QueryBodyType { encoding, payload }, more))
    }
}

// Consolidation
impl<W> WCodec<ConsolidationMode, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: ConsolidationMode) -> Self::Output {
        let v: u8 = match x {
            ConsolidationMode::None => 0,
            ConsolidationMode::Monotonic => 1,
            ConsolidationMode::Latest => 2,
        };
        self.write(&mut *writer, v)
    }
}

impl<R> RCodec<ConsolidationMode, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ConsolidationMode, Self::Error> {
        let v: u8 = self.read(&mut *reader)?;
        let c = match v {
            0 => ConsolidationMode::None,
            1 => ConsolidationMode::Monotonic,
            2 => ConsolidationMode::Latest,
            _ => return Err(DidntRead),
        };
        Ok(c)
    }
}

impl<W> WCodec<&Query, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Query) -> Self::Output {
        // Header
        let mut header = id::QUERY;
        if !x.parameters.is_empty() {
            header |= flag::P;
        }
        if x.consolidation != ConsolidationMode::default() {
            header |= flag::C;
        }
        let mut n_exts = (x.ext_sinfo.is_some() as u8) + (x.ext_body.is_some() as u8);
        if n_exts != 0 {
            header |= flag::Z;
        }
        self.write(&mut *writer, header)?;

        // Body
        if !x.parameters.is_empty() {
            self.write(&mut *writer, &x.parameters)?;
        }
        if x.consolidation != ConsolidationMode::default() {
            self.write(&mut *writer, x.consolidation)?;
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

        Ok(())
    }
}

impl<R> RCodec<Query, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Query, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);
        codec.read(reader)
    }
}

impl<R> RCodec<Query, &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Query, Self::Error> {
        if imsg::mid(self.header) != id::QUERY {
            return Err(DidntRead);
        }

        // Body
        let mut parameters = String::new();
        if imsg::has_flag(self.header, flag::P) {
            parameters = self.codec.read(&mut *reader)?;
        }

        let mut consolidation = ConsolidationMode::default();
        if imsg::has_flag(self.header, flag::C) {
            consolidation = self.codec.read(&mut *reader)?;
        }

        // Extensions
        let mut ext_sinfo: Option<ext::SourceInfoType> = None;
        let mut ext_body: Option<ext::QueryBodyType> = None;

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
                ext::QueryBody::ID => {
                    let (s, ext): (ext::QueryBodyType, bool) = eodec.read(&mut *reader)?;
                    ext_body = Some(s);
                    has_ext = ext;
                }
                _ => {
                    has_ext = extension::skip(reader, "Put", ext)?;
                }
            }
        }

        Ok(Query {
            parameters,
            consolidation,
            ext_sinfo,
            ext_body,
        })
    }
}
