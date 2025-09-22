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
    buffer::Buffer,
    reader::{DidntRead, HasReader},
    writer::{DidntWrite, HasWriter, Writer},
    BBuf, ZBufReader, ZSlice, ZSliceBuffer,
};
use zenoh_codec::{
    transport::{
        batch::{BatchError, Zenoh080Batch},
        frame::FrameReader,
    },
    RCodec, WCodec,
};
use zenoh_protocol::{
    network::NetworkMessageRef,
    transport::{fragment::FragmentHeader, frame::FrameHeader, BatchSize, TransportMessage},
};
use zenoh_result::{zerror, ZResult};
#[cfg(feature = "transport_compression")]
use {std::sync::Arc, zenoh_protocol::common::imsg};

const L_LEN: usize = (BatchSize::BITS / 8) as usize;
const H_LEN: usize = BatchHeader::SIZE;

// Split the inner buffer into (length, header, payload) immutable slices
macro_rules! zsplit {
    ($slice:expr, $config:expr) => {{
        match ($config.is_streamed, $config.has_header()) {
            (true, true) => {
                let (l, s) = $slice.split_at(L_LEN);
                let (h, p) = s.split_at(H_LEN);
                (l, h, p)
            }
            (true, false) => {
                let (l, p) = $slice.split_at(L_LEN);
                (l, &[], p)
            }
            (false, true) => {
                let (h, p) = $slice.split_at(H_LEN);
                (&[], h, p)
            }
            (false, false) => (&[], &[], $slice),
        }
    }};
}

macro_rules! zsplit_mut {
    ($slice:expr, $config:expr) => {{
        match ($config.is_streamed, $config.has_header()) {
            (true, true) => {
                let (l, s) = $slice.split_at_mut(L_LEN);
                let (h, p) = s.split_at_mut(H_LEN);
                (l, h, p)
            }
            (true, false) => {
                let (l, p) = $slice.split_at_mut(L_LEN);
                (l, &mut [], p)
            }
            (false, true) => {
                let (h, p) = $slice.split_at_mut(H_LEN);
                (&mut [], h, p)
            }
            (false, false) => (&mut [], &mut [], $slice),
        }
    }};
}

// Batch config
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct BatchConfig {
    pub mtu: BatchSize,
    pub is_streamed: bool,
    #[cfg(feature = "transport_compression")]
    pub is_compression: bool,
}

impl Default for BatchConfig {
    fn default() -> Self {
        BatchConfig {
            mtu: BatchSize::MAX,
            is_streamed: false,
            #[cfg(feature = "transport_compression")]
            is_compression: false,
        }
    }
}

impl BatchConfig {
    const fn has_header(&self) -> bool {
        #[cfg(not(feature = "transport_compression"))]
        {
            false
        }
        #[cfg(feature = "transport_compression")]
        {
            self.is_compression
        }
    }

    fn header(&self) -> Option<BatchHeader> {
        #[cfg(not(feature = "transport_compression"))]
        {
            None
        }
        #[cfg(feature = "transport_compression")]
        {
            self.is_compression
                .then_some(BatchHeader::new(BatchHeader::COMPRESSION))
        }
    }
}

// Batch header
#[repr(transparent)]
#[derive(Copy, Clone, Debug)]
pub struct BatchHeader(u8);

impl BatchHeader {
    const SIZE: usize = 1;
    #[cfg(feature = "transport_compression")]
    const COMPRESSION: u8 = 1; // 1 << 0

    #[cfg(feature = "transport_compression")]
    const fn new(h: u8) -> Self {
        Self(h)
    }

    const fn as_u8(&self) -> u8 {
        self.0
    }

