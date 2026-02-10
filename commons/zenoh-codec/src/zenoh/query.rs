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
use alloc::{string::String, vec::Vec};

use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{
    common::{iext, imsg},
    zenoh::{
        id,
        query::{ext, flag, ConsolidationMode, Query},
    },
};

#[cfg(feature = "shared-memory")]
use crate::zenoh::shm::{ZBufShmReader, ZBufShmWriter};
use crate::{
    common::extension,
    zenoh::shm::{ZBufRCodec, ZBufRawReader, ZBufRawWriter},
    RCodec, WCodec, Zenoh080, Zenoh080Header,
};

// Consolidation
impl<W> WCodec<ConsolidationMode, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: ConsolidationMode) -> Self::Output {
        let v: u64 = match x {
            ConsolidationMode::Auto => 0,
            ConsolidationMode::None => 1,
            ConsolidationMode::Monotonic => 2,
            ConsolidationMode::Latest => 3,
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
        let v: u64 = self.read(&mut *reader)?;
        let c = match v {
            0 => ConsolidationMode::Auto,
            1 => ConsolidationMode::None,
            2 => ConsolidationMode::Monotonic,
            3 => ConsolidationMode::Latest,
            _ => ConsolidationMode::Auto, // Fallback on Auto if Consolidation is unknown
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
        let Query {
            consolidation,
            parameters,
            ext_sinfo,
            ext_body,
            ext_attachment,
            #[cfg(feature = "shared-memory")]
            ext_shm,
            ext_unknown,
        } = x;

        // Header
        let mut header = id::QUERY;
        if consolidation != &ConsolidationMode::DEFAULT {
            header |= flag::C;
        }
        if !parameters.is_empty() {
            header |= flag::P;
        }
        let mut n_exts = (ext_sinfo.is_some() as u8)
            + (ext_body.is_some() as u8)
            + (ext_attachment.is_some() as u8)
            + (ext_unknown.len() as u8);
        #[cfg(feature = "shared-memory")]
        {
            n_exts += ext_shm.is_some() as u8;
        }
        if n_exts != 0 {
            header |= flag::Z;
        }
        self.write(&mut *writer, header)?;

        // Body
        if consolidation != &ConsolidationMode::DEFAULT {
            self.write(&mut *writer, *consolidation)?;
        }
        if !parameters.is_empty() {
            self.write(&mut *writer, parameters)?;
        }

        // Extensions
        if let Some(sinfo) = ext_sinfo.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (sinfo, n_exts != 0))?;
        }
        #[cfg(feature = "shared-memory")]
        if let Some(eshm) = ext_shm.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (eshm, n_exts != 0))?;
        }
        if let Some(body) = ext_body.as_ref() {
            n_exts -= 1;
            #[cfg(feature = "shared-memory")]
            if ext_shm.is_some() {
                self.write(&mut *writer, (body, n_exts != 0, ZBufShmWriter {}))?;
            } else {
                self.write(&mut *writer, (body, n_exts != 0, ZBufRawWriter {}))?;
            }
            #[cfg(not(feature = "shared-memory"))]
            self.write(&mut *writer, (body, n_exts != 0, ZBufRawWriter {}))?;
        }
        if let Some(att) = ext_attachment.as_ref() {
            n_exts -= 1;
            #[cfg(feature = "shared-memory")]
            if ext_shm.is_some() {
                self.write(&mut *writer, (att, n_exts != 0, ZBufShmWriter {}))?;
            } else {
                self.write(&mut *writer, (att, n_exts != 0, ZBufRawWriter {}))?;
            }
            #[cfg(not(feature = "shared-memory"))]
            self.write(&mut *writer, (att, n_exts != 0, ZBufRawWriter {}))?;
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
        let mut consolidation = ConsolidationMode::DEFAULT;
        if imsg::has_flag(self.header, flag::C) {
            consolidation = self.codec.read(&mut *reader)?;
        }

        let mut parameters = String::new();
        if imsg::has_flag(self.header, flag::P) {
            parameters = self.codec.read(&mut *reader)?;
        }

        // Extensions
        let mut ext_sinfo: Option<ext::SourceInfoType> = None;
        let mut ext_body: Option<ext::QueryBodyType> = None;
        let mut ext_attachment: Option<ext::AttachmentType> = None;
        #[cfg(feature = "shared-memory")]
        let mut ext_shm: Option<ext::ShmType> = None;
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
                ext::QueryBodyType::VID => {
                    let (s, ext): (ext::QueryBodyType, bool) = {
                        fn read<ZBufR: ZBufRCodec, R: Reader>(
                            reader: &mut R,
                            codec: &Zenoh080Header,
                        ) -> Result<(ext::QueryBodyType, bool), DidntRead> {
                            let (a, ext, _): (ext::QueryBodyType, bool, ZBufR) =
                                codec.read(reader)?;
                            Ok((a, ext))
                        }

                        #[cfg(feature = "shared-memory")]
                        if ext_shm.is_some() {
                            read::<ZBufShmReader, R>(reader, &eodec)
                        } else {
                            read::<ZBufRawReader, R>(reader, &eodec)
                        }
                        #[cfg(not(feature = "shared-memory"))]
                        read::<ZBufRawReader, R>(reader, &eodec)
                    }?;
                    ext_body = Some(s);
                    has_ext = ext;
                }
                #[cfg(feature = "shared-memory")]
                ext::Shm::ID => {
                    let (s, ext): (ext::ShmType, bool) = eodec.read(&mut *reader)?;
                    ext_shm = Some(s);
                    has_ext = ext;
                }
                ext::Attachment::ID => {
                    let (a, ext): (ext::AttachmentType, bool) = {
                        fn read<ZBufR: ZBufRCodec, R: Reader>(
                            reader: &mut R,
                            codec: &Zenoh080Header,
                        ) -> Result<(ext::AttachmentType, bool), DidntRead>
                        {
                            let (a, ext, _): (ext::AttachmentType, bool, ZBufR) =
                                codec.read(reader)?;
                            Ok((a, ext))
                        }

                        #[cfg(feature = "shared-memory")]
                        if ext_shm.is_some() {
                            read::<ZBufShmReader, R>(reader, &eodec)
                        } else {
                            read::<ZBufRawReader, R>(reader, &eodec)
                        }
                        #[cfg(not(feature = "shared-memory"))]
                        read::<ZBufRawReader, R>(reader, &eodec)
                    }?;
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
            consolidation,
            parameters,
            ext_sinfo,
            #[cfg(feature = "shared-memory")]
            ext_shm,
            ext_body,
            ext_attachment,
            ext_unknown,
        })
    }
}
