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
use crate::{RCodec, WCodec, Zenoh080, Zenoh080Header, Zenoh080Length};
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
    ZSlice,
};
use zenoh_protocol::{
    common::{imsg, ZExtUnknown},
    core::{WhatAmI, ZenohId},
    transport::{
        id,
        init::{ext, flag, InitAck, InitSyn, Resolution},
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
        let has_extensions = x.qos.is_some() || x.shm.is_some() || x.auth.is_some();
        if has_extensions {
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
            self.write(&mut *writer, x.batch_size)?;
        }

        // Extensions
        if let Some(qos) = x.qos.as_ref() {
            let has_more = x.shm.is_some() || x.auth.is_some();
            self.write(&mut *writer, (qos, has_more))?;
        }

        if let Some(shm) = x.shm.as_ref() {
            let has_more = x.auth.is_some();
            self.write(&mut *writer, (shm, has_more))?;
        }

        if let Some(auth) = x.auth.as_ref() {
            let has_more = false;
            self.write(&mut *writer, (auth, has_more))?;
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
        let mut batch_size = u16::MAX;
        if imsg::has_flag(self.header, flag::S) {
            let flags: u8 = self.codec.read(&mut *reader)?;
            resolution = Resolution::from(flags & 0b00111111);
            batch_size = self.codec.read(&mut *reader)?;
        }

        // Extensions
        let mut qos = None;
        let mut shm = None;
        let mut auth = None;

        let mut has_more = imsg::has_flag(self.header, flag::Z);
        while has_more {
            let ext: u8 = self.codec.read(&mut *reader)?;
            let eodec = Zenoh080Header::new(ext);
            match imsg::mid(ext) {
                ext::QoS::ID => {
                    let (q, more): (ext::QoS, bool) = eodec.read(&mut *reader)?;
                    qos = Some(q);
                    has_more = more;
                }
                ext::Shm::ID => {
                    let (s, more): (ext::Shm, bool) = eodec.read(&mut *reader)?;
                    shm = Some(s);
                    has_more = more;
                }
                ext::Auth::ID => {
                    let (a, more): (ext::Auth, bool) = eodec.read(&mut *reader)?;
                    auth = Some(a);
                    has_more = more;
                }
                _ => {
                    let (_, more): (ZExtUnknown, bool) = eodec.read(&mut *reader)?;
                    has_more = more;
                }
            }
        }

        Ok(InitSyn {
            version,
            whatami,
            zid,
            resolution,
            batch_size,
            qos,
            shm,
            auth,
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
        let has_extensions = x.qos.is_some() || x.shm.is_some() || x.auth.is_some();
        if has_extensions {
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
            self.write(&mut *writer, x.batch_size)?;
        }

        self.write(&mut *writer, &x.cookie)?;

        // Extensions
        if let Some(qos) = x.qos.as_ref() {
            let has_more = x.shm.is_some() || x.auth.is_some();
            self.write(&mut *writer, (qos, has_more))?;
        }

        if let Some(shm) = x.shm.as_ref() {
            let has_more = x.auth.is_some();
            self.write(&mut *writer, (shm, has_more))?;
        }

        if let Some(auth) = x.auth.as_ref() {
            let has_more = false;
            self.write(&mut *writer, (auth, has_more))?;
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
        let mut batch_size = u16::MAX;
        if imsg::has_flag(self.header, flag::S) {
            let flags: u8 = self.codec.read(&mut *reader)?;
            resolution = Resolution::from(flags & 0b00111111);
            batch_size = self.codec.read(&mut *reader)?;
        }

        let cookie: ZSlice = self.codec.read(&mut *reader)?;

        // Extensions
        let mut qos = None;
        let mut shm = None;
        let mut auth = None;

        let mut has_more = imsg::has_flag(self.header, flag::Z);
        while has_more {
            let ext: u8 = self.codec.read(&mut *reader)?;
            let eodec = Zenoh080Header::new(ext);
            match imsg::mid(ext) {
                ext::QOS => {
                    let (q, more): (ext::QoS, bool) = eodec.read(&mut *reader)?;
                    qos = Some(q);
                    has_more = more;
                }
                ext::SHM => {
                    let (s, more): (ext::Shm, bool) = eodec.read(&mut *reader)?;
                    shm = Some(s);
                    has_more = more;
                }
                ext::AUTH => {
                    let (a, more): (ext::Auth, bool) = eodec.read(&mut *reader)?;
                    auth = Some(a);
                    has_more = more;
                }
                _ => {
                    let (_, more): (ZExtUnknown, bool) = eodec.read(&mut *reader)?;
                    has_more = more;
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
            qos,
            shm,
            auth,
        })
    }
}
