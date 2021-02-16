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
use super::core::{Channel, ZInt};
use super::io::WBuf;
use super::proto::{SessionMessage, ZenohMessage};
use super::SeqNumGenerator;
use async_std::sync::{Arc, Mutex};

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
/// A [`SerializationBatch`][SerializationBatch] is a non-expandable and contigous region of memory
/// that is used to serialize [`SessionMessage`][SessionMessage] and [`ZenohMessage`][ZenohMessage].
///
/// [`SessionMessage`][SessionMessage] are always serialized on the batch as theyr are, while
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
pub(super) struct SerializationBatch {
    // The buffer to perform the batching on
    buffer: WBuf,
    // It is a streamed batch
    is_streamed: bool,
    // The link this batch is associated to
    current_frame: CurrentFrame,
    // The sn generators
    sn_reliable: Arc<Mutex<SeqNumGenerator>>,
    sn_best_effort: Arc<Mutex<SeqNumGenerator>>,
}

impl SerializationBatch {
    /// Create a new [`SerializationBatch`][SerializationBatch] with a given size in bytes.
    ///
    /// # Arguments
    /// * `size` - The size in bytes of the contigous memory buffer to allocate on.
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
    pub(super) fn new(
        size: usize,
        is_streamed: bool,
        sn_reliable: Arc<Mutex<SeqNumGenerator>>,
        sn_best_effort: Arc<Mutex<SeqNumGenerator>>,
    ) -> SerializationBatch {
        let mut batch = SerializationBatch {
            buffer: WBuf::new(size, true),
            is_streamed,
            current_frame: CurrentFrame::None,
            sn_reliable,
            sn_best_effort,
        };

        // Bring the batch in a clear state
        batch.clear();

        batch
    }

    /// Verify that the [`SerializationBatch`][SerializationBatch] has no serialized bytes.
    #[inline]
    pub(super) fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get the total number of bytes that have been serialized on the [`SerializationBatch`][SerializationBatch].
    #[inline]
    pub(super) fn len(&self) -> usize {
        let len = self.buffer.len();
        if self.is_streamed() {
            len - LENGTH_BYTES.len()
        } else {
            len
        }
    }

    /// Verify that the [`SerializationBatch`][SerializationBatch] is for a stream-based protocol, i.e., the first
    /// 2 bytes are reserved to encode the total amount of serialized bytes as 16-bits little endian.
    #[inline]
    pub(super) fn is_streamed(&self) -> bool {
        self.is_streamed
    }

    /// Clear the [`SerializationBatch`][SerializationBatch] memory buffer and related internal state.
    #[inline]
    pub(super) fn clear(&mut self) {
        self.current_frame = CurrentFrame::None;
        self.buffer.clear();
        if self.is_streamed() {
            self.buffer.write_bytes(&LENGTH_BYTES);
        }
    }

    /// In case the [`SerializationBatch`][SerializationBatch] is for a stream-based protocol, use the first 2 bytes
    /// to encode the total amount of serialized bytes as 16-bits little endian.
    #[inline]
    pub(super) fn write_len(&mut self) {
        if self.is_streamed() {
            let length = self.len() as LengthType;
            let bits = self.buffer.get_first_slice_mut(..LENGTH_BYTES.len());
            bits.copy_from_slice(&length.to_le_bytes());
        }
    }

