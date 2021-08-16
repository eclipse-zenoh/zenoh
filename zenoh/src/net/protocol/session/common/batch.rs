//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
use super::core::{Priority, Reliability};
use super::io::WBuf;
use super::proto::{SessionMessage, ZenohMessage};
use super::seq_num::SeqNumGenerator;

type LengthType = u16;
const LENGTH_BYTES: [u8; 2] = [0u8, 0u8];

#[derive(Clone, Debug)]
enum CurrentFrame {
    Reliable,
    BestEffort,
    None,
}

/// Serialization Batch
///
/// A [`SerializationBatch`][SerializationBatch] is a non-expandable and contiguous region of memory
/// that is used to serialize [`SessionMessage`][SessionMessage] and [`ZenohMessage`][ZenohMessage].
///
/// [`SessionMessage`][SessionMessage] are always serialized on the batch as they are, while
/// [`ZenohMessage`][ZenohMessage] are always serializaed on the batch as part of a [`SessionMessage`]
/// [SessionMessage] Frame. Reliable and Best Effort Frames can be interleaved on the same
/// [`SerializationBatch`][SerializationBatch] as long as they fit in the remaining buffer capacity.
///
/// In the serialized form, the [`SerializationBatch`][SerializationBatch] always contains one or more
/// [`SessionMessage`][SessionMessage]. In the particular case of [`SessionMessage`][SessionMessage] Frame,
/// its payload is either (i) one or more complete [`ZenohMessage`][ZenohMessage] or (ii) a fragment of a
/// a [`ZenohMessage`][ZenohMessage].
///
/// As an example, the content of the [`SerializationBatch`][SerializationBatch] in memory could be:
///
/// | Keep Alive | Frame Reliable<Zenoh Message, Zenoh Message> | Frame Best Effort<Zenoh Message Fragment> |
///
#[derive(Clone, Debug)]
pub(crate) struct SerializationBatch {
    // The buffer to perform the batching on
    buffer: WBuf,
    // It is a streamed batch
    is_streamed: bool,
    // The current frame being serialized: BestEffort/Reliable
    current_frame: CurrentFrame,
}

