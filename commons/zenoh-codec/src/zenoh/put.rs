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
    ZBuf,
};
use zenoh_protocol::{
    common::{iext, imsg, ZExtUnknown},
    core::Encoding,
    zenoh::{
        id,
        put::{ext, flag, Put},
    },
};

#[cfg(not(feature = "shared-memory"))]
use crate::Zenoh080Bounded;
#[cfg(feature = "shared-memory")]
use crate::Zenoh080Sliced;
use crate::{common::extension, RCodec, WCodec, Zenoh080, Zenoh080Header};

impl<W> WCodec<&Put, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    #[inline(always)]
    fn write(self, writer: &mut W, x: &Put) -> Self::Output {
        let Put {
            timestamp,
            encoding,
            ext_sinfo,
            ext_attachment,
            #[cfg(feature = "shared-memory")]
            ext_shm,
            ext_unknown,
            payload,
        }: &Put = x;

        // Header
        let mut header = id::PUT;
        if timestamp.is_some() {
            header |= flag::T;
        }
        if encoding != &Encoding::empty() {
            header |= flag::E;
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
        if encoding != &Encoding::empty() {
            self.write(&mut *writer, encoding)?;
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
            self.write(&mut *writer, (att, n_exts != 0))?;
        }
        for u in ext_unknown.iter() {
            n_exts -= 1;
            self.write(&mut *writer, (u, n_exts != 0))?;
        }

        // Payload
        #[cfg(feature = "shared-memory")]
        {
            let codec = Zenoh080Sliced::<u32>::new(ext_shm.is_some());
            codec.write(&mut *writer, payload)?;
        }

        #[cfg(not(feature = "shared-memory"))]
        {
            let bodec = Zenoh080Bounded::<u32>::new();
            bodec.write(&mut *writer, payload)?;
        }

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

    #[inline(always)]
    fn read(self, reader: &mut R) -> Result<Put, Self::Error> {
        if imsg::mid(self.header) != id::PUT {
            return Err(DidntRead);
        }

        // Body
        let mut timestamp: Option<uhlc::Timestamp> = None;
        if imsg::has_flag(self.header, flag::T) {
            #[cold]
            fn read_timestampt<R: Reader>(reader: &mut R) -> Result<uhlc::Timestamp, DidntRead> {
                let codec = Zenoh080::new();
                codec.read(&mut *reader)
            }
            timestamp = Some(read_timestampt(reader)?);
        }

        let mut encoding = Encoding::empty();
        if imsg::has_flag(self.header, flag::E) {
            #[cold]
            fn read_encoding<R: Reader>(reader: &mut R) -> Result<Encoding, DidntRead> {
                let codec = Zenoh080::new();
                codec.read(&mut *reader)
            }
            encoding = read_encoding(reader)?;
        }

        // Extensions
        let mut ext_sinfo: Option<ext::SourceInfoType> = None;
        #[cfg(feature = "shared-memory")]
        let mut ext_shm: Option<ext::ShmType> = None;
        let mut ext_attachment: Option<ext::AttachmentType> = None;
        let mut ext_unknown = Vec::new();

        let mut has_ext = imsg::has_flag(self.header, flag::Z);
        while has_ext {
            #[cold]
            fn read_exts<R: Reader>(
                reader: &mut R,
                ext_sinfo: &mut Option<ext::SourceInfoType>,
                #[cfg(feature = "shared-memory")] ext_shm: &mut Option<ext::ShmType>,
                ext_attachment: &mut Option<ext::AttachmentType>,
                ext_unknown: &mut Vec<ZExtUnknown>,
            ) -> Result<bool, DidntRead> {
                let codec = Zenoh080::new();
                let ext: u8 = codec.read(&mut *reader)?;
                let eodec = Zenoh080Header::new(ext);
                Ok(match iext::eid(ext) {
                    ext::SourceInfo::ID => {
                        let (s, ext): (ext::SourceInfoType, bool) = eodec.read(&mut *reader)?;
                        *ext_sinfo = Some(s);
                        ext
                    }
                    #[cfg(feature = "shared-memory")]
                    ext::Shm::ID => {
                        let (s, ext): (ext::ShmType, bool) = eodec.read(&mut *reader)?;
                        *ext_shm = Some(s);
                        ext
                    }
                    ext::Attachment::ID => {
                        let (a, ext): (ext::AttachmentType, bool) = eodec.read(&mut *reader)?;
                        *ext_attachment = Some(a);
                        ext
                    }
                    _ => {
                        let (u, ext) = extension::read(reader, "Put", ext)?;
                        ext_unknown.push(u);
                        ext
                    }
                })
            }
            has_ext = read_exts(
                reader,
                &mut ext_sinfo,
                #[cfg(feature = "shared-memory")]
                &mut ext_shm,
                &mut ext_attachment,
                &mut ext_unknown,
            )?;
        }

        // Payload
        let payload: ZBuf = {
            #[cfg(feature = "shared-memory")]
            {
                let codec = Zenoh080Sliced::<u32>::new(ext_shm.is_some());
                codec.read(&mut *reader)?
            }

            #[cfg(not(feature = "shared-memory"))]
            {
                let bodec = Zenoh080Bounded::<u32>::new();
                bodec.read(&mut *reader)?
            }
        };

        Ok(Put {
            timestamp,
            encoding,
            ext_sinfo,
            #[cfg(feature = "shared-memory")]
            ext_shm,
            ext_attachment,
            ext_unknown,
            payload,
        })
    }
}
