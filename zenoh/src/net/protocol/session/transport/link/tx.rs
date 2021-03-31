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
use super::core::Channel;
use super::io::WBuf;
use super::proto::{SessionMessage, ZenohMessage};
use super::session::defaults::{
    // Constants
    QUEUE_NUM,
    QUEUE_PRIO_CTRL,
    QUEUE_PRIO_DATA,
    QUEUE_PRIO_RETX,
    QUEUE_PULL_BACKOFF,
    // Configurable constants
    QUEUE_SIZE_CTRL,
    QUEUE_SIZE_DATA,
    QUEUE_SIZE_RETX,
};
use super::{SeqNumGenerator, SerializationBatch};
use async_std::sync::{Arc, Mutex, MutexGuard};
use async_std::task;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use zenoh_util::sync::Condition;
use zenoh_util::zasynclock;

macro_rules! zgetbatch {
    ($self:expr, $priority:expr, $stage_in:expr, $is_droppable:expr) => {
        // Try to get a pointer to the first batch
        loop {
            if let Some(batch) = $stage_in.inner.front_mut() {
                break batch;
            } else {
                // Refill the batches
                let mut refill_guard = zasynclock!($self.stage_refill[$priority]);
                if refill_guard.is_empty() {
                    // Execute the dropping strategy if provided
                    if $is_droppable {
                        // Drop the guard to allow the sending task to
                        // refill the queue of empty batches
                        drop(refill_guard);
                        // Yield this task
                        task::yield_now().await;
                        return;
                    }
                    // Drop the guard and wait for the batches to be available
                    $self.cond_canrefill[$priority].wait(refill_guard).await;
                    refill_guard = zasynclock!($self.stage_refill[$priority]);
                }
                // Drain all the empty batches
                while let Some(batch) = refill_guard.try_pull() {
                    $stage_in.inner.push_back(batch);
                }
            }
        }
    };
}

struct StageIn {
    inner: VecDeque<SerializationBatch>,
    bytes_topull: Arc<AtomicUsize>,
}

impl StageIn {
    fn new(
        capacity: usize,
        batch_size: usize,
        is_streamed: bool,
        sn_reliable: Arc<Mutex<SeqNumGenerator>>,
        sn_best_effort: Arc<Mutex<SeqNumGenerator>>,
        bytes_topull: Arc<AtomicUsize>,
    ) -> StageIn {
        let mut inner = VecDeque::<SerializationBatch>::with_capacity(capacity);
        for _ in 0..capacity {
            inner.push_back(SerializationBatch::new(
                batch_size,
                is_streamed,
                sn_reliable.clone(),
                sn_best_effort.clone(),
            ));
        }

        StageIn {
            inner,
            bytes_topull,
        }
    }

    fn try_pull(&mut self) -> Option<SerializationBatch> {
        if let Some(batch) = self.inner.front_mut() {
            if !batch.is_empty() {
                self.bytes_topull.store(0, Ordering::Release);
                // Write the batch len before removing the batch
                batch.write_len();
                // There is an incomplete batch, pop it
                return self.inner.pop_front();
            }
        }
        None
    }
}

struct StageOut {
    inner: VecDeque<SerializationBatch>,
    batches_out: Arc<AtomicUsize>,
}

impl StageOut {
    fn new(capacity: usize, batches_out: Arc<AtomicUsize>) -> StageOut {
        StageOut {
            inner: VecDeque::<SerializationBatch>::with_capacity(capacity),
            batches_out,
        }
    }

    #[inline]
    fn push(&mut self, batch: SerializationBatch) {
        self.batches_out.store(self.inner.len(), Ordering::Release);
        self.inner.push_back(batch);
    }

    #[inline]
    fn try_pull(&mut self) -> Option<SerializationBatch> {
        if let Some(batch) = self.inner.pop_front() {
            self.batches_out.store(self.inner.len(), Ordering::Release);
            Some(batch)
        } else {
            None
        }
    }
}

