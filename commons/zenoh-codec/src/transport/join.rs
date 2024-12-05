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
use alloc::boxed::Box;
use core::time::Duration;

use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{
    common::{iext, imsg, ZExtZBufHeader},
    core::{Priority, Resolution, WhatAmI, ZenohIdProto},
    transport::{
        batch_size, id,
        join::{ext, flag, Join},
        BatchSize, PrioritySn, TransportSn,
    },
};

use crate::{common::extension, LCodec, RCodec, WCodec, Zenoh080, Zenoh080Header, Zenoh080Length};

impl LCodec<&PrioritySn> for Zenoh080 {
    fn w_len(self, p: &PrioritySn) -> usize {
        let PrioritySn {
            reliable,
            best_effort,
        } = p;
        self.w_len(*reliable) + self.w_len(*best_effort)
    }
}

impl<W> WCodec<&PrioritySn, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &PrioritySn) -> Self::Output {
        let PrioritySn {
            reliable,
            best_effort,
        } = x;

        self.write(&mut *writer, reliable)?;
        self.write(&mut *writer, best_effort)?;
        Ok(())
    }
}

impl<R> RCodec<PrioritySn, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<PrioritySn, Self::Error> {
        let reliable: TransportSn = self.read(&mut *reader)?;
        let best_effort: TransportSn = self.read(&mut *reader)?;

        Ok(PrioritySn {
            reliable,
            best_effort,
        })
    }
}

// Extension
impl<W> WCodec<(&ext::QoSType, bool), &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: (&ext::QoSType, bool)) -> Self::Output {
        let (x, more) = x;

        // Header
        let len = x.iter().fold(0, |acc, p| acc + self.w_len(p));
        let header = ZExtZBufHeader::<{ ext::QoS::ID }>::new(len);
        self.write(&mut *writer, (&header, more))?;

        // Body
        for p in x.iter() {
            self.write(&mut *writer, p)?;
        }

        Ok(())
    }
}

impl<R> RCodec<(ext::QoSType, bool), &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(ext::QoSType, bool), Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);
        codec.read(reader)
    }
}

impl<R> RCodec<(ext::QoSType, bool), &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(ext::QoSType, bool), Self::Error> {
        // Header
        let (_, more): (ZExtZBufHeader<{ ext::QoS::ID }>, bool) = self.read(&mut *reader)?;

        // Body
        let mut ext_qos = Box::new([PrioritySn::DEFAULT; Priority::NUM]);
        for p in ext_qos.iter_mut() {
            *p = self.codec.read(&mut *reader)?;
        }

        Ok((ext_qos, more))
    }
}

// Join
impl<W> WCodec<&Join, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Join) -> Self::Output {
        let Join {
            version,
            whatami,
            zid,
            resolution,
            batch_size,
            lease,
            next_sn,
            ext_qos,
            ext_shm,
            ext_patch,
        } = x;

        // Header
        let mut header = id::JOIN;
        if lease.as_millis() % 1_000 == 0 {
            header |= flag::T;
        }
        if resolution != &Resolution::default() || batch_size != &batch_size::MULTICAST {
            header |= flag::S;
        }
        let mut n_exts = (ext_qos.is_some() as u8)
            + (ext_shm.is_some() as u8)
            + (*ext_patch != ext::PatchType::NONE) as u8;
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

        if imsg::has_flag(header, flag::T) {
            self.write(&mut *writer, lease.as_secs())?;
        } else {
            self.write(&mut *writer, lease.as_millis() as u64)?;
        }
        self.write(&mut *writer, next_sn)?;

        // Extensions
        if let Some(qos) = ext_qos.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (qos, n_exts != 0))?;
        }
        if let Some(shm) = ext_shm.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (shm, n_exts != 0))?;
        }
        if *ext_patch != ext::PatchType::NONE {
            n_exts -= 1;
            self.write(&mut *writer, (*ext_patch, n_exts != 0))?;
        }

        Ok(())
    }
}

impl<R> RCodec<Join, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Join, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);
        codec.read(reader)
    }
}

impl<R> RCodec<Join, &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Join, Self::Error> {
        if imsg::mid(self.header) != id::JOIN {
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
        let mut batch_size = batch_size::MULTICAST.to_le_bytes();
        if imsg::has_flag(self.header, flag::S) {
            let flags: u8 = self.codec.read(&mut *reader)?;
            resolution = Resolution::from(flags & 0b00111111);
            batch_size = self.codec.read(&mut *reader)?;
        }
        let batch_size = BatchSize::from_le_bytes(batch_size);

        let lease: u64 = self.codec.read(&mut *reader)?;
        let lease = if imsg::has_flag(self.header, flag::T) {
            Duration::from_secs(lease)
        } else {
            Duration::from_millis(lease)
        };
        let next_sn: PrioritySn = self.codec.read(&mut *reader)?;

        // Extensions
        let mut ext_qos = None;
        let mut ext_shm = None;
        let mut ext_patch = ext::PatchType::NONE;

        let mut has_ext = imsg::has_flag(self.header, flag::Z);
        while has_ext {
            let ext: u8 = self.codec.read(&mut *reader)?;
            let eodec = Zenoh080Header::new(ext);
            match iext::eid(ext) {
                ext::QoS::ID => {
                    let (q, ext): (ext::QoSType, bool) = eodec.read(&mut *reader)?;
                    ext_qos = Some(q);
                    has_ext = ext;
                }
                ext::Shm::ID => {
                    let (s, ext): (ext::Shm, bool) = eodec.read(&mut *reader)?;
                    ext_shm = Some(s);
                    has_ext = ext;
                }
                ext::Patch::ID => {
                    let (p, ext): (ext::PatchType, bool) = eodec.read(&mut *reader)?;
                    ext_patch = p;
                    has_ext = ext;
                }
                _ => {
                    has_ext = extension::skip(reader, "Join", ext)?;
                }
            }
        }

        Ok(Join {
            version,
            whatami,
            zid,
            resolution,
            batch_size,
            lease,
            next_sn,
            ext_qos,
            ext_shm,
            ext_patch,
        })
    }
}
