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
use std::{
    num::{NonZeroU8, NonZeroUsize},
    sync::Arc,
};
use zenoh_buffers::{
    buffer::Buffer,
    reader::{DidntRead, HasReader},
    writer::{DidntWrite, HasWriter, Writer},
    BBuf, ZBufReader, ZSlice, ZSliceBuffer,
};
use zenoh_codec::{
    transport::batch::{BatchError, Zenoh080Batch},
    RCodec, WCodec,
};
use zenoh_protocol::{
    common::imsg,
    network::NetworkMessage,
    transport::{fragment::FragmentHeader, frame::FrameHeader, BatchSize, TransportMessage},
};
use zenoh_result::{zerror, ZResult};

// Split the inner buffer into (length, header, payload) inmutable slices
macro_rules! zsplit {
    ($slice:expr, $header:expr) => {{
        match $header.get() {
            Some(_) => $slice.split_at(BatchHeader::INDEX + 1),
            None => (&[], $slice),
        }
    }};
}

// Batch config
#[derive(Copy, Clone, Debug)]
pub struct BatchConfig {
    pub mtu: BatchSize,
    #[cfg(feature = "transport_compression")]
    pub is_compression: bool,
}

impl BatchConfig {
    fn header(&self) -> BatchHeader {
        let mut h = 0;
        #[cfg(feature = "transport_compression")]
        if self.is_compression {
            h |= BatchHeader::COMPRESSION;
        }
        BatchHeader::new(h)
    }
}

// Batch header
#[repr(transparent)]
#[derive(Copy, Clone, Debug)]
pub struct BatchHeader(Option<NonZeroU8>);

impl BatchHeader {
    const INDEX: usize = 0;

    #[cfg(feature = "transport_compression")]
    const COMPRESSION: u8 = 1;

    fn new(h: u8) -> Self {
        Self(NonZeroU8::new(h))
    }

    const fn is_empty(&self) -> bool {
        self.0.is_none()
    }

    const fn get(&self) -> Option<NonZeroU8> {
        self.0
    }

    /// Verify that the [`WBatch`][WBatch] is for a stream-based protocol, i.e., the first
    /// 2 bytes are reserved to encode the total amount of serialized bytes as 16-bits little endian.
    #[cfg(feature = "transport_compression")]
    #[inline(always)]
    pub fn is_compression(&self) -> bool {
        self.0
            .is_some_and(|h| imsg::has_flag(h.get(), Self::COMPRESSION))
    }
}

// WRITE BATCH
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
#[derive(Clone, Debug)]
pub struct WBatch {
    // The buffer to perform the batching on
    pub buffer: BBuf,
    // The batch codec
    pub codec: Zenoh080Batch,
    // It contains 1 byte as additional header, e.g. to signal the batch is compressed
    pub header: BatchHeader,
    // Statistics related to this batch
    #[cfg(feature = "stats")]
    pub stats: WBatchStats,
}

impl WBatch {
    pub fn new(config: BatchConfig) -> Self {
        let mut batch = Self {
            buffer: BBuf::with_capacity(config.mtu as usize),
            codec: Zenoh080Batch::new(),
            header: config.header(),
            #[cfg(feature = "stats")]
            stats: WBatchStats::default(),
        };

        // Bring the batch in a clear state
        batch.clear();

        batch
    }

    /// Verify that the [`WBatch`][WBatch] has no serialized bytes.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get the total number of bytes that have been serialized on the [`WBatch`][WBatch].
    #[inline(always)]
    pub fn len(&self) -> BatchSize {
        self.buffer.len() as BatchSize
    }

