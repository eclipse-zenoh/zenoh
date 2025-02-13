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
    cmp::min,
    fmt, mem,
    sync::{
        atomic::{AtomicI8, Ordering},
        Arc, Mutex, MutexGuard,
    },
    time::{Duration, Instant},
};

use futures::future::OptionFuture;
use ringbuffer_spsc::{RingBuffer, RingBufferReader, RingBufferWriter};
use zenoh_buffers::{
    reader::{HasReader, Reader},
    writer::HasWriter,
    ZBuf,
};
use zenoh_codec::{transport::batch::BatchError, WCodec, Zenoh080};
use zenoh_config::QueueSizeConf;
use zenoh_core::zlock;
use zenoh_protocol::{
    core::Priority,
    network::NetworkMessage,
    transport::{
        fragment,
        fragment::FragmentHeader,
        frame::{self, FrameHeader},
        BatchSize, TransportMessage,
    },
};
use zenoh_sync::{event, Notifier, WaitDeadlineError, WaitError, Waiter};

use super::{
    batch::{Encode, WBatch},
    priority::TransportPriorityTx,
};
use crate::common::batch::BatchConfig;

const RBLEN: usize = QueueSizeConf::MAX;

#[derive(Debug)]
pub(crate) struct TransportClosed;
impl fmt::Display for TransportClosed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "transport closed")
    }
}
impl std::error::Error for TransportClosed {}

/// Count of available batches for a pipeline.
///
/// When no batch is available, a "congested" flag can be set and checked later
/// to shortcut the pipeline.
///
/// # Implementation
///
/// The counter is an atomic integer, initialized with the batch count. It is decremented
/// when batches are acquired, and incremented when they are released. If the count is zero,
/// meaning no batch is available, it can be updated to -1, meaning that the pipeline is
/// congested.
struct AvailableBatches(AtomicI8);

impl AvailableBatches {
    /// Initializes the count of available batch.
    fn new(count: usize) -> Self {
        Self(AtomicI8::new(count as i8))
    }

    /// Tries to acquire a batch, returns if there is one available.
    ///
    /// If no batch is available and `set_congested` is `true`, then
    /// the congested flag will be set.
    fn try_acquire(&self, set_congested: bool) -> bool {
        if set_congested {
            // Try to acquire a batch; if count was zero, it is set to -1,
            // resulting in the congested flag set.
            self.0.fetch_sub(1, Ordering::Relaxed) > 0
        } else if self.0.load(Ordering::Relaxed) > 0 {
            // No need of a compare-and-swap operation to decrease the count
            // only if it's positive, as there should only be a single acquirer.
            self.0.fetch_sub(1, Ordering::Relaxed);
            return true;
        } else {
            false
        }
    }

    /// Returns if the congested flag has been set, meaning no batch is available.
    /// The congested flag should usually be set after waiting a bit.
    fn is_congested(&self) -> bool {
        self.0.load(Ordering::Relaxed) == -1
    }

