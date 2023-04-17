use crate::common::batch::WError;

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
// use super::batch::SerializationBatch;
use super::batch::{Encode, WBatch};
use super::conduit::{TransportChannelTx, TransportConduitTx};
use async_std::prelude::FutureExt;
use flume::{bounded, Receiver, Sender};
use ringbuffer_spsc::{RingBuffer, RingBufferReader, RingBufferWriter};
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::Duration;
use zenoh_buffers::{
    reader::{HasReader, Reader},
    writer::HasWriter,
    ZBuf,
};
use zenoh_codec::{WCodec, Zenoh060};
use zenoh_config::QueueSizeConf;
use zenoh_core::zlock;
use zenoh_protocol::{
    core::{Channel, Priority},
    transport::TransportMessage,
    zenoh::ZenohMessage,
};

// It's faster to work directly with nanoseconds.
// Backoff will never last more the u32::MAX nanoseconds.
type NanoSeconds = u32;

const RBLEN: usize = QueueSizeConf::MAX;
const TSLOT: NanoSeconds = 100;

// Inner structure to reuse serialization batches
struct StageInRefill {
    n_ref_r: Receiver<()>,
    s_ref_r: RingBufferReader<WBatch, RBLEN>,
}

impl StageInRefill {
    fn pull(&mut self) -> Option<WBatch> {
        self.s_ref_r.pull()
    }

    fn wait(&self) -> bool {
        self.n_ref_r.recv().is_ok()
    }
}

// Inner structure to link the initial stage with the final stage of the pipeline
struct StageInOut {
    n_out_w: Sender<()>,
    s_out_w: RingBufferWriter<WBatch, RBLEN>,
    bytes: Arc<AtomicU16>,
    backoff: Arc<AtomicBool>,
}

impl StageInOut {
    #[inline]
    fn notify(&self, bytes: u16) {
        self.bytes.store(bytes, Ordering::Relaxed);
        if !self.backoff.load(Ordering::Relaxed) {
            let _ = self.n_out_w.try_send(());
        }
    }

    #[inline]
    fn move_batch(&mut self, batch: WBatch) {
        let _ = self.s_out_w.push(batch);
        self.bytes.store(0, Ordering::Relaxed);
        let _ = self.n_out_w.try_send(());
    }
}

// Inner structure containing mutexes for current serialization batch and SNs
struct StageInMutex {
    current: Arc<Mutex<Option<WBatch>>>,
    conduit: TransportConduitTx,
}

impl StageInMutex {
    #[inline]
    fn current(&self) -> MutexGuard<'_, Option<WBatch>> {
        zlock!(self.current)
    }

    #[inline]
    fn channel(&self, is_reliable: bool) -> MutexGuard<'_, TransportChannelTx> {
        if is_reliable {
            zlock!(self.conduit.reliable)
        } else {
            zlock!(self.conduit.best_effort)
        }
    }
}

// This is the initial stage of the pipeline where messages are serliazed on
struct StageIn {
    s_ref: StageInRefill,
    s_out: StageInOut,
    mutex: StageInMutex,
    fragbuf: ZBuf,
}

