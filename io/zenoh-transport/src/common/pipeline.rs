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
use super::batch::SerializationBatch;
use super::conduit::{TransportChannelTx, TransportConduitTx};
use super::protocol::core::Priority;
use super::protocol::io::WBuf;
use super::protocol::proto::{TransportMessage, ZenohMessage};
use async_std::prelude::FutureExt;
use flume::{Receiver, Sender};
use ringbuffer_spsc::{RingBuffer, RingBufferReader, RingBufferWriter};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};
use zenoh_core::{zlock, Result as ZResult};
use zenoh_protocol::proto::MessageWriter;

const RBLEN: usize = 16;

macro_rules! zgetbatch {
    ($self:expr, $c_guard:expr, $droppable:expr) => {
        loop {
            match $c_guard.take() {
                Some(batch) => break batch,
                None => match $self.s_ref.pull() {
                    Some(mut batch) => {
                        batch.clear();
                        break batch;
                    }
                    None => {
                        drop($c_guard);
                        if $droppable {
                            thread::yield_now();
                            return false;
                        } else {
                            if !$self.s_ref.wait() {
                                return false;
                            }
                        }
                        // Retry
                        $c_guard = $self.mutex.current();
                    }
                },
            }
        }
    };
}

struct StageInRefill {
    n_ref_r: Receiver<()>,
    s_ref_r: RingBufferReader<SerializationBatch, RBLEN>,
}

impl StageInRefill {
    fn pull(&mut self) -> Option<SerializationBatch> {
        self.s_ref_r.pull()
    }

    fn wait(&self) -> bool {
        self.n_ref_r.recv().is_ok()
    }
}

struct StageInOut {
    n_out_w: Sender<()>,
    s_out_w: RingBufferWriter<SerializationBatch, RBLEN>,
}

impl StageInOut {
    fn move_batch(&mut self, batch: SerializationBatch) {
        let _ = self.s_out_w.push(batch);
        let _ = self.n_out_w.try_send(());
    }
}

struct StageInMutex {
    current: Arc<Mutex<Option<SerializationBatch>>>,
    conduit: TransportConduitTx,
}

impl StageInMutex {
    fn current(&self) -> MutexGuard<'_, Option<SerializationBatch>> {
        zlock!(self.current)
    }

    fn channel(&self, is_reliable: bool) -> MutexGuard<'_, TransportChannelTx> {
        if is_reliable {
            zlock!(self.conduit.reliable)
        } else {
            zlock!(self.conduit.best_effort)
        }
    }
}

struct StageIn {
    s_ref: StageInRefill,
    s_out: StageInOut,
    mutex: StageInMutex,
    backoff: bool,
    fragbuf: WBuf,
}