    /// Releases a batch, making it available.
    /// If the
    fn release(&self) {
        // If the pipeline was congested, increment the count a second time
        if self.0.fetch_add(1, Ordering::Relaxed) == -1 {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[derive(Debug)]
struct Deadline {
    wait_time: Duration,
    max_wait_time: Option<Duration>,
    expiration: Option<Instant>,
}

impl Deadline {
    fn new(wait_time: Duration, max_wait_time: Option<Duration>) -> Self {
        Self {
            wait_time,
            max_wait_time,
            expiration: None,
        }
    }

    fn get_expiration(&mut self) -> Option<Instant> {
        if self.expiration.is_none() && self.wait_time > Duration::ZERO {
            self.expiration = Some(Instant::now() + self.wait_time);
        }
        self.expiration
    }

    fn renew(&mut self) {
        if let Some(expiration) = self.get_expiration() {
            self.expiration = Some(expiration + self.wait_time);
            if let Some(max) = self.max_wait_time {
                self.max_wait_time = max.checked_sub(self.wait_time);
                self.wait_time *= 2;
            }
        }
    }
}

#[derive(Debug, Default)]
struct CurrentBatch {
    batch: Option<WBatch>,
    timestamp: Option<Instant>,
}

struct StageInShared {
    queue: Mutex<StageIn>,
    current: Arc<Mutex<CurrentBatch>>,
    chan_tx: TransportPriorityTx,
    available: Arc<AvailableBatches>,
}

// This is the initial stage of the pipeline where messages are serialized on
struct StageIn {
    batch_tx: RingBufferWriter<WBatch, RBLEN>,
    batch_notifier: Notifier,
    available: Arc<AvailableBatches>,
    available_waiter: Waiter,
    use_small_batch: bool,
    fragbuf: ZBuf,
    batching: bool,
    batch_config: BatchConfig,
}

impl StageIn {
    const SMALL_BATCH_SIZE: BatchSize = 1 << 11;

    /// Allocate a new batch, using small batch size when appropriate.
    fn new_batch(&self) -> WBatch {
        let mut batch_config = self.batch_config;
        if self.use_small_batch {
            batch_config.mtu = min(self.batch_config.mtu, Self::SMALL_BATCH_SIZE);
        }
        WBatch::new(batch_config)
    }

    /// Retrieve the current batch, or allocate a new one if there is a slot available.
    fn get_batch(
        &self,
        current: &mut MutexGuard<'_, CurrentBatch>,
        mut deadline: Option<&mut Deadline>,
    ) -> Result<Option<WBatch>, TransportClosed> {
        // retrieve current batch if any
        if let Some(batch) = current.batch.take() {
            return Ok(Some(batch));
        }
        loop {
            // try to acquire an available batch
            if self.available.try_acquire(false) {
                return Ok(Some(self.new_batch()));
            }
            // otherwise, wait until one is available
            match deadline.as_mut().map(|d| d.get_expiration()) {
                Some(Some(deadline)) => match self.available_waiter.wait_deadline(deadline) {
                    Ok(..) => continue,
                    Err(WaitDeadlineError::Deadline) => break,
                    Err(WaitDeadlineError::WaitError) => return Err(TransportClosed),
                },
                Some(None) => break,
                None => match self.available_waiter.wait() {
                    Ok(..) => continue,
                    Err(WaitError) => return Err(TransportClosed),
                },
            }
        }
        // the deadline has been exceeded, try a last time, setting the congested flag
        // if there is still no batch
        if self.available.try_acquire(true) {
            return Ok(Some(self.new_batch()));
        }
        Ok(None)
    }

    /// Push a batch to the TX task.
    ///
    /// If batching is enabled, unless `force` is set to `true`, then the batch will be
    /// kept for reuse.
    /// If the batch is small, the next batches will be allocated with a small capacity too.
    fn push_batch(
        &mut self,
        current: &mut MutexGuard<'_, CurrentBatch>,
        batch: WBatch,
        force: bool,
    ) {
        if batch.len() < Self::SMALL_BATCH_SIZE {
            self.use_small_batch = true;
        }
        if !self.batching || force {
            self.batch_tx.push(batch);
            let _ = self.batch_notifier.notify();
        } else {
            current.batch = Some(batch);
            if current.timestamp.is_none() {
                current.timestamp = Some(Instant::now());
                let _ = self.batch_notifier.notify();
            }
        }
    }

    fn push_network_message(
        &mut self,
        shared: &StageInShared,
        msg: &NetworkMessage,
        priority: Priority,
        deadline: &mut Deadline,
    ) -> Result<bool, TransportClosed> {
        // Lock the current serialization batch.
        let mut current = zlock!(shared.current);

        // Attempt the serialization on the current batch.
        let Some(mut batch) = self.get_batch(&mut current, Some(deadline))? else {
            return Ok(false);
        };
        let need_new_frame = match batch.encode(msg) {
            Ok(_) => {
                self.push_batch(&mut current, batch, msg.is_express());
                return Ok(true);
            }
            Err(BatchError::NewFrame) => true,
            Err(_) => false,
        };

        // Lock the channel. We are the only one that will be writing on it.
        let tch = if msg.is_reliable() {
            &shared.chan_tx.reliable
        } else {
            &shared.chan_tx.best_effort
        };
        let mut tch = zlock!(tch);

        // Retrieve the next SN.
        let sn = tch.sn.get();

        // The Frame
        let frame = FrameHeader {
            reliability: msg.reliability,
            sn,
            ext_qos: frame::ext::QoSType::new(priority),
        };

        // Attempt a serialization with a new frame.
        if need_new_frame && batch.encode((msg, &frame)).is_ok() {
            self.push_batch(&mut current, batch, msg.is_express());
            return Ok(true);
        }

        // Push the batch if not empty
        if !batch.is_empty() {
            self.push_batch(&mut current, batch, true);
        } else {
            drop(batch);
            self.available.release();
        }

        // Attempt a second serialization on fully empty batch (do not use small batch)
        self.use_small_batch = false;
        match self.get_batch(&mut current, Some(deadline))? {
            Some(b) => batch = b,
            None => {
                tch.sn.set(sn).unwrap();
                return Ok(false);
            }
        }
        if batch.encode((msg, &frame)).is_ok() {
            self.push_batch(&mut current, batch, msg.is_express());
            return Ok(true);
        }

        // The second serialization attempt has failed. This means that the message is
        // too large for the current batch size: we need to fragment.
        // Reinsert the current batch for fragmentation.
        current.batch = Some(batch);

        // Take the expandable buffer and serialize the totality of the message
        let mut fragbuf = mem::take(&mut self.fragbuf);

        let mut writer = fragbuf.writer();
        let codec = Zenoh080::new();
        codec.write(&mut writer, msg).unwrap();

        // Fragment the whole message
        let mut fragment = FragmentHeader {
            reliability: frame.reliability,
            more: true,
            sn,
            ext_qos: frame.ext_qos,
            ext_first: Some(fragment::ext::First::new()),
            ext_drop: None,
        };
        let mut reader = fragbuf.reader();
        while reader.can_read() {
            match self.get_batch(&mut current, Some(deadline))? {
                Some(b) => batch = b,
                None => {
                    // If no fragment has been sent, the sequence number is just reset
                    if fragment.ext_first.is_some() {
                        tch.sn.set(sn).unwrap()
                    // Otherwise, an ephemeral batch is created to send the stop fragment
                    } else {
                        self.use_small_batch = true;
                        let mut batch = self.new_batch();
                        fragment.ext_drop = Some(fragment::ext::Drop::new());
                        let _ = batch.encode((&mut self.fragbuf.reader(), &mut fragment));
                        self.push_batch(&mut current, batch, true);
                    }
                    return Ok(false);
                }
            }
            // Serialize the message fragment
            match batch.encode((&mut reader, &mut fragment)) {
                Ok(_) => {
                    // Update the SN
                    fragment.sn = tch.sn.get();
                    fragment.ext_first = None;
                    self.push_batch(&mut current, batch, true);
                }
                Err(_) => {
                    // Restore the sequence number
                    tch.sn.set(sn).unwrap();
                    // Reinsert the batch
                    current.batch = Some(batch);
                    tracing::warn!(
                        "Zenoh message dropped because it can not be fragmented: {:?}",
                        msg
                    );
                    break;
                }
            }

            // renew deadline for the next fragment
            deadline.renew();
        }

        // Clean the fragbuf
        self.fragbuf = fragbuf;
        self.fragbuf.clear();

        Ok(true)
    }

    #[inline]
    fn push_transport_message(
        &mut self,
        shared: &StageInShared,
        msg: TransportMessage,
    ) -> Result<bool, TransportClosed> {
        // Lock the current serialization batch.
        let mut current = zlock!(shared.current);

        // Attempt the serialization on the current batch.
        let mut batch = self.get_batch(&mut current, None)?.unwrap();

        // Attempt the serialization on the current batch
        if batch.encode(&msg).is_ok() {
            self.push_batch(&mut current, batch, true);
            return Ok(true);
        }
        // The first serialization attempt has failed. This means that the current
        // batch is full. Therefore, we move the current batch to stage out.
        self.push_batch(&mut current, batch, true);
        self.use_small_batch = false;
        batch = self.get_batch(&mut current, None)?.unwrap();
        if batch.encode(&msg).is_ok() {
            self.push_batch(&mut current, batch, true);
            return Ok(true);
        }
        self.available.release();
        Ok(false)
    }
}

struct StageOut {
    batch_rx: RingBufferReader<WBatch, RBLEN>,
    current: Arc<Mutex<CurrentBatch>>,
    available: Arc<AvailableBatches>,
    available_notifier: Notifier,
    batching_time_limit: Duration,
}

impl StageOut {
    const DEFAULT_BACKOFF: Duration = Duration::from_millis(1);

    fn pull(&mut self) -> Result<Option<WBatch>, Duration> {
        if let Some(batch) = self.batch_rx.pull() {
            return Ok(Some(batch));
        }
        let Ok(mut current) = self.current.try_lock() else {
            return Err(Self::DEFAULT_BACKOFF);
        };
        if let Some(timestamp) = current.timestamp {
            let elapsed = timestamp.elapsed();
            if elapsed > self.batching_time_limit {
                current.timestamp = None;
                Ok(current.batch.take())
            } else {
                Err(self.batching_time_limit - elapsed)
            }
        } else {
            Ok(None)
        }
    }

    fn drain(&mut self) -> Vec<WBatch> {
        let mut batches = vec![];
        // Empty the ring buffer
        while let Some(batch) = self.batch_rx.pull() {
            batches.push(batch);
        }
        // Take the current batch
        if let Some(batch) = zlock!(self.current).batch.take() {
            batches.push(batch);
        }
        batches
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TransmissionPipelineConf {
    pub(crate) batch: BatchConfig,
    pub(crate) queue_size: [usize; Priority::NUM],
    pub(crate) wait_before_drop: (Duration, Duration),
    pub(crate) wait_before_close: Duration,
    pub(crate) batching_enabled: bool,
    pub(crate) batching_time_limit: Duration,
}

// A 2-stage transmission pipeline
pub(crate) struct TransmissionPipeline;
impl TransmissionPipeline {
    // A MPSC pipeline
    pub(crate) fn make(
        config: TransmissionPipelineConf,
        priority: &[TransportPriorityTx],
    ) -> (TransmissionPipelineProducer, TransmissionPipelineConsumer) {
        let mut stage_in = vec![];
        let mut stage_out = vec![];

        let default_queue_size = [config.queue_size[Priority::DEFAULT as usize]];
        let size_iter = if priority.len() == 1 {
            default_queue_size.iter()
        } else {
            config.queue_size.iter()
        };

        let (disable_notifier, disable_waiter) = event::new();
        let (batch_notifier, batch_waiter) = event::new();

        for (prio, &num) in size_iter.enumerate() {
            assert!(num != 0 && num <= RBLEN);
            let available = Arc::new(AvailableBatches::new(num));
            let (available_notifier, available_waiter) = event::new();

            let (batch_tx, batch_rx) = RingBuffer::<WBatch, RBLEN>::init();

            let current = Arc::new(Mutex::new(CurrentBatch::default()));

            stage_in.push(StageInShared {
                queue: Mutex::new(StageIn {
                    batch_tx,
                    batch_notifier: batch_notifier.clone(),
                    available: available.clone(),
                    available_waiter,
                    use_small_batch: true,
                    fragbuf: ZBuf::empty(),
                    batching: config.batching_enabled,
                    batch_config: config.batch,
                }),
                current: current.clone(),
                chan_tx: priority[prio].clone(),
                available: available.clone(),
            });

            // The stage out for this priority
            stage_out.push(StageOut {
                batch_rx,
                current,
                available,
                available_notifier,
                batching_time_limit: config.batching_time_limit,
            });
        }

        let producer = TransmissionPipelineProducer {
            stage_in: stage_in.into_boxed_slice().into(),
            disable_notifier,
            wait_before_drop: config.wait_before_drop,
            wait_before_close: config.wait_before_close,
        };
        let consumer = TransmissionPipelineConsumer {
            stage_out: stage_out.into_boxed_slice(),
            batch_waiter,
            disable_waiter,
        };

        (producer, consumer)
    }
}

#[derive(Clone)]
pub(crate) struct TransmissionPipelineProducer {
    // Each priority queue has its own Mutex
    stage_in: Arc<[StageInShared]>,
    disable_notifier: Notifier,
    wait_before_drop: (Duration, Duration),
    wait_before_close: Duration,
}

impl TransmissionPipelineProducer {
    #[inline]
    pub(crate) fn push_network_message(
        &self,
        msg: NetworkMessage,
    ) -> Result<bool, TransportClosed> {
        // If the queue is not QoS, it means that we only have one priority with index 0.
        let (idx, priority) = if self.stage_in.len() > 1 {
            let priority = msg.priority();
            (priority as usize, priority)
        } else {
            (0, Priority::DEFAULT)
        };

        let stage_in = &self.stage_in[idx];

        // If message is droppable, compute a deadline after which the sample could be dropped
        let (wait_time, max_wait_time) = if msg.is_droppable() {
            // Checked if we are blocked on the priority queue and we drop directly the message
            if stage_in.available.is_congested() {
                return Ok(false);
            }
            (self.wait_before_drop.0, Some(self.wait_before_drop.1))
        } else {
            (self.wait_before_close, None)
        };
        let mut deadline = Deadline::new(wait_time, max_wait_time);
        // Lock the channel. We are the only one that will be writing on it.
        let mut queue = zlock!(stage_in.queue);
        // Check again for congestion in case it happens when blocking on the mutex.
        if stage_in.available.is_congested() {
            return Ok(false);
        }
        queue.push_network_message(stage_in, &msg, priority, &mut deadline)
    }

    #[inline]
    pub(crate) fn push_transport_message(
        &self,
        msg: TransportMessage,
        priority: Priority,
    ) -> Result<bool, TransportClosed> {
        // If the queue is not QoS, it means that we only have one priority with index 0.
        let priority = if self.stage_in.len() > 1 {
            priority as usize
        } else {
            0
        };
        let stage_in = &self.stage_in[priority];
        // Lock the channel. We are the only one that will be writing on it.
        let mut queue = zlock!(stage_in.queue);
        queue.push_transport_message(stage_in, msg)
    }

    pub(crate) fn disable(&self) {
        let _ = self.disable_notifier.notify();
    }
}

pub(crate) struct TransmissionPipelineConsumer {
    // A single Mutex for all the priority queues
    stage_out: Box<[StageOut]>,
    batch_waiter: Waiter,
    disable_waiter: Waiter,
}

impl TransmissionPipelineConsumer {
    pub(crate) async fn pull(&mut self) -> Option<(WBatch, Priority)> {
        loop {
            let mut sleep = OptionFuture::default();
            for (i, stage_out) in self.stage_out.iter_mut().enumerate() {
                let prio = Priority::try_from(i as u8).unwrap();
                match stage_out.pull() {
                    Ok(Some(batch)) => return Some((batch, prio)),
                    Ok(None) => continue,
                    Err(backoff) => {
                        sleep = Some(tokio::time::sleep(backoff)).into();
                        break;
                    }
                }
            }
            tokio::select! {
                biased;
                _ = self.disable_waiter.wait_async() => return None,
                _ = self.batch_waiter.wait_async() => {},
                Some(_) = sleep => {}
            }
        }
    }

    pub(crate) fn refill(&mut self, _batch: WBatch, priority: Priority) {
        let stage_out = &self.stage_out[priority as usize];
        stage_out.available.release();
        let _ = stage_out.available_notifier.notify();
    }

    pub(crate) fn drain(&mut self) -> Vec<(WBatch, usize)> {
        self.stage_out
            .iter_mut()
            .map(StageOut::drain)
            .enumerate()
            .flat_map(|(prio, batches)| batches.into_iter().map(move |batch| (batch, prio)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::{
        convert::TryFrom,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
        time::{Duration, Instant},
    };

    use tokio::{task, time::timeout};
    use zenoh_buffers::{
        reader::{DidntRead, HasReader},
        ZBuf,
    };
    use zenoh_codec::{RCodec, Zenoh080};
    use zenoh_protocol::{
        core::{Bits, CongestionControl, Encoding, Priority},
        network::{ext, Push},
        transport::{BatchSize, Fragment, Frame, TransportBody, TransportSn},
        zenoh::{PushBody, Put},
    };
    use zenoh_result::ZResult;

    use super::*;

    const SLEEP: Duration = Duration::from_millis(100);
    const TIMEOUT: Duration = Duration::from_secs(60);

    const CONFIG_STREAMED: TransmissionPipelineConf = TransmissionPipelineConf {
        batch: BatchConfig {
            mtu: BatchSize::MAX,
            is_streamed: true,
            #[cfg(feature = "transport_compression")]
            is_compression: true,
        },
        queue_size: [1; Priority::NUM],
        batching_enabled: true,
        wait_before_drop: (Duration::from_millis(1), Duration::from_millis(1024)),
        wait_before_close: Duration::from_secs(5),
        batching_time_limit: Duration::from_micros(1),
    };

    const CONFIG_NOT_STREAMED: TransmissionPipelineConf = TransmissionPipelineConf {
        batch: BatchConfig {
            mtu: BatchSize::MAX,
            is_streamed: false,
            #[cfg(feature = "transport_compression")]
            is_compression: false,
        },
        queue_size: [1; Priority::NUM],
        batching_enabled: true,
        wait_before_drop: (Duration::from_millis(1), Duration::from_millis(1024)),
        wait_before_close: Duration::from_secs(5),
        batching_time_limit: Duration::from_micros(1),
    };

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn tx_pipeline_flow() -> ZResult<()> {
        fn schedule(queue: TransmissionPipelineProducer, num_msg: usize, payload_size: usize) {
            // Send reliable messages
            let key = "test".into();
            let payload = ZBuf::from(vec![0_u8; payload_size]);

            let message: NetworkMessage = Push {
                wire_expr: key,
                ext_qos: ext::QoSType::new(Priority::Control, CongestionControl::Block, false),
                ext_tstamp: None,
                ext_nodeid: ext::NodeIdType::DEFAULT,
                payload: PushBody::Put(Put {
                    timestamp: None,
                    encoding: Encoding::empty(),
                    ext_sinfo: None,
                    #[cfg(feature = "shared-memory")]
                    ext_shm: None,
                    ext_attachment: None,
                    ext_unknown: vec![],
                    payload,
                }),
            }
            .into();

            println!(
                "Pipeline Flow [>>>]: Sending {num_msg} messages with payload size of {payload_size} bytes"
            );
            for i in 0..num_msg {
                println!(
                    "Pipeline Flow [>>>]: Pushed {} msgs ({payload_size} bytes)",
                    i + 1
                );
                queue.push_network_message(message.clone()).unwrap();
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
                let bytes = batch.as_slice();
                // Deserialize the messages
                let mut reader = bytes.reader();
                let codec = Zenoh080::new();

                loop {
                    let res: Result<TransportMessage, DidntRead> = codec.read(&mut reader);
                    match res {
                        Ok(msg) => {
                            match msg.body {
                                TransportBody::Frame(Frame { payload, .. }) => {
                                    msgs += payload.len()
                                }
                                TransportBody::Fragment(Fragment { more, .. }) => {
                                    fragments += 1;
                                    if !more {
                                        msgs += 1;
                                    }
                                }
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

        // Pipeline priorities
        let tct = TransportPriorityTx::make(Bits::from(TransportSn::MAX))?;
        let priorities = vec![tct];

        // Total amount of bytes to send in each test
        let bytes: usize = 100_000_000;
        let max_msgs: usize = 1_000;
        // Payload size of the messages
        let payload_sizes = [8, 64, 512, 4_096, 8_192, 32_768, 262_144, 2_097_152];

        for ps in payload_sizes.iter() {
            if u64::try_from(*ps).is_err() {
                break;
            }

            // Compute the number of messages to send
            let num_msg = max_msgs.min(bytes / ps);

            let (producer, consumer) =
                TransmissionPipeline::make(CONFIG_NOT_STREAMED, priorities.as_slice());

            let t_c = task::spawn(async move {
                consume(consumer, num_msg).await;
            });

            let c_ps = *ps;
            let t_s = task::spawn_blocking(move || {
                schedule(producer, num_msg, c_ps);
            });

            let res = tokio::time::timeout(TIMEOUT, futures::future::join_all([t_c, t_s])).await;
            assert!(res.is_ok());
        }

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn tx_pipeline_blocking() -> ZResult<()> {
        fn schedule(queue: TransmissionPipelineProducer, counter: Arc<AtomicUsize>, id: usize) {
            // Make sure to put only one message per batch: set the payload size
            // to half of the batch in such a way the serialized zenoh message
            // will be larger then half of the batch size (header + payload).
            let payload_size = (CONFIG_STREAMED.batch.mtu / 2) as usize;

            // Send reliable messages
            let key = "test".into();
            let payload = ZBuf::from(vec![0_u8; payload_size]);

            let message: NetworkMessage = Push {
                wire_expr: key,
                ext_qos: ext::QoSType::new(Priority::Control, CongestionControl::Block, false),
                ext_tstamp: None,
                ext_nodeid: ext::NodeIdType::DEFAULT,
                payload: PushBody::Put(Put {
                    timestamp: None,
                    encoding: Encoding::empty(),
                    ext_sinfo: None,
                    #[cfg(feature = "shared-memory")]
                    ext_shm: None,
                    ext_attachment: None,
                    ext_unknown: vec![],
                    payload,
                }),
            }
            .into();

            // The last push should block since there shouldn't any more batches
            // available for serialization.
            let num_msg = 1 + CONFIG_STREAMED.queue_size[0];
            for i in 0..num_msg {
                println!(
                    "Pipeline Blocking [>>>]: ({id}) Scheduling message #{i} with payload size of {payload_size} bytes"
                );
                queue.push_network_message(message.clone()).unwrap();
                let c = counter.fetch_add(1, Ordering::AcqRel);
                println!(
                    "Pipeline Blocking [>>>]: ({}) Scheduled message #{} (tot {}) with payload size of {} bytes",
                    id, i, c + 1,
                    payload_size
                );
            }
        }

        // Pipeline
        let tct = TransportPriorityTx::make(Bits::from(TransportSn::MAX))?;
        let priorities = vec![tct];
        let (producer, mut consumer) =
            TransmissionPipeline::make(CONFIG_NOT_STREAMED, priorities.as_slice());

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

        // Wait to have sent enough messages and to have blocked
        println!(
            "Pipeline Blocking [---]: waiting to have {} messages being scheduled",
            CONFIG_STREAMED.queue_size[Priority::MAX as usize]
        );
        let check = async {
            while counter.load(Ordering::Acquire)
                < CONFIG_STREAMED.queue_size[Priority::MAX as usize]
            {
                tokio::time::sleep(SLEEP).await;
            }
        };

        timeout(TIMEOUT, check).await?;

        // Drain the queue (but don't drop it to avoid dropping the messages)
        let _consumer = timeout(
            TIMEOUT,
            task::spawn_blocking(move || {
                println!("Pipeline Blocking [---]: draining the queue");
                let _ = consumer.drain();
                consumer
            }),
        )
        .await??;

        // Make sure that the tasks scheduling have been unblocked
        println!("Pipeline Blocking [---]: waiting for schedule (1) to be unblocked");
        timeout(TIMEOUT, h1).await??;
        println!("Pipeline Blocking [---]: waiting for schedule (2) to be unblocked");
        timeout(TIMEOUT, h2).await??;

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore]
    async fn tx_pipeline_thr() {
        // Queue
        let tct = TransportPriorityTx::make(Bits::from(TransportSn::MAX)).unwrap();
        let priorities = vec![tct];
        let (producer, mut consumer) =
            TransmissionPipeline::make(CONFIG_STREAMED, priorities.as_slice());
        let count = Arc::new(AtomicUsize::new(0));
        let size = Arc::new(AtomicUsize::new(0));

        let c_size = size.clone();
        task::spawn_blocking(move || {
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

                    let message: NetworkMessage = Push {
                        wire_expr: key,
                        ext_qos: ext::QoSType::new(
                            Priority::Control,
                            CongestionControl::Block,
                            false,
                        ),
                        ext_tstamp: None,
                        ext_nodeid: ext::NodeIdType::DEFAULT,
                        payload: PushBody::Put(Put {
                            timestamp: None,
                            encoding: Encoding::empty(),
                            ext_sinfo: None,
                            #[cfg(feature = "shared-memory")]
                            ext_shm: None,
                            ext_attachment: None,
                            ext_unknown: vec![],
                            payload,
                        }),
                    }
                    .into();

                    let duration = Duration::from_millis(5_500);
                    let start = Instant::now();
                    while start.elapsed() < duration {
                        producer.push_network_message(message.clone()).unwrap();
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

        let mut prev_size: usize = usize::MAX;
        loop {
            let received = count.swap(0, Ordering::AcqRel);
            let current: usize = size.load(Ordering::Acquire);
            if current == prev_size {
                let thr = (8.0 * received as f64) / 1_000_000_000.0;
                println!("{} bytes: {:.6} Gbps", current, 2.0 * thr);
            }
            prev_size = current;
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn tx_pipeline_closed() -> ZResult<()> {
        // Pipeline
        let tct = TransportPriorityTx::make(Bits::from(TransportSn::MAX))?;
        let priorities = vec![tct];
        let (producer, consumer) =
            TransmissionPipeline::make(CONFIG_NOT_STREAMED, priorities.as_slice());
        // Drop consumer to close the pipeline
        drop(consumer);

        let message: NetworkMessage = Push {
            wire_expr: "test".into(),
            ext_qos: ext::QoSType::new(Priority::Control, CongestionControl::Block, true),
            ext_tstamp: None,
            ext_nodeid: ext::NodeIdType::DEFAULT,
            payload: PushBody::Put(Put {
                timestamp: None,
                encoding: Encoding::empty(),
                ext_sinfo: None,
                #[cfg(feature = "shared-memory")]
                ext_shm: None,
                ext_attachment: None,
                ext_unknown: vec![],
                payload: vec![42u8].into(),
            }),
        }
        .into();
        // First message should not be rejected as the is one batch available in the queue
        assert!(producer.push_network_message(message.clone()).is_ok());
        // Second message should be rejected
        assert!(producer.push_network_message(message.clone()).is_err());

        Ok(())
    }
}
