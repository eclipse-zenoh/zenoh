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
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
    ZBuf,
};
use zenoh_protocol::{
    common::{iext, imsg, ZExtBody},
    network::{
        id,
        oam::{ext, flag, Oam, OamId},
    },
};

use crate::{common::extension, RCodec, WCodec, Zenoh080, Zenoh080Header};

impl<W> WCodec<&Oam, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Oam) -> Self::Output {
        let Oam {
            id,
            body,
            ext_qos,
            ext_tstamp,
        } = x;

        // Header
        let mut header = id::OAM;
        match &body {
            ZExtBody::Unit => {
                header |= iext::ENC_UNIT;
            }
            ZExtBody::Z64(_) => {
                header |= iext::ENC_Z64;
            }
            ZExtBody::ZBuf(_) => {
                header |= iext::ENC_ZBUF;
            }
        }
        let mut n_exts = ((ext_qos != &ext::QoSType::DEFAULT) as u8) + (ext_tstamp.is_some() as u8);
        if n_exts != 0 {
            header |= flag::Z;
        }
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, id)?;

        // Extensions
        if ext_qos != &ext::QoSType::DEFAULT {
            n_exts -= 1;
            self.write(&mut *writer, (*ext_qos, n_exts != 0))?;
        }
        if let Some(ts) = ext_tstamp.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (ts, n_exts != 0))?;
        }

        // Payload
        match body {
            ZExtBody::Unit => {}
            ZExtBody::Z64(u64) => {
                self.write(&mut *writer, u64)?;
            }
            ZExtBody::ZBuf(zbuf) => {
                self.write(&mut *writer, zbuf)?;
            }
        }

        Ok(())
    }
}

impl<R> RCodec<Oam, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Oam, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);
        codec.read(reader)
    }
}

impl<R> RCodec<Oam, &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Oam, Self::Error> {
        if imsg::mid(self.header) != id::OAM {
            return Err(DidntRead);
        }

        // Body
        let id: OamId = self.codec.read(&mut *reader)?;

        // Extensions
        let mut ext_qos = ext::QoSType::DEFAULT;
        let mut ext_tstamp = None;

        let mut has_ext = imsg::has_flag(self.header, flag::Z);
        while has_ext {
            let ext: u8 = self.codec.read(&mut *reader)?;
            let eodec = Zenoh080Header::new(ext);
            match iext::eid(ext) {
                ext::QoS::ID => {
                    let (q, ext): (ext::QoSType, bool) = eodec.read(&mut *reader)?;
                    ext_qos = q;
                    has_ext = ext;
                }
                ext::Timestamp::ID => {
                    let (t, ext): (ext::TimestampType, bool) = eodec.read(&mut *reader)?;
                    ext_tstamp = Some(t);
                    has_ext = ext;
                }
                _ => {
                    has_ext = extension::skip(reader, "OAM", ext)?;
                }
            }
        }

        // Payload
        let body = match self.header & iext::ENC_MASK {
            iext::ENC_UNIT => ZExtBody::Unit,
            iext::ENC_Z64 => {
                let u64: u64 = self.codec.read(&mut *reader)?;
                ZExtBody::Z64(u64)
            }
            iext::ENC_ZBUF => {
                let zbuf: ZBuf = self.codec.read(&mut *reader)?;
                ZExtBody::ZBuf(zbuf)
            }
            _ => return Err(DidntRead),
        };

        Ok(Oam {
            id,
            body,
            ext_qos,
            ext_tstamp,
        })
    }
}
