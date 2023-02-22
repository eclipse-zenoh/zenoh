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
use crate::{RCodec, WCodec, Zenoh080, Zenoh080Header, Zenoh080Reliability};
use zenoh_buffers::{
    reader::{BacktrackableReader, DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_collections::SingleOrVec;
use zenoh_protocol::{
    common::{imsg, ZExtUnknown, ZExtZInt},
    core::{Reliability, ZInt},
    transport::{
        frame::{ext, flag, Frame, FrameHeader},
        id,
    },
    zenoh::ZenohMessage,
};

// Extensions: QoS
impl<W> WCodec<(&ext::QoS, bool), &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: (&ext::QoS, bool)) -> Self::Output {
        let (ext, more) = x;
        self.write(&mut *writer, (&ext.inner, more))
    }
}

impl<R> RCodec<(ext::QoS, bool), &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(ext::QoS, bool), Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);

        codec.read(reader)
    }
}

impl<R> RCodec<(ext::QoS, bool), &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(ext::QoS, bool), Self::Error> {
        if imsg::mid(self.header) != ext::QOS {
            return Err(DidntRead);
        }

        let (inner, more): (ZExtZInt<{ ext::QOS }>, bool) = self.read(&mut *reader)?;
        Ok((ext::QoS { inner }, more))
    }
}

// FrameHeader
impl<W> WCodec<&FrameHeader, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &FrameHeader) -> Self::Output {
        // Header
        let mut header = id::FRAME;
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

impl<R> RCodec<FrameHeader, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<FrameHeader, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);
        codec.read(reader)
    }
}

impl<R> RCodec<FrameHeader, &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<FrameHeader, Self::Error> {
        if imsg::mid(self.header) != id::FRAME {
            return Err(DidntRead);
        }

        let reliability = match imsg::has_flag(self.header, flag::R) {
            true => Reliability::Reliable,
            false => Reliability::BestEffort,
        };
        let sn: ZInt = self.codec.read(&mut *reader)?;

        // Extensions
        let mut qos = ext::QoS::default();

        let mut has_ext = imsg::has_flag(self.header, flag::Z);
        while has_ext {
            let ext: u8 = self.codec.read(&mut *reader)?;
            let eodec = Zenoh080Header::new(ext);
            match imsg::mid(ext) {
                ext::QOS => {
                    let (q, ext): (ext::QoS, bool) = eodec.read(&mut *reader)?;
                    qos = q;
                    has_ext = ext;
                }
                _ => {
                    let (_, ext): (ZExtUnknown, bool) = eodec.read(&mut *reader)?;
                    has_ext = ext;
                }
            }
        }

        Ok(FrameHeader {
            reliability,
            sn,
            qos,
        })
    }
}

// Frame
impl<W> WCodec<&Frame, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Frame) -> Self::Output {
        // Header
        let header = FrameHeader {
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

impl<R> RCodec<Frame, &mut R> for Zenoh080
where
    R: Reader + BacktrackableReader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Frame, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);
        codec.read(reader)
    }
}

impl<R> RCodec<Frame, &mut R> for Zenoh080Header
where
    R: Reader + BacktrackableReader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Frame, Self::Error> {
        let header: FrameHeader = self.read(&mut *reader)?;

        let rcode = Zenoh080Reliability::new(header.reliability);
        let mut payload = SingleOrVec::default();
        while reader.can_read() {
            let mark = reader.mark();
            let res: Result<ZenohMessage, DidntRead> = rcode.read(&mut *reader);
            match res {
                Ok(m) => payload.push(m),
                Err(_) => {
                    reader.rewind(mark);
                    break;
                }
            }
        }

        Ok(Frame {
            reliability: header.reliability,
            sn: header.sn,
            qos: header.qos,
            payload,
        })
    }
}
