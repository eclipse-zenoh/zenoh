//
// Copyright (c) 2023 ZettaScale Technology
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
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
    ZSlice,
};
use zenoh_protocol::{
    common::{iext, imsg},
    core::{Resolution, WhatAmI, ZenohIdProto},
    transport::{
        batch_size, id,
        init::{ext, flag, InitAck, InitSyn},
        BatchSize,
    },
};

use crate::{
    common::extension, RCodec, WCodec, Zenoh080, Zenoh080Bounded, Zenoh080Header, Zenoh080Length,
};

// InitSyn
impl<W> WCodec<&InitSyn, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &InitSyn) -> Self::Output {
        let InitSyn {
            version,
            whatami,
            zid,
            resolution,
            batch_size,
            ext_qos,
            ext_qos_link,
            #[cfg(feature = "shared-memory")]
            ext_shm,
            ext_auth,
            ext_mlink,
            ext_lowlatency,
            ext_compression,
            ext_patch,
        } = x;

        // Header
        let mut header = id::INIT;
        if resolution != &Resolution::default() || batch_size != &batch_size::UNICAST {
            header |= flag::S;
        }
        let mut n_exts = (ext_qos.is_some() as u8)
            + (ext_qos_link.is_some() as u8)
            + (ext_auth.is_some() as u8)
            + (ext_mlink.is_some() as u8)
            + (ext_lowlatency.is_some() as u8)
            + (ext_compression.is_some() as u8)
            + (*ext_patch != ext::PatchType::NONE) as u8;

        #[cfg(feature = "shared-memory")]
        {
            n_exts += ext_shm.is_some() as u8;
        }

        if n_exts != 0 {
            header |= flag::Z;
        }
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, version)?;

        let whatami: u8 = match whatami {
            WhatAmI::Router => 0b00,
            WhatAmI::Peer => 0b01,
            WhatAmI::Client => 0b10,
        };
        let flags: u8 = ((zid.size() as u8 - 1) << 4) | whatami;
        self.write(&mut *writer, flags)?;

        let lodec = Zenoh080Length::new(zid.size());
        lodec.write(&mut *writer, zid)?;

        if imsg::has_flag(header, flag::S) {
            self.write(&mut *writer, resolution.as_u8())?;
            self.write(&mut *writer, batch_size.to_le_bytes())?;
        }

        // Extensions
        if let Some(qos) = ext_qos.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (qos, n_exts != 0))?;
        }
        if let Some(qos_link) = ext_qos_link.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (qos_link, n_exts != 0))?;
        }
        #[cfg(feature = "shared-memory")]
        if let Some(shm) = ext_shm.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (shm, n_exts != 0))?;
        }
        if let Some(auth) = ext_auth.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (auth, n_exts != 0))?;
        }
        if let Some(mlink) = ext_mlink.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (mlink, n_exts != 0))?;
        }
        if let Some(lowlatency) = ext_lowlatency.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (lowlatency, n_exts != 0))?;
        }
        if let Some(compression) = ext_compression.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (compression, n_exts != 0))?;
        }
        if *ext_patch != ext::PatchType::NONE {
            n_exts -= 1;
            self.write(&mut *writer, (*ext_patch, n_exts != 0))?;
        }

        Ok(())
    }
}

impl<R> RCodec<InitSyn, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<InitSyn, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);
        codec.read(reader)
    }
}

impl<R> RCodec<InitSyn, &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<InitSyn, Self::Error> {
        if imsg::mid(self.header) != id::INIT || imsg::has_flag(self.header, flag::A) {
            return Err(DidntRead);
        }

        // Body
        let version: u8 = self.codec.read(&mut *reader)?;

        let flags: u8 = self.codec.read(&mut *reader)?;
        let whatami = match flags & 0b11 {
            0b00 => WhatAmI::Router,
            0b01 => WhatAmI::Peer,
            0b10 => WhatAmI::Client,
            _ => return Err(DidntRead),
        };
        let length = 1 + ((flags >> 4) as usize);
        let lodec = Zenoh080Length::new(length);
        let zid: ZenohIdProto = lodec.read(&mut *reader)?;

        let mut resolution = Resolution::default();
        let mut batch_size = batch_size::UNICAST.to_le_bytes();
        if imsg::has_flag(self.header, flag::S) {
            let flags: u8 = self.codec.read(&mut *reader)?;
            resolution = Resolution::from(flags & 0b00111111);
            batch_size = self.codec.read(&mut *reader)?;
        }
        let batch_size = BatchSize::from_le_bytes(batch_size);

        // Extensions
        let mut ext_qos = None;
        let mut ext_qos_link = None;
        #[cfg(feature = "shared-memory")]
        let mut ext_shm = None;
        let mut ext_auth = None;
        let mut ext_mlink = None;
        let mut ext_lowlatency = None;
        let mut ext_compression = None;
        let mut ext_patch = ext::PatchType::NONE;

