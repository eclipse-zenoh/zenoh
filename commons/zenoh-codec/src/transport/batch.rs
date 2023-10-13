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
use crate::{RCodec, WCodec, Zenoh080};
use alloc::sync::Arc;
use core::{
    fmt,
    num::{NonZeroU8, NonZeroUsize},
};
use zenoh_buffers::{
    buffer::Buffer,
    reader::{BacktrackableReader, DidntRead, HasReader, Reader, SiphonableReader},
    writer::{BacktrackableWriter, DidntWrite, HasWriter, Writer},
    BBuf, ZBufReader, ZSlice, ZSliceBuffer,
};
use zenoh_protocol::{
    common::imsg,
    core::{Locator, Reliability},
    network::NetworkMessage,
    transport::{
        fragment::FragmentHeader, frame::FrameHeader, BatchSize, TransportMessage, TransportSn,
    },
};

type HeaderSize = u8;

const LENGTH_BYTES: [u8; 2] = BatchSize::MIN.to_le_bytes();
const HEADER_BYTES: [u8; 1] = HeaderSize::MIN.to_le_bytes();

// Split the inner buffer into (length, header, payload) inmutable slices
macro_rules! zsplit {
    ($slice:expr, $has_length:expr, $has_header:expr) => {{
        match ($has_length, $has_header) {
            (false, false) => (&[], &[], $slice),
            (true, false) => {
                let (length, payload) = $slice.split_at(LENGTH_BYTES.len());
                (length, &[], payload)
            }
            (false, true) => {
                let (header, payload) = $slice.split_at(HEADER_BYTES.len());
                (&[], header, payload)
            }
            (true, true) => {
                let (length, tmp) = $slice.split_at(LENGTH_BYTES.len());
                let (header, payload) = tmp.split_at(HEADER_BYTES.len());
                (length, header, payload)
            }
        }
    }};
}

// Split the inner buffer into (length, header, payload) mutable slices
macro_rules! zsplitmut {
    ($slice:expr, $has_length:expr, $has_header:expr) => {{
        match ($has_length, $has_header) {
            (false, false) => (&mut [], &mut [], $slice),
            (true, false) => {
                let (length, payload) = $slice.split_at_mut(LENGTH_BYTES.len());
                (length, &mut [], payload)
            }
            (false, true) => {
                let (header, payload) = $slice.split_at_mut(HEADER_BYTES.len());
                (&mut [], header, payload)
            }
            (true, true) => {
                let (length, tmp) = $slice.split_at_mut(LENGTH_BYTES.len());
                let (header, payload) = tmp.split_at_mut(HEADER_BYTES.len());
                (length, header, payload)
            }
        }
    }};
}

mod header {
    #[cfg(feature = "transport_compression")]
    pub(super) const COMPRESSION: u8 = 1;
}

// Batch config
pub struct BatchConfig {
    pub is_streamed: bool,
    #[cfg(feature = "transport_compression")]
    pub is_compression: bool,
}

impl BatchConfig {
    fn build(self) -> (bool, Option<NonZeroU8>) {
        let mut h = 0;
        #[cfg(feature = "transport_compression")]
        if self.is_compression {
            h |= header::COMPRESSION;
        }
        // (has_length, header)
        (self.is_streamed, NonZeroU8::new(h))
    }
}

// WRITE BATCH
pub trait Encode<Message> {
    type Output;
    fn encode(self, message: Message) -> Self::Output;
}

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
    fn clear(&mut self) {
        self.reliable = None;
        self.best_effort = None;
    }
}

#[cfg(feature = "stats")]
#[derive(Clone, Copy, Debug, Default)]
pub struct WBatchStats {
    pub t_msgs: usize,
}

