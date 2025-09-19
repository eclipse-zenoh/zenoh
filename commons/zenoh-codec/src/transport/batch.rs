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
use core::num::NonZeroUsize;

use zenoh_buffers::{
    reader::{BacktrackableReader, DidntRead, Reader, SiphonableReader},
    writer::{BacktrackableWriter, DidntWrite, Writer},
    ZBufReader,
};
use zenoh_protocol::{
    core::Reliability,
    network::{NetworkMessageExt, NetworkMessageRef},
    transport::{
        Fragment, FragmentHeader, Frame, FrameHeader, TransportBody, TransportMessage, TransportSn,
    },
};

use crate::{transport::frame::FrameReader, RCodec, WCodec, Zenoh080};

#[derive(Clone, Copy, Debug)]
#[repr(u8)]
pub enum CurrentFrame {
    Reliable,
    BestEffort,
    None,
}

#[derive(Clone, Copy, Debug)]
pub struct LatestSn {
    pub reliable: Option<TransportSn>,
    pub best_effort: Option<TransportSn>,
}

impl LatestSn {
    const fn new() -> Self {
        Self {
            reliable: None,
            best_effort: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Zenoh080Batch {
    // The current frame being serialized: BestEffort/Reliable
    pub current_frame: CurrentFrame,
    // The latest SN
    pub latest_sn: LatestSn,
}

impl Default for Zenoh080Batch {
    fn default() -> Self {
        Self::new()
    }
}

impl Zenoh080Batch {
    pub const fn new() -> Self {
        Self {
            current_frame: CurrentFrame::None,
            latest_sn: LatestSn::new(),
        }
    }

    pub fn clear(&mut self) {
        self.current_frame = CurrentFrame::None;
        self.latest_sn = LatestSn::new();
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatchError {
    NewFrame,
    DidntWrite,
}

impl<W> WCodec<&TransportMessage, &mut W> for &mut Zenoh080Batch
where
    W: Writer + BacktrackableWriter,
    <W as BacktrackableWriter>::Mark: Copy,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &TransportMessage) -> Self::Output {
        // Mark the write operation
        let mark = writer.mark();

        let codec = Zenoh080::new();
        codec.write(&mut *writer, x).map_err(|e| {
            // Revert the write operation
            writer.rewind(mark);
            e
        })?;

        // Reset the current frame value
        self.current_frame = CurrentFrame::None;

        Ok(())
    }
}

impl<W> WCodec<NetworkMessageRef<'_>, &mut W> for &mut Zenoh080Batch
where
    W: Writer + BacktrackableWriter,
    <W as BacktrackableWriter>::Mark: Copy,
{
    type Output = Result<(), BatchError>;

    fn write(self, writer: &mut W, x: NetworkMessageRef) -> Self::Output {
        // Eventually update the current frame and sn based on the current status
        if let (CurrentFrame::Reliable, false)
        | (CurrentFrame::BestEffort, true)
        | (CurrentFrame::None, _) = (self.current_frame, x.is_reliable())
        {
            // We are not serializing on the right frame.
            return Err(BatchError::NewFrame);
        }

        // Mark the write operation
        let mark = writer.mark();

        let codec = Zenoh080::new();
        codec.write(&mut *writer, x).map_err(|_| {
            // Revert the write operation
            writer.rewind(mark);
            BatchError::DidntWrite
        })
    }
}

impl<W> WCodec<(NetworkMessageRef<'_>, &FrameHeader), &mut W> for &mut Zenoh080Batch
where
    W: Writer + BacktrackableWriter,
    <W as BacktrackableWriter>::Mark: Copy,
{
    type Output = Result<(), BatchError>;

    fn write(self, writer: &mut W, x: (NetworkMessageRef, &FrameHeader)) -> Self::Output {
        let (m, f) = x;

        if let (Reliability::Reliable, false) | (Reliability::BestEffort, true) =
            (f.reliability, m.is_reliable())
        {
            // We are not serializing on the right frame.
            return Err(BatchError::NewFrame);
        }

        // Mark the write operation
        let mark = writer.mark();

        let codec = Zenoh080::new();
        // Write the frame header
        codec.write(&mut *writer, f).map_err(|_| {
            // Revert the write operation
            writer.rewind(mark);
            BatchError::DidntWrite
        })?;
        // Write the zenoh message
        codec.write(&mut *writer, m).map_err(|_| {
            // Revert the write operation
            writer.rewind(mark);
            BatchError::DidntWrite
        })?;
        // Update the frame
        self.current_frame = match f.reliability {
            Reliability::Reliable => {
                self.latest_sn.reliable = Some(f.sn);
                CurrentFrame::Reliable
            }
            Reliability::BestEffort => {
                self.latest_sn.best_effort = Some(f.sn);
                CurrentFrame::BestEffort
            }
        };
        Ok(())
    }
}

impl<W> WCodec<(&mut ZBufReader<'_>, &mut FragmentHeader), &mut W> for &mut Zenoh080Batch
where
    W: Writer + BacktrackableWriter,
    <W as BacktrackableWriter>::Mark: Copy,
{
    type Output = Result<NonZeroUsize, DidntWrite>;

    fn write(self, writer: &mut W, x: (&mut ZBufReader<'_>, &mut FragmentHeader)) -> Self::Output {
        let (r, f) = x;

        // Mark the buffer for the writing operation
        let mark = writer.mark();

        let codec = Zenoh080::new();
        // Write the fragment header
        codec.write(&mut *writer, &*f).map_err(|e| {
            // Revert the write operation
            writer.rewind(mark);
            e
        })?;

        // Check if it is really the final fragment
        if r.remaining() <= writer.remaining() {
            // Revert the buffer
            writer.rewind(mark);
            // It is really the finally fragment, reserialize the header
            f.more = false;
            // Write the fragment header
            codec.write(&mut *writer, &*f).map_err(|e| {
                // Revert the write operation
                writer.rewind(mark);
                e
            })?;
        }

        // Write the fragment
        r.siphon(&mut *writer).map_err(|_| {
            // Revert the write operation
            writer.rewind(mark);
            DidntWrite
        })
    }
}

impl<R> RCodec<TransportMessage, &mut R> for &mut Zenoh080Batch
where
    R: Reader + BacktrackableReader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<TransportMessage, Self::Error> {
        let codec = Zenoh080::new();
        let x: TransportMessage = codec.read(reader)?;

        match &x.body {
            TransportBody::Frame(Frame {
                reliability, sn, ..
            })
            | TransportBody::Fragment(Fragment {
                reliability, sn, ..
            }) => match reliability {
                Reliability::Reliable => {
                    self.current_frame = CurrentFrame::Reliable;
                    self.latest_sn.reliable = Some(*sn);
                }
                Reliability::BestEffort => {
                    self.current_frame = CurrentFrame::BestEffort;
                    self.latest_sn.best_effort = Some(*sn);
                }
            },
            _ => self.current_frame = CurrentFrame::None,
        }

        Ok(x)
    }
}

impl<'a, R: BacktrackableReader> RCodec<FrameReader<'a, R>, &'a mut R> for &mut Zenoh080Batch {
    type Error = DidntRead;

    fn read(self, reader: &'a mut R) -> Result<FrameReader<'a, R>, Self::Error> {
        let codec = Zenoh080::new();
        let frame: FrameReader<R> = codec.read(reader)?;
        match frame.reliability {
            Reliability::Reliable => {
                self.current_frame = CurrentFrame::Reliable;
                self.latest_sn.reliable = Some(frame.sn);
            }
            Reliability::BestEffort => {
                self.current_frame = CurrentFrame::BestEffort;
                self.latest_sn.best_effort = Some(frame.sn);
            }
        }
        Ok(frame)
    }
}