        let mut has_ext = imsg::has_flag(self.header, flag::Z);
        while has_ext {
            let ext: u8 = self.codec.read(&mut *reader)?;
            let eodec = Zenoh080Header::new(ext);
            match iext::eid(ext) {
                ext::QoS::ID => {
                    let (q, ext): (ext::QoS, bool) = eodec.read(&mut *reader)?;
                    ext_qos = Some(q);
                    has_ext = ext;
                }
                ext::QoSLink::ID => {
                    let (q, ext): (ext::QoSLink, bool) = eodec.read(&mut *reader)?;
                    ext_qos_link = Some(q);
                    has_ext = ext;
                }
                #[cfg(feature = "shared-memory")]
                ext::Shm::ID => {
                    let (s, ext): (ext::Shm, bool) = eodec.read(&mut *reader)?;
                    ext_shm = Some(s);
                    has_ext = ext;
                }
                ext::Auth::ID => {
                    let (a, ext): (ext::Auth, bool) = eodec.read(&mut *reader)?;
                    ext_auth = Some(a);
                    has_ext = ext;
                }
                ext::MultiLink::ID => {
                    let (a, ext): (ext::MultiLink, bool) = eodec.read(&mut *reader)?;
                    ext_mlink = Some(a);
                    has_ext = ext;
                }
                ext::LowLatency::ID => {
                    let (q, ext): (ext::LowLatency, bool) = eodec.read(&mut *reader)?;
                    ext_lowlatency = Some(q);
                    has_ext = ext;
                }
                ext::Compression::ID => {
                    let (q, ext): (ext::Compression, bool) = eodec.read(&mut *reader)?;
                    ext_compression = Some(q);
                    has_ext = ext;
                }
                ext::Patch::ID => {
                    let (p, ext): (ext::PatchType, bool) = eodec.read(&mut *reader)?;
                    ext_patch = p;
                    has_ext = ext;
                }
                _ => {
                    has_ext = extension::skip(reader, "InitSyn", ext)?;
                }
            }
        }

        Ok(InitSyn {
            version,
            whatami,
            zid,
            resolution,
            batch_size,
            ext_qos,
            ext_qos_link,
            #[cfg(feature = "shared-memory")]
            ext_shm,
            ext_auth,
            ext_mlink,
            ext_lowlatency,
            ext_compression,
            ext_patch,
        })
    }
}

// InitAck
impl<W> WCodec<&InitAck, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &InitAck) -> Self::Output {
        let InitAck {
            version,
            whatami,
            zid,
            resolution,
            batch_size,
            cookie,
            ext_qos,
            ext_qos_link,
            #[cfg(feature = "shared-memory")]
            ext_shm,
            ext_auth,
            ext_mlink,
            ext_lowlatency,
            ext_compression,
            ext_patch,
        } = x;

        // Header
        let mut header = id::INIT | flag::A;
        if resolution != &Resolution::default() || batch_size != &batch_size::UNICAST {
            header |= flag::S;
        }
        let mut n_exts = (ext_qos.is_some() as u8)
            + (ext_qos_link.is_some() as u8)
            + (ext_auth.is_some() as u8)
            + (ext_mlink.is_some() as u8)
            + (ext_lowlatency.is_some() as u8)
            + (ext_compression.is_some() as u8)
            + (*ext_patch != ext::PatchType::NONE) as u8;

        #[cfg(feature = "shared-memory")]
        {
            n_exts += ext_shm.is_some() as u8;
        }

        if n_exts != 0 {
            header |= flag::Z;
        }
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, version)?;

        let whatami: u8 = match whatami {
            WhatAmI::Router => 0b00,
            WhatAmI::Peer => 0b01,
            WhatAmI::Client => 0b10,
        };
        let flags: u8 = ((zid.size() as u8 - 1) << 4) | whatami;
        self.write(&mut *writer, flags)?;

        let lodec = Zenoh080Length::new(zid.size());
        lodec.write(&mut *writer, zid)?;

        if imsg::has_flag(header, flag::S) {
            self.write(&mut *writer, resolution.as_u8())?;
            self.write(&mut *writer, batch_size.to_le_bytes())?;
        }

        let zodec = Zenoh080Bounded::<BatchSize>::new();
        zodec.write(&mut *writer, cookie)?;

        // Extensions
        if let Some(qos) = ext_qos.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (qos, n_exts != 0))?;
        }
        if let Some(qos_link) = ext_qos_link.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (qos_link, n_exts != 0))?;
        }
        #[cfg(feature = "shared-memory")]
        if let Some(shm) = ext_shm.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (shm, n_exts != 0))?;
        }
        if let Some(auth) = ext_auth.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (auth, n_exts != 0))?;
        }
        if let Some(mlink) = ext_mlink.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (mlink, n_exts != 0))?;
        }
        if let Some(lowlatency) = ext_lowlatency.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (lowlatency, n_exts != 0))?;
        }
        if let Some(compression) = ext_compression.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (compression, n_exts != 0))?;
        }
        if *ext_patch != ext::PatchType::NONE {
            n_exts -= 1;
            self.write(&mut *writer, (*ext_patch, n_exts != 0))?;
        }

        Ok(())
    }
}

