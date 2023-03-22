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
use crate::{RCodec, WCodec, Zenoh080, Zenoh080Bounded, Zenoh080Header, Zenoh080Length};
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
    ZSlice,
};
use zenoh_protocol::{
    common::{iext, imsg, ZExtUnknown},
    core::{Resolution, WhatAmI, ZenohId},
    transport::{
        id,
        init::{ext, flag, InitAck, InitSyn},
        BatchSize,
    },
};

// InitSyn
impl<W> WCodec<&InitSyn, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &InitSyn) -> Self::Output {
        // Header
        let mut header = id::INIT;
        if x.resolution != Resolution::default() || x.batch_size != u16::MAX {
            header |= flag::S;
        }
        let mut n_exts = (x.ext_qos.is_some() as u8)
            + (x.ext_shm.is_some() as u8)
            + (x.ext_auth.is_some() as u8)
            + (x.ext_mlink.is_some() as u8);
        if n_exts != 0 {
            header |= flag::Z;
        }
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, x.version)?;

        let whatami: u8 = match x.whatami {
            WhatAmI::Router => 0b00,
            WhatAmI::Peer => 0b01,
            WhatAmI::Client => 0b10,
        };
        let flags = ((x.zid.size() as u8 - 1) << 4) | whatami;
        self.write(&mut *writer, flags)?;

        let lodec = Zenoh080Length::new(x.zid.size());
        lodec.write(&mut *writer, &x.zid)?;

        if imsg::has_flag(header, flag::S) {
            self.write(&mut *writer, x.resolution.as_u8())?;
            self.write(&mut *writer, x.batch_size.to_le_bytes())?;
        }

        // Extensions
        if let Some(qos) = x.ext_qos.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (qos, n_exts != 0))?;
        }
        if let Some(shm) = x.ext_shm.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (shm, n_exts != 0))?;
        }
        if let Some(auth) = x.ext_auth.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (auth, n_exts != 0))?;
        }
        if let Some(mlink) = x.ext_mlink.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (mlink, n_exts != 0))?;
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
        let zid: ZenohId = lodec.read(&mut *reader)?;

        let mut resolution = Resolution::default();
        let mut batch_size = BatchSize::MAX.to_le_bytes();
        if imsg::has_flag(self.header, flag::S) {
            let flags: u8 = self.codec.read(&mut *reader)?;
            resolution = Resolution::from(flags & 0b00111111);
            batch_size = self.codec.read(&mut *reader)?;
        }
        let batch_size = BatchSize::from_le_bytes(batch_size);

        // Extensions
        let mut ext_qos = None;
        let mut ext_shm = None;
        let mut ext_auth = None;
        let mut ext_mlink = None;

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
                _ => {
                    let (_, ext): (ZExtUnknown, bool) = eodec.read(&mut *reader)?;
                    has_ext = ext;
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
            ext_shm,
            ext_auth,
            ext_mlink,
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
        // Header
        let mut header = id::INIT | flag::A;
        if x.resolution != Resolution::default() || x.batch_size != u16::MAX {
            header |= flag::S;
        }
        let mut n_exts = (x.ext_qos.is_some() as u8)
            + (x.ext_shm.is_some() as u8)
            + (x.ext_auth.is_some() as u8)
            + (x.ext_mlink.is_some() as u8);
        if n_exts != 0 {
            header |= flag::Z;
        }
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, x.version)?;

        let whatami: u8 = match x.whatami {
            WhatAmI::Router => 0b00,
            WhatAmI::Peer => 0b01,
            WhatAmI::Client => 0b10,
        };
        let flags = ((x.zid.size() as u8 - 1) << 4) | whatami;
        self.write(&mut *writer, flags)?;

        let lodec = Zenoh080Length::new(x.zid.size());
        lodec.write(&mut *writer, &x.zid)?;

        if imsg::has_flag(header, flag::S) {
            self.write(&mut *writer, x.resolution.as_u8())?;
            self.write(&mut *writer, x.batch_size.to_le_bytes())?;
        }

        let zodec = Zenoh080Bounded::<BatchSize>::new();
        zodec.write(&mut *writer, &x.cookie)?;

        // Extensions
        if let Some(qos) = x.ext_qos.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (qos, n_exts != 0))?;
        }
        if let Some(shm) = x.ext_shm.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (shm, n_exts != 0))?;
        }
        if let Some(auth) = x.ext_auth.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (auth, n_exts != 0))?;
        }
        if let Some(mlink) = x.ext_mlink.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (mlink, n_exts != 0))?;
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
        let zid: ZenohId = lodec.read(&mut *reader)?;

        let mut resolution = Resolution::default();
        let mut batch_size = BatchSize::MAX.to_le_bytes();
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
        let mut ext_shm = None;
        let mut ext_auth = None;
        let mut ext_mlink = None;

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
                _ => {
                    let (_, ext): (ZExtUnknown, bool) = eodec.read(&mut *reader)?;
                    has_ext = ext;
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
            ext_shm,
            ext_auth,
            ext_mlink,
        })
    }
}