#[cfg(feature = "stats")]
impl WBatchStats {
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
/// [`WBatch`][WBatch] as long as they fit in the remaining buffer capacity.
///
/// In the serialized form, the [`WBatch`][WBatch] always contains one or more
/// [`TransportMessage`][TransportMessage]. In the particular case of [`TransportMessage`][TransportMessage] Frame,
/// its payload is either (i) one or more complete [`ZenohMessage`][ZenohMessage] or (ii) a fragment of a
/// a [`ZenohMessage`][ZenohMessage].
///
/// As an example, the content of the [`WBatch`][WBatch] in memory could be:
///
/// | Keep Alive | Frame Reliable<Zenoh Message, Zenoh Message> | Frame Best Effort<Zenoh Message Fragment> |
///
#[derive(Debug)]
pub struct WBatch<T>
where
    T: Buffer + HasWriter,
    <T as HasWriter>::Writer: BacktrackableWriter,
    <<T as HasWriter>::Writer as BacktrackableWriter>::Mark: fmt::Debug + Copy,
{
    mark: <<T as HasWriter>::Writer as BacktrackableWriter>::Mark,
    // It contains 2 bytes indicating how many bytes are in the batch
    has_length: bool,
    // It contains 1 byte as additional header, e.g. to signal the batch is compressed
    header: Option<NonZeroU8>,
    // The current frame being serialized: BestEffort/Reliable
    current_frame: CurrentFrame,
    // The latest SN
    pub latest_sn: LatestSn,
    // Statistics related to this batch
    #[cfg(feature = "stats")]
    pub stats: WBatchStats,
}

impl<T> WBatch<T>
where
    T: Buffer + HasWriter,
    <T as HasWriter>::Writer: BacktrackableWriter,
    <<T as HasWriter>::Writer as BacktrackableWriter>::Mark: core::fmt::Debug + Copy,
{
    pub fn new(config: BatchConfig, buffer: T) -> Self {
        let (has_length, header) = config.build();
        let mark = buffer.writer().mark();

        Self {
            mark,
            has_length,
            header,
            current_frame: CurrentFrame::None,
            latest_sn: LatestSn {
                reliable: None,
                best_effort: None,
            },
            #[cfg(feature = "stats")]
            stats: WBatchStats::default(),
        }
    }

    /// Verify that the [`WBatch`][WBatch] is for a stream-based protocol, i.e., the first
    /// 2 bytes are reserved to encode the total amount of serialized bytes as 16-bits little endian.
    #[inline(always)]
    const fn has_length(&self) -> bool {
        self.has_length
    }

    /// Verify that the [`WBatch`][WBatch] is for a stream-based protocol, i.e., the first
    /// 2 bytes are reserved to encode the total amount of serialized bytes as 16-bits little endian.
    #[inline(always)]
    const fn has_header(&self) -> bool {
        self.header.is_some()
    }

    // /// Clear the [`WBatch`][WBatch] memory buffer and related internal state.
    #[inline(always)]
    pub fn reset(&mut self, buffer: T) {
        let mut writer = buffer.writer();
        writer.rewind(self.mark);

        self.current_frame = CurrentFrame::None;
        self.latest_sn.clear();
        #[cfg(feature = "stats")]
        {
            self.stats.clear();
        }
        if self.has_length() {
            let _ = writer.write_exact(&LENGTH_BYTES);
        }
        if let Some(h) = self.header {
            let _ = writer.write_u8(h.get());
        }
    }

    /// In case the [`WBatch`][WBatch] is for a stream-based protocol, use the first 2 bytes
    /// to encode the total amount of serialized bytes as 16-bits little endian.
    pub fn finalize(&mut self, buffer: T) -> Result<(), DidntWrite> {
        if self.has_length() {
            let len = buffer.len() - LENGTH_BYTES.len();

            let mut writer = buffer.writer();
            let mark = writer.mark();
            writer.rewind(self.mark);
            writer.write_exact(&len.to_le_bytes())?;
            writer.rewind(mark);
        }

        // #[cfg(feature = "transport_compression")]
        // if self.is_compression() {
        //     self.compress()?;
        // }

        Ok(())
    }

    //     /// Get a `&[u8]` to access the internal memory buffer, usually for transmitting it on the network.
    //     #[inline(always)]
    //     pub fn as_slice(&self) -> &[u8] {
    //         self.buffer.as_slice()
    //     }

    //     // Split (length, header, payload) internal buffer slice
    //     #[inline(always)]
    //     fn split(&self) -> (&[u8], &[u8], &[u8]) {
    //         zsplit!(self.buffer.as_slice(), self.has_length(), self.has_header())
    //     }

    //     // Split (length, header, payload) internal buffer slice
    //     #[inline(always)]
    //     fn split_mut(&mut self) -> (&mut [u8], &mut [u8], &mut [u8]) {
    //         zsplitmut!(
    //             self.buffer.as_mut_slice(),
    //             self.has_length(),
    //             self.has_header()
    //         )
    //     }

    // #[cfg(feature = "transport_compression")]
    fn compress(&mut self) -> Result<(), DidntWrite> {
        // let (_length, _header, payload) = self.split();

        // // Create a new empty buffer
        // let mut buffer = BBuf::with_capacity(self.buffer.capacity());

        // // Write the initial bytes for the batch
        // let mut writer = buffer.writer();
        // if self.has_length() {
        //     let _ = writer.write_exact(&LENGTH_BYTES);
        // }
        // if let Some(h) = self.header {
        //     let _ = writer.write_u8(h.get());
        // }

        // // Compress the actual content
        // writer.with_slot(writer.remaining(), |b| {
        //     lz4_flex::block::compress_into(payload, b).unwrap_or(0)
        // })?;

        // // Verify wether the resulting compressed data is smaller than the initial input
        // if buffer.len() < self.buffer.len() {
        //     // Replace the buffer in this batch
        //     self.buffer = buffer;
        // } else {
        //     // Keep the original uncompressed buffer and unset the compression flag from the header
        //     let (_l, h, _p) = self.split_mut();
        //     h[0] &= !header::COMPRESSION;
        // }

        Ok(())
    }
}

// impl Encode<&TransportMessage> for &mut WBatch {
//     type Output = Result<(), DidntWrite>;

//     /// Try to serialize a [`TransportMessage`][TransportMessage] on the [`WBatch`][WBatch].
//     ///
//     /// # Arguments
//     /// * `message` - The [`TransportMessage`][TransportMessage] to serialize.
//     ///
//     fn encode(self, message: &TransportMessage) -> Self::Output {
//         // Mark the write operation
//         let mut writer = self.buffer.writer();
//         let mark = writer.mark();

//         let codec = Zenoh080::new();
//         codec.write(&mut writer, message).map_err(|e| {
//             // Revert the write operation
//             writer.rewind(mark);
//             e
//         })?;

//         // Reset the current frame value
//         self.current_frame = CurrentFrame::None;
//         #[cfg(feature = "stats")]
//         {
//             self.stats.t_msgs += 1;
//         }
//         Ok(())
//     }
// }

// #[repr(u8)]
// pub enum WError {
//     NewFrame,
//     DidntWrite,
// }

// impl Encode<&NetworkMessage> for &mut WBatch {
//     type Output = Result<(), WError>;

//     /// Try to serialize a [`NetworkMessage`][NetworkMessage] on the [`WBatch`][WBatch].
//     ///
//     /// # Arguments
//     /// * `message` - The [`NetworkMessage`][NetworkMessage] to serialize.
//     ///
//     fn encode(self, message: &NetworkMessage) -> Self::Output {
//         // Eventually update the current frame and sn based on the current status
//         if let (CurrentFrame::Reliable, false)
//         | (CurrentFrame::BestEffort, true)
//         | (CurrentFrame::None, _) = (self.current_frame, message.is_reliable())
//         {
//             // We are not serializing on the right frame.
//             return Err(WError::NewFrame);
//         };

//         // Mark the write operation
//         let mut writer = self.buffer.writer();
//         let mark = writer.mark();

//         let codec = Zenoh080::new();
//         codec.write(&mut writer, message).map_err(|_| {
//             // Revert the write operation
//             writer.rewind(mark);
//             WError::DidntWrite
//         })
//     }
// }

// impl Encode<(&NetworkMessage, FrameHeader)> for &mut WBatch {
//     type Output = Result<(), DidntWrite>;

//     /// Try to serialize a [`NetworkMessage`][NetworkMessage] on the [`WBatch`][WBatch].
//     ///
//     /// # Arguments
//     /// * `message` - The [`NetworkMessage`][NetworkMessage] to serialize.
//     ///
//     fn encode(self, message: (&NetworkMessage, FrameHeader)) -> Self::Output {
//         let (message, frame) = message;

//         // Mark the write operation
//         let mut writer = self.buffer.writer();
//         let mark = writer.mark();

//         let codec = Zenoh080::new();
//         // Write the frame header
//         codec.write(&mut writer, &frame).map_err(|e| {
//             // Revert the write operation
//             writer.rewind(mark);
//             e
//         })?;
//         // Write the zenoh message
//         codec.write(&mut writer, message).map_err(|e| {
//             // Revert the write operation
//             writer.rewind(mark);
//             e
//         })?;
//         // Update the frame
//         self.current_frame = match frame.reliability {
//             Reliability::Reliable => {
//                 self.latest_sn.reliable = Some(frame.sn);
//                 CurrentFrame::Reliable
//             }
//             Reliability::BestEffort => {
//                 self.latest_sn.best_effort = Some(frame.sn);
//                 CurrentFrame::BestEffort
//             }
//         };
//         Ok(())
//     }
// }

// impl Encode<(&mut ZBufReader<'_>, FragmentHeader)> for &mut WBatch {
//     type Output = Result<NonZeroUsize, DidntWrite>;

//     /// Try to serialize a [`ZenohMessage`][ZenohMessage] on the [`WBatch`][WBatch].
//     ///
//     /// # Arguments
//     /// * `message` - The [`ZenohMessage`][ZenohMessage] to serialize.
//     ///
//     fn encode(self, message: (&mut ZBufReader<'_>, FragmentHeader)) -> Self::Output {
//         let (reader, mut fragment) = message;

//         let mut writer = self.buffer.writer();
//         let codec = Zenoh080::new();

//         // Mark the buffer for the writing operation
//         let mark = writer.mark();

//         // Write the frame header
//         codec.write(&mut writer, &fragment).map_err(|e| {
//             // Revert the write operation
//             writer.rewind(mark);
//             e
//         })?;

//         // Check if it is really the final fragment
//         if reader.remaining() <= writer.remaining() {
//             // Revert the buffer
//             writer.rewind(mark);
//             // It is really the finally fragment, reserialize the header
//             fragment.more = false;
//             // Write the frame header
//             codec.write(&mut writer, &fragment).map_err(|e| {
//                 // Revert the write operation
//                 writer.rewind(mark);
//                 e
//             })?;
//         }

//         // Write the fragment
//         reader.siphon(&mut writer).map_err(|_| {
//             // Revert the write operation
//             writer.rewind(mark);
//             DidntWrite
//         })
//     }
// }

// // READ BATCH
// pub trait Decode<Message> {
//     type Error;
//     fn decode(self) -> Result<Message, Self::Error>;
// }

// #[derive(Debug)]
// pub struct RBatch {
//     // The buffer to perform deserializationn from
//     buffer: ZSlice,
//     // It contains 2 bytes indicating how many bytes are in the batch
//     has_length: bool,
//     // It contains 1 byte as additional header, e.g. to signal the batch is compressed
//     has_header: bool,
// }

// impl RBatch {
//     pub(crate) async fn read_unicast<C, T>(
//         config: BatchConfig,
//         from: &LinkUnicast,
//         buff: C,
//     ) -> ZResult<Self>
//     where
//         C: Fn() -> T + Copy,
//         T: ZSliceBuffer + 'static,
//     {
//         let (has_length, header) = config.build();

//         let mut into = (buff)();
//         let end = if has_length {
//             let start = LENGTH_BYTES.len();
//             // Read and decode the message length
//             from.read_exact(&mut into.as_mut_slice()[..start]).await?;

//             let (length, payload) = into.as_mut_slice().split_at_mut(start);
//             let n = BatchSize::from_le_bytes(
//                 length
//                     .try_into()
//                     .map_err(|e| zerror!("Invalid batch length: {e}"))?,
//             ) as usize;

//             // Read the bytes
//             from.read_exact(&mut payload[..n]).await?;
//             start + n
//         } else {
//             // Read the bytes
//             from.read(into.as_mut_slice()).await?
//         };

//         let buffer = ZSlice::make(Arc::new(into), 0, end)
//             .map_err(|_| zerror!("ZSlice index(es) out of bounds"))?;
//         Ok(Self {
//             buffer,
//             has_length,
//             has_header: header.is_some(),
//         })
//     }

//     pub(crate) async fn read_multicast<C, T>(
//         config: BatchConfig,
//         from: &LinkMulticast,
//         buff: C,
//     ) -> ZResult<(Self, Locator)>
//     where
//         C: Fn() -> T + Copy,
//         T: ZSliceBuffer + 'static,
//     {
//         let (has_length, header) = config.build();

//         // Read the bytes
//         let mut into = (buff)();
//         let (n, locator) = from.read(into.as_mut_slice()).await?;
//         let buffer = ZSlice::make(Arc::new(into), 0, n).map_err(|_| zerror!("Error"))?;
//         Ok((
//             Self {
//                 buffer,
//                 has_length,
//                 has_header: header.is_some(),
//             },
//             locator.into_owned(),
//         ))
//     }

//     #[inline(always)]
//     pub const fn is_empty(&self) -> bool {
//         self.buffer.is_empty()
//     }

//     #[inline(always)]
//     pub const fn is_streamed(&self) -> bool {
//         self.has_length()
//     }

//     /// Verify that the [`WBatch`][WBatch] is for a compression-enabled link,
//     /// i.e., the third byte is used to signa encode the total amount of serialized bytes as 16-bits little endian.
//     #[cfg(feature = "transport_compression")]
//     #[inline(always)]
//     pub fn is_compression(&self) -> bool {
//         let (_l, h, _p) = self.split();
//         !h.is_empty() && imsg::has_flag(h[0], header::COMPRESSION)
//     }

//     /// Verify that the [`WBatch`][WBatch] is for a stream-based protocol, i.e., the first
//     /// 2 bytes are reserved to encode the total amount of serialized bytes as 16-bits little endian.
//     #[inline(always)]
//     const fn has_length(&self) -> bool {
//         self.has_length
//     }

//     /// Verify that the [`WBatch`][WBatch] is for a compression-enabled link,
//     /// i.e., the third byte is used to signa encode the total amount of serialized bytes as 16-bits little endian.
//     #[inline(always)]
//     const fn has_header(&self) -> bool {
//         self.has_header
//     }

//     pub fn finalize<C, T>(&mut self, buff: C) -> Result<(), DidntRead>
//     where
//         C: Fn() -> T + Copy,
//         T: ZSliceBuffer + 'static,
//     {
//         macro_rules! zsubslice {
//             () => {
//                 let (l, h, _p) = self.split();
//                 let start = l.len() + h.len();
//                 let end = self.buffer.len();
//                 self.buffer = ZSlice::subslice(&self.buffer, start, end).ok_or(DidntRead)?;
//             };
//         }

//         #[cfg(feature = "transport_compression")]
//         if self.is_compression() {
//             self.uncompress(buff)?;
//         } else {
//             zsubslice!();
//         }
//         #[cfg(not(feature = "transport_compression"))]
//         {
//             zsubslice!();
//         }

//         Ok(())
//     }

//     // Split (length, header, payload) internal buffer slice
//     #[inline(always)]
//     fn split(&self) -> (&[u8], &[u8], &[u8]) {
//         zsplit!(self.buffer.as_slice(), self.has_length(), self.has_header())
//     }

//     #[cfg(feature = "transport_compression")]
//     fn uncompress<T>(&mut self, mut buff: impl FnMut() -> T) -> Result<(), DidntRead>
//     where
//         T: ZSliceBuffer + 'static,
//     {
//         let (_l, _h, p) = self.split();
//         let mut into = (buff)();
//         let n = lz4_flex::block::decompress_into(p, into.as_mut_slice()).map_err(|_| DidntRead)?;
//         self.buffer = ZSlice::make(Arc::new(into), 0, n).map_err(|_| DidntRead)?;

//         Ok(())
//     }
// }

// impl Decode<TransportMessage> for &mut RBatch {
//     type Error = DidntRead;

//     fn decode(self) -> Result<TransportMessage, Self::Error> {
//         let codec = Zenoh080::new();
//         let mut reader = self.buffer.reader();
//         codec.read(&mut reader)
//     }
// }

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use rand::Rng;
//     use zenoh_buffers::ZBuf;
//     use zenoh_protocol::{
//         core::{CongestionControl, Encoding, Priority, Reliability, WireExpr},
//         network::{ext, Push},
//         transport::{
//             frame::{self, FrameHeader},
//             KeepAlive, TransportMessage,
//         },
//         zenoh::{PushBody, Put},
//     };

//     #[test]
//     fn rw_batch() {
//         let mut rng = rand::thread_rng();

//         for _ in 0..1_000 {
//             let msg_in = TransportMessage::rand();

//             let config = BatchConfig {
//                 is_streamed: rng.gen_bool(0.5),
//                 #[cfg(feature = "transport_compression")]
//                 is_compression: rng.gen_bool(0.5),
//             };
//             let mut wbatch = WBatch::with_capacity(config, BatchSize::MAX);
//             wbatch.encode(&msg_in).unwrap();
//             wbatch.finalize().unwrap();
//             println!("WBatch: {:?}", wbatch);

//             let (has_length, has_header) = (wbatch.has_length(), wbatch.has_header());
//             let mut rbatch = RBatch {
//                 buffer: wbatch.buffer.into(),
//                 has_length,
//                 has_header,
//             };

//             rbatch
//                 .finalize(|| vec![0u8; BatchSize::MAX as usize].into_boxed_slice())
//                 .unwrap();
//             println!("RBatch: {:?}", rbatch);
//             let msg_out: TransportMessage = rbatch.decode().unwrap();
//             assert_eq!(msg_in, msg_out);
//         }
//     }

//     #[test]
//     fn serialization_batch() {
//         let config = BatchConfig {
//             is_streamed: false,
//             #[cfg(feature = "transport_compression")]
//             is_compression: false,
//         };
//         let mut batch = WBatch::with_capacity(config, BatchSize::MAX);

//         let tmsg: TransportMessage = KeepAlive.into();
//         let nmsg: NetworkMessage = Push {
//             wire_expr: WireExpr::empty(),
//             ext_qos: ext::QoSType::new(Priority::default(), CongestionControl::Block, false),
//             ext_tstamp: None,
//             ext_nodeid: ext::NodeIdType::default(),
//             payload: PushBody::Put(Put {
//                 timestamp: None,
//                 encoding: Encoding::default(),
//                 ext_sinfo: None,
//                 #[cfg(feature = "shared-memory")]
//                 ext_shm: None,
//                 ext_unknown: vec![],
//                 payload: ZBuf::from(vec![0u8; 8]),
//             }),
//         }
//         .into();

//         let mut tmsgs_in = vec![];
//         let mut nmsgs_in = vec![];

//         // Serialize assuming there is already a frame
//         batch.clear();
//         assert!(batch.encode(&nmsg).is_err());
//         assert_eq!(batch.len(), 0);

//         let mut frame = FrameHeader {
//             reliability: Reliability::Reliable,
//             sn: 0,
//             ext_qos: frame::ext::QoSType::default(),
//         };

//         // Serialize with a frame
//         batch.encode((&nmsg, frame)).unwrap();
//         assert_ne!(batch.len(), 0);
//         nmsgs_in.push(nmsg.clone());

//         frame.reliability = Reliability::BestEffort;
//         batch.encode((&nmsg, frame)).unwrap();
//         assert_ne!(batch.len(), 0);
//         nmsgs_in.push(nmsg.clone());

//         // Transport
//         batch.encode(&tmsg).unwrap();
//         tmsgs_in.push(tmsg.clone());

//         // Serialize assuming there is already a frame
//         assert!(batch.encode(&nmsg).is_err());
//         assert_ne!(batch.len(), 0);

//         // Serialize with a frame
//         frame.sn = 1;
//         batch.encode((&nmsg, frame)).unwrap();
//         assert_ne!(batch.len(), 0);
//         nmsgs_in.push(nmsg.clone());
//     }
// }

// #[cfg(all(feature = "transport_compression", feature = "unstable"))]
// #[test]
// fn tx_compression_test() {
//     const COMPRESSION_BYTE: usize = 1;
//     let payload = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4];
//     let mut buff: Box<[u8]> =
//         vec![0; lz4_flex::block::get_maximum_output_size(MAX_BATCH_SIZE) + 3].into_boxed_slice();

//     // Compression done for the sake of comparing the result.
//     let payload_compression_size = lz4_flex::block::compress_into(&payload, &mut buff).unwrap();

//     fn get_header_value(buff: &[u8]) -> u16 {
//         let mut header = [0_u8, 0_u8];
//         header[..HEADER_BYTES_SIZE].copy_from_slice(&buff[..HEADER_BYTES_SIZE]);
//         u16::from_le_bytes(header)
//     }

//     // Streamed with compression enabled
//     let batch = [16, 0, 1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4];
//     let (batch_size, was_compressed) = tx_compressed(true, true, &batch, &mut buff).unwrap();
//     let header = get_header_value(&buff);
//     assert!(was_compressed);
//     assert_eq!(header as usize, payload_compression_size + COMPRESSION_BYTE);
//     assert!(batch_size < batch.len() + COMPRESSION_BYTE);
//     assert_eq!(batch_size, payload_compression_size + 3);

//     // Not streamed with compression enabled
//     let batch = payload;
//     let (batch_size, was_compressed) = tx_compressed(true, false, &batch, &mut buff).unwrap();
//     assert!(was_compressed);
//     assert!(batch_size < batch.len() + COMPRESSION_BYTE);
//     assert_eq!(batch_size, payload_compression_size + COMPRESSION_BYTE);

//     // Streamed with compression disabled
//     let batch = [16, 0, 1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4];
//     let (batch_size, was_compressed) = tx_compressed(false, true, &batch, &mut buff).unwrap();
//     let header = get_header_value(&buff);
//     assert!(!was_compressed);
//     assert_eq!(header as usize, payload.len() + COMPRESSION_BYTE);
//     assert_eq!(batch_size, batch.len() + COMPRESSION_BYTE);

//     // Not streamed and compression disabled
//     let batch = payload;
//     let (batch_size, was_compressed) = tx_compressed(false, false, &batch, &mut buff).unwrap();
//     assert!(!was_compressed);
//     assert_eq!(batch_size, payload.len() + COMPRESSION_BYTE);

//     // Verify that if the compression result is bigger than the original payload size, then the non compressed payload is returned.
//     let batch = [16, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]; // a non compressable payload with no repetitions
//     let (batch_size, was_compressed) = tx_compressed(true, true, &batch, &mut buff).unwrap();
//     assert!(!was_compressed);
//     assert_eq!(batch_size, batch.len() + COMPRESSION_BYTE);
// }

// #[cfg(all(feature = "transport_compression", feature = "unstable"))]
// #[test]
// fn rx_compression_test() {
//     let pool = RecyclingObjectPool::new(2, || vec![0_u8; MAX_BATCH_SIZE].into_boxed_slice());
//     let mut buffer = pool.try_take().unwrap_or_else(|| pool.alloc());

//     // Compressed batch
//     let payload: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
//     let compression_size = lz4_flex::block::compress_into(&payload, &mut buffer[1..]).unwrap();
//     buffer[0] = 1; // is compressed byte

//     let mut start_pos: usize = 0;
//     let mut end_pos: usize = 0;

//     rx_decompress(
//         &mut buffer,
//         &pool,
//         compression_size + 1,
//         &mut start_pos,
//         &mut end_pos,
//     )
//     .unwrap();

//     assert_eq!(start_pos, 0);
//     assert_eq!(end_pos, payload.len());
//     assert_eq!(buffer[start_pos..end_pos], payload);

//     // Non compressed batch
//     let mut start_pos: usize = 0;
//     let mut end_pos: usize = 0;

//     buffer[0] = 0;
//     buffer[1..payload.len() + 1].copy_from_slice(&payload[..]);
//     rx_decompress(
//         &mut buffer,
//         &pool,
//         payload.len() + 1,
//         &mut start_pos,
//         &mut end_pos,
//     )
//     .unwrap();

//     assert_eq!(start_pos, 1);
//     assert_eq!(end_pos, payload.len() + 1);
//     assert_eq!(buffer[start_pos..end_pos], payload);
// }