impl<R> RCodec<InitAck, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<InitAck, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);
        codec.read(reader)
    }
}

impl<R> RCodec<InitAck, &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<InitAck, Self::Error> {
        if imsg::mid(self.header) != id::INIT || !imsg::has_flag(self.header, flag::A) {
            return Err(DidntRead);
        }

        // Body
        let version: u8 = self.codec.read(&mut *reader)?;

        let flags: u8 = self.codec.read(&mut *reader)?;
        let whatami = match flags & 0b11 {
            0b00 => WhatAmI::Router,
            0b01 => WhatAmI::Peer,
            0b10 => WhatAmI::Client,
            _ => return Err(DidntRead),
        };
        let length = 1 + ((flags >> 4) as usize);
        let lodec = Zenoh080Length::new(length);
        let zid: ZenohIdProto = lodec.read(&mut *reader)?;

        let mut resolution = Resolution::default();
        let mut batch_size = batch_size::UNICAST.to_le_bytes();
        if imsg::has_flag(self.header, flag::S) {
            let flags: u8 = self.codec.read(&mut *reader)?;
            resolution = Resolution::from(flags & 0b00111111);
            batch_size = self.codec.read(&mut *reader)?;
        }
        let batch_size = BatchSize::from_le_bytes(batch_size);

        let zodec = Zenoh080Bounded::<BatchSize>::new();
        let cookie: ZSlice = zodec.read(&mut *reader)?;

        // Extensions
        let mut ext_qos = None;
        let mut ext_qos_link = None;
        #[cfg(feature = "shared-memory")]
        let mut ext_shm = None;
        let mut ext_auth = None;
        let mut ext_mlink = None;
        let mut ext_lowlatency = None;
        let mut ext_compression = None;
        let mut ext_patch = ext::PatchType::NONE;

        let mut has_ext = imsg::has_flag(self.header, flag::Z);
        while has_ext {
            let ext: u8 = self.codec.read(&mut *reader)?;
            let eodec = Zenoh080Header::new(ext);
            match iext::eid(ext) {
                ext::QoS::ID => {
                    let (q, ext): (ext::QoS, bool) = eodec.read(&mut *reader)?;
                    ext_qos = Some(q);
                    has_ext = ext;
                }
                ext::QoSLink::ID => {
                    let (q, ext): (ext::QoSLink, bool) = eodec.read(&mut *reader)?;
                    ext_qos_link = Some(q);
                    has_ext = ext;
                }
                #[cfg(feature = "shared-memory")]
                ext::Shm::ID => {
                    let (s, ext): (ext::Shm, bool) = eodec.read(&mut *reader)?;
                    ext_shm = Some(s);
                    has_ext = ext;
                }
                ext::Auth::ID => {
                    let (a, ext): (ext::Auth, bool) = eodec.read(&mut *reader)?;
                    ext_auth = Some(a);
                    has_ext = ext;
                }
                ext::MultiLink::ID => {
                    let (a, ext): (ext::MultiLink, bool) = eodec.read(&mut *reader)?;
                    ext_mlink = Some(a);
                    has_ext = ext;
                }
                ext::LowLatency::ID => {
                    let (q, ext): (ext::LowLatency, bool) = eodec.read(&mut *reader)?;
                    ext_lowlatency = Some(q);
                    has_ext = ext;
                }
                ext::Compression::ID => {
                    let (q, ext): (ext::Compression, bool) = eodec.read(&mut *reader)?;
                    ext_compression = Some(q);
                    has_ext = ext;
                }
                ext::Patch::ID => {
                    let (p, ext): (ext::PatchType, bool) = eodec.read(&mut *reader)?;
                    ext_patch = p;
                    has_ext = ext;
                }
                _ => {
                    has_ext = extension::skip(reader, "InitAck", ext)?;
                }
            }
        }

        Ok(InitAck {
            version,
            whatami,
            zid,
            resolution,
            batch_size,
            cookie,
            ext_qos,
            ext_qos_link,
            #[cfg(feature = "shared-memory")]
            ext_shm,
            ext_auth,
            ext_mlink,
            ext_lowlatency,
            ext_compression,
            ext_patch,
        })
    }
}
