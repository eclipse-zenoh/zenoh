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
use core::future::Future;
use std::{
    num::{NonZeroU8, NonZeroUsize},
    process::Output,
};
use zenoh_buffers::{
    reader::{DidntRead, Reader, SiphonableReader},
    writer::{BacktrackableWriter, DidntWrite, HasWriter, Writer},
    BBuf, ZBufReader, ZSlice,
};
use zenoh_codec::{WCodec, Zenoh080};
use zenoh_link::LinkUnicast;
use zenoh_protocol::{
    common::imsg,
    core::Reliability,
    network::NetworkMessage,
    transport::{
        fragment::FragmentHeader, frame::FrameHeader, BatchSize, TransportMessage, TransportSn,
    },
};
use zenoh_result::ZResult;

const LENGTH_BYTES: [u8; 2] = BatchSize::MIN.to_le_bytes();
const HEADER_BYTES: [u8; 1] = u8::MIN.to_le_bytes();

mod header {
    #[cfg(feature = "transport_compression")]
    pub(super) const COMPRESSION: u8 = 1;
}

// Split the inner buffer into (length, header, payload) inmutable slices
macro_rules! zsplit {
    ($batch:expr) => {{
        let slice = $batch.buffer.as_slice();
        match ($batch.has_length(), $batch.has_header()) {
            (false, false) => (&[], &[], slice),
            (true, false) => {
                let (length, payload) = slice.split_at(LENGTH_BYTES.len());
                (length, &[], payload)
            }
            (false, true) => {
                let (header, payload) = slice.split_at(HEADER_BYTES.len());
                (&[], header, payload)
            }
            (true, true) => {
                let (length, tmp) = slice.split_at(LENGTH_BYTES.len());
                let (header, payload) = tmp.split_at(HEADER_BYTES.len());
                (length, header, payload)
            }
        }
    }};
}

// Split the inner buffer into (length, header, payload) mutable slices
macro_rules! zsplitmut {
    ($batch:expr) => {{
        let (has_length, has_header) = ($batch.has_length(), $batch.has_header());
        let slice = $batch.buffer.as_mut_slice();
        match (has_length, has_header) {
            (false, false) => (&mut [], &mut [], slice),
            (true, false) => {
                let (length, payload) = slice.split_at_mut(LENGTH_BYTES.len());
                (length, &mut [], payload)
            }
            (false, true) => {
                let (header, payload) = slice.split_at_mut(HEADER_BYTES.len());
                (&mut [], header, payload)
            }
            (true, true) => {
                let (length, tmp) = slice.split_at_mut(LENGTH_BYTES.len());
                let (header, payload) = tmp.split_at_mut(HEADER_BYTES.len());
                (length, header, payload)
            }
        }
    }};
}

// WRITE BATCH
pub(crate) trait Encode<Message> {
    type Output;
    fn encode(self, message: Message) -> Self::Output;
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
    // It contains 2 bytes indicating how many bytes are in the batch
    has_length: bool,
    // It contains 1 byte as additional header, e.g. to signal the batch is compressed
    header: Option<NonZeroU8>,
    // The current frame being serialized: BestEffort/Reliable
    current_frame: CurrentFrame,
    // The latest SN
    pub(crate) latest_sn: LatestSn,
    // Statistics related to this batch
    #[cfg(feature = "stats")]
    pub(crate) stats: SerializationBatchStats,
}