impl StageIn {
    fn push_zenoh_message(&mut self, msg: &mut ZenohMessage, priority: Priority) -> bool {
        // Lock the current serialization batch.
        let mut c_guard = self.mutex.current();

        // Check congestion control
        let is_droppable = msg.is_droppable();

        macro_rules! zgetbatch_rets {
            ($fragment:expr) => {
                loop {
                    match c_guard.take() {
                        Some(batch) => break batch,
                        None => match self.s_ref.pull() {
                            Some(mut batch) => {
                                batch.clear();
                                break batch;
                            }
                            None => {
                                drop(c_guard);
                                if !$fragment && is_droppable {
                                    // We are in the congestion scenario
                                    // The yield is to avoid the writing task to spin
                                    // indefinitely and monopolize the CPU usage.
                                    thread::yield_now();
                                    return false;
                                } else {
                                    if !self.s_ref.wait() {
                                        return false;
                                    }
                                }
                                c_guard = self.mutex.current();
                            }
                        },
                    }
                }
            };
        }

        macro_rules! zretok {
            ($batch:expr) => {{
                let bytes = $batch.len();
                *c_guard = Some($batch);
                drop(c_guard);
                self.s_out.notify(bytes);
                return true;
            }};
        }

        // Get the current serialization batch.
        let mut batch = zgetbatch_rets!(false);
        // Attempt the serialization on the current batch
        let e = match batch.encode(&*msg) {
            Ok(_) => zretok!(batch),
            Err(e) => e,
        };

        // Lock the channel. We are the only one that will be writing on it.
        let mut tch = self.mutex.channel(msg.is_reliable());

        // Create the channel
        let channel = Channel {
            reliability: msg.channel.reliability,
            priority,
        };

        // Retrieve the next SN
        let mut sn = tch.sn.get();

        if let WError::NewFrame = e {
            // Attempt a serialization with a new frame
            if batch.encode((&*msg, channel, sn)).is_ok() {
                zretok!(batch);
            };
        }

        if !batch.is_empty() {
            // Move out existing batch
            self.s_out.move_batch(batch);
            batch = zgetbatch_rets!(false);
        }

        // Attempt a second serialization on fully empty batch
        if batch.encode((&*msg, channel, sn)).is_ok() {
            zretok!(batch);
        };

        // The second serialization attempt has failed. This means that the message is
        // too large for the current batch size: we need to fragment.
        // Reinsert the current batch for fragmentation.
        *c_guard = Some(batch);

        // Take the expandable buffer and serialize the totality of the message
        self.fragbuf.clear();

        let mut writer = self.fragbuf.writer();
        let codec = Zenoh060::default();
        codec.write(&mut writer, &*msg).unwrap();

        // Fragment the whole message
        let mut reader = self.fragbuf.reader();
        while reader.can_read() {
            // Get the current serialization batch
            // Treat all messages as non-droppable once we start fragmenting
            batch = zgetbatch_rets!(true);

            // Serialize the message fragmnet
            match batch.encode((&mut reader, channel, sn)) {
                Ok(_) => {
                    // Update the SN
                    sn = tch.sn.get();
                    // Move the serialization batch into the OUT pipeline
                    self.s_out.move_batch(batch);
                }
                Err(_) => {
                    // Restore the sequence number
                    tch.sn.set(sn).unwrap();
                    // Reinsert the batch
                    *c_guard = Some(batch);
                    log::warn!(
                        "Zenoh message dropped because it can not be fragmented: {:?}",
                        msg
                    );
                    break;
                }
            }
        }

        // Clean the fragbuf
        self.fragbuf.clear();

        true
    }

    #[inline]
    fn push_transport_message(&mut self, msg: TransportMessage) -> bool {
        // Lock the current serialization batch.
        let mut c_guard = self.mutex.current();

        macro_rules! zgetbatch_rets {
            () => {
                loop {
                    match c_guard.take() {
                        Some(batch) => break batch,
                        None => match self.s_ref.pull() {
                            Some(mut batch) => {
                                batch.clear();
                                break batch;
                            }
                            None => {
                                drop(c_guard);
                                if !self.s_ref.wait() {
                                    return false;
                                }
                                c_guard = self.mutex.current();
                            }
                        },
                    }
                }
            };
        }

        macro_rules! zretok {
            ($batch:expr) => {{
                let bytes = $batch.len();
                *c_guard = Some($batch);
                drop(c_guard);
                self.s_out.notify(bytes);
                return true;
            }};
        }

        // Get the current serialization batch.
        let mut batch = zgetbatch_rets!();
        // Attempt the serialization on the current batch
        // Attempt the serialization on the current batch
        match batch.encode(&msg) {
            Ok(_) => zretok!(batch),
            Err(_) => {
                if !batch.is_empty() {
                    self.s_out.move_batch(batch);
                    batch = zgetbatch_rets!();
                }
            }
        };

        // The first serialization attempt has failed. This means that the current
        // batch is full. Therefore, we move the current batch to stage out.
        batch.encode(&msg).is_ok()
    }
}

// The result of the pull operation
enum Pull {
    Some(WBatch),
    None,
    Backoff(NanoSeconds),
}

// Inner structure to keep track and signal backoff operations
#[derive(Clone)]
struct Backoff {
    retry_time: NanoSeconds,
    last_bytes: u16,
    bytes: Arc<AtomicU16>,
    backoff: Arc<AtomicBool>,
}

