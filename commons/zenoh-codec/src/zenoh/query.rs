// Copyright (c) 2024 ZettaScale Technology
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

use crate::{common::extension, RCodec, WCodec, Zenoh080, Zenoh080Header};
use alloc::{string::String, vec::Vec};
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};

use zenoh_protocol::{
    common::{iext, imsg},
    zenoh::{
        id,
        query::{ext, flag, Query},
    },
};

// Extension Consolidation
impl<W> WCodec<(ext::ConsolidationType, bool), &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: (ext::ConsolidationType, bool)) -> Self::Output {
        let (x, more) = x;
        let v: u64 = match x {
            ext::ConsolidationType::Auto => 0,
            ext::ConsolidationType::None => 1,
            ext::ConsolidationType::Monotonic => 2,
            ext::ConsolidationType::Latest => 3,
            ext::ConsolidationType::Unique => 4,
        };
        let v = ext::Consolidation::new(v);
        self.write(&mut *writer, (&v, more))
    }
}

impl<R> RCodec<(ext::ConsolidationType, bool), &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(ext::ConsolidationType, bool), Self::Error> {
        let (ext, more): (ext::Consolidation, bool) = self.read(&mut *reader)?;
        let c = match ext.value {
            0 => ext::ConsolidationType::Auto,
            1 => ext::ConsolidationType::None,
            2 => ext::ConsolidationType::Monotonic,
            3 => ext::ConsolidationType::Latest,
            4 => ext::ConsolidationType::Unique,
            _ => return Err(DidntRead),
        };
        Ok((c, more))
    }
}

impl<W> WCodec<&Query, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Query) -> Self::Output {
        let Query {
            parameters,
            ext_sinfo,
            ext_consolidation,
            ext_body,
            ext_attachment,
            ext_unknown,
        } = x;

        // Header
        let mut header = id::QUERY;
        if !parameters.is_empty() {
            header |= flag::P;
        }
        let mut n_exts = (ext_sinfo.is_some() as u8)
            + ((ext_consolidation != &ext::ConsolidationType::default()) as u8)
            + (ext_body.is_some() as u8)
            + (ext_attachment.is_some() as u8)
            + (ext_unknown.len() as u8);
        if n_exts != 0 {
            header |= flag::Z;
        }
        self.write(&mut *writer, header)?;

        // Body
        if !parameters.is_empty() {
            self.write(&mut *writer, parameters)?;
        }

        // Extensions
        if let Some(sinfo) = ext_sinfo.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (sinfo, n_exts != 0))?;
        }
        if ext_consolidation != &ext::ConsolidationType::default() {
            n_exts -= 1;
            self.write(&mut *writer, (*ext_consolidation, n_exts != 0))?;
        }
        if let Some(body) = ext_body.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (body, n_exts != 0))?;
        }
        if let Some(att) = ext_attachment.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (att, n_exts != 0))?;
        }
        for u in ext_unknown.iter() {
            n_exts -= 1;
            self.write(&mut *writer, (u, n_exts != 0))?;
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

        // Extensions
        let mut ext_sinfo: Option<ext::SourceInfoType> = None;
        let mut ext_consolidation = ext::ConsolidationType::default();
        let mut ext_body: Option<ext::QueryBodyType> = None;
        let mut ext_attachment: Option<ext::AttachmentType> = None;
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
                ext::Consolidation::ID => {
                    let (c, ext): (ext::ConsolidationType, bool) = eodec.read(&mut *reader)?;
                    ext_consolidation = c;
                    has_ext = ext;
                }
                ext::QueryBodyType::SID | ext::QueryBodyType::VID => {
                    let (s, ext): (ext::QueryBodyType, bool) = eodec.read(&mut *reader)?;
                    ext_body = Some(s);
                    has_ext = ext;
                }
                ext::Attachment::ID => {
                    let (a, ext): (ext::AttachmentType, bool) = eodec.read(&mut *reader)?;
                    ext_attachment = Some(a);
                    has_ext = ext;
                }
                _ => {
                    let (u, ext) = extension::read(reader, "Query", ext)?;
                    ext_unknown.push(u);
                    has_ext = ext;
                }
            }
        }

        Ok(Query {
            parameters,
            ext_sinfo,
            ext_consolidation,
            ext_body,
            ext_attachment,
            ext_unknown,
        })
    }
}
