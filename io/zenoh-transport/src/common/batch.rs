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
use std::num::NonZeroUsize;
use zenoh_buffers::{
    reader::{Reader, SiphonableReader},
    writer::{BacktrackableWriter, DidntWrite, HasWriter, Writer},
    BBuf, ZBufReader,
};
use zenoh_codec::{WCodec, Zenoh080};
use zenoh_protocol::{
    core::Reliability,
    network::NetworkMessage,
    transport::{
        fragment::FragmentHeader, frame::FrameHeader, BatchSize, TransportMessage, TransportSn,
    },
    zenoh::ZenohMessage,
};

const LENGTH_BYTES: [u8; 2] = u16::MIN.to_be_bytes();

pub(crate) trait Encode<Message> {
    type Output;
    fn encode(self, message: Message) -> Self::Output;
}

pub(crate) trait Decode<Message> {
    type Error;
    fn decode(self) -> Result<Message, Self::Error>;
}

#[derive(Clone, Copy, Debug)]
#[repr(u8)]
pub(crate) enum CurrentFrame {
    Reliable,
    BestEffort,
    None,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct LatestSn {
    pub(crate) reliable: Option<TransportSn>,
    pub(crate) best_effort: Option<TransportSn>,
}

impl LatestSn {
    fn clear(&mut self) {
        self.reliable = None;
        self.best_effort = None;
    }
}

#[cfg(feature = "stats")]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SerializationBatchStats {
    pub(crate) t_msgs: usize,
}

#[cfg(feature = "stats")]
impl SerializationBatchStats {
    fn clear(&mut self) {
        self.t_msgs = 0;
    }
}

/// Write Batch
///
/// A [`WBatch`][WBatch] is a non-expandable and contiguous region of memory
/// that is used to serialize [`TransportMessage`][TransportMessage] and [`ZenohMessage`][ZenohMessage].
///
/// [`TransportMessage`][TransportMessage] are always serialized on the batch as they are, while
/// [`ZenohMessage`][ZenohMessage] are always serializaed on the batch as part of a [`TransportMessage`]
/// [TransportMessage] Frame. Reliable and Best Effort Frames can be interleaved on the same
/// [`SerializationBatch`][SerializationBatch] as long as they fit in the remaining buffer capacity.
///
/// In the serialized form, the [`SerializationBatch`][SerializationBatch] always contains one or more
/// [`TransportMessage`][TransportMessage]. In the particular case of [`TransportMessage`][TransportMessage] Frame,
/// its payload is either (i) one or more complete [`ZenohMessage`][ZenohMessage] or (ii) a fragment of a
/// a [`ZenohMessage`][ZenohMessage].
///
/// As an example, the content of the [`SerializationBatch`][SerializationBatch] in memory could be:
///
/// | Keep Alive | Frame Reliable<Zenoh Message, Zenoh Message> | Frame Best Effort<Zenoh Message Fragment> |
///
#[derive(Debug)]
pub(crate) struct WBatch {
    // The buffer to perform the batching on
    buffer: BBuf,
    // It is a streamed batch
    is_streamed: bool,
    // The current frame being serialized: BestEffort/Reliable
    current_frame: CurrentFrame,
    // The latest SN
    pub(crate) latest_sn: LatestSn,
    // Statistics related to this batch
    #[cfg(feature = "stats")]
    pub(crate) stats: SerializationBatchStats,
}

impl WBatch {
    pub(crate) fn new(size: BatchSize, is_streamed: bool) -> Self {
        let mut batch = Self {
            buffer: BBuf::with_capacity(size as usize),
            is_streamed,
            current_frame: CurrentFrame::None,
            latest_sn: LatestSn {
                reliable: None,
                best_effort: None,
            },
            #[cfg(feature = "stats")]
            stats: SerializationBatchStats::default(),
        };

        // Bring the batch in a clear state
        batch.clear();

        batch
    }

    /// Verify that the [`SerializationBatch`][SerializationBatch] has no serialized bytes.
    #[inline(always)]
    pub(crate) fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get the total number of bytes that have been serialized on the [`SerializationBatch`][SerializationBatch].
    #[inline(always)]
    pub(crate) fn len(&self) -> BatchSize {
        let len = self.buffer.len() as BatchSize;
        if self.is_streamed() {
            len - (LENGTH_BYTES.len() as BatchSize)
        } else {
            len
        }
    }

    /// Verify that the [`SerializationBatch`][SerializationBatch] is for a stream-based protocol, i.e., the first
    /// 2 bytes are reserved to encode the total amount of serialized bytes as 16-bits little endian.
    #[inline(always)]
    pub(crate) fn is_streamed(&self) -> bool {
        self.is_streamed
    }

    /// Clear the [`SerializationBatch`][SerializationBatch] memory buffer and related internal state.
    #[inline(always)]
    pub(crate) fn clear(&mut self) {
        self.buffer.clear();
        self.current_frame = CurrentFrame::None;
        self.latest_sn.clear();
        #[cfg(feature = "stats")]
        {
            self.stats.clear();
        }
        if self.is_streamed() {
            let mut writer = self.buffer.writer();
            let _ = writer.write_exact(&LENGTH_BYTES[..]);
        }
    }