struct StageRefill {
    inner: VecDeque<SerializationBatch>,
}

impl StageRefill {
    fn new(capacity: usize) -> StageRefill {
        StageRefill {
            inner: VecDeque::<SerializationBatch>::with_capacity(capacity),
        }
    }

    #[inline]
    fn push(&mut self, mut batch: SerializationBatch) {
        batch.clear();
        self.inner.push_back(batch);
    }

    #[inline]
    fn try_pull(&mut self) -> Option<SerializationBatch> {
        self.inner.pop_front()
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

/// Link queue
pub(crate) struct TransmissionPipeline {
    // The default batch size
    batch_size: usize,
    // The sn generator for the reliable channel
    sn_reliable: Arc<Mutex<SeqNumGenerator>>,
    // The sn generator for the best effor channel
    sn_best_effort: Arc<Mutex<SeqNumGenerator>>,
    // Each priority queue has its own Mutex
    stage_in: Box<[Arc<Mutex<StageIn>>]>,
    // Amount of bytes available in each stage IN priority queue
    bytes_in: Box<[Arc<AtomicUsize>]>,
    // A single Mutex for all the priority queues
    stage_out: Arc<Mutex<Box<[StageOut]>>>,
    // Number of batches in each stage OUT priority queue
    batches_out: Box<[Arc<AtomicUsize>]>,
    // Each priority queue has its own Mutex
    stage_refill: Box<[Arc<Mutex<StageRefill>>]>,
    // Each priority queue has its own Conditional variable
    // The conditional variable requires a MutexGuard from stage_refill
    cond_canrefill: Box<[Arc<Condition>]>,
    // A single conditional variable for all the priority queues
    // The conditional variable requires a MutexGuard from stage_out
    cond_canpull: Condition,
}

impl TransmissionPipeline {
    /// Create a new link queue.
    pub(crate) fn new(
        batch_size: usize,
        is_streamed: bool,
        sn_reliable: Arc<Mutex<SeqNumGenerator>>,
        sn_best_effort: Arc<Mutex<SeqNumGenerator>>,
    ) -> TransmissionPipeline {
        // Conditional variables
        let mut cond_canrefill = vec![];
        cond_canrefill.resize_with(QUEUE_NUM, || Arc::new(Condition::new()));
        let cond_canpull = Condition::new();

        // Build the stage EMPTY
        let mut stage_refill = Vec::with_capacity(QUEUE_NUM);
        stage_refill.push(Arc::new(Mutex::new(StageRefill::new(*QUEUE_SIZE_CTRL))));
        stage_refill.push(Arc::new(Mutex::new(StageRefill::new(*QUEUE_SIZE_RETX))));
        stage_refill.push(Arc::new(Mutex::new(StageRefill::new(*QUEUE_SIZE_DATA))));

        // Batches to be pulled from stage OUT
        let mut batches_out = vec![];
        batches_out.resize_with(QUEUE_NUM, || Arc::new(AtomicUsize::new(0)));
        // Build the stage OUT
        let mut stage_out = Vec::with_capacity(QUEUE_NUM);
        stage_out.push(StageOut::new(
            *QUEUE_SIZE_CTRL,
            batches_out[QUEUE_PRIO_CTRL].clone(),
        ));
        stage_out.push(StageOut::new(
            *QUEUE_SIZE_RETX,
            batches_out[QUEUE_PRIO_RETX].clone(),
        ));
        stage_out.push(StageOut::new(
            *QUEUE_SIZE_DATA,
            batches_out[QUEUE_PRIO_DATA].clone(),
        ));
        let stage_out = Arc::new(Mutex::new(stage_out.into_boxed_slice()));

        // Bytes to be pulled from stage IN
        let mut bytes_in = vec![];
        bytes_in.resize_with(QUEUE_NUM, || Arc::new(AtomicUsize::new(0)));
        // Build the stage IN
        let mut stage_in = Vec::with_capacity(QUEUE_NUM);
        stage_in.push(Arc::new(Mutex::new(StageIn::new(
            *QUEUE_SIZE_CTRL,
            batch_size,
            is_streamed,
            sn_reliable.clone(),
            sn_best_effort.clone(),
            bytes_in[QUEUE_PRIO_CTRL].clone(),
        ))));
        stage_in.push(Arc::new(Mutex::new(StageIn::new(
            *QUEUE_SIZE_RETX,
            batch_size,
            is_streamed,
            sn_reliable.clone(),
            sn_best_effort.clone(),
            bytes_in[QUEUE_PRIO_RETX].clone(),
        ))));
        stage_in.push(Arc::new(Mutex::new(StageIn::new(
            *QUEUE_SIZE_DATA,
            batch_size,
            is_streamed,
            sn_reliable.clone(),
            sn_best_effort.clone(),
            bytes_in[QUEUE_PRIO_DATA].clone(),
        ))));

        TransmissionPipeline {
            batch_size,
            sn_reliable,
            sn_best_effort,
            stage_in: stage_in.into_boxed_slice(),
            bytes_in: bytes_in.into_boxed_slice(),
            stage_out,
            batches_out: batches_out.into_boxed_slice(),
            stage_refill: stage_refill.into_boxed_slice(),
            cond_canrefill: cond_canrefill.into_boxed_slice(),
            cond_canpull,
        }
    }

    #[inline]
    pub(super) async fn push_session_message(&self, message: SessionMessage, priority: usize) {
        let mut in_guard = zasynclock!(self.stage_in[priority]);

        macro_rules! zserialize {
            () => {
                // Get the current serialization batch
                let batch = zgetbatch!(self, priority, in_guard, false);
                if batch.serialize_session_message(&message).await {
                    self.bytes_in[priority].store(batch.len(), Ordering::Release);
                    self.cond_canpull.notify_one();
                    return;
                }
            };
        }

        // Attempt the serialization on the current batch
        zserialize!();

        // The first serialization attempt has failed. This means that the current
        // batch is full. Therefore:
        //   1) remove the current batch from the IN pipeline
        //   2) add the batch to the OUT pipeline
        if let Some(batch) = in_guard.try_pull() {
            // The previous batch wasn't empty
            let mut out_guard = zasynclock!(self.stage_out);
            out_guard[priority].push(batch);
            drop(out_guard);
            self.cond_canpull.notify_one();

            // Attempt the serialization on a new empty batch
            zserialize!();
        }

        log::warn!(
            "Session message dropped because it can not be fragmented: {:?}",
            message
        );
    }

    #[inline]
    pub(super) async fn push_zenoh_message(&self, message: ZenohMessage, priority: usize) {
        let mut in_guard = zasynclock!(self.stage_in[priority]);

        macro_rules! zserialize {
            () => {
                // Get the current serialization batch. Drop the message
                // if no batches are available
                let batch = zgetbatch!(self, priority, in_guard, message.is_droppable());
                if batch.serialize_zenoh_message(&message).await {
                    self.bytes_in[priority].store(batch.len(), Ordering::Release);
                    self.cond_canpull.notify_one();
                    return;
                }
            };
        }

        // Attempt the serialization on the current batch
        zserialize!();

        // The first serialization attempt has failed. This means that the current
        // batch is either full or the message is too large. In case of the former,
        // try to do:
        //   1) remove the current batch from the IN pipeline
        //   2) add the batch to the OUT pipeline
        if let Some(batch) = in_guard.try_pull() {
            // The previous batch wasn't empty, move it to the stage OUT pipeline
            let mut out_guard = zasynclock!(self.stage_out);
            out_guard[priority].push(batch);
            drop(out_guard);
            self.cond_canpull.notify_one();

            // Attempt the serialization on a new empty batch
            zserialize!();
        }

        // The second serialization attempt has failed. This means that the message is
        // too large for the current batch size: we need to fragment.
        self.fragment_zenoh_message(message, priority, in_guard)
            .await;
    }

    async fn fragment_zenoh_message(
        &self,
        message: ZenohMessage,
        priority: usize,
        mut in_guard: MutexGuard<'_, StageIn>,
    ) {
        // Create an expandable buffer and serialize the totality of the message
        let mut wbuf = WBuf::new(self.batch_size, false);
        wbuf.write_zenoh_message(&message);

        // Acquire the lock on the SN generator to ensure that we have all
        // sequential sequence numbers for the fragments
        let (ch, sn) = if message.is_reliable() {
            (Channel::Reliable, self.sn_reliable.clone())
        } else {
            (Channel::BestEffort, self.sn_best_effort.clone())
        };
        let mut guard = zasynclock!(sn);

        // Fragment the whole message
        let mut to_write = wbuf.len();
        while to_write > 0 {
            // Get the current serialization batch
            let batch = zgetbatch!(self, priority, in_guard, false);

            // Get the frame SN
            let sn = guard.get();

            // Serialize the message
            let written = batch
                .serialize_zenoh_fragment(ch, sn, &mut wbuf, to_write)
                .await;

            // Update the amount of bytes left to write
            to_write -= written;

            // 0 bytes written means error
            if written != 0 {
                // Move the serialization batch into the OUT pipeline
                let batch = in_guard.try_pull().unwrap();
                let mut out_guard = zasynclock!(self.stage_out);
                out_guard[priority].push(batch);
                drop(out_guard);
                self.cond_canpull.notify_one();
            } else {
                // Reinsert the SN back to the pool
                guard.set(sn);
                log::warn!(
                    "Zenoh message dropped because it can not be fragmented: {:?}",
                    message
                );
                break;
            }
        }
    }

    #[allow(clippy::comparison_chain)]
    pub(super) async fn try_pull_queue(&self, priority: usize) -> Option<SerializationBatch> {
        let mut backoff = Duration::from_nanos(*QUEUE_PULL_BACKOFF);
        let mut bytes_in_pre: usize = 0;
        loop {
            // Check first if we have complete batches available for transmission
            if self.batches_out[priority].load(Ordering::Acquire) > 0 {
                let mut out_guard = zasynclock!(self.stage_out);
                let batch = out_guard[priority].try_pull().unwrap();
                return Some(batch);
            }
            // Check then if there are incomplete batches available for transmission
            else {
                let bytes_in_now = self.bytes_in[priority].load(Ordering::Acquire);
                if bytes_in_now > 0 {
                    if bytes_in_now < bytes_in_pre {
                        // There should be a new batch in Stage OUT
                        bytes_in_pre = bytes_in_now;
                        continue;
                    } else if bytes_in_now == bytes_in_pre {
                        // No new bytes have been written on the batch, try to pull
                        let mut out_guard = zasynclock!(self.stage_out);
                        if let Some(batch) = out_guard[priority].try_pull() {
                            return Some(batch);
                        } else {
                            // Check if an incomplete (non-empty) batch is available in the state IN pipeline.
                            if let Some(mut in_guard) = self.stage_in[priority].try_lock() {
                                if let Some(batch) = in_guard.try_pull() {
                                    return Some(batch);
                                } else {
                                    break;
                                }
                            } else {
                                drop(out_guard);
                                // Batch is being filled up, let's backoff and retry
                                task::sleep(backoff).await;
                                backoff = 2 * backoff;
                                continue;
                            }
                        }
                    } else {
                        // Batch is being filled up, let's backoff and retry
                        bytes_in_pre = bytes_in_now;
                        task::sleep(backoff).await;
                        backoff = 2 * backoff;
                        continue;
                    }
                } else {
                    break;
                }
            }
        }
        // There are no batches in this queue to be pulled
        None
    }

    pub(super) async fn pull(&self) -> (SerializationBatch, usize) {
        let mut backoff = Duration::from_nanos(*QUEUE_PULL_BACKOFF);
        loop {
            for priority in 0..QUEUE_NUM {
                if let Some(batch) = self.try_pull_queue(priority).await {
                    return (batch, priority);
                }
            }

            let mut out_guard = zasynclock!(self.stage_out);
            let mut is_pipeline_really_empty = true;
            for priority in 0..out_guard.len() {
                if let Some(batch) = out_guard[priority].try_pull() {
                    return (batch, priority);
                } else {
                    // Check if an incomplete (non-empty) batch is available in the state IN pipeline.
                    if let Some(mut in_guard) = self.stage_in[priority].try_lock() {
                        if let Some(batch) = in_guard.try_pull() {
                            return (batch, priority);
                        }
                    } else {
                        is_pipeline_really_empty = false
                    }
                }
            }

            if is_pipeline_really_empty {
                self.cond_canpull.wait(out_guard).await;
            } else {
                drop(out_guard);
                // Batches are being filled up, let's backoff and retry
                task::sleep(backoff).await;
                backoff = 2 * backoff;
            }
        }
    }

    pub(super) async fn refill(&self, batch: SerializationBatch, priority: usize) {
        let mut refill_guard = zasynclock!(self.stage_refill[priority]);
        refill_guard.push(batch);
        drop(refill_guard);
        self.cond_canrefill[priority].notify_one();
    }

    pub(super) async fn drain(&self) -> Option<SerializationBatch> {
        // First try to drain the stage OUT pipeline
        let mut out_guard = zasynclock!(self.stage_out);
        for priority in 0..out_guard.len() {
            if let Some(batch) = out_guard[priority].try_pull() {
                return Some(batch);
            }
        }
        drop(out_guard);

        // Then try to drain what left in the stage IN pipeline
        for priority in 0..self.stage_in.len() {
            let mut in_guard = zasynclock!(self.stage_in[priority]);
            if let Some(batch) = in_guard.try_pull() {
                return Some(batch);
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::super::core::{CongestionControl, Reliability, ResKey, ZInt};
    use super::super::io::RBuf;
    use super::super::proto::{Frame, FramePayload, SessionBody, ZenohMessage};
    use super::super::session::defaults::{
        QUEUE_PRIO_DATA, SESSION_BATCH_SIZE, SESSION_SEQ_NUM_RESOLUTION,
    };
    use super::*;
    use async_std::prelude::*;
    use async_std::sync::{Arc, Mutex};
    use async_std::task;
    use std::convert::TryFrom;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    const TIMEOUT: Duration = Duration::from_secs(60);

    #[test]
    fn tx_pipeline() {
        async fn schedule(queue: Arc<TransmissionPipeline>, num_msg: usize, payload_size: usize) {
            // Send reliable messages
            let key = ResKey::RName("test".to_string());
            let payload = RBuf::from(vec![0u8; payload_size]);
            let reliability = Reliability::Reliable;
            let congestion_control = CongestionControl::Block;
            let data_info = None;
            let routing_context = None;
            let reply_context = None;
            let attachment = None;

            let message = ZenohMessage::make_data(
                key,
                payload,
                reliability,
                congestion_control,
                data_info,
                routing_context,
                reply_context,
                attachment,
            );

            println!(
                ">>> Sending {} messages with payload size: {}",
                num_msg, payload_size
            );
            for _ in 0..num_msg {
                queue
                    .push_zenoh_message(message.clone(), QUEUE_PRIO_DATA)
                    .await;
            }
        }

        async fn consume(queue: Arc<TransmissionPipeline>, num_msg: usize) {
            let mut batches: usize = 0;
            let mut bytes: usize = 0;
            let mut msgs: usize = 0;
            let mut fragments: usize = 0;

            while msgs != num_msg {
                let (batch, priority) = queue.pull().await;
                batches += 1;
                bytes += batch.len();
                // Create a RBuf for deserialization starting from the batch
                let mut rbuf: RBuf = batch.get_serialized_messages().into();
                // Deserialize the messages
                while let Some(msg) = rbuf.read_session_message() {
                    match msg.body {
                        SessionBody::Frame(Frame { payload, .. }) => match payload {
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
                }
                // Reinsert the batch
                queue.refill(batch, priority).await;
            }

            println!(
                "<<< Received {} messages, {} bytes, {} batches, {} fragments",
                msgs, bytes, batches, fragments
            );
        }

        // Queue
        let batch_size = *SESSION_BATCH_SIZE;
        let is_streamed = true;
        let sn_reliable = Arc::new(Mutex::new(SeqNumGenerator::new(
            0,
            *SESSION_SEQ_NUM_RESOLUTION,
        )));
        let sn_best_effort = Arc::new(Mutex::new(SeqNumGenerator::new(
            0,
            *SESSION_SEQ_NUM_RESOLUTION,
        )));
        let queue = Arc::new(TransmissionPipeline::new(
            batch_size,
            is_streamed,
            sn_reliable,
            sn_best_effort,
        ));

        // Total amount of bytes to send in each test
        let bytes: usize = 100_000_000;
        let max_msgs: usize = 1_000;
        // Paylod size of the messages
        let payload_sizes = [8, 64, 512, 4_096, 8_192, 32_768, 262_144, 2_097_152];

        task::block_on(async {
            for ps in payload_sizes.iter() {
                if ZInt::try_from(*ps).is_err() {
                    break;
                }

                // Compute the number of messages to send
                let num_msg = max_msgs.min(bytes / ps);

                let c_queue = queue.clone();
                let t_c = task::spawn(async move {
                    consume(c_queue, num_msg).await;
                });

                let c_queue = queue.clone();
                let c_ps = *ps;
                let t_s = task::spawn(async move {
                    schedule(c_queue, num_msg, c_ps).await;
                });

                let res = t_c.join(t_s).timeout(TIMEOUT).await;
                assert!(res.is_ok());
            }
        });
    }

    #[test]
    #[ignore]
    fn tx_pipeline_thr() {
        // Queue
        let batch_size = *SESSION_BATCH_SIZE;
        let is_streamed = true;
        let sn_reliable = Arc::new(Mutex::new(SeqNumGenerator::new(
            0,
            *SESSION_SEQ_NUM_RESOLUTION,
        )));
        let sn_best_effort = Arc::new(Mutex::new(SeqNumGenerator::new(
            0,
            *SESSION_SEQ_NUM_RESOLUTION,
        )));
        let pipeline = Arc::new(TransmissionPipeline::new(
            batch_size,
            is_streamed,
            sn_reliable,
            sn_best_effort,
        ));
        let count = Arc::new(AtomicUsize::new(0));
        let size = Arc::new(AtomicUsize::new(0));

        let c_pipeline = pipeline.clone();
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
                    let key = ResKey::RName("/pipeline/thr".to_string());
                    let payload = RBuf::from(vec![0u8; *size]);
                    let reliability = Reliability::Reliable;
                    let congestion_control = CongestionControl::Block;
                    let data_info = None;
                    let routing_context = None;
                    let reply_context = None;
                    let attachment = None;

                    let message = ZenohMessage::make_data(
                        key,
                        payload,
                        reliability,
                        congestion_control,
                        data_info,
                        routing_context,
                        reply_context,
                        attachment,
                    );

                    let duration = Duration::from_millis(5_500);
                    let start = Instant::now();
                    while start.elapsed() < duration {
                        c_pipeline
                            .push_zenoh_message(message.clone(), QUEUE_PRIO_DATA)
                            .await;
                    }
                }
            }
        });

        let c_pipeline = pipeline.clone();
        let c_count = count.clone();
        task::spawn(async move {
            loop {
                let (batch, priority) = c_pipeline.pull().await;
                c_count.fetch_add(batch.len(), Ordering::AcqRel);
                task::sleep(Duration::from_nanos(100)).await;
                c_pipeline.refill(batch, priority).await;
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
