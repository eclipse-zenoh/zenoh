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
use crate::*;
use zenoh_buffers::{
    reader::{BacktrackableReader, DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{
    common::imsg,
    core::{Channel, Priority, Reliability, ZInt},
    transport::{tmsg, Frame, FramePayload},
    zenoh::ZenohMessage,
};

impl<W> WCodec<&mut W, &Frame> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Frame) -> Self::Output {
        // Decorator
        if x.channel.priority != Priority::default() {
            self.write(&mut *writer, &x.channel.priority)?;
        }

        // Header
        let mut header = tmsg::id::FRAME;
        if let Reliability::Reliable = x.channel.reliability {
            header |= tmsg::flag::R;
        }
        if let FramePayload::Fragment { is_final, .. } = x.payload {
            header |= tmsg::flag::F;
            if is_final {
                header |= tmsg::flag::E;
            }
        }
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, x.sn)?;
        match &x.payload {
            FramePayload::Fragment { buffer, .. } => writer.write_zslice(buffer.clone())?,
            FramePayload::Messages { messages } => {
                for m in messages.iter() {
                    self.write(&mut *writer, m)?;
                }
            }
        }
        Ok(())
    }
}

impl<R> RCodec<&mut R, Frame> for Zenoh060
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

impl<R> RCodec<&mut R, Frame> for Zenoh060Header
where
    R: Reader + BacktrackableReader,
{
    type Error = DidntRead;

    fn read(mut self, reader: &mut R) -> Result<Frame, Self::Error> {
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

        let payload = if imsg::has_flag(self.header, tmsg::flag::F) {
            // A fragmented frame is not supposed to be followed by
            // any other frame in the same batch. Read all the bytes.
            let buffer = reader.read_zslice(reader.remaining())?;
            let is_final = imsg::has_flag(self.header, tmsg::flag::E);
            FramePayload::Fragment { buffer, is_final }
        } else {
            let rcode = Zenoh060Reliability {
                reliability,
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
        };

        Ok(Frame {
            channel,
            sn,
            payload,
        })
    }
}