    /// Get a `&[u8]` to access the internal memory buffer, usually for transmitting it on the network.
    #[inline]
    pub(super) fn get_buffer(&self) -> &[u8] {
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
    pub(super) async fn serialize_zenoh_fragment(
        &mut self,
        ch: Channel,
        sn: ZInt,
        to_fragment: &mut WBuf,
        to_write: usize,
    ) -> usize {
        // Assume first that this is not the final fragment
        let mut is_final = false;
        loop {
            // Mark the buffer for the writing operation
            self.buffer.mark();
            // Write the frame header
            let fragment = Some(is_final);
            let attachment = None;
            let res = self.buffer.write_frame_header(ch, sn, fragment, attachment);
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
                // Revert the buffer
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
    pub(super) async fn serialize_zenoh_message(&mut self, message: &ZenohMessage) -> bool {
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
            let mut guard = if is_reliable {
                zasynclock!(self.sn_reliable)
            } else {
                zasynclock!(self.sn_best_effort)
            };
            // Get a new sequence number
            let sn = guard.get();

            // Serialize the new frame and the zenoh message
            let ch = if is_reliable {
                Channel::Reliable
            } else {
                Channel::BestEffort
            };
            let res = self.buffer.write_frame_header(ch, sn, None, None)
                && self.buffer.write_zenoh_message(&message);
            if res {
                self.current_frame = frame;
            } else {
                // Restore the sequence number
                guard.set(sn);
            }
            // Drop the guard
            drop(guard);
            // Return
            res
        } else {
            self.buffer.write_zenoh_message(&message)
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
    pub(super) async fn serialize_session_message(&mut self, message: &SessionMessage) -> bool {
        // Mark the write operation
        self.buffer.mark();
        let res = self.buffer.write_session_message(&message);
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
    pub(super) fn get_serialized_messages(&self) -> &[u8] {
        if self.is_streamed() {
            self.buffer.get_first_slice(LENGTH_BYTES.len()..)
        } else {
            self.buffer.get_first_slice(..)
        }
    }
}

#[cfg(test)]
mod tests {
    use async_std::task;
    use std::convert::TryFrom;

    use super::*;

    use super::super::core::{CongestionControl, Reliability, ResKey};
    use super::super::io::{RBuf, WBuf};
    use super::super::proto::{Frame, FramePayload, SessionBody, SessionMessage, ZenohMessage};
    use super::super::session::defaults::SESSION_SEQ_NUM_RESOLUTION;

    use zenoh_util::zasynclock;

    async fn serialize_no_fragmentation(batch_size: usize, payload_size: usize) {
        for is_streamed in [false, true].iter() {
            print!(
                "Streamed: {}\t\tBatch: {}\t\tPload: {}",
                is_streamed, batch_size, payload_size
            );

            // Create the serialization batch
            let sn_reliable = Arc::new(Mutex::new(SeqNumGenerator::new(
                0,
                *SESSION_SEQ_NUM_RESOLUTION,
            )));
            let sn_best_effort = Arc::new(Mutex::new(SeqNumGenerator::new(
                0,
                *SESSION_SEQ_NUM_RESOLUTION,
            )));
            let mut batch = SerializationBatch::new(
                batch_size,
                *is_streamed,
                sn_reliable.clone(),
                sn_best_effort.clone(),
            );

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
                    let res = batch.serialize_session_message(&msg).await;
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
                let payload = RBuf::from(vec![0u8; payload_size]);
                let reliability = if reliable {
                    Reliability::Reliable
                } else {
                    Reliability::BestEffort
                };
                let congestion_control = if dropping {
                    CongestionControl::Drop
                } else {
                    CongestionControl::Block
                };
                let data_info = None;
                let routing_context = None;
                let reply_context = None;
                let attachment = None;
                let msg = ZenohMessage::make_data(
                    key,
                    payload,
                    reliability,
                    congestion_control,
                    data_info,
                    routing_context,
                    reply_context,
                    attachment,
                );
                // Serialize the ZenohMessage
                let res = batch.serialize_zenoh_message(&msg).await;
                if !res {
                    batch.write_len();
                    assert!(!zmsgs_in.is_empty());
                    break;
                }
                zmsgs_in.push(msg);
            }

            // Verify that we deserialize the same messages we have serialized
            let mut deserialized: Vec<SessionMessage> = Vec::new();
            // Convert the buffer into an RBuf
            let mut rbuf: RBuf = batch.get_serialized_messages().into();
            // Deserialize the messages
            while let Some(msg) = rbuf.read_session_message() {
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

    async fn serialize_fragmentation(batch_size: usize, payload_size: usize) {
        for is_streamed in [false, true].iter() {
            // Create the sequence number generators
            let sn_reliable = Arc::new(Mutex::new(SeqNumGenerator::new(
                0,
                *SESSION_SEQ_NUM_RESOLUTION,
            )));
            let sn_best_effort = Arc::new(Mutex::new(SeqNumGenerator::new(
                0,
                *SESSION_SEQ_NUM_RESOLUTION,
            )));

            for reliability in [Reliability::BestEffort, Reliability::Reliable].iter() {
                for congestion_control in [CongestionControl::Drop, CongestionControl::Block].iter()
                {
                    // Create the ZenohMessage
                    let key = ResKey::RName("test".to_string());
                    let payload = RBuf::from(vec![0u8; payload_size]);
                    let data_info = None;
                    let routing_context = None;
                    let reply_context = None;
                    let attachment = None;
                    let msg_in = ZenohMessage::make_data(
                        key,
                        payload,
                        *reliability,
                        *congestion_control,
                        data_info,
                        routing_context,
                        reply_context,
                        attachment,
                    );

                    // Acquire the lock on the sn generators to ensure that we have
                    // sequential sequence numbers for all the fragments
                    let mut guard = if msg_in.is_reliable() {
                        zasynclock!(sn_reliable)
                    } else {
                        zasynclock!(sn_best_effort)
                    };

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
                        let mut batch = SerializationBatch::new(
                            batch_size,
                            *is_streamed,
                            sn_reliable.clone(),
                            sn_best_effort.clone(),
                        );
                        let ch = if msg_in.is_reliable() {
                            Channel::Reliable
                        } else {
                            Channel::BestEffort
                        };
                        let written = batch
                            .serialize_zenoh_fragment(ch, guard.get(), &mut wbuf, to_write)
                            .await;
                        assert_ne!(written, 0);
                        // Keep serializing
                        to_write -= written;
                        batches.push(batch);
                    }

                    assert!(!batches.is_empty());

                    let mut fragments = RBuf::new();
                    for batch in batches.iter() {
                        // Convert the buffer into an RBuf
                        let mut rbuf: RBuf = batch.get_serialized_messages().into();
                        // Deserialize the messages
                        let msg = rbuf.read_session_message().unwrap();

                        match msg.body {
                            SessionBody::Frame(Frame { payload, .. }) => match payload {
                                FramePayload::Fragment { buffer, is_final } => {
                                    assert!(!buffer.is_empty());
                                    for s in buffer.drain_slices().drain(..) {
                                        fragments.add_slice(s)
                                    }
                                    if is_final {
                                        break;
                                    }
                                }
                                _ => assert!(false),
                            },
                            _ => assert!(false),
                        }
                    }

                    assert!(!fragments.is_empty());

                    // Deserialize the message
                    let msg_out = fragments.read_zenoh_message(*reliability);
                    assert!(msg_out.is_some());
                    assert_eq!(msg_in, msg_out.unwrap());

                    println!("\t\tFragments: {}", batches.len());
                }
            }
        }
    }

    #[test]
    fn serialization_batch() {
        task::block_on(async {
            let batch_size: Vec<usize> =
                vec![128, 512, 1_024, 4_096, 8_192, 16_384, 32_768, 65_535];
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
                        serialize_no_fragmentation(*bs, *ps).await;
                    } else {
                        serialize_fragmentation(*bs, *ps).await;
                    }
                }
            }
        });
    }
}
