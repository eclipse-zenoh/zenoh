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
use crate::{RCodec, WCodec, Zenoh080, Zenoh080Header};
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{
    common::{iext, imsg, ZExtUnknown},
    network::{
        id,
        pull::{ext, flag},
        Pull,
    },
    zenoh::PullId,
};

impl<W> WCodec<&Pull, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Pull) -> Self::Output {
        // Header
        let mut header = id::PULL;
        let mut n_exts =
            ((x.ext_qos != ext::QoSType::default()) as u8) + (x.ext_tstamp.is_some() as u8);
        if n_exts != 0 {
            header |= flag::Z;
        }
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, x.id)?;

        // Extensions
        if x.ext_qos != ext::QoSType::default() {
            n_exts -= 1;
            self.write(&mut *writer, (x.ext_qos, n_exts != 0))?;
        }
        if let Some(ts) = x.ext_tstamp.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (ts, n_exts != 0))?;
        }

        Ok(())
    }
}

impl<R> RCodec<Pull, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Pull, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);
        codec.read(reader)
    }
}

impl<R> RCodec<Pull, &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Pull, Self::Error> {
        if imsg::mid(self.header) != id::PULL {
            return Err(DidntRead);
        }

        // Body
        let id: PullId = self.codec.read(&mut *reader)?;

        // Extensions
        let mut ext_qos = ext::QoSType::default();
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
                    let (_, ext): (ZExtUnknown, bool) = eodec.read(&mut *reader)?;
                    has_ext = ext;
                }
            }
        }

        Ok(Pull {
            id,
            ext_qos,
            ext_tstamp,
        })
    }
}