impl StageIn {
    fn push_zenoh_message(&mut self, mut msg: ZenohMessage) -> bool {
        // Lock the channel. We are the only one that will be writing on it.
        let mut channel = self.mutex.channel(msg.is_reliable());
        // Lock the current serialization batch.
        let mut c_guard = self.mutex.current();

        // Check congestion control
        let is_droppable = msg.is_droppable();

        macro_rules! zserialize {
            () => {{
                // Get the current serialization batch. Drop the message
                // if no batches are available
                let mut batch = zgetbatch!(self, c_guard, is_droppable);
                let prio = msg.channel.priority;
                if batch.serialize_zenoh_message(&mut msg, prio, &mut channel.sn) {
                    if self.backoff {
                        *c_guard = Some(batch);
                    } else {
                        self.s_out.move_batch(batch);
                    }
                    return true;
                }
                batch
            }};
        }

        // Attempt the serialization on the current batch
        let batch = zserialize!();

        // The first serialization attempt has failed. This means that the current
        // batch is full. Therefore move the current batch to stage out.
        self.s_out.move_batch(batch);

        let mut batch = zserialize!();

        // The second serialization attempt has failed. This means that the message is
        // too large for the current batch size: we need to fragment.
        //     self.fragment_zenoh_message(msg, channel, c_guard, batch)
        // }

        // fn fragment_zenoh_message(
        //     &mut self,
        //     mut msg: ZenohMessage,
        //     channel: MutexGuard<'_, TransportChannelTx>,
        //     c_guard: MutexGuard<'_, Option<SerializationBatch>>,
        //     batch: SerializationBatch,
        // ) -> bool {
        //     // Assign the stage_in to in_guard to avoid lifetime warnings
        //     let mut channel = channel;
        //     let mut c_guard = c_guard;
        //     let mut batch = batch;
        //     let is_droppable = msg.is_droppable();

        // Take the expandable buffer and serialize the totality of the message
        self.fragbuf.clear();
        self.fragbuf.write_zenoh_message(&mut msg);

        // Fragment the whole message
        let mut to_write = self.fragbuf.len();
        let mut fragbuf_reader = self.fragbuf.reader();
        while to_write > 0 {
            // Serialize the message
            let written = batch.serialize_zenoh_fragment(
                msg.channel.reliability,
                msg.channel.priority,
                &mut channel.sn,
                &mut fragbuf_reader,
                to_write,
            );

            // Update the amount of bytes left to write
            to_write -= written;

            // 0 bytes written means error
            if written != 0 {
                // Move the serialization batch into the OUT pipeline
                self.s_out.move_batch(batch);
            } else {
                log::warn!(
                    "Zenoh message dropped because it can not be fragmented: {:?}",
                    msg
                );
                break;
            }

            // Get the current serialization batch
            // Treat all messages as non-droppable once we start fragmenting
            batch = zgetbatch!(self, c_guard, is_droppable);
        }

        true
    }

    #[inline]
    fn push_transport_message(&mut self, mut msg: TransportMessage) -> bool {
        true
    }
}

enum PullResult {
    Some(SerializationBatch),
    None,
    Backoff(Duration),
}
struct StageOut {
    n_ref_w: Sender<()>,
    s_ref_w: RingBufferWriter<SerializationBatch, RBLEN>,
    s_out_r: RingBufferReader<SerializationBatch, RBLEN>,
    current: Arc<Mutex<Option<SerializationBatch>>>,
    backoff: Duration,
    last_pull: Instant,
}

impl StageOut {
    fn try_pull(&mut self) -> PullResult {
        if let Some(mut batch) = self.s_out_r.pull() {
            batch.write_len();
            return PullResult::Some(batch);
        }
        let now = Instant::now();
        let diff = now - self.last_pull;
        if diff < self.backoff {
            self.last_pull = now;
            return PullResult::Backoff(self.backoff - diff);
        }

        match self.current.try_lock() {
            Ok(mut g) => g.take().map_or(PullResult::None, |mut batch| {
                batch.write_len();
                PullResult::Some(batch)
            }),
            Err(_) => PullResult::Backoff(self.backoff),
        }
    }

    fn refill(&mut self, batch: SerializationBatch) {
        let _ = self.s_ref_w.push(batch);
        let _ = self.n_ref_w.try_send(());
    }

