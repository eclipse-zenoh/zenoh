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
use alloc::vec::Vec;

use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{
    common::{iext, imsg},
    zenoh::{
        del::{ext, flag, Del},
        id,
    },
};

#[cfg(feature = "shared-memory")]
use crate::zenoh::shm::{ZBufShmReader, ZBufShmWriter};
use crate::{
    common::extension,
    zenoh::shm::{ZBufRCodec, ZBufRawReader, ZBufRawWriter},
    RCodec, WCodec, Zenoh080, Zenoh080Header,
};

impl<W> WCodec<&Del, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Del) -> Self::Output {
        let Del {
            timestamp,
            ext_sinfo,
            ext_attachment,
            #[cfg(feature = "shared-memory")]
            ext_shm,
            ext_unknown,
        } = x;

        // Header
        let mut header = id::DEL;
        if timestamp.is_some() {
            header |= flag::T;
        }
        let mut n_exts = (ext_sinfo.is_some()) as u8
            + (ext_attachment.is_some()) as u8
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
        if let Some(ts) = timestamp.as_ref() {
            self.write(&mut *writer, ts)?;
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

impl<R> RCodec<Del, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Del, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);
        codec.read(reader)
    }
}

impl<R> RCodec<Del, &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Del, Self::Error> {
        if imsg::mid(self.header) != id::DEL {
            return Err(DidntRead);
        }

        // Body
        let mut timestamp: Option<uhlc::Timestamp> = None;
        if imsg::has_flag(self.header, flag::T) {
            timestamp = Some(self.codec.read(&mut *reader)?);
        }

        // Extensions
        let mut ext_sinfo: Option<ext::SourceInfoType> = None;
        #[cfg(feature = "shared-memory")]
        let mut ext_shm: Option<ext::ShmType> = None;
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
                        if ext_sinfo.is_some() {
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
                    let (u, ext) = extension::read(reader, "Del", ext)?;
                    ext_unknown.push(u);
                    has_ext = ext;
                }
            }
        }

        Ok(Del {
            timestamp,
            ext_sinfo,
            #[cfg(feature = "shared-memory")]
            ext_shm,
            ext_attachment,
            ext_unknown,
        })
    }
}