    /// Verify that the [`WBatch`] is for a stream-based protocol, i.e., the first
    /// 2 bytes are reserved to encode the total amount of serialized bytes as 16-bits little endian.
    #[cfg(feature = "transport_compression")]
    #[inline(always)]
    pub fn is_compression(&self) -> bool {
        imsg::has_flag(self.as_u8(), Self::COMPRESSION)
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

#[repr(u8)]
#[derive(Debug)]
pub enum Finalize {
    Batch,
    Buffer,
}

/// Write Batch
///
/// A [`WBatch`] is a non-expandable and contiguous region of memory
/// that is used to serialize [`TransportMessage`] and [`NetworkMessage`].
///
/// [`TransportMessage`] are always serialized on the batch as they are, while
/// [`NetworkMessage`] are always serializaed on the batch as part of a [`TransportMessage`]
/// [TransportMessage] Frame. Reliable and Best Effort Frames can be interleaved on the same
/// [`WBatch`] as long as they fit in the remaining buffer capacity.
///
/// In the serialized form, the [`WBatch`] always contains one or more
/// [`TransportMessage`]. In the particular case of [`TransportMessage`] Frame,
/// its payload is either (i) one or more complete [`NetworkMessage`] or (ii) a fragment of a
/// a [`NetworkMessage`].
///
/// As an example, the content of the [`WBatch`] in memory could be:
///
/// | Keep Alive | Frame Reliable\<Zenoh Message, Zenoh Message\> | Frame Best Effort\<Zenoh Message Fragment\> |
///
/// [`NetworkMessage`]: zenoh_protocol::network::NetworkMessage
#[derive(Clone, Debug)]
pub struct WBatch {
    // The buffer to perform the batching on
    pub buffer: BBuf,
    // The batch codec
    pub codec: Zenoh080Batch,
    // It contains 1 byte as additional header, e.g. to signal the batch is compressed
    pub config: BatchConfig,
    // Statistics related to this batch
    #[cfg(feature = "stats")]
    pub stats: WBatchStats,
    // an ephemeral batch will not be recycled in the pipeline
    // it can be used to push a stop fragment when no batch are available
    pub ephemeral: bool,
}

impl WBatch {
    pub fn new(config: BatchConfig) -> Self {
        let mut batch = Self {
            buffer: BBuf::with_capacity(config.mtu as usize),
            codec: Zenoh080Batch::new(),
            config,
            ephemeral: false,
            #[cfg(feature = "stats")]
            stats: WBatchStats::default(),
        };

        // Bring the batch in a clear state
        batch.clear();

        batch
    }

    pub fn new_ephemeral(config: BatchConfig) -> Self {
        Self {
            ephemeral: true,
            ..Self::new(config)
        }
    }

    pub fn is_ephemeral(&self) -> bool {
        self.ephemeral
    }

    /// Verify that the [`WBatch`] has no serialized bytes.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get the total number of bytes that have been serialized on the [`WBatch`].
    #[inline(always)]
    pub fn len(&self) -> BatchSize {
        let (_l, _h, p) = Self::split(self.buffer.as_slice(), &self.config);
        p.len() as BatchSize
    }

    /// Clear the [`WBatch`] memory buffer and related internal state.
    #[inline(always)]
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.codec.clear();
        #[cfg(feature = "stats")]
        {
            self.stats.clear();
        }
        Self::init(&mut self.buffer, &self.config);
    }

    /// Get a `&[u8]` to access the internal memory buffer, usually for transmitting it on the network.
    #[inline(always)]
    pub fn as_slice(&self) -> &[u8] {
        self.buffer.as_slice()
    }

    fn init(buffer: &mut BBuf, config: &BatchConfig) {
        let writer = buffer.writer();
        if config.is_streamed {
            let _ = writer.write_exact(&BatchSize::MIN.to_be_bytes());
        }
        if let Some(h) = config.header() {
            let _ = writer.write_u8(h.as_u8());
        }
    }

    // Split (length, header, payload) internal buffer slice
    #[inline(always)]
    fn split<'a>(buffer: &'a [u8], config: &BatchConfig) -> (&'a [u8], &'a [u8], &'a [u8]) {
        zsplit!(buffer, config)
    }

    // Split (length, header, payload) internal buffer slice
    #[inline(always)]
    fn split_mut<'a>(
        buffer: &'a mut [u8],
        config: &BatchConfig,
    ) -> (&'a mut [u8], &'a mut [u8], &'a mut [u8]) {
        zsplit_mut!(buffer, config)
    }

    pub fn finalize(&mut self, mut buffer: Option<&mut BBuf>) -> ZResult<Finalize> {
        #[allow(unused_mut)]
        let mut res = Finalize::Batch;

        #[cfg(feature = "transport_compression")]
        if let Some(h) = self.config.header() {
            if h.is_compression() {
                let buffer = buffer
                    .as_mut()
                    .ok_or_else(|| zerror!("Support buffer not provided"))?;
                res = self.compress(buffer)?;
            }
        }

        if self.config.is_streamed {
            let buff = match res {
                Finalize::Batch => self.buffer.as_mut_slice(),
                Finalize::Buffer => buffer
                    .as_mut()
                    .ok_or_else(|| zerror!("Support buffer not provided"))?
                    .as_mut_slice(),
            };
            let (length, header, payload) = Self::split_mut(buff, &self.config);
            let len: BatchSize = (header.len() as BatchSize) + (payload.len() as BatchSize);
            length.copy_from_slice(&len.to_le_bytes());
        }

        Ok(res)
    }

    #[cfg(feature = "transport_compression")]
    fn compress(&mut self, support: &mut BBuf) -> ZResult<Finalize> {
        // Write the initial bytes for the batch
        support.clear();
        Self::init(support, &self.config);

        // Compress the actual content
        let (_length, _header, payload) = Self::split(self.buffer.as_slice(), &self.config);
        let writer = support.writer();
        // SAFETY: assertion ensures `with_slot` precondition
        unsafe {
            writer.with_slot(writer.remaining(), |b| {
                let len = lz4_flex::block::compress_into(payload, b).unwrap_or(0);
                assert!(len <= b.len());
                len
            })
        }
        .map_err(|_| zerror!("Compression error"))?;

        // Verify whether the resulting compressed data is smaller than the initial input
        if support.len() < self.buffer.len() {
            Ok(Finalize::Buffer)
        } else {
            // Keep the original uncompressed buffer and unset the compression flag from the header
            let (_l, h, _p) = Self::split_mut(self.buffer.as_mut_slice(), &self.config);
            let h = h.first_mut().ok_or_else(|| zerror!("Empty BatchHeader"))?;
            *h &= !BatchHeader::COMPRESSION;
            Ok(Finalize::Batch)
        }
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
        let res = self.codec.write(&mut writer, x);
        #[cfg(feature = "stats")]
        {
            if res.is_ok() {
                self.stats.t_msgs += 1;
            }
        }
        res
    }
}