impl Backoff {
    fn new(bytes: Arc<AtomicU16>, backoff: Arc<AtomicBool>) -> Self {
        Self {
            retry_time: 0,
            last_bytes: 0,
            bytes,
            backoff,
        }
    }

    fn next(&mut self) {
        if self.retry_time == 0 {
            self.retry_time = TSLOT;
            self.backoff.store(true, Ordering::Relaxed);
        } else {
            self.retry_time *= 2;
        }
    }

    fn stop(&mut self) {
        self.retry_time = 0;
        self.backoff.store(false, Ordering::Relaxed);
    }
}

// Inner structure to link the final stage with the initial stage of the pipeline
struct StageOutIn {
    s_out_r: RingBufferReader<WBatch, RBLEN>,
    current: Arc<Mutex<Option<WBatch>>>,
    backoff: Backoff,
}

impl StageOutIn {
    #[inline]
    fn try_pull(&mut self) -> Pull {
        if let Some(mut batch) = self.s_out_r.pull() {
            batch.write_len();
            self.backoff.stop();
            return Pull::Some(batch);
        }

        self.try_pull_deep()
    }

    fn try_pull_deep(&mut self) -> Pull {
        let new_bytes = self.backoff.bytes.load(Ordering::Relaxed);
        let old_bytes = self.backoff.last_bytes;
        self.backoff.last_bytes = new_bytes;

        match new_bytes.cmp(&old_bytes) {
            std::cmp::Ordering::Equal => {
                // No new bytes have been written on the batch, try to pull
                if let Ok(mut g) = self.current.try_lock() {
                    // First try to pull from stage OUT
                    if let Some(mut batch) = self.s_out_r.pull() {
                        batch.write_len();
                        self.backoff.stop();
                        return Pull::Some(batch);
                    }

                    // An incomplete (non-empty) batch is available in the state IN pipeline.
                    match g.take() {
                        Some(mut batch) => {
                            batch.write_len();
                            self.backoff.stop();
                            return Pull::Some(batch);
                        }
                        None => {
                            self.backoff.stop();
                            return Pull::None;
                        }
                    }
                }
                // Go to backoff
            }
            std::cmp::Ordering::Less => {
                // There should be a new batch in Stage OUT
                if let Some(mut batch) = self.s_out_r.pull() {
                    batch.write_len();
                    self.backoff.stop();
                    return Pull::Some(batch);
                }
                // Go to backoff
            }
            std::cmp::Ordering::Greater => {
                // Go to backoff
            }
        }

        // Do backoff
        self.backoff.next();
        Pull::Backoff(self.backoff.retry_time)
    }
}

struct StageOutRefill {
    n_ref_w: Sender<()>,
    s_ref_w: RingBufferWriter<WBatch, RBLEN>,
}

impl StageOutRefill {
    fn refill(&mut self, batch: WBatch) {
        assert!(self.s_ref_w.push(batch).is_none());
        let _ = self.n_ref_w.try_send(());
    }
}

struct StageOut {
    s_in: StageOutIn,
    s_ref: StageOutRefill,
}

impl StageOut {
    #[inline]
    fn try_pull(&mut self) -> Pull {
        self.s_in.try_pull()
    }

    #[inline]
    fn refill(&mut self, batch: WBatch) {
        self.s_ref.refill(batch);
    }

