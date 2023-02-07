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
    reader::{BacktrackableReader, DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{
    common::{imsg, ZExtUnknown},
    core::{Reliability, ZInt},
    transport::{
        fragment::{ext, flag, Fragment, FragmentHeader},
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
        if x.qos != ext::QoS::default() {
            header |= flag::Z;
        }
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, x.sn)?;

        // Extensions
        if x.qos != ext::QoS::default() {
            self.write(&mut *writer, (&x.qos.inner, false))?;
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
        let sn: ZInt = self.codec.read(&mut *reader)?;

        // Extensions
        let mut qos = ext::QoS::default();

        let mut has_more = imsg::has_flag(self.header, flag::Z);
        while has_more {
            let ext: u8 = self.codec.read(&mut *reader)?;
            let eodec = Zenoh080Header::new(ext);
            match imsg::mid(ext) {
                ext::QOS => {
                    let (q, more): (ext::QoS, bool) = eodec.read(&mut *reader)?;
                    qos = q;
                    has_more = more;
                }
                _ => {
                    let (_, more): (ZExtUnknown, bool) = eodec.read(&mut *reader)?;
                    has_more = more;
                }
            }
        }

        Ok(FragmentHeader {
            reliability,
            sn,
            qos,
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
            sn: x.sn,
            qos: x.qos,
        };
        self.write(&mut *writer, &header)?;

        // Body
        for m in x.payload.as_ref() {
            self.write(&mut *writer, m)?;
        }

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
            sn: header.sn,
            qos: header.qos,
            payload,
        })
    }
}