    /// Clear the [`WBatch`][WBatch] memory buffer and related internal state.
    #[inline(always)]
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.codec.clear();
        if let Some(h) = self.header.get() {
            let mut writer = self.buffer.writer();
            let _ = writer.write_u8(h.get());
        }
    }

    /// Get a `&[u8]` to access the internal memory buffer, usually for transmitting it on the network.
    #[inline(always)]
    pub fn as_slice(&self) -> &[u8] {
        self.buffer.as_slice()
    }

    // Split (length, header, payload) internal buffer slice
    #[inline(always)]
    fn split(&self) -> (&[u8], &[u8]) {
        zsplit!(self.buffer.as_slice(), self.header)
    }

    pub fn finalize(&mut self) -> ZResult<()> {
        #[cfg(feature = "transport_compression")]
        if self.header.is_compression() {
            self.compress()?;
        }

        Ok(())
    }

    #[cfg(feature = "transport_compression")]
    fn compress(&mut self) -> ZResult<()> {
        let (_header, payload) = self.split();

        // Create a new empty buffer
        let mut buffer =
            BBuf::with_capacity(lz4_flex::block::get_maximum_output_size(self.buffer.len()));

        // Write the initial bytes for the batch
        let mut writer = buffer.writer();
        if let Some(h) = self.header.get() {
            let _ = writer.write_u8(h.get());
        }

        // Compress the actual content
        writer
            .with_slot(writer.remaining(), |b| {
                lz4_flex::block::compress_into(payload, b).unwrap_or(0)
            })
            .map_err(|_| zerror!("Compression error"))?;

        // Verify wether the resulting compressed data is smaller than the initial input
        if buffer.len() < self.buffer.len() {
            // Replace the buffer in this batch
            self.buffer = buffer;
        } else {
            // Keep the original uncompressed buffer and unset the compression flag from the header
            let h = self
                .buffer
                .as_mut_slice()
                .get_mut(BatchHeader::INDEX)
                .ok_or_else(|| zerror!("Header not present"))?;
            *h &= !BatchHeader::COMPRESSION;
        }

        Ok(())
    }
}

pub trait Encode<Message> {
    type Output;

    fn encode(self, x: Message) -> Self::Output;
}

impl Encode<&TransportMessage> for &mut WBatch {
    type Output = Result<(), DidntWrite>;

    fn encode(self, x: &TransportMessage) -> Self::Output {
        let mut writer = self.buffer.writer();
        self.codec.write(&mut writer, x)
    }
}

impl Encode<&NetworkMessage> for &mut WBatch {
    type Output = Result<(), BatchError>;

    fn encode(self, x: &NetworkMessage) -> Self::Output {
        let mut writer = self.buffer.writer();
        self.codec.write(&mut writer, x)
    }
}

impl Encode<(&NetworkMessage, &FrameHeader)> for &mut WBatch {
    type Output = Result<(), BatchError>;

    fn encode(self, x: (&NetworkMessage, &FrameHeader)) -> Self::Output {
        let mut writer = self.buffer.writer();
        self.codec.write(&mut writer, x)
    }
}

