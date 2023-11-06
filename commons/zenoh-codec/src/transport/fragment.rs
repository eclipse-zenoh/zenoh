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
use crate::{common::extension, RCodec, WCodec, Zenoh080, Zenoh080Header};
use zenoh_buffers::{
    reader::{BacktrackableReader, DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{
    common::{iext, imsg},
    core::Reliability,
    transport::{
        fragment::{ext, flag, Fragment, FragmentHeader, TransportSn},
        id,
    },
};

// FragmentHeader
impl<W> WCodec<&FragmentHeader, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &FragmentHeader) -> Self::Output {
        // Header
        let mut header = id::FRAGMENT;
        if let Reliability::Reliable = x.reliability {
            header |= flag::R;
        }
        if x.more {
            header |= flag::M;
        }
        if x.ext_qos != ext::QoSType::default() {
            header |= flag::Z;
        }
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, x.sn)?;

        // Extensions
        if x.ext_qos != ext::QoSType::default() {
            self.write(&mut *writer, (x.ext_qos, false))?;
        }

        Ok(())
    }
}

impl<R> RCodec<FragmentHeader, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<FragmentHeader, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);
        codec.read(reader)
    }
}

impl<R> RCodec<FragmentHeader, &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<FragmentHeader, Self::Error> {
        if imsg::mid(self.header) != id::FRAGMENT {
            return Err(DidntRead);
        }

        let reliability = match imsg::has_flag(self.header, flag::R) {
            true => Reliability::Reliable,
            false => Reliability::BestEffort,
        };
        let more = imsg::has_flag(self.header, flag::M);
        let sn: TransportSn = self.codec.read(&mut *reader)?;

        // Extensions
        let mut ext_qos = ext::QoSType::default();

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
                _ => {
                    has_ext = extension::skip(reader, "Fragment", ext)?;
                }
            }
        }

        Ok(FragmentHeader {
            reliability,
            more,
            sn,
            ext_qos,
        })
    }
}

// Fragment
impl<W> WCodec<&Fragment, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Fragment) -> Self::Output {
        // Header
        let header = FragmentHeader {
            reliability: x.reliability,
            more: x.more,
            sn: x.sn,
            ext_qos: x.ext_qos,
        };
        self.write(&mut *writer, &header)?;

        // Body
        writer.write_zslice(&x.payload)?;

        Ok(())
    }
}

impl<R> RCodec<Fragment, &mut R> for Zenoh080
where
    R: Reader + BacktrackableReader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Fragment, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);
        codec.read(reader)
    }
}

impl<R> RCodec<Fragment, &mut R> for Zenoh080Header
where
    R: Reader + BacktrackableReader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Fragment, Self::Error> {
        let header: FragmentHeader = self.read(&mut *reader)?;
        let payload = reader.read_zslice(reader.remaining())?;

        Ok(Fragment {
            reliability: header.reliability,
            more: header.more,
            sn: header.sn,
            ext_qos: header.ext_qos,
            payload,
        })
    }
}
