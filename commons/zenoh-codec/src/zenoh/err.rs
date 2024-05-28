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
    common::{iext, imsg},
    core::Encoding,
    zenoh::{
        err::{ext, flag, Err},
        id,
    },
};

use crate::{common::extension, RCodec, WCodec, Zenoh080, Zenoh080Bounded, Zenoh080Header};

impl<W> WCodec<&Err, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Err) -> Self::Output {
        let Err {
            encoding,
            ext_sinfo,
            #[cfg(feature = "shared-memory")]
            ext_shm,
            ext_unknown,
            payload,
        } = x;

        // Header
        let mut header = id::ERR;
        if encoding != &Encoding::empty() {
            header |= flag::E;
        }
        let mut n_exts = (ext_sinfo.is_some() as u8) + (ext_unknown.len() as u8);
        #[cfg(feature = "shared-memory")]
        {
            n_exts += ext_shm.is_some() as u8;
        }
        if n_exts != 0 {
            header |= flag::Z;
        }
        self.write(&mut *writer, header)?;

        // Body
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
        for u in ext_unknown.iter() {
            n_exts -= 1;
            self.write(&mut *writer, (u, n_exts != 0))?;
        }

        // Payload
        let bodec = Zenoh080Bounded::<u32>::new();
        bodec.write(&mut *writer, payload)?;

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
        let mut encoding = Encoding::empty();
        if imsg::has_flag(self.header, flag::E) {
            encoding = self.codec.read(&mut *reader)?;
        }

        // Extensions
        let mut ext_sinfo: Option<ext::SourceInfoType> = None;
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
                #[cfg(feature = "shared-memory")]
                ext::Shm::ID => {
                    let (s, ext): (ext::ShmType, bool) = eodec.read(&mut *reader)?;
                    ext_shm = Some(s);
                    has_ext = ext;
                }
                _ => {
                    let (u, ext) = extension::read(reader, "Err", ext)?;
                    ext_unknown.push(u);
                    has_ext = ext;
                }
            }
        }

        // Payload
        let bodec = Zenoh080Bounded::<u32>::new();
        let payload: ZBuf = bodec.read(&mut *reader)?;

        Ok(Err {
            encoding,
            ext_sinfo,
            #[cfg(feature = "shared-memory")]
            ext_shm,
            ext_unknown,
            payload,
        })
    }
}
