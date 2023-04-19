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
use crate::{RCodec, WCodec, Zenoh060, Zenoh060Header, Zenoh060Reliability};
use alloc::vec::Vec;
use zenoh_buffers::{
    reader::{BacktrackableReader, DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{
    common::imsg,
    core::{Channel, Priority, Reliability, ZInt},
    transport::{tmsg, Frame, FrameHeader, FrameKind, FramePayload},
    zenoh::ZenohMessage,
};

// FrameHeader
impl<W> WCodec<&FrameHeader, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &FrameHeader) -> Self::Output {
        // Decorator
        if x.channel.priority != Priority::default() {
            self.write(&mut *writer, &x.channel.priority)?;
        }

        // Header
        let mut header = tmsg::id::FRAME;
        if let Reliability::Reliable = x.channel.reliability {
            header |= tmsg::flag::R;
        }
        match x.kind {
            FrameKind::Messages => {}
            FrameKind::SomeFragment => {
                header |= tmsg::flag::F;
            }
            FrameKind::LastFragment => {
                header |= tmsg::flag::F;
                header |= tmsg::flag::E;
            }
        }
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, x.sn)?;

        Ok(())
    }
}

impl<R> RCodec<FrameHeader, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<FrameHeader, Self::Error> {
        let codec = Zenoh060Header {
            header: self.read(&mut *reader)?,
            ..Default::default()
        };
        codec.read(reader)
    }
}

impl<R> RCodec<FrameHeader, &mut R> for Zenoh060Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(mut self, reader: &mut R) -> Result<FrameHeader, Self::Error> {
        let mut priority = Priority::default();
        if imsg::mid(self.header) == tmsg::id::PRIORITY {
            // Decode priority
            priority = self.read(&mut *reader)?;
            // Read next header
            self.header = self.codec.read(&mut *reader)?;
        }

        if imsg::mid(self.header) != tmsg::id::FRAME {
            return Err(DidntRead);
        }

        let reliability = match imsg::has_flag(self.header, tmsg::flag::R) {
            true => Reliability::Reliable,
            false => Reliability::BestEffort,
        };
        let channel = Channel {
            priority,
            reliability,
        };
        let sn: ZInt = self.codec.read(&mut *reader)?;

        let kind = if imsg::has_flag(self.header, tmsg::flag::F) {
            if imsg::has_flag(self.header, tmsg::flag::E) {
                FrameKind::LastFragment
            } else {
                FrameKind::SomeFragment
            }
        } else {
            FrameKind::Messages
        };

        Ok(FrameHeader { channel, sn, kind })
    }
}

// Frame
impl<W> WCodec<&Frame, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Frame) -> Self::Output {
        // Header
        let kind = match &x.payload {
            FramePayload::Fragment { is_final, .. } => {
                if *is_final {
                    FrameKind::LastFragment
                } else {
                    FrameKind::SomeFragment
                }
            }
            FramePayload::Messages { .. } => FrameKind::Messages,
        };
        let header = FrameHeader {
            channel: x.channel,
            sn: x.sn,
            kind,
        };
        self.write(&mut *writer, &header)?;

        // Body
        match &x.payload {
            FramePayload::Fragment { buffer, .. } => writer.write_zslice(buffer)?,
            FramePayload::Messages { messages } => {
                for m in messages.iter() {
                    self.write(&mut *writer, m)?;
                }
            }
        }
        Ok(())
    }
}

impl<R> RCodec<Frame, &mut R> for Zenoh060
where
    R: Reader + BacktrackableReader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Frame, Self::Error> {
        let codec = Zenoh060Header {
            header: self.read(&mut *reader)?,
            ..Default::default()
        };
        codec.read(reader)
    }
}

impl<R> RCodec<Frame, &mut R> for Zenoh060Header
where
    R: Reader + BacktrackableReader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Frame, Self::Error> {
        let header: FrameHeader = self.read(&mut *reader)?;

        let payload = match header.kind {
            FrameKind::Messages => {
                let rcode = Zenoh060Reliability {
                    reliability: header.channel.reliability,
                    ..Default::default()
                };

                let mut messages: Vec<ZenohMessage> = Vec::with_capacity(1);
                while reader.can_read() {
                    let mark = reader.mark();
                    let res: Result<ZenohMessage, DidntRead> = rcode.read(&mut *reader);
                    match res {
                        Ok(m) => messages.push(m),
                        Err(_) => {
                            reader.rewind(mark);
                            break;
                        }
                    }
                }
                FramePayload::Messages { messages }
            }
            FrameKind::SomeFragment | FrameKind::LastFragment => {
                // A fragmented frame is not supposed to be followed by
                // any other frame in the same batch. Read all the bytes.
                let buffer = reader.read_zslice(reader.remaining())?;
                let is_final = header.kind == FrameKind::LastFragment;
                FramePayload::Fragment { buffer, is_final }
            }
        };

        Ok(Frame {
            channel: header.channel,
            sn: header.sn,
            payload,
        })
    }
}