impl Encode<(&mut ZBufReader<'_>, &mut FragmentHeader)> for &mut WBatch {
    type Output = Result<NonZeroUsize, DidntWrite>;

    fn encode(self, x: (&mut ZBufReader<'_>, &mut FragmentHeader)) -> Self::Output {
        let mut writer = self.buffer.writer();
        self.codec.write(&mut writer, x)
    }
}

// Read batch
#[derive(Debug)]
pub struct RBatch {
    // The buffer to perform deserializationn from
    buffer: ZSlice,
    // The batch codec
    codec: Zenoh080Batch,
    // It contains 1 byte as additional header, e.g. to signal the batch is compressed
    header: BatchHeader,
}

impl RBatch {
    pub fn new(config: BatchConfig, buffer: ZSlice) -> Self {
        Self {
            buffer,
            codec: Zenoh080Batch::new(),
            header: config.header(),
        }
    }

    #[inline(always)]
    pub const fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    // Split (length, header, payload) internal buffer slice
    #[inline(always)]
    fn split(&self) -> (&[u8], &[u8]) {
        zsplit!(self.buffer.as_slice(), self.header)
    }

    pub fn initialize<C, T>(&mut self, buff: C) -> ZResult<()>
    where
        C: Fn() -> T + Copy,
        T: ZSliceBuffer + 'static,
    {
        #[cfg(feature = "transport_compression")]
        if !self.header.is_empty() {
            let h = *self
                .buffer
                .get(BatchHeader::INDEX)
                .ok_or_else(|| zerror!("Batch header not present"))?;
            let header = BatchHeader::new(h);

            if header.is_compression() {
                self.decompress(buff)?;
            } else {
                self.buffer = self
                    .buffer
                    .subslice(BatchHeader::INDEX + 1, self.buffer.len())
                    .ok_or_else(|| zerror!("Batch length is invalid"))?;
            }
        }

        Ok(())
    }

    #[cfg(feature = "transport_compression")]
    fn decompress<T>(&mut self, mut buff: impl FnMut() -> T) -> ZResult<()>
    where
        T: ZSliceBuffer + 'static,
    {
        let (_h, p) = self.split();

        let mut into = (buff)();
        let n = lz4_flex::block::decompress_into(p, into.as_mut_slice())
            .map_err(|_| zerror!("Decompression error"))?;
        self.buffer = ZSlice::make(Arc::new(into), 0, n)
            .map_err(|_| zerror!("Invalid decompression buffer length"))?;

        Ok(())
    }
}

pub trait Decode<Message> {
    type Error;

    fn decode(self) -> Result<Message, Self::Error>;
}

impl Decode<TransportMessage> for &mut RBatch {
    type Error = DidntRead;

    fn decode(self) -> Result<TransportMessage, Self::Error> {
        let mut reader = self.buffer.reader();
        self.codec.read(&mut reader)
    }
}

#[cfg(test)]
mod tests {
    use std::vec;

    use super::*;
    use rand::Rng;
    use zenoh_buffers::ZBuf;
    use zenoh_protocol::{
        core::{CongestionControl, Encoding, Priority, Reliability, WireExpr},
        network::{ext, Push},
        transport::{
            frame::{self, FrameHeader},
            Fragment, KeepAlive, TransportMessage,
        },
        zenoh::{PushBody, Put},
    };

    #[test]
    fn rw_batch() {
        let mut rng = rand::thread_rng();

        for _ in 0..1_000 {
            let msg_ins: [TransportMessage; 2] = [TransportMessage::rand(), {
                let mut msg_in = Fragment::rand();
                msg_in.payload = vec![0u8; rng.gen_range(8..1_024)].into();
                msg_in.into()
            }];
            for msg_in in msg_ins {
                let config = BatchConfig {
                    mtu: BatchSize::MAX,
                    #[cfg(feature = "transport_compression")]
                    is_compression: rng.gen_bool(0.5),
                };
                let mut wbatch = WBatch::new(config);
                wbatch.encode(&msg_in).unwrap();
                println!("Encoded WBatch: {:?}", wbatch);
                wbatch.finalize().unwrap();
                println!("Finalized WBatch: {:?}", wbatch);

                let mut rbatch = RBatch::new(
                    config,
                    wbatch.buffer.as_slice().to_vec().into_boxed_slice().into(),
                );
                println!("Decoded RBatch: {:?}", rbatch);
                rbatch
                    .initialize(|| {
                        zenoh_buffers::vec::uninit(config.mtu as usize).into_boxed_slice()
                    })
                    .unwrap();
                println!("Initialized RBatch: {:?}", rbatch);
                let msg_out: TransportMessage = rbatch.decode().unwrap();
                assert_eq!(msg_in, msg_out);
            }
        }
    }

    #[test]
    fn serialization_batch() {
        let config = BatchConfig {
            mtu: BatchSize::MAX,
            #[cfg(feature = "transport_compression")]
            is_compression: false,
        };
        let mut batch = WBatch::new(config);

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
        batch.encode((&nmsg, &frame)).unwrap();
        assert_ne!(batch.len(), 0);
        nmsgs_in.push(nmsg.clone());

        frame.reliability = Reliability::BestEffort;
        batch.encode((&nmsg, &frame)).unwrap();
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
        batch.encode((&nmsg, &frame)).unwrap();
        assert_ne!(batch.len(), 0);
        nmsgs_in.push(nmsg.clone());
    }
}

#[cfg(all(feature = "transport_compression", feature = "unstable"))]
#[test]
fn tx_compression_test() {
    const COMPRESSION_BYTE: usize = 1;
    let payload = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4];
    let mut buff: Box<[u8]> =
        vec![0; lz4_flex::block::get_maximum_output_size(MAX_BATCH_SIZE) + 3].into_boxed_slice();

    // Compression done for the sake of comparing the result.
    let payload_compression_size = lz4_flex::block::compress_into(&payload, &mut buff).unwrap();

    fn get_header_value(buff: &[u8]) -> u16 {
        let mut header = [0_u8, 0_u8];
        header[..HEADER_BYTES_SIZE].copy_from_slice(&buff[..HEADER_BYTES_SIZE]);
        u16::from_le_bytes(header)
    }

    // Streamed with compression enabled
    let batch = [16, 0, 1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4];
    let (batch_size, was_compressed) = tx_compressed(true, true, &batch, &mut buff).unwrap();
    let header = get_header_value(&buff);
    assert!(was_compressed);
    assert_eq!(header as usize, payload_compression_size + COMPRESSION_BYTE);
    assert!(batch_size < batch.len() + COMPRESSION_BYTE);
    assert_eq!(batch_size, payload_compression_size + 3);

    // Not streamed with compression enabled
    let batch = payload;
    let (batch_size, was_compressed) = tx_compressed(true, false, &batch, &mut buff).unwrap();
    assert!(was_compressed);
    assert!(batch_size < batch.len() + COMPRESSION_BYTE);
    assert_eq!(batch_size, payload_compression_size + COMPRESSION_BYTE);

    // Streamed with compression disabled
    let batch = [16, 0, 1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4];
    let (batch_size, was_compressed) = tx_compressed(false, true, &batch, &mut buff).unwrap();
    let header = get_header_value(&buff);
    assert!(!was_compressed);
    assert_eq!(header as usize, payload.len() + COMPRESSION_BYTE);
    assert_eq!(batch_size, batch.len() + COMPRESSION_BYTE);

    // Not streamed and compression disabled
    let batch = payload;
    let (batch_size, was_compressed) = tx_compressed(false, false, &batch, &mut buff).unwrap();
    assert!(!was_compressed);
    assert_eq!(batch_size, payload.len() + COMPRESSION_BYTE);

    // Verify that if the compression result is bigger than the original payload size, then the non compressed payload is returned.
    let batch = [16, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]; // a non compressable payload with no repetitions
    let (batch_size, was_compressed) = tx_compressed(true, true, &batch, &mut buff).unwrap();
    assert!(!was_compressed);
    assert_eq!(batch_size, batch.len() + COMPRESSION_BYTE);
}

