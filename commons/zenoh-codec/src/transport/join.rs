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
use crate::{common::extension, RCodec, WCodec, Zenoh080, Zenoh080Header, Zenoh080Length};
use core::time::Duration;
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{
    common::{iext, imsg},
    core::{Resolution, WhatAmI, ZenohId},
    transport::{
        id,
        join::{ext, flag, Join},
        BatchSize, TransportSn,
    },
};

impl<W> WCodec<&Join, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Join) -> Self::Output {
        // Header
        let mut header = id::JOIN;
        if x.lease.as_millis() % 1_000 == 0 {
            header |= flag::T;
        }
        if x.resolution != Resolution::default() || x.batch_size != BatchSize::MAX {
            header |= flag::S;
        }
        let mut n_exts = (x.ext_qos.is_some() as u8) + (x.ext_shm.is_some() as u8);
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
        let flags: u8 = ((x.zid.size() as u8 - 1) << 4) | whatami;
        self.write(&mut *writer, flags)?;

        let lodec = Zenoh080Length::new(x.zid.size());
        lodec.write(&mut *writer, &x.zid)?;

        if imsg::has_flag(header, flag::S) {
            self.write(&mut *writer, x.resolution.as_u8())?;
            self.write(&mut *writer, x.batch_size.to_le_bytes())?;
        }

        if imsg::has_flag(header, flag::T) {
            self.write(&mut *writer, x.lease.as_secs())?;
        } else {
            self.write(&mut *writer, x.lease.as_millis() as u64)?;
        }
        self.write(&mut *writer, x.next_sn)?;

        // Extensions
        if let Some(qos) = x.ext_qos.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (qos, n_exts != 0))?;
        }
        if let Some(shm) = x.ext_shm.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (shm, n_exts != 0))?;
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
        let zid: ZenohId = lodec.read(&mut *reader)?;

        let mut resolution = Resolution::default();
        let mut batch_size = BatchSize::MAX.to_le_bytes();
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
        let next_sn: TransportSn = self.codec.read(&mut *reader)?;

        // Extensions
        let mut ext_qos = None;
        let mut ext_shm = None;

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
        })
    }
}