    fn drain(
        &mut self,
        guard: &mut MutexGuard<'_, Option<SerializationBatch>>,
    ) -> Vec<SerializationBatch> {
        let mut batches = vec![];
        // Empty the ring buffer
        while let Some(mut batch) = self.s_out_r.pull() {
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

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TransmissionPipelineConf {
    pub(crate) is_streamed: bool,
    pub(crate) batch_size: u16,
    pub(crate) queue_size: [usize; Priority::NUM],
    pub(crate) backoff: Duration,
}

impl Default for TransmissionPipelineConf {
    fn default() -> Self {
        Self {
            is_streamed: true,
            batch_size: u16::MAX,
            queue_size: [1; Priority::NUM],
            backoff: Duration::from_micros(1),
        }
    }
}

pub(crate) struct TransmissionPipeline;
impl TransmissionPipeline {
    // A MPSC pipeline
    pub(crate) fn new(
        config: TransmissionPipelineConf,
        conduit: &[TransportConduitTx],
    ) -> (TransmissionPipelineProducer, TransmissionPipelineConsumer) {
        let mut stage_in = vec![];
        let mut stage_out = vec![];

        let default_queue = [Priority::default() as usize];
        let size_iter = if conduit.len() == 1 {
            default_queue.iter()
        } else {
            config.queue_size.iter()
        };

        // Create the channel for notifying that new batches are in the out ring buffer
        // This is a MPSC channel
        let (n_out_w, n_out_r) = flume::bounded(1);

        // @TODO: remove this workaround
        if conduit.len() == 0 {
            let producer = TransmissionPipelineProducer {
                stage_in: stage_in.into_boxed_slice().into(),
            };
            let consumer = TransmissionPipelineConsumer {
                stage_out: stage_out.into_boxed_slice(),
                n_out_r,
            };

            return (producer, consumer);
        }

        for (prio, num) in size_iter.enumerate() {
            // Create the refill ring buffer
            // This is a SPSC ring buffer
            let (mut s_ref_w, s_ref_r) = RingBuffer::<SerializationBatch, RBLEN>::new();
            // Fill the refill ring buffer with batches
            for _ in 0..*num {
                assert!(s_ref_w
                    .push(SerializationBatch::new(
                        config.batch_size,
                        config.is_streamed
                    ))
                    .is_none());
            }
            // Create the channel for notifying that new batches are in the refill ring buffer
            // This is a SPSC channel
            let (n_ref_w, n_ref_r) = flume::bounded(1);

            // Create the refill ring buffer
            // This is a SPSC ring buffer
            let (s_out_w, s_out_r) = RingBuffer::<SerializationBatch, RBLEN>::new();

            // The batch being serialized upon
            let current = Arc::new(Mutex::new(None));

            // The stage in for this priority
            stage_in.push(Arc::new(Mutex::new(StageIn {
                s_ref: StageInRefill { n_ref_r, s_ref_r },
                s_out: StageInOut {
                    n_out_w: n_out_w.clone(),
                    s_out_w,
                },
                mutex: StageInMutex {
                    current: current.clone(),
                    conduit: conduit[prio].clone(),
                },
                backoff: !(prio == 0 || prio == 1),
                fragbuf: WBuf::new(config.batch_size as usize, false),
            })));

            // The stage out for this priority
            stage_out.push(StageOut {
                n_ref_w,
                s_ref_w,
                s_out_r,
                current,
                backoff: Duration::from_micros(if prio == 0 || prio == 1 {
                    0 as u64
                } else {
                    5 * prio.pow(2) as u64
                }),
                last_pull: Instant::now(),
            });
        }

        let producer = TransmissionPipelineProducer {
            stage_in: stage_in.into_boxed_slice().into(),
        };
        let consumer = TransmissionPipelineConsumer {
            stage_out: stage_out.into_boxed_slice(),
            n_out_r,
        };

        (producer, consumer)
    }
}

#[derive(Clone)]
pub(crate) struct TransmissionPipelineProducer {
    // Each priority queue has its own Mutex
    stage_in: Arc<[Arc<Mutex<StageIn>>]>,
}

impl TransmissionPipelineProducer {
    #[inline]
    pub(crate) fn push_zenoh_message(&self, mut msg: ZenohMessage) -> bool {
        // If the queue is not QoS, it means that we only have one priority with index 0.
        let priority = if self.stage_in.len() > 1 {
            msg.channel.priority as usize
        } else {
            msg.channel.priority = Priority::default();
            0
        };
        // Lock the channel. We are the only one that will be writing on it.
        let mut queue = zlock!(self.stage_in[priority]);
        queue.push_zenoh_message(msg)
    }

    #[inline]
    pub(crate) fn push_transport_message(
        &self,
        mut message: TransportMessage,
        priority: Priority,
    ) -> bool {
        false
    }

    pub(crate) fn disable(&mut self) {
        self.stage_in = vec![].into_boxed_slice().into();
    }
}

pub(crate) struct TransmissionPipelineConsumer {
    // A single Mutex for all the priority queues
    stage_out: Box<[StageOut]>,
    n_out_r: Receiver<()>,
}

impl TransmissionPipelineConsumer {
    pub(crate) async fn pull(&mut self) -> Option<(SerializationBatch, usize)> {
        async fn notification(ch: &Receiver<()>) -> bool {
            match ch.recv_async().await {
                Ok(_) => true,
                Err(_) => false,
            }
        }

        async fn backoff(bo: Duration) -> bool {
            async_std::task::sleep(bo).await;
            true
        }

        loop {
            let mut bo = Duration::from_micros(u32::MAX as u64);
            for (prio, queue) in self.stage_out.iter_mut().enumerate() {
                match queue.try_pull() {
                    PullResult::Some(batch) => return Some((batch, prio)),
                    PullResult::Backoff(b) => {
                        if b < bo {
                            bo = b;
                        }
                    }
                    PullResult::None => {}
                }
            }

            if !notification(&self.n_out_r).race(backoff(bo)).await {
                return None;
            };
        }
    }

    pub(crate) fn refill(&mut self, batch: SerializationBatch, priority: usize) {
        self.stage_out[priority].refill(batch);
    }

    pub(crate) fn disable(&mut self) {
        self.stage_out = vec![].into()
    }

    pub(crate) fn drain(&mut self) -> Vec<(SerializationBatch, usize)> {
        // Drain the remaining batches
        let mut batches = vec![];

        // Acquire all the locks, in_guard first, out_guard later
        // Use the same locking order as in disable to avoid deadlocks
        let locks = self
            .stage_out
            .iter()
            .map(|x| x.current.clone())
            .collect::<Vec<_>>();
        let mut currents: Vec<MutexGuard<'_, Option<SerializationBatch>>> =
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
    use async_std::prelude::FutureExt;
    use async_std::task;
    use std::convert::TryFrom;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use zenoh_buffers::reader::HasReader;
    use zenoh_protocol::io::ZBuf;
    use zenoh_protocol::proto::defaults::{BATCH_SIZE, SEQ_NUM_RES};
    use zenoh_protocol::proto::MessageReader;
    use zenoh_protocol::proto::{Frame, FramePayload, TransportBody, ZenohMessage};
    use zenoh_protocol_core::{Channel, CongestionControl, Priority, Reliability, ZInt};

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
                "Pipeline Flow [>>>]: Sending {} messages with payload size of {} bytes",
                num_msg, payload_size
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
                bytes += batch.len();
                // Create a ZBuf for deserialization starting from the batch
                let zbuf: ZBuf = batch.get_serialized_messages().to_vec().into();
                // Deserialize the messages
                let mut reader = zbuf.reader();
                while let Some(msg) = reader.read_transport_message() {
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
                println!("Pipeline Flow [+++]: Refill {} msgs", msgs + 1);
                // Reinsert the batch
                queue.refill(batch, priority);
            }

            println!(
                "Pipeline Flow [<<<]: Received {} messages, {} bytes, {} batches, {} fragments",
                msgs, bytes, batches, fragments
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

                let (producer, consumer) = TransmissionPipeline::new(
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
                    "Pipeline Blocking [>>>]: ({}) Scheduling message #{} with payload size of {} bytes",
                    id, i,
                    payload_size
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
            TransmissionPipeline::new(TransmissionPipelineConf::default(), conduits.as_slice());

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
                println!("Pipeline Blocking [---]: disabling the queue");
                consumer.disable();
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
    // #[ignore]
    fn tx_pipeline_thr() {
        // Queue
        let tct = TransportConduitTx::make(SEQ_NUM_RES).unwrap();
        let conduits = vec![tct];
        let (producer, mut consumer) = TransmissionPipeline::new(CONFIG, conduits.as_slice());
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
                c_count.fetch_add(batch.len(), Ordering::AcqRel);
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