#[cfg(all(feature = "transport_compression", feature = "unstable"))]
#[test]
fn rx_compression_test() {
    let pool = RecyclingObjectPool::new(2, || vec![0_u8; MAX_BATCH_SIZE].into_boxed_slice());
    let mut buffer = pool.try_take().unwrap_or_else(|| pool.alloc());

    // Compressed batch
    let payload: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
    let compression_size = lz4_flex::block::compress_into(&payload, &mut buffer[1..]).unwrap();
    buffer[0] = 1; // is compressed byte

    let mut start_pos: usize = 0;
    let mut end_pos: usize = 0;

    rx_decompress(
        &mut buffer,
        &pool,
        compression_size + 1,
        &mut start_pos,
        &mut end_pos,
    )
    .unwrap();

    assert_eq!(start_pos, 0);
    assert_eq!(end_pos, payload.len());
    assert_eq!(buffer[start_pos..end_pos], payload);

    // Non compressed batch
    let mut start_pos: usize = 0;
    let mut end_pos: usize = 0;

    buffer[0] = 0;
    buffer[1..payload.len() + 1].copy_from_slice(&payload[..]);
    rx_decompress(
        &mut buffer,
        &pool,
        payload.len() + 1,
        &mut start_pos,
        &mut end_pos,
    )
    .unwrap();

    assert_eq!(start_pos, 1);
    assert_eq!(end_pos, payload.len() + 1);
    assert_eq!(buffer[start_pos..end_pos], payload);
}
