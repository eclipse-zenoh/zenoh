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
use crate::{common::extension, RCodec, WCodec, Zenoh080, Zenoh080Header, Zenoh080Reliability};
use alloc::vec::Vec;
use zenoh_buffers::{
    reader::{BacktrackableReader, DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{
    common::{iext, imsg},
    core::Reliability,
    network::NetworkMessage,
    transport::{
        frame::{ext, flag, Frame, FrameHeader},
        id, TransportSn,
    },
};

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
                    has_ext = extension::skip(reader, "Frame", ext)?;
                }
            }
        }

        Ok(FrameHeader {
            reliability,
            sn,
            ext_qos,
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
            ext_qos: x.ext_qos,
        };
        self.write(&mut *writer, &header)?;

        // Body
        for m in x.payload.iter() {
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
        let mut payload = Vec::new();
        while reader.can_read() {
            let mark = reader.mark();
            let res: Result<NetworkMessage, DidntRead> = rcode.read(&mut *reader);
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
            ext_qos: header.ext_qos,
            payload,
        })
    }
}