    /// In case the [`SerializationBatch`][SerializationBatch] is for a stream-based protocol, use the first 2 bytes
    /// to encode the total amount of serialized bytes as 16-bits little endian.
    #[inline(always)]
    pub(crate) fn write_len(&mut self) {
        if self.is_streamed() {
            let length = self.len();
            self.buffer.as_mut_slice()[..LENGTH_BYTES.len()].copy_from_slice(&length.to_le_bytes());
        }
    }

    /// Get a `&[u8]` to access the internal memory buffer, usually for transmitting it on the network.
    #[inline(always)]
    pub(crate) fn as_bytes(&self) -> &[u8] {
        self.buffer.as_slice()
    }
}

impl Encode<&TransportMessage> for &mut WBatch {
    type Output = Result<(), DidntWrite>;

    /// Try to serialize a [`TransportMessage`][TransportMessage] on the [`SerializationBatch`][SerializationBatch].
    ///
    /// # Arguments
    /// * `message` - The [`TransportMessage`][TransportMessage] to serialize.
    ///
    fn encode(self, message: &TransportMessage) -> Self::Output {
        // Mark the write operation
        let mut writer = self.buffer.writer();
        let mark = writer.mark();

        let codec = Zenoh080::new();
        codec.write(&mut writer, message).map_err(|e| {
            // Revert the write operation
            writer.rewind(mark);
            e
        })?;

        // Reset the current frame value
        self.current_frame = CurrentFrame::None;
        #[cfg(feature = "stats")]
        {
            self.stats.t_msgs += 1;
        }
        Ok(())
    }
}

#[repr(u8)]
pub(crate) enum WError {
    NewFrame,
    DidntWrite,
}

impl Encode<&NetworkMessage> for &mut WBatch {
    type Output = Result<(), WError>;

    /// Try to serialize a [`NetworkMessage`][NetworkMessage] on the [`SerializationBatch`][SerializationBatch].
    ///
    /// # Arguments
    /// * `message` - The [`NetworkMessage`][NetworkMessage] to serialize.
    ///
    fn encode(self, message: &NetworkMessage) -> Self::Output {
        // Eventually update the current frame and sn based on the current status
        if let (CurrentFrame::Reliable, false)
        | (CurrentFrame::BestEffort, true)
        | (CurrentFrame::None, _) = (self.current_frame, message.is_reliable())
        {
            // We are not serializing on the right frame.
            return Err(WError::NewFrame);
        };

        // Mark the write operation
        let mut writer = self.buffer.writer();
        let mark = writer.mark();

        let codec = Zenoh080::new();
        codec.write(&mut writer, message).map_err(|_| {
            // Revert the write operation
            writer.rewind(mark);
            WError::DidntWrite
        })
    }
}

impl Encode<(&NetworkMessage, FrameHeader)> for &mut WBatch {
    type Output = Result<(), DidntWrite>;

    /// Try to serialize a [`NetworkMessage`][NetworkMessage] on the [`SerializationBatch`][SerializationBatch].
    ///
    /// # Arguments
    /// * `message` - The [`NetworkMessage`][NetworkMessage] to serialize.
    ///
    fn encode(self, message: (&NetworkMessage, FrameHeader)) -> Self::Output {
        let (message, frame) = message;

        // Mark the write operation
        let mut writer = self.buffer.writer();
        let mark = writer.mark();

        let codec = Zenoh080::new();
        // Write the frame header
        codec.write(&mut writer, &frame).map_err(|e| {
            // Revert the write operation
            writer.rewind(mark);
            e
        })?;
        // Write the zenoh message
        codec.write(&mut writer, message).map_err(|e| {
            // Revert the write operation
            writer.rewind(mark);
            e
        })?;
        // Update the frame
        self.current_frame = match frame.reliability {
            Reliability::Reliable => {
                self.latest_sn.reliable = Some(frame.sn);
                CurrentFrame::Reliable
            }
            Reliability::BestEffort => {
                self.latest_sn.best_effort = Some(frame.sn);
                CurrentFrame::BestEffort
            }
        };
        Ok(())
    }
}

impl Encode<&ZenohMessage> for &mut WBatch {
    type Output = Result<(), WError>;

    /// Try to serialize a [`ZenohMessage`][ZenohMessage] on the [`SerializationBatch`][SerializationBatch].
    ///
    /// # Arguments
    /// * `message` - The [`ZenohMessage`][ZenohMessage] to serialize.
    ///
    fn encode(self, message: &ZenohMessage) -> Self::Output {
        // Eventually update the current frame and sn based on the current status
        if let (CurrentFrame::Reliable, false)
        | (CurrentFrame::BestEffort, true)
        | (CurrentFrame::None, _) = (self.current_frame, message.is_reliable())
        {
            // We are not serializing on the right frame.
            return Err(WError::NewFrame);
        };

        // Mark the write operation
        let mut writer = self.buffer.writer();
        let mark = writer.mark();

        let codec = Zenoh080::new();
        codec.write(&mut writer, message).map_err(|_| {
            // Revert the write operation
            writer.rewind(mark);
            WError::DidntWrite
        })
    }
}