impl SerializationBatch {
    /// Create a new [`SerializationBatch`][SerializationBatch] with a given size in bytes.
    ///
    /// # Arguments
    /// * `size` - The size in bytes of the contiguous memory buffer to allocate on.
    ///
    /// * `is_streamed` - The serialization batch is meant to be used for a stream-based transport
    ///                   protocol (e.g., TCP) in constrast to datagram-based transport protocol (e.g., UDP).
    ///                   In case of `is_streamed` being true, the first 2 bytes of the serialization batch
    ///                   are used to encode the total amount of serialized bytes as 16-bits little endian.
    ///                   Writing these 2 bytes allows the receiver to detect the amount of bytes it is expected
    ///                   to read when operating on non-boundary preserving transport protocols.
    ///
    /// * `sn_reliable` - The sequence number generator for the reliable channel.
    ///
    /// * `sn_best_effort` - The sequence number generator for the best effort channel.
    ///
    pub(crate) fn new(size: usize, is_streamed: bool) -> SerializationBatch {
        let mut batch = SerializationBatch {
            buffer: WBuf::new(size, true),
            is_streamed,
            current_frame: CurrentFrame::None,
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
    pub(crate) fn len(&self) -> usize {
        let len = self.buffer.len();
        if self.is_streamed() {
            len - LENGTH_BYTES.len()
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
        self.current_frame = CurrentFrame::None;
        self.buffer.clear();
        if self.is_streamed() {
            self.buffer.write_bytes(&LENGTH_BYTES);
        }
    }

    /// In case the [`SerializationBatch`][SerializationBatch] is for a stream-based protocol, use the first 2 bytes
    /// to encode the total amount of serialized bytes as 16-bits little endian.
    #[inline(always)]
    pub(crate) fn write_len(&mut self) {
        if self.is_streamed() {
            let length = self.len() as LengthType;
            let bits = self.buffer.get_first_slice_mut(..LENGTH_BYTES.len());
            bits.copy_from_slice(&length.to_le_bytes());
        }
    }

    /// Get a `&[u8]` to access the internal memory buffer, usually for transmitting it on the network.
    #[inline(always)]
    pub(crate) fn as_bytes(&self) -> &[u8] {
        self.buffer.get_first_slice(..)
    }

    /// Gets a serialized [`ZenohMessage`][ZenohMessage] and fragment it on the [`SerializationBatch`][SerializationBatch].
    ///
    /// # Arguments
    /// * `reliable` - The serialized [`ZenohMessage`][ZenohMessage] is for the reliable or the best effort channel.
    ///
    /// * `sn` - The reliable/best effort sequence number of the new [`ZenohMessage`][ZenohMessage] fragment.
    ///
    /// * `to_fragment` - The buffer containing the serialized [`ZenohMessage`][ZenohMessage] that requires fragmentation.
    ///
    /// * `to_write` - The amount of bytes that still need to be fragmented.
    ///
    pub(crate) fn serialize_zenoh_fragment(
        &mut self,
        reliability: Reliability,
        priority: Priority,
        sn_gen: &mut SeqNumGenerator,
        to_fragment: &mut WBuf,
        to_write: usize,
    ) -> usize {
        // Assume first that this is not the final fragment
        let sn = sn_gen.get();
        let mut is_final = false;
        loop {
            // Mark the buffer for the writing operation
            self.buffer.mark();
            // Write the frame header
            let fragment = Some(is_final);
            let attachment = None;
            let res =
                self.buffer
                    .write_frame_header(priority, reliability, sn, fragment, attachment);
            if res {
                // Compute the amount left
                let space_left = self.buffer.capacity() - self.buffer.len();
                // Check if it is really the final fragment
                if !is_final && (to_write <= space_left) {
                    // Revert the buffer
                    self.buffer.revert();
                    // It is really the finally fragment, reserialize the header
                    is_final = true;
                    continue;
                }
                // Write the fragment
                let written = to_write.min(space_left);
                to_fragment.copy_into_wbuf(&mut self.buffer, written);

                return written;
            } else {
                // Revert the buffer and the SN
                sn_gen.set(sn);
                self.buffer.revert();
                return 0;
            }
        }
    }

    /// Try to serialize a [`ZenohMessage`][ZenohMessage] on the [`SerializationBatch`][SerializationBatch].
    ///
    /// # Arguments
    /// * `message` - The [`ZenohMessage`][ZenohMessage] to serialize.
    ///
    pub(crate) fn serialize_zenoh_message(
        &mut self,
        message: &ZenohMessage,
        priority: Priority,
        sn_gen: &mut SeqNumGenerator,
    ) -> bool {
        // Keep track of eventual new frame and new sn
        let mut new_frame = None;

        // Eventually update the current frame and sn based on the current status
        match self.current_frame {
            CurrentFrame::Reliable => {
                if !message.is_reliable() {
                    // A new best-effort frame needs to be started
                    new_frame = Some(CurrentFrame::BestEffort);
                }
            }
            CurrentFrame::BestEffort => {
                if message.is_reliable() {
                    // A new reliable frame needs to be started
                    new_frame = Some(CurrentFrame::Reliable);
                }
            }
            CurrentFrame::None => {
                if message.is_reliable() {
                    // A new reliable frame needs to be started
                    new_frame = Some(CurrentFrame::Reliable);
                } else {
                    // A new best-effort frame needs to be started
                    new_frame = Some(CurrentFrame::BestEffort);
                }
            }
        }

        // Mark the write operation
        self.buffer.mark();

        // If a new sequence number has been provided, it means we are in the case we need
        // to start a new frame. Write a new frame header.
        let res = if let Some(frame) = new_frame {
            // Acquire the lock on the sn generator
            let is_reliable = message.is_reliable();
            // Get a new sequence number
            let sn = sn_gen.get();

            // Serialize the new frame and the zenoh message
            let reliability = if is_reliable {
                Reliability::Reliable
            } else {
                Reliability::BestEffort
            };
            let res = self
                .buffer
                .write_frame_header(priority, reliability, sn, None, None)
                && self.buffer.write_zenoh_message(message);
            if res {
                self.current_frame = frame;
            } else {
                // Restore the sequence number
                sn_gen.set(sn);
            }
            res
        } else {
            self.buffer.write_zenoh_message(message)
        };

        if !res {
            // Revert the write operation
            self.buffer.revert();
        }

        res
    }

    /// Try to serialize a [`SessionMessage`][SessionMessage] on the [`SerializationBatch`][SerializationBatch].
    ///
    /// # Arguments
    /// * `message` - The [`SessionMessage`][SessionMessage] to serialize.
    ///
    pub(crate) fn serialize_session_message(&mut self, message: &SessionMessage) -> bool {
        // Mark the write operation
        self.buffer.mark();
        let res = self.buffer.write_session_message(message);
        if res {
            // Reset the current frame value
            self.current_frame = CurrentFrame::None;
        } else {
            // Revert the write operation
            self.buffer.revert();
        }
        res
    }

    #[cfg(test)]
    pub(crate) fn get_serialized_messages(&self) -> &[u8] {
        if self.is_streamed() {
            self.buffer.get_first_slice(LENGTH_BYTES.len()..)
        } else {
            self.buffer.get_first_slice(..)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::core::{Channel, CongestionControl, Priority, Reliability, ResKey, ZInt};
    use super::super::io::{WBuf, ZBuf};
    use super::super::proto::{Frame, FramePayload, SessionBody, SessionMessage, ZenohMessage};
    use super::super::session::defaults::ZN_DEFAULT_SEQ_NUM_RESOLUTION;
    use super::*;
    use std::convert::TryFrom;

    fn serialize_no_fragmentation(batch_size: usize, payload_size: usize) {
        for is_streamed in [false, true].iter() {
            print!(
                "Streamed: {}\t\tBatch: {}\t\tPload: {}",
                is_streamed, batch_size, payload_size
            );

            // Create the serialization batch
            let priority = Priority::default();
            let mut sn_gen = SeqNumGenerator::new(0, ZN_DEFAULT_SEQ_NUM_RESOLUTION);
            let mut batch = SerializationBatch::new(batch_size, *is_streamed);

            // Serialize the messages until the batch is full
            let mut smsgs_in: Vec<SessionMessage> = Vec::new();
            let mut zmsgs_in: Vec<ZenohMessage> = Vec::new();
            let mut reliable = true;
            let mut dropping = true;
            loop {
                // Insert a session message every 3 ZenohMessage
                if zmsgs_in.len() % 3 == 0 {
                    // Create a SessionMessage
                    let pid = None;
                    let attachment = None;
                    let msg = SessionMessage::make_keep_alive(pid, attachment);

                    // Serialize the SessionMessage
                    let res = batch.serialize_session_message(&msg);
                    if !res {
                        assert!(!zmsgs_in.is_empty());
                        break;
                    }
                    smsgs_in.push(msg);
                }

                // Create a ZenohMessage
                if zmsgs_in.len() % 4 == 0 {
                    // Change reliability every four messages
                    reliable = !reliable;
                }
                if zmsgs_in.len() % 3 == 0 {
                    // Change dropping strategy every three messages
                    dropping = !dropping;
                }
                let key = ResKey::RName(format!("test{}", zmsgs_in.len()));
                let payload = ZBuf::from(vec![0u8; payload_size]);
                let channel = Channel {
                    priority,
                    reliability: if reliable {
                        Reliability::Reliable
                    } else {
                        Reliability::BestEffort
                    },
                };
                let congestion_control = CongestionControl::default();
                let data_info = None;
                let routing_context = None;
                let reply_context = None;
                let attachment = None;

                let msg = ZenohMessage::make_data(
                    key,
                    payload,
                    channel,
                    congestion_control,
                    data_info,
                    routing_context,
                    reply_context,
                    attachment,
                );
                // Serialize the ZenohMessage
                let res = batch.serialize_zenoh_message(&msg, channel.priority, &mut sn_gen);
                if !res {
                    batch.write_len();
                    assert!(!zmsgs_in.is_empty());
                    break;
                }
                zmsgs_in.push(msg);
            }

            // Verify that we deserialize the same messages we have serialized
            let mut deserialized: Vec<SessionMessage> = Vec::new();
            // Convert the buffer into an ZBuf
            let mut zbuf: ZBuf = batch.get_serialized_messages().into();
            // Deserialize the messages
            while let Some(msg) = zbuf.read_session_message() {
                deserialized.push(msg);
            }
            assert!(!deserialized.is_empty());

            let mut smsgs_out: Vec<SessionMessage> = Vec::new();
            let mut zmsgs_out: Vec<ZenohMessage> = Vec::new();
            for msg in deserialized.drain(..) {
                match msg.body {
                    SessionBody::Frame(Frame { payload, .. }) => match payload {
                        FramePayload::Messages { mut messages } => zmsgs_out.append(&mut messages),
                        _ => assert!(false),
                    },
                    _ => smsgs_out.push(msg),
                }
            }

            assert_eq!(smsgs_in, smsgs_out);
            assert_eq!(zmsgs_in, zmsgs_out);

            println!("\t\tMessages: {}", zmsgs_out.len());
        }
    }

    fn serialize_fragmentation(batch_size: usize, payload_size: usize) {
        for is_streamed in [false, true].iter() {
            // Create the sequence number generators
            let mut sn_gen = SeqNumGenerator::new(0, ZN_DEFAULT_SEQ_NUM_RESOLUTION);

            for reliability in [Reliability::BestEffort, Reliability::Reliable].iter() {
                let channel = Channel {
                    priority: Priority::default(),
                    reliability: *reliability,
                };
                let congestion_control = CongestionControl::default();
                // Create the ZenohMessage
                let key = ResKey::RName("test".to_string());
                let payload = ZBuf::from(vec![0u8; payload_size]);
                let data_info = None;
                let routing_context = None;
                let reply_context = None;
                let attachment = None;
                let msg_in = ZenohMessage::make_data(
                    key,
                    payload,
                    channel,
                    congestion_control,
                    data_info,
                    routing_context,
                    reply_context,
                    attachment,
                );

                // Serialize the message
                let mut wbuf = WBuf::new(batch_size, false);
                wbuf.write_zenoh_message(&msg_in);

                print!(
                    "Streamed: {}\t\tBatch: {}\t\tPload: {}",
                    is_streamed, batch_size, payload_size
                );

                // Store all the batches
                let mut batches: Vec<SerializationBatch> = Vec::new();
                // Fragment the message
                let mut to_write = wbuf.len();
                while to_write > 0 {
                    // Create the serialization batch
                    let mut batch = SerializationBatch::new(batch_size, *is_streamed);
                    let written = batch.serialize_zenoh_fragment(
                        msg_in.channel.reliability,
                        msg_in.channel.priority,
                        &mut sn_gen,
                        &mut wbuf,
                        to_write,
                    );
                    assert_ne!(written, 0);
                    // Keep serializing
                    to_write -= written;
                    batches.push(batch);
                }

                assert!(!batches.is_empty());

                let mut fragments = WBuf::new(0, false);
                for batch in batches.iter() {
                    // Convert the buffer into an ZBuf
                    let mut zbuf: ZBuf = batch.get_serialized_messages().into();
                    // Deserialize the messages
                    let msg = zbuf.read_session_message().unwrap();

                    match msg.body {
                        SessionBody::Frame(Frame { payload, .. }) => match payload {
                            FramePayload::Fragment { buffer, is_final } => {
                                assert!(!buffer.is_empty());
                                fragments.write_zslice(buffer.clone());
                                if is_final {
                                    break;
                                }
                            }
                            _ => assert!(false),
                        },
                        _ => assert!(false),
                    }
                }
                let mut fragments: ZBuf = fragments.into();

                assert!(!fragments.is_empty());

                // Deserialize the message
                let msg_out = fragments.read_zenoh_message(*reliability);
                assert!(msg_out.is_some());
                assert_eq!(msg_in, msg_out.unwrap());

                println!("\t\tFragments: {}", batches.len());
            }
        }
    }

    #[test]
    fn serialization_batch() {
        let batch_size: Vec<usize> = vec![128, 512, 1_024, 4_096, 8_192, 16_384, 32_768, 65_535];
        let mut payload_size: Vec<usize> = Vec::new();
        let mut size: usize = 8;
        for _ in 0..16 {
            if ZInt::try_from(size).is_err() {
                break;
            }
            payload_size.push(size);
            size *= 2;
        }

        for bs in batch_size.iter() {
            for ps in payload_size.iter() {
                if ps < bs {
                    serialize_no_fragmentation(*bs, *ps);
                } else {
                    serialize_fragmentation(*bs, *ps);
                }
            }
        }
    }
}