    fn drain(&mut self, guard: &mut MutexGuard<'_, Option<WBatch>>) -> Vec<WBatch> {
        let mut batches = vec![];
        // Empty the ring buffer
        while let Some(mut batch) = self.s_in.s_out_r.pull() {
            batch.write_len();
            batches.push(batch);
        }
        // Take the current batch
        if let Some(batch) = guard.take() {
            batches.push(batch);
        }
        batches
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TransmissionPipelineConf {
    pub(crate) is_streamed: bool,
    pub(crate) batch_size: u16,
    pub(crate) queue_size: [usize; Priority::NUM],
    pub(crate) backoff: Duration,
}

impl Default for TransmissionPipelineConf {
    fn default() -> Self {
        Self {
            is_streamed: false,
            batch_size: u16::MAX,
            queue_size: [1; Priority::NUM],
            backoff: Duration::from_micros(1),
        }
    }
}

// A 2-stage transmission pipeline
pub(crate) struct TransmissionPipeline;
impl TransmissionPipeline {
    // A MPSC pipeline
    pub(crate) fn make(
        config: TransmissionPipelineConf,
        conduit: &[TransportConduitTx],
    ) -> (TransmissionPipelineProducer, TransmissionPipelineConsumer) {
        let mut stage_in = vec![];
        let mut stage_out = vec![];

        let default_queue_size = [config.queue_size[Priority::default() as usize]];
        let size_iter = if conduit.len() == 1 {
            default_queue_size.iter()
        } else {
            config.queue_size.iter()
        };

        // Create the channel for notifying that new batches are in the out ring buffer
        // This is a MPSC channel
        let (n_out_w, n_out_r) = bounded(1);

        for (prio, num) in size_iter.enumerate() {
            assert!(*num != 0 && *num <= RBLEN);

            // Create the refill ring buffer
            // This is a SPSC ring buffer
            let (mut s_ref_w, s_ref_r) = RingBuffer::<WBatch, RBLEN>::init();
            // Fill the refill ring buffer with batches
            for _ in 0..*num {
                assert!(s_ref_w
                    .push(WBatch::new(config.batch_size, config.is_streamed))
                    .is_none());
            }
            // Create the channel for notifying that new batches are in the refill ring buffer
            // This is a SPSC channel
            let (n_ref_w, n_ref_r) = bounded(1);

            // Create the refill ring buffer
            // This is a SPSC ring buffer
            let (s_out_w, s_out_r) = RingBuffer::<WBatch, RBLEN>::init();
            let current = Arc::new(Mutex::new(None));
            let bytes = Arc::new(AtomicU16::new(0));
            let backoff = Arc::new(AtomicBool::new(false));

            stage_in.push(Mutex::new(StageIn {
                s_ref: StageInRefill { n_ref_r, s_ref_r },
                s_out: StageInOut {
                    n_out_w: n_out_w.clone(),
                    s_out_w,
                    bytes: bytes.clone(),
                    backoff: backoff.clone(),
                },
                mutex: StageInMutex {
                    current: current.clone(),
                    conduit: conduit[prio].clone(),
                },
                fragbuf: ZBuf::default(),
            }));

            // The stage out for this priority
            stage_out.push(StageOut {
                s_in: StageOutIn {
                    s_out_r,
                    current,
                    backoff: Backoff::new(bytes, backoff),
                },
                s_ref: StageOutRefill { n_ref_w, s_ref_w },
            });
        }

        let active = Arc::new(AtomicBool::new(true));
        let producer = TransmissionPipelineProducer {
            stage_in: stage_in.into_boxed_slice().into(),
            active: active.clone(),
        };
        let consumer = TransmissionPipelineConsumer {
            stage_out: stage_out.into_boxed_slice(),
            n_out_r,
            active,
        };

        (producer, consumer)
    }
}

#[derive(Clone)]
pub(crate) struct TransmissionPipelineProducer {
    // Each priority queue has its own Mutex
    stage_in: Arc<[Mutex<StageIn>]>,
    active: Arc<AtomicBool>,
}

impl TransmissionPipelineProducer {
    #[inline]
    pub(crate) fn push_zenoh_message(&self, mut msg: ZenohMessage) -> bool {
        // If the queue is not QoS, it means that we only have one priority with index 0.
        let (idx, priority) = if self.stage_in.len() > 1 {
            (msg.channel.priority as usize, msg.channel.priority)
        } else {
            (0, Priority::default())
        };
        // Lock the channel. We are the only one that will be writing on it.
        let mut queue = zlock!(self.stage_in[idx]);
        queue.push_zenoh_message(&mut msg, priority)
    }

    #[inline]
    pub(crate) fn push_transport_message(&self, msg: TransportMessage, priority: Priority) -> bool {
        // If the queue is not QoS, it means that we only have one priority with index 0.
        let priority = if self.stage_in.len() > 1 {
            priority as usize
        } else {
            0
        };
        // Lock the channel. We are the only one that will be writing on it.
        let mut queue = zlock!(self.stage_in[priority]);
        queue.push_transport_message(msg)
    }

    pub(crate) fn disable(&self) {
        self.active.store(false, Ordering::Relaxed);

        // Acquire all the locks, in_guard first, out_guard later
        // Use the same locking order as in drain to avoid deadlocks
        let mut in_guards: Vec<MutexGuard<'_, StageIn>> =
            self.stage_in.iter().map(|x| zlock!(x)).collect();

        // Unblock waiting pullers
        for ig in in_guards.iter_mut() {
            ig.s_out.notify(u16::MAX);
        }
    }
}

pub(crate) struct TransmissionPipelineConsumer {
    // A single Mutex for all the priority queues
    stage_out: Box<[StageOut]>,
    n_out_r: Receiver<()>,
    active: Arc<AtomicBool>,
}

impl TransmissionPipelineConsumer {
    pub(crate) async fn pull(&mut self) -> Option<(WBatch, usize)> {
        while self.active.load(Ordering::Relaxed) {
            // Calculate the backoff maximum
            let mut bo = NanoSeconds::MAX;
            for (prio, queue) in self.stage_out.iter_mut().enumerate() {
                match queue.try_pull() {
                    Pull::Some(batch) => {
                        return Some((batch, prio));
                    }
                    Pull::Backoff(b) => {
                        if b < bo {
                            bo = b;
                        }
                    }
                    Pull::None => {}
                }
            }

            // Wait for the backoff to expire or for a new message
            let _ = self
                .n_out_r
                .recv_async()
                .timeout(Duration::from_nanos(bo as u64))
                .await;
        }
        None
    }

    pub(crate) fn refill(&mut self, batch: WBatch, priority: usize) {
        self.stage_out[priority].refill(batch);
    }

    pub(crate) fn drain(&mut self) -> Vec<(WBatch, usize)> {
        // Drain the remaining batches
        let mut batches = vec![];

        // Acquire all the locks, in_guard first, out_guard later
        // Use the same locking order as in disable to avoid deadlocks
        let locks = self
            .stage_out
            .iter()
            .map(|x| x.s_in.current.clone())
            .collect::<Vec<_>>();
        let mut currents: Vec<MutexGuard<'_, Option<WBatch>>> =
            locks.iter().map(|x| zlock!(x)).collect::<Vec<_>>();

        for (prio, s_out) in self.stage_out.iter_mut().enumerate() {
            let mut bs = s_out.drain(&mut currents[prio]);
            for b in bs.drain(..) {
                batches.push((b, prio));
            }
        }

        batches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_std::{prelude::FutureExt, task};
    use std::{
        convert::TryFrom,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
        time::{Duration, Instant},
    };
    use zenoh_buffers::{
        reader::{DidntRead, HasReader},
        ZBuf,
    };
    use zenoh_codec::{RCodec, Zenoh060};
    use zenoh_protocol::{
        core::{Channel, CongestionControl, Priority, Reliability, ZInt},
        defaults::{BATCH_SIZE, SEQ_NUM_RES},
        transport::{Frame, FramePayload, TransportBody},
        zenoh::ZenohMessage,
    };

    const SLEEP: Duration = Duration::from_millis(100);
    const TIMEOUT: Duration = Duration::from_secs(60);

    const CONFIG: TransmissionPipelineConf = TransmissionPipelineConf {
        is_streamed: true,
        batch_size: BATCH_SIZE,
        queue_size: [1; Priority::NUM],
        backoff: Duration::from_micros(1),
    };

    #[test]
    fn tx_pipeline_flow() {
        fn schedule(queue: TransmissionPipelineProducer, num_msg: usize, payload_size: usize) {
            // Send reliable messages
            let key = "test".into();
            let payload = ZBuf::from(vec![0_u8; payload_size]);
            let data_info = None;
            let routing_context = None;
            let reply_context = None;
            let attachment = None;
            let channel = Channel {
                priority: Priority::Control,
                reliability: Reliability::Reliable,
            };
            let congestion_control = CongestionControl::Block;

            let message = ZenohMessage::make_data(
                key,
                payload,
                channel,
                congestion_control,
                data_info,
                routing_context,
                reply_context,
                attachment,
            );

            println!(
                "Pipeline Flow [>>>]: Sending {num_msg} messages with payload size of {payload_size} bytes"
            );
            for i in 0..num_msg {
                println!("Pipeline Flow [>>>]: Pushed {} msgs", i + 1);
                queue.push_zenoh_message(message.clone());
            }
        }

        async fn consume(mut queue: TransmissionPipelineConsumer, num_msg: usize) {
            let mut batches: usize = 0;
            let mut bytes: usize = 0;
            let mut msgs: usize = 0;
            let mut fragments: usize = 0;

            while msgs != num_msg {
                let (batch, priority) = queue.pull().await.unwrap();
                batches += 1;
                bytes += batch.len() as usize;
                // Create a ZBuf for deserialization starting from the batch
                let bytes = batch.as_bytes();
                // Deserialize the messages
                let mut reader = bytes.reader();
                let codec = Zenoh060::default();

                loop {
                    let res: Result<TransportMessage, DidntRead> = codec.read(&mut reader);
                    match res {
                        Ok(msg) => {
                            match msg.body {
                                TransportBody::Frame(Frame { payload, .. }) => match payload {
                                    FramePayload::Messages { messages } => {
                                        msgs += messages.len();
                                    }
                                    FramePayload::Fragment { is_final, .. } => {
                                        fragments += 1;
                                        if is_final {
                                            msgs += 1;
                                        }
                                    }
                                },
                                _ => {
                                    msgs += 1;
                                }
                            }
                            println!("Pipeline Flow [<<<]: Pulled {} msgs", msgs + 1);
                        }
                        Err(_) => break,
                    }
                }
                println!("Pipeline Flow [+++]: Refill {} msgs", msgs + 1);
                // Reinsert the batch
                queue.refill(batch, priority);
            }

            println!(
                "Pipeline Flow [<<<]: Received {msgs} messages, {bytes} bytes, {batches} batches, {fragments} fragments"
            );
        }

        // Pipeline conduits
        let tct = TransportConduitTx::make(SEQ_NUM_RES).unwrap();
        let conduits = vec![tct];

        // Total amount of bytes to send in each test
        let bytes: usize = 100_000_000;
        let max_msgs: usize = 1_000;
        // Payload size of the messages
        let payload_sizes = [8, 64, 512, 4_096, 8_192, 32_768, 262_144, 2_097_152];

        task::block_on(async {
            for ps in payload_sizes.iter() {
                if ZInt::try_from(*ps).is_err() {
                    break;
                }

                // Compute the number of messages to send
                let num_msg = max_msgs.min(bytes / ps);

                let (producer, consumer) = TransmissionPipeline::make(
                    TransmissionPipelineConf::default(),
                    conduits.as_slice(),
                );

                let t_c = task::spawn(async move {
                    consume(consumer, num_msg).await;
                });

                let c_ps = *ps;
                let t_s = task::spawn(async move {
                    schedule(producer, num_msg, c_ps);
                });

                let res = t_c.join(t_s).timeout(TIMEOUT).await;
                assert!(res.is_ok());
            }
        });
    }

    #[test]
    fn tx_pipeline_blocking() {
        fn schedule(queue: TransmissionPipelineProducer, counter: Arc<AtomicUsize>, id: usize) {
            // Make sure to put only one message per batch: set the payload size
            // to half of the batch in such a way the serialized zenoh message
            // will be larger then half of the batch size (header + payload).
            let payload_size = (CONFIG.batch_size / 2) as usize;

            // Send reliable messages
            let key = "test".into();
            let payload = ZBuf::from(vec![0_u8; payload_size]);
            let channel = Channel {
                priority: Priority::Control,
                reliability: Reliability::Reliable,
            };
            let congestion_control = CongestionControl::Block;
            let data_info = None;
            let routing_context = None;
            let reply_context = None;
            let attachment = None;

            let message = ZenohMessage::make_data(
                key,
                payload,
                channel,
                congestion_control,
                data_info,
                routing_context,
                reply_context,
                attachment,
            );

            // The last push should block since there shouldn't any more batches
            // available for serialization.
            let num_msg = 1 + CONFIG.queue_size[0];
            for i in 0..num_msg {
                println!(
                    "Pipeline Blocking [>>>]: ({id}) Scheduling message #{i} with payload size of {payload_size} bytes"
                );
                queue.push_zenoh_message(message.clone());
                let c = counter.fetch_add(1, Ordering::AcqRel);
                println!(
                    "Pipeline Blocking [>>>]: ({}) Scheduled message #{} (tot {}) with payload size of {} bytes",
                    id, i, c + 1,
                    payload_size
                );
            }
        }

        // Pipeline
        let tct = TransportConduitTx::make(SEQ_NUM_RES).unwrap();
        let conduits = vec![tct];
        let (producer, mut consumer) =
            TransmissionPipeline::make(TransmissionPipelineConf::default(), conduits.as_slice());

        let counter = Arc::new(AtomicUsize::new(0));

        let c_producer = producer.clone();
        let c_counter = counter.clone();
        let h1 = task::spawn_blocking(move || {
            schedule(c_producer, c_counter, 1);
        });

        let c_counter = counter.clone();
        let h2 = task::spawn_blocking(move || {
            schedule(producer, c_counter, 2);
        });

        task::block_on(async {
            // Wait to have sent enough messages and to have blocked
            println!(
                "Pipeline Blocking [---]: waiting to have {} messages being scheduled",
                CONFIG.queue_size[Priority::MAX as usize]
            );
            let check = async {
                while counter.load(Ordering::Acquire) < CONFIG.queue_size[Priority::MAX as usize] {
                    task::sleep(SLEEP).await;
                }
            };
            check.timeout(TIMEOUT).await.unwrap();

            // Disable and drain the queue
            task::spawn_blocking(move || {
                println!("Pipeline Blocking [---]: draining the queue");
                let _ = consumer.drain();
            })
            .timeout(TIMEOUT)
            .await
            .unwrap();

            // Make sure that the tasks scheduling have been unblocked
            println!("Pipeline Blocking [---]: waiting for schedule (1) to be unblocked");
            h1.timeout(TIMEOUT).await.unwrap();
            println!("Pipeline Blocking [---]: waiting for schedule (2) to be unblocked");
            h2.timeout(TIMEOUT).await.unwrap();
        });
    }

    #[test]
    #[ignore]
    fn tx_pipeline_thr() {
        // Queue
        let tct = TransportConduitTx::make(SEQ_NUM_RES).unwrap();
        let conduits = vec![tct];
        let (producer, mut consumer) = TransmissionPipeline::make(CONFIG, conduits.as_slice());
        let count = Arc::new(AtomicUsize::new(0));
        let size = Arc::new(AtomicUsize::new(0));

        let c_size = size.clone();
        task::spawn(async move {
            loop {
                let payload_sizes: [usize; 16] = [
                    8, 16, 32, 64, 128, 256, 512, 1_024, 2_048, 4_096, 8_192, 16_384, 32_768,
                    65_536, 262_144, 1_048_576,
                ];
                for size in payload_sizes.iter() {
                    c_size.store(*size, Ordering::Release);

                    // Send reliable messages
                    let key = "pipeline/thr".into();
                    let payload = ZBuf::from(vec![0_u8; *size]);
                    let channel = Channel {
                        priority: Priority::Control,
                        reliability: Reliability::Reliable,
                    };
                    let congestion_control = CongestionControl::Block;
                    let data_info = None;
                    let routing_context = None;
                    let reply_context = None;
                    let attachment = None;

                    let message = ZenohMessage::make_data(
                        key,
                        payload,
                        channel,
                        congestion_control,
                        data_info,
                        routing_context,
                        reply_context,
                        attachment,
                    );

                    let duration = Duration::from_millis(5_500);
                    let start = Instant::now();
                    while start.elapsed() < duration {
                        producer.push_zenoh_message(message.clone());
                    }
                }
            }
        });

        let c_count = count.clone();
        task::spawn(async move {
            loop {
                let (batch, priority) = consumer.pull().await.unwrap();
                c_count.fetch_add(batch.len() as usize, Ordering::AcqRel);
                consumer.refill(batch, priority);
            }
        });

        task::block_on(async {
            let mut prev_size: usize = usize::MAX;
            loop {
                let received = count.swap(0, Ordering::AcqRel);
                let current: usize = size.load(Ordering::Acquire);
                if current == prev_size {
                    let thr = (8.0 * received as f64) / 1_000_000_000.0;
                    println!("{} bytes: {:.6} Gbps", current, 2.0 * thr);
                }
                prev_size = current;
                task::sleep(Duration::from_millis(500)).await;
            }
        });
    }
}