impl Encode<(&ZenohMessage, FrameHeader)> for &mut WBatch {
    type Output = Result<(), DidntWrite>;

    /// Try to serialize a [`ZenohMessage`][ZenohMessage] on the [`SerializationBatch`][SerializationBatch].
    ///
    /// # Arguments
    /// * `message` - The [`ZenohMessage`][ZenohMessage] to serialize.
    ///
    fn encode(self, message: (&ZenohMessage, FrameHeader)) -> Self::Output {
        let (message, frame) = message;

        // Mark the write operation
        let mut writer = self.buffer.writer();
        let mark = writer.mark();

        let codec = Zenoh080::new();
        // Write the frame header
        codec.write(&mut writer, &frame).map_err(|e| {
            // Revert the write operation
            writer.rewind(mark);
            e
        })?;
        // Write the zenoh message
        codec.write(&mut writer, message).map_err(|e| {
            // Revert the write operation
            writer.rewind(mark);
            e
        })?;
        // Update the frame
        self.current_frame = match frame.reliability {
            Reliability::Reliable => {
                self.latest_sn.reliable = Some(frame.sn);
                CurrentFrame::Reliable
            }
            Reliability::BestEffort => {
                self.latest_sn.best_effort = Some(frame.sn);
                CurrentFrame::BestEffort
            }
        };
        Ok(())
    }
}

impl Encode<(&mut ZBufReader<'_>, FragmentHeader)> for &mut WBatch {
    type Output = Result<NonZeroUsize, DidntWrite>;

    /// Try to serialize a [`ZenohMessage`][ZenohMessage] on the [`SerializationBatch`][SerializationBatch].
    ///
    /// # Arguments
    /// * `message` - The [`ZenohMessage`][ZenohMessage] to serialize.
    ///
    fn encode(self, message: (&mut ZBufReader<'_>, FragmentHeader)) -> Self::Output {
        let (reader, mut fragment) = message;

        let mut writer = self.buffer.writer();
        let codec = Zenoh080::new();

        // Mark the buffer for the writing operation
        let mark = writer.mark();

        // Write the frame header
        codec.write(&mut writer, &fragment).map_err(|e| {
            // Revert the write operation
            writer.rewind(mark);
            e
        })?;

        // Check if it is really the final fragment
        if reader.remaining() <= writer.remaining() {
            // Revert the buffer
            writer.rewind(mark);
            // It is really the finally fragment, reserialize the header
            fragment.more = false;
            // Write the frame header
            codec.write(&mut writer, &fragment).map_err(|e| {
                // Revert the write operation
                writer.rewind(mark);
                e
            })?;
        }

        // Write the fragment
        reader.siphon(&mut writer).map_err(|_| {
            // Revert the write operation
            writer.rewind(mark);
            DidntWrite
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zenoh_buffers::ZBuf;
    use zenoh_protocol::{
        core::{Channel, CongestionControl, Priority, Reliability},
        transport::{
            frame::{self, FrameHeader},
            KeepAlive, TransportMessage,
        },
        zenoh::ZenohMessage,
    };

    #[test]
    fn serialization_batch() {
        let mut batch = WBatch::new(u16::MAX, true);

        let tmsg: TransportMessage = KeepAlive.into();
        let mut zmsg = ZenohMessage::make_data(
            0.into(),
            ZBuf::from(vec![0u8; 8]),
            Channel {
                priority: Priority::default(),
                reliability: Reliability::Reliable,
            },
            CongestionControl::Block,
            None,
            None,
            None,
        );

        let mut tmsgs_in = vec![];
        let mut zmsgs_in = vec![];

        // Serialize assuming there is already a frame
        batch.clear();
        assert!(batch.encode(&zmsg).is_err());
        assert_eq!(batch.len(), 0);

        let mut frame = FrameHeader {
            reliability: zmsg.channel.reliability,
            sn: 0,
            ext_qos: frame::ext::QoSType::default(),
        };

        // Serialize with a frame
        batch.encode((&zmsg, frame)).unwrap();
        assert_ne!(batch.len(), 0);
        zmsgs_in.push(zmsg.clone());

        // Change reliability
        zmsg.channel.reliability = Reliability::BestEffort;
        assert!(batch.encode(&zmsg).is_err());
        assert_ne!(batch.len(), 0);

        frame.reliability = zmsg.channel.reliability;
        batch.encode((&zmsg, frame)).unwrap();
        assert_ne!(batch.len(), 0);
        zmsgs_in.push(zmsg.clone());

        // Transport
        batch.encode(&tmsg).unwrap();
        tmsgs_in.push(tmsg.clone());

        // Serialize assuming there is already a frame
        assert!(batch.encode(&zmsg).is_err());
        assert_ne!(batch.len(), 0);

        // Serialize with a frame
        frame.sn = 1;
        batch.encode((&zmsg, frame)).unwrap();
        assert_ne!(batch.len(), 0);
        zmsgs_in.push(zmsg.clone());
    }
}