impl Encode<NetworkMessageRef<'_>> for &mut WBatch {
    type Output = Result<(), BatchError>;

    fn encode(self, x: NetworkMessageRef) -> Self::Output {
        let mut writer = self.buffer.writer();
        self.codec.write(&mut writer, x)
    }
}

impl Encode<(NetworkMessageRef<'_>, &FrameHeader)> for &mut WBatch {
    type Output = Result<(), BatchError>;

    fn encode(self, x: (NetworkMessageRef, &FrameHeader)) -> Self::Output {
        let mut writer = self.buffer.writer();
        let res = self.codec.write(&mut writer, x);
        #[cfg(feature = "stats")]
        {
            if res.is_ok() {
                self.stats.t_msgs += 1;
            }
        }
        res
    }
}

impl Encode<(&mut ZBufReader<'_>, &mut FragmentHeader)> for &mut WBatch {
    type Output = Result<NonZeroUsize, DidntWrite>;

    fn encode(self, x: (&mut ZBufReader<'_>, &mut FragmentHeader)) -> Self::Output {
        let mut writer = self.buffer.writer();
        let res = self.codec.write(&mut writer, x);
        #[cfg(feature = "stats")]
        {
            if res.is_ok() {
                self.stats.t_msgs += 1;
            }
        }
        res
    }
}

// Read batch
#[derive(Debug)]
pub struct RBatch {
    // The buffer to perform deserializationn from
    buffer: ZSlice,
    // The batch codec
    codec: Zenoh080Batch,
    // The batch config
    config: BatchConfig,
}

impl RBatch {
    pub fn new<T>(config: BatchConfig, buffer: T) -> Self
    where
        T: Into<ZSlice>,
    {
        Self {
            buffer: buffer.into(),
            codec: Zenoh080Batch::new(),
            config,
        }
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    #[inline(always)]
    pub const fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    // Split (length, header, payload) internal buffer slice
    #[inline(always)]
    fn split<'a>(buffer: &'a [u8], config: &BatchConfig) -> (&'a [u8], &'a [u8], &'a [u8]) {
        zsplit!(buffer, config)
    }

    pub fn initialize<C, T>(&mut self, #[allow(unused_variables)] buff: C) -> ZResult<()>
    where
        C: Fn() -> T + Copy,
        T: AsMut<[u8]> + ZSliceBuffer + 'static,
    {
        #[allow(unused_variables)]
        let (l, h, p) = Self::split(self.buffer.as_slice(), &self.config);

        #[cfg(feature = "transport_compression")]
        {
            if self.config.has_header() {
                let b = *h
                    .first()
                    .ok_or_else(|| zerror!("Batch header not present"))?;
                let header = BatchHeader::new(b);

                if header.is_compression() {
                    let zslice = self.decompress(p, buff)?;
                    self.buffer = zslice;
                    return Ok(());
                }
            }
        }

        self.buffer = self
            .buffer
            .subslice(l.len() + h.len()..self.buffer.len())
            .ok_or_else(|| zerror!("Invalid batch length"))?;

        Ok(())
    }

    #[cfg(feature = "transport_compression")]
    fn decompress<T>(&self, payload: &[u8], mut buff: impl FnMut() -> T) -> ZResult<ZSlice>
    where
        T: AsMut<[u8]> + ZSliceBuffer + 'static,
    {
        let mut into = (buff)();
        let n = lz4_flex::block::decompress_into(payload, into.as_mut())
            .map_err(|_| zerror!("Decompression error"))?;
        let zslice = ZSlice::new(Arc::new(into), 0, n)
            .map_err(|_| zerror!("Invalid decompression buffer length"))?;
        Ok(zslice)
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

impl Decode<(TransportMessage, BatchSize)> for &mut RBatch {
    type Error = DidntRead;

    fn decode(self) -> Result<(TransportMessage, BatchSize), Self::Error> {
        let len = self.buffer.len() as BatchSize;
        let mut reader = self.buffer.reader();
        let msg = self.codec.read(&mut reader)?;
        let end = self.buffer.len() as BatchSize;
        Ok((msg, len - end))
    }
}

impl<'a> Decode<FrameReader<'a, ZSlice>> for &'a mut RBatch {
    type Error = DidntRead;

    fn decode(self) -> Result<FrameReader<'a, ZSlice>, Self::Error> {
        self.codec.read(&mut self.buffer)
    }
}

#[cfg(test)]
mod tests {
    use std::vec;

    use rand::Rng;
    use zenoh_core::zcondfeat;
    use zenoh_protocol::{
        core::{CongestionControl, Priority, Reliability, WireExpr},
        network::{ext, NetworkMessage, NetworkMessageExt, Push},
        transport::{
            frame::{self, FrameHeader},
            Fragment, KeepAlive, TransportMessage,
        },
    };

    use super::*;

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
                    is_streamed: rng.gen_bool(0.5),
                    #[cfg(feature = "transport_compression")]
                    is_compression: rng.gen_bool(0.5),
                };
                let mut wbatch = WBatch::new(config);
                wbatch.encode(&msg_in).unwrap();
                println!("Encoded WBatch: {wbatch:?}");

                let mut buffer = zcondfeat!(
                    "transport_compression",
                    config.is_compression.then_some(BBuf::with_capacity(
                        lz4_flex::block::get_maximum_output_size(wbatch.as_slice().len()),
                    )),
                    None
                );

                let res = wbatch.finalize(buffer.as_mut()).unwrap();
                let bytes = match res {
                    Finalize::Batch => wbatch.as_slice(),
                    Finalize::Buffer => buffer.as_mut().unwrap().as_slice(),
                };
                println!("Finalized WBatch: {bytes:02x?}");

                let mut rbatch = RBatch::new(config, bytes.to_vec().into_boxed_slice());
                println!("Decoded RBatch: {rbatch:?}");
                rbatch
                    .initialize(|| {
                        zenoh_buffers::vec::uninit(config.mtu as usize).into_boxed_slice()
                    })
                    .unwrap();
                println!("Initialized RBatch: {rbatch:?}");
                let msg_out: TransportMessage = rbatch.decode().unwrap();
                assert_eq!(msg_in, msg_out);
            }
        }
    }

    #[test]
    fn serialization_batch() {
        let config = BatchConfig {
            mtu: BatchSize::MAX,
            is_streamed: false,
            #[cfg(feature = "transport_compression")]
            is_compression: false,
        };
        let mut batch = WBatch::new(config);

        let tmsg: TransportMessage = KeepAlive.into();
        let mut nmsg = NetworkMessage::from(Push {
            wire_expr: WireExpr::empty(),
            ext_qos: ext::QoSType::new(Priority::DEFAULT, CongestionControl::Block, false),
            ..Push::from(vec![0u8; 8])
        });

        let mut tmsgs_in = vec![];
        let mut nmsgs_in = vec![];

        // Serialize assuming there is already a frame
        batch.clear();
        assert!(batch.encode(nmsg.as_ref()).is_err());
        assert_eq!(batch.len(), 0);

        let mut frame = FrameHeader {
            reliability: Reliability::Reliable,
            sn: 0,
            ext_qos: frame::ext::QoSType::DEFAULT,
        };
        nmsg.reliability = frame.reliability;

        // Serialize with a frame
        batch.encode((nmsg.as_ref(), &frame)).unwrap();
        assert_ne!(batch.len(), 0);
        nmsgs_in.push(nmsg.clone());

        frame.reliability = Reliability::BestEffort;
        nmsg.reliability = frame.reliability;
        batch.encode((nmsg.as_ref(), &frame)).unwrap();
        assert_ne!(batch.len(), 0);
        nmsgs_in.push(nmsg.clone());

        // Transport
        batch.encode(&tmsg).unwrap();
        tmsgs_in.push(tmsg.clone());

        // Serialize assuming there is already a frame
        assert!(batch.encode(nmsg.as_ref()).is_err());
        assert_ne!(batch.len(), 0);

        // Serialize with a frame
        frame.sn = 1;
        batch.encode((nmsg.as_ref(), &frame)).unwrap();
        assert_ne!(batch.len(), 0);
        nmsgs_in.push(nmsg.clone());
    }
}