impl WBatch {
    pub(crate) fn new(size: BatchSize) -> Self {
        let mut h = 0;

        let mut batch = Self {
            buffer: BBuf::with_capacity(size as usize),
            has_length: false,
            header: None,
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

    /// Verify that the [`SerializationBatch`][SerializationBatch] is for a compression-enabled link,
    /// i.e., the third byte is used to signa encode the total amount of serialized bytes as 16-bits little endian.
    #[inline(always)]
    pub(crate) fn set_streamed(mut self, v: bool) -> Self {
        self.has_length = v;
        self
    }

    #[inline(always)]
    pub(crate) const fn get_streamed(&self) -> bool {
        self.has_length
    }

    /// Verify that the [`SerializationBatch`][SerializationBatch] is for a compression-enabled link,
    /// i.e., the third byte is used to signa encode the total amount of serialized bytes as 16-bits little endian.
    #[cfg(feature = "transport_compression")]
    #[inline(always)]
    pub(crate) fn set_compression(mut self, v: bool) -> Self {
        if v {
            self.header = match self.header.as_ref() {
                Some(h) => NonZeroU8::new(h.get() | header::COMPRESSION),
                None => NonZeroU8::new(header::COMPRESSION),
            };
        } else {
            self.header = self
                .header
                .and_then(|h| NonZeroU8::new(h.get() & !header::COMPRESSION))
        }
        self
    }

    #[cfg(feature = "transport_compression")]
    #[inline(always)]
    pub(crate) fn get_compression(&self) -> bool {
        self.header
            .is_some_and(|h| imsg::has_flag(h.get(), header::COMPRESSION))
    }

    /// Verify that the [`SerializationBatch`][SerializationBatch] has no serialized bytes.
    #[inline(always)]
    pub(crate) fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get the total number of bytes that have been serialized on the [`SerializationBatch`][SerializationBatch].
    #[inline(always)]
    pub(crate) fn len(&self) -> BatchSize {
        let mut len = self.buffer.len() as BatchSize;
        if self.has_length() {
            len -= LENGTH_BYTES.len() as BatchSize;
        }
        len
    }

    /// Verify that the [`SerializationBatch`][SerializationBatch] is for a stream-based protocol, i.e., the first
    /// 2 bytes are reserved to encode the total amount of serialized bytes as 16-bits little endian.
    #[inline(always)]
    pub(crate) const fn has_length(&self) -> bool {
        self.has_length
    }

    /// Verify that the [`SerializationBatch`][SerializationBatch] is for a stream-based protocol, i.e., the first
    /// 2 bytes are reserved to encode the total amount of serialized bytes as 16-bits little endian.
    #[inline(always)]
    pub(crate) const fn has_header(&self) -> bool {
        self.header.is_some()
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
        if self.has_length() {
            let mut writer = self.buffer.writer();
            let _ = writer.write_exact(&LENGTH_BYTES[..]);
        }
        if let Some(h) = self.header {
            let mut writer = self.buffer.writer();
            let _ = writer.write_u8(h.get());
        }
    }

    /// In case the [`SerializationBatch`][SerializationBatch] is for a stream-based protocol, use the first 2 bytes
    /// to encode the total amount of serialized bytes as 16-bits little endian.
    pub(crate) fn finalize(mut self) -> ZResult<BBuf> {
        if self.has_length() {
            let (length, _h, _p) = self.split_mut();
            length.copy_from_slice(&self.len().to_le_bytes());
        }

        if let Some(header) = self.header {
            #[cfg(feature = "transport_compression")]
            if self.get_compression() {
                self.compress();
            }
        }

        Ok(self.buffer)
    }

    /// Get a `&[u8]` to access the internal memory buffer, usually for transmitting it on the network.
    #[inline(always)]
    pub(crate) fn as_slice(&self) -> &[u8] {
        self.buffer.as_slice()
    }

    // Split (length, header, payload) internal buffer slice
    #[inline(always)]
    fn split(&self) -> (&[u8], &[u8], &[u8]) {
        zsplit!(self)
    }

    // Split (length, header, payload) internal buffer slice
    #[inline(always)]
    fn split_mut(&mut self) -> (&mut [u8], &mut [u8], &mut [u8]) {
        zsplitmut!(self)
    }

    #[cfg(feature = "transport_compression")]
    fn compress(&mut self) -> Result<(), DidntWrite> {
        let (_length, _header, payload) = self.split();

        let mut buffer = BBuf::with_capacity(self.buffer.capacity());
        let mut writer = buffer.writer();
        writer.with_slot(writer.remaining(), |b| {
            lz4_flex::block::compress_into(payload, b).unwrap_or(0)
        })?;

        if buffer.len() < self.buffer.len() {
            self.buffer = buffer;
        } else {
            let (_length, header, _payload) = self.split_mut();
            header[0] &= !header::COMPRESSION;
        }

        Ok(())
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

// // READ BATCH
// #[derive(Debug)]
// pub(crate) struct RBatch {
//     // The buffer to perform deserializationn from
//     buffer: Box<[u8]>,
//     // It contains 2 bytes indicating how many bytes are in the batch
//     has_length: bool,
//     // It contains 1 byte as additional header, e.g. to signal the batch is compressed
//     has_header: bool,
// }

// impl RBatch {
//     /// Verify that the [`SerializationBatch`][SerializationBatch] is for a stream-based protocol, i.e., the first
//     /// 2 bytes are reserved to encode the total amount of serialized bytes as 16-bits little endian.
//     #[inline(always)]
//     pub(crate) const fn has_length(&self) -> bool {
//         self.has_length
//     }

//     /// Verify that the [`SerializationBatch`][SerializationBatch] is for a compression-enabled link,
//     /// i.e., the third byte is used to signa encode the total amount of serialized bytes as 16-bits little endian.

//     #[inline(always)]
//     pub(crate) const fn has_header(&self) -> bool {
//         self.has_header
//     }

//     // Split (length, header, payload) internal buffer slice
//     #[inline(always)]
//     fn split(&self) -> (&[u8], &[u8], &[u8]) {
//         zsplit!(self)
//     }

//     // Split (length, header, payload) internal buffer slice
//     #[inline(always)]
//     fn split_mut(&mut self) -> (&mut [u8], &mut [u8], &mut [u8]) {
//         zsplitmut!(self)
//     }

//     pub(crate) async fn read_unicast(&mut self, link: &LinkUnicast) -> ZResult<usize> {
//         let n = if self.has_length() {
//             let mut length = [0_u8, 0_u8];
//             link.read_exact(&mut length).await?;
//             let n = BatchSize::from_le_bytes(length) as usize;
//             link.read_exact(&mut self.buffer[0..n]).await?;
//             n
//         } else {
//             link.read(&mut self.buffer).await?
//         };

//         Ok(n)
//     }

// #[cfg(feature = "transport_compression")]
// pub(crate) fn uncompress_into(&mut self, batch: &mut WBatch) -> Result<(), DidntRead> {
//     use zenoh_protocol::common::imsg;

//         self.clear();
//         let mut writer = self.buffer.writer();
//     // let (_length, header, payload) = self.split();
//     // if !header.is_empty() && imsg::has_flag(header[0], header::COMPRESSED) {
//     // } else {
//     // }

//     Ok(())
// }
// }

#[cfg(test)]
mod tests {
    use super::*;
    use zenoh_buffers::ZBuf;
    use zenoh_protocol::{
        core::{CongestionControl, Encoding, Priority, Reliability, WireExpr},
        network::{ext, Push},
        transport::{
            frame::{self, FrameHeader},
            KeepAlive, TransportMessage,
        },
        zenoh::{PushBody, Put},
    };

    #[test]
    fn serialization_batch() {
        let mut batch = WBatch::new(BatchSize::MAX);

        let tmsg: TransportMessage = KeepAlive.into();
        let nmsg: NetworkMessage = Push {
            wire_expr: WireExpr::empty(),
            ext_qos: ext::QoSType::new(Priority::default(), CongestionControl::Block, false),
            ext_tstamp: None,
            ext_nodeid: ext::NodeIdType::default(),
            payload: PushBody::Put(Put {
                timestamp: None,
                encoding: Encoding::default(),
                ext_sinfo: None,
                #[cfg(feature = "shared-memory")]
                ext_shm: None,
                ext_unknown: vec![],
                payload: ZBuf::from(vec![0u8; 8]),
            }),
        }
        .into();

        let mut tmsgs_in = vec![];
        let mut nmsgs_in = vec![];

        // Serialize assuming there is already a frame
        batch.clear();
        assert!(batch.encode(&nmsg).is_err());
        assert_eq!(batch.len(), 0);

        let mut frame = FrameHeader {
            reliability: Reliability::Reliable,
            sn: 0,
            ext_qos: frame::ext::QoSType::default(),
        };

        // Serialize with a frame
        batch.encode((&nmsg, frame)).unwrap();
        assert_ne!(batch.len(), 0);
        nmsgs_in.push(nmsg.clone());

        frame.reliability = Reliability::BestEffort;
        batch.encode((&nmsg, frame)).unwrap();
        assert_ne!(batch.len(), 0);
        nmsgs_in.push(nmsg.clone());

        // Transport
        batch.encode(&tmsg).unwrap();
        tmsgs_in.push(tmsg.clone());

        // Serialize assuming there is already a frame
        assert!(batch.encode(&nmsg).is_err());
        assert_ne!(batch.len(), 0);

        // Serialize with a frame
        frame.sn = 1;
        batch.encode((&nmsg, frame)).unwrap();
        assert_ne!(batch.len(), 0);
        nmsgs_in.push(nmsg.clone());
    }
}
