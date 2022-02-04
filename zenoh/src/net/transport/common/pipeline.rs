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
use super::super::defaults::{
    // Constants
    ZN_QUEUE_PULL_BACKOFF,
    // Configurable constants
    ZN_QUEUE_SIZE_BACKGROUND,
    ZN_QUEUE_SIZE_CONTROL,
    ZN_QUEUE_SIZE_DATA,
    ZN_QUEUE_SIZE_DATA_HIGH,
    ZN_QUEUE_SIZE_DATA_LOW,
    ZN_QUEUE_SIZE_INTERACTIVE_HIGH,
    ZN_QUEUE_SIZE_INTERACTIVE_LOW,
    ZN_QUEUE_SIZE_REAL_TIME,
};
use super::batch::SerializationBatch;
use super::conduit::{TransportChannelTx, TransportConduitTx};
use super::protocol::core::Priority;
use super::protocol::io::WBuf;
use super::protocol::message::{TransportProto, ZMessage, ZenohMessage};
use async_std::task;
use std::collections::VecDeque;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;
use std::time::Duration;
use zenoh_util::sync::{Condition as AsyncCondvar, ConditionWaiter as AsyncCondvarWaiter};
use zenoh_util::zlock;

macro_rules! zgetbatch {
    ($self:expr, $priority:expr, $stage_in:expr, $is_droppable:expr) => {
        // Try to get a pointer to the first batch
        loop {
            if let Some(batch) = $stage_in.inner.front_mut() {
                break batch;
            }

            // Refill the batches
            let mut refill_guard = zlock!($self.stage_refill[$priority]);
            if refill_guard.is_empty() {
                // Execute the dropping strategy if provided
                if $is_droppable {
                    // Drop the guard to allow the sending task to
                    // refill the queue of empty batches
                    drop(refill_guard);
                    // Yield this thread to not spin the msg pusher
                    thread::yield_now();
                    return false;
                }

                // Drop the stage_in and refill guards and wait for the batches to be available
                drop($stage_in);

                // Verify that the pipeline is still active
                if !$self.active.load(Ordering::Acquire) {
                    return false;
                }

                refill_guard = $self.cond_canrefill[$priority].wait(refill_guard).unwrap();

                // Verify that the pipeline is still active
                if !$self.active.load(Ordering::Acquire) {
                    return false;
                }

                $stage_in = zlock!($self.stage_in[$priority]);
            }

            // Drain all the empty batches
            while let Some(batch) = refill_guard.try_pull() {
                $stage_in.inner.push_back(batch);
            }
        }
    };
}

struct StageIn {
    priority: usize,
    inner: VecDeque<SerializationBatch>,
    bytes_topull: Arc<[AtomicUsize]>,
    fragbuf: Option<WBuf>,
}

impl StageIn {
    fn new(
        priority: usize,
        capacity: usize,
        batch_size: u16,
        is_streamed: bool,
        bytes_topull: Arc<[AtomicUsize]>,
    ) -> StageIn {
        let mut inner = VecDeque::<SerializationBatch>::with_capacity(capacity);
        for _ in 0..capacity {
            inner.push_back(SerializationBatch::new(batch_size, is_streamed));
        }

        StageIn {
            priority,
            inner,
            bytes_topull,
            fragbuf: Some(WBuf::new(batch_size as usize, false)),
        }
    }

    fn try_pull(&mut self) -> Option<SerializationBatch> {
        if let Some(batch) = self.inner.front_mut() {
            if !batch.is_empty() {
                self.bytes_topull[self.priority].store(0, Ordering::Release);
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
    priority: usize,
    inner: VecDeque<SerializationBatch>,
    batches_out: Arc<[AtomicUsize]>,
}

impl StageOut {
    fn new(priority: usize, capacity: usize, batches_out: Arc<[AtomicUsize]>) -> StageOut {
        StageOut {
            priority,
            inner: VecDeque::<SerializationBatch>::with_capacity(capacity),
            batches_out,
        }
    }

    #[inline]
    fn push(&mut self, batch: SerializationBatch) {
        self.batches_out[self.priority].store(self.inner.len(), Ordering::Release);
        self.inner.push_back(batch);
    }

    #[inline]
    fn try_pull(&mut self) -> Option<SerializationBatch> {
        if let Some(batch) = self.inner.pop_front() {
            self.batches_out[self.priority].store(self.inner.len(), Ordering::Release);
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
    // Status variable of transmission pipeline
    active: AtomicBool,
    // The conduit TX containing the SN generators
    conduit: Arc<[TransportConduitTx]>,
    // Each conduit queue has its own Mutex
    stage_in: Box<[Mutex<StageIn>]>,
    // Amount of bytes available in each stage IN conduit queue
    bytes_in: Arc<[AtomicUsize]>,
    // A single Mutex for all the conduit queues
    stage_out: Mutex<Box<[StageOut]>>,
    // Number of batches in each stage OUT conduit queue
    batches_out: Arc<[AtomicUsize]>,
    // Each conduit queue has its own Mutex
    stage_refill: Box<[Mutex<StageRefill>]>,
    // Each conduit queue has its own Conditional variable
    // The conditional variable requires a MutexGuard from stage_refill
    cond_canrefill: Box<[Condvar]>,
    // A single conditional variable for all the conduit queues
    // The conditional variable requires a MutexGuard from stage_out
    cond_canpull: AsyncCondvar,
}

impl TransmissionPipeline {
    /// Create a new link queue.
    pub(crate) fn new(
        batch_size: u16,
        is_streamed: bool,
        conduit: Arc<[TransportConduitTx]>,
    ) -> TransmissionPipeline {
        macro_rules! zcapacity {
            ($conduit:expr) => {
                match $conduit.priority {
                    Priority::Control => *ZN_QUEUE_SIZE_CONTROL,
                    Priority::RealTime => *ZN_QUEUE_SIZE_REAL_TIME,
                    Priority::InteractiveHigh => *ZN_QUEUE_SIZE_INTERACTIVE_HIGH,
                    Priority::InteractiveLow => *ZN_QUEUE_SIZE_INTERACTIVE_LOW,
                    Priority::DataHigh => *ZN_QUEUE_SIZE_DATA_HIGH,
                    Priority::Data => *ZN_QUEUE_SIZE_DATA,
                    Priority::DataLow => *ZN_QUEUE_SIZE_DATA_LOW,
                    Priority::Background => *ZN_QUEUE_SIZE_BACKGROUND,
                }
            };
        }

        // Conditional variables
        let mut cond_canrefill = vec![];
        cond_canrefill.resize_with(conduit.len(), Condvar::new);
        let cond_canrefill = cond_canrefill.into_boxed_slice();

        let cond_canpull = AsyncCondvar::new();

        // Build the stage REFILL
        let mut stage_refill = Vec::with_capacity(conduit.len());
        for c in conduit.iter() {
            let capacity = zcapacity!(c);
            stage_refill.push(Mutex::new(StageRefill::new(capacity)));
        }
        let stage_refill = stage_refill.into_boxed_slice();

        // Batches to be pulled from stage OUT
        let mut batches_out = vec![];
        batches_out.resize_with(conduit.len(), || AtomicUsize::new(0));
        let batches_out: Arc<[AtomicUsize]> = batches_out.into_boxed_slice().into();

        // Build the stage OUT
        let mut stage_out = Vec::with_capacity(conduit.len());
        for (priority, c) in conduit.iter().enumerate() {
            let capacity = zcapacity!(c);
            stage_out.push(StageOut::new(priority, capacity, batches_out.clone()));
        }
        let stage_out = Mutex::new(stage_out.into_boxed_slice());

        // Bytes to be pulled from stage IN
        let mut bytes_in = vec![];
        bytes_in.resize_with(conduit.len(), || AtomicUsize::new(0));
        let bytes_in: Arc<[AtomicUsize]> = bytes_in.into_boxed_slice().into();

        // Build the stage IN
        let mut stage_in = Vec::with_capacity(conduit.len());
        for (priority, c) in conduit.iter().enumerate() {
            let capacity = zcapacity!(c);
            stage_in.push(Mutex::new(StageIn::new(
                priority,
                capacity,
                batch_size,
                is_streamed,
                bytes_in.clone(),
            )));
        }
        let stage_in = stage_in.into_boxed_slice();

        TransmissionPipeline {
            active: AtomicBool::new(true),
            conduit,
            stage_in,
            bytes_in,
            stage_out,
            batches_out,
            stage_refill,
            cond_canrefill,
            cond_canpull,
        }
    }

    #[inline(always)]
    fn is_qos(&self) -> bool {
        self.conduit.len() > 1
    }

    #[inline]
    pub(crate) fn push_transport_message<T: ZMessage<Proto = TransportProto>>(
        &self,
        mut message: T,
        priority: Priority,
    ) -> bool {
        // Check it is a valid conduit
        let priority = if self.is_qos() { priority as usize } else { 0 };
        let mut in_guard = zlock!(self.stage_in[priority]);

        macro_rules! zserialize {
            () => {
                // Get the current serialization batch
                let batch = zgetbatch!(self, priority, in_guard, false);
                if batch.serialize_message(&mut message) {
                    self.bytes_in[priority].store(batch.len(), Ordering::Release);
                    self.cond_canpull.notify_one();
                    return true;
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
            let mut out_guard = zlock!(self.stage_out);
            out_guard[priority].push(batch);
            drop(out_guard);
            self.cond_canpull.notify_one();

            // Attempt the serialization on a new empty batch
            zserialize!();
        }

        log::warn!(
            "Transport message dropped because it can not be fragmented: {:?}",
            message
        );

        false
    }

    #[inline]
    pub(crate) fn push_zenoh_message(&self, mut message: ZenohMessage) -> bool {
        // If the queue is not QoS, it means that we only have one priority with index 0.
        let priority = if self.is_qos() {
            message.channel.priority as usize
        } else {
            message.channel.priority = Priority::default();
            0
        };
        // Lock the channel. We are the only one that will be writing on it.
        let mut ch_guard = if message.is_reliable() {
            zlock!(self.conduit[priority].reliable)
        } else {
            zlock!(self.conduit[priority].best_effort)
        };
        // Lock the stage in containing the serialization batches.
        let mut in_guard = zlock!(self.stage_in[priority]);

        macro_rules! zserialize {
            () => {
                // Get the current serialization batch. Drop the message
                // if no batches are available
                let batch = zgetbatch!(self, priority, in_guard, message.is_droppable());
                let mp = message.channel.priority;
                if batch.serialize_zenoh_message(&mut message, mp, &mut ch_guard.sn) {
                    self.bytes_in[priority].store(batch.len(), Ordering::Release);
                    self.cond_canpull.notify_one();
                    return true;
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
            let mut out_guard = zlock!(self.stage_out);
            out_guard[priority].push(batch);
            drop(out_guard);
            self.cond_canpull.notify_one();

            // Attempt the serialization on a new empty batch
            zserialize!();
        }

        // The second serialization attempt has failed. This means that the message is
        // too large for the current batch size: we need to fragment.
        self.fragment_zenoh_message(message, priority, ch_guard, in_guard)
    }

    fn fragment_zenoh_message(
        &self,
        mut message: ZenohMessage,
        priority: usize,
        channel: MutexGuard<'_, TransportChannelTx>,
        stage_in: MutexGuard<'_, StageIn>,
    ) -> bool {
        // Assign the stage_in to in_guard to avoid lifetime warnings
        let mut ch_guard = channel;
        let mut in_guard = stage_in;

        // Take the expandable buffer and serialize the totality of the message
        let mut fragbuf = in_guard.fragbuf.take().unwrap();
        fragbuf.clear();
        fragbuf.write_zenoh_message(&mut message);

        // Fragment the whole message
        let mut to_write = fragbuf.len();
        while to_write > 0 {
            // Get the current serialization batch
            // Treat all messages as non-droppable once we start fragmenting
            let batch = zgetbatch!(self, priority, in_guard, false);

            // Serialize the message
            let written = batch.serialize_zenoh_fragment(
                message.channel.reliability,
                message.channel.priority,
                &mut ch_guard.sn,
                &mut fragbuf,
                to_write,
            );

            // Update the amount of bytes left to write
            to_write -= written;

            // 0 bytes written means error
            if written != 0 {
                // Move the serialization batch into the OUT pipeline
                let batch = in_guard.try_pull().unwrap();
                let mut out_guard = zlock!(self.stage_out);
                out_guard[priority].push(batch);
                drop(out_guard);
                self.cond_canpull.notify_one();
            } else {
                log::warn!(
                    "Zenoh message dropped because it can not be fragmented: {:?}",
                    message
                );
                break;
            }
        }
        // Reinsert the fragbuf
        in_guard.fragbuf = Some(fragbuf);

        true
    }

    pub(crate) async fn try_pull_queue(&self, priority: usize) -> Option<SerializationBatch> {
        let mut backoff = Duration::from_nanos(*ZN_QUEUE_PULL_BACKOFF);
        let mut bytes_in_pre: usize = 0;
        loop {
            // Check first if we have complete batches available for transmission
            if self.batches_out[priority].load(Ordering::Acquire) > 0 {
                let mut out_guard = zlock!(self.stage_out);
                return out_guard[priority].try_pull();
            }

            // Check then if there are incomplete batches available for transmission
            let bytes_in_now = self.bytes_in[priority].load(Ordering::Acquire);
            if bytes_in_now == 0 {
                // Nothing in the batch, return immediately
                return None;
            }

            if bytes_in_now < bytes_in_pre {
                // There should be a new batch in Stage OUT
                bytes_in_pre = bytes_in_now;
                continue;
            }

            if bytes_in_now == bytes_in_pre {
                // No new bytes have been written on the batch, try to pull
                // First try to pull from stage OUT
                let mut out_guard = zlock!(self.stage_out);
                if let Some(batch) = out_guard[priority].try_pull() {
                    return Some(batch);
                }

                // An incomplete (non-empty) batch is available in the state IN pipeline.
                if let Ok(mut in_guard) = self.stage_in[priority].try_lock() {
                    return in_guard.try_pull();
                }
            }

            // Batch is being filled up, let's backoff and retry
            bytes_in_pre = bytes_in_now;
            task::sleep(backoff).await;
            backoff = 2 * backoff;
        }
    }

    pub(crate) async fn pull(&self) -> Option<(SerializationBatch, usize)> {
        enum Action {
            Wait(AsyncCondvarWaiter),
            Sleep,
        }

        let mut backoff = Duration::from_nanos(*ZN_QUEUE_PULL_BACKOFF);
        loop {
            for conduit in 0..self.conduit.len() {
                if let Some(batch) = self.try_pull_queue(conduit).await {
                    return Some((batch, conduit));
                }
            }

            let action = {
                let mut out_guard = zlock!(self.stage_out);
                let mut is_pipeline_really_empty = true;
                for conduit in 0..out_guard.len() {
                    if let Some(batch) = out_guard[conduit].try_pull() {
                        return Some((batch, conduit));
                    }

                    // Check if an incomplete (non-empty) batch is available in the state IN pipeline.
                    if let Ok(mut in_guard) = self.stage_in[conduit].try_lock() {
                        if let Some(batch) = in_guard.try_pull() {
                            return Some((batch, conduit));
                        }
                    } else {
                        is_pipeline_really_empty = false
                    }
                }

                if is_pipeline_really_empty {
                    let waiter = self.cond_canpull.waiter(out_guard);
                    Action::Wait(waiter)
                } else {
                    Action::Sleep
                }
            };

            match action {
                Action::Wait(waiter) => {
                    // Check if the pipeline is still active
                    if !self.active.load(Ordering::Acquire) {
                        return None;
                    }

                    waiter.await;

                    // Check if the pipeline is still active
                    if !self.active.load(Ordering::Acquire) {
                        return None;
                    }
                }
                Action::Sleep => {
                    // Batches are being filled up, let's backoff and retry
                    task::sleep(backoff).await;
                    backoff = 2 * backoff;
                }
            }
        }
    }

    pub(crate) fn refill(&self, batch: SerializationBatch, queue: usize) {
        let mut refill_guard = zlock!(self.stage_refill[queue]);
        refill_guard.push(batch);
        drop(refill_guard);
        self.cond_canrefill[queue].notify_one();
    }

    pub(crate) fn disable(&self) {
        // Mark the pipeline as no longer active
        self.active.store(false, Ordering::Release);

        // Acquire all the locks, in_guard first, out_guard later
        // Use the same locking order as in drain to avoid deadlocks
        let _in_guards: Vec<MutexGuard<'_, StageIn>> =
            self.stage_in.iter().map(|x| zlock!(x)).collect();
        let _out_guard = zlock!(self.stage_out);
        let _re_guards: Vec<MutexGuard<'_, StageRefill>> =
            self.stage_refill.iter().map(|x| zlock!(x)).collect();

        // Unblock waiting pushers
        for cr in self.cond_canrefill.iter() {
            cr.notify_all();
        }
        // Unblock waiting pullers
        self.cond_canpull.notify_all();
    }

    pub(crate) fn drain(&self) -> Vec<(SerializationBatch, usize)> {
        // Drain the remaining batches
        let mut batches = vec![];

        // Acquire all the locks, in_guard first, out_guard later
        // Use the same locking order as in disable to avoid deadlocks
        let mut in_guards: Vec<MutexGuard<'_, StageIn>> =
            self.stage_in.iter().map(|x| zlock!(x)).collect();
        let mut out_guard = zlock!(self.stage_out);

        for priority in 0..out_guard.len() {
            // Drain first the batches in stage OUT
            if let Some(b) = out_guard[priority].try_pull() {
                batches.push((b, priority));
            }
            // Then try to drain what left in the stage IN
            if let Some(b) = in_guards[priority].try_pull() {
                batches.push((b, priority));
            }
        }

        batches
    }
}

impl fmt::Debug for TransmissionPipeline {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TransmissionPipeline")
            .field("queues", &self.stage_in.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::protocol::core::{Channel, CongestionControl, Priority, Reliability, ZInt};
    use crate::net::protocol::io::ZBuf;
    use crate::net::protocol::message::defaults::{BATCH_SIZE, SEQ_NUM_RES};
    use crate::net::protocol::message::{
        Fragment, Frame, TransportBody, TransportMessage, ZenohMessage,
    };
    use crate::net::transport::defaults::ZN_QUEUE_SIZE_CONTROL;
    use async_std::prelude::*;
    use async_std::task;
    use std::convert::TryFrom;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    const SLEEP: Duration = Duration::from_millis(100);
    const TIMEOUT: Duration = Duration::from_secs(60);

    #[test]
    fn tx_pipeline_flow() {
        fn schedule(queue: Arc<TransmissionPipeline>, num_msg: usize, payload_size: usize) {
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
            for _ in 0..num_msg {
                queue.push_zenoh_message(message.clone());
            }
        }

        async fn consume(queue: Arc<TransmissionPipeline>, num_msg: usize) {
            let mut batches: usize = 0;
            let mut bytes: usize = 0;
            let mut msgs: usize = 0;
            let mut fragments: usize = 0;

            while msgs != num_msg {
                let (batch, priority) = queue.pull().await.unwrap();
                batches += 1;
                bytes += batch.len();
                // Create a ZBuf for deserialization starting from the batch
                let mut zbuf: ZBuf = batch.get_serialized_messages().to_vec().into();
                // Deserialize the messages
                while let Some(msg) = TransportMessage::read(&mut zbuf) {
                    match msg.body {
                        TransportBody::Frame(Frame { payload, .. }) => {
                            msgs += payload.len();
                        }
                        TransportBody::Fragment(Fragment { has_more, .. }) => {
                            fragments += 1;
                            if !has_more {
                                msgs += 1;
                            }
                        }
                        _ => {
                            msgs += 1;
                        }
                    }
                }

                // Reinsert the batch
                queue.refill(batch, priority);
            }

            println!(
                "Pipeline Flow [<<<]: Received {} messages, {} bytes, {} batches, {} fragments",
                msgs, bytes, batches, fragments
            );
        }

        // Pipeline
        let batch_size = BATCH_SIZE;
        let is_streamed = true;
        let tct = TransportConduitTx::make(Priority::Control, SEQ_NUM_RES).unwrap();
        let conduit = vec![tct].into_boxed_slice();
        let queue = Arc::new(TransmissionPipeline::new(
            batch_size,
            is_streamed,
            conduit.into(),
        ));

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

                let c_queue = queue.clone();
                let t_c = task::spawn(async move {
                    consume(c_queue, num_msg).await;
                });

                let c_queue = queue.clone();
                let c_ps = *ps;
                let t_s = task::spawn(async move {
                    schedule(c_queue, num_msg, c_ps);
                });

                let res = t_c.join(t_s).timeout(TIMEOUT).await;
                assert!(res.is_ok());
            }
        });
    }

    #[test]
    fn tx_pipeline_blocking() {
        fn schedule(queue: Arc<TransmissionPipeline>, counter: Arc<AtomicUsize>, id: usize) {
            // Make sure to put only one message per batch: set the payload size
            // to half of the batch in such a way the serialized zenoh message
            // will be larger then half of the batch size (header + payload).
            let payload_size = (BATCH_SIZE / 2) as usize;

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
            let num_msg = 1 + *ZN_QUEUE_SIZE_CONTROL;
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
        let batch_size = BATCH_SIZE;
        let is_streamed = true;
        let tct = TransportConduitTx::make(Priority::Control, SEQ_NUM_RES).unwrap();
        let conduit = vec![tct].into_boxed_slice();
        let queue = Arc::new(TransmissionPipeline::new(
            batch_size,
            is_streamed,
            conduit.into(),
        ));

        let counter = Arc::new(AtomicUsize::new(0));

        let c_queue = queue.clone();
        let c_counter = counter.clone();
        let h1 = task::spawn_blocking(move || {
            schedule(c_queue, c_counter, 1);
        });

        let c_queue = queue.clone();
        let c_counter = counter.clone();
        let h2 = task::spawn_blocking(move || {
            schedule(c_queue, c_counter, 2);
        });

        task::block_on(async {
            // Wait to have sent enough messages and to have blocked
            println!(
                "Pipeline Blocking [---]: waiting to have {} messages being scheduled",
                *ZN_QUEUE_SIZE_CONTROL
            );
            let check = async {
                while counter.load(Ordering::Acquire) < *ZN_QUEUE_SIZE_CONTROL {
                    task::sleep(SLEEP).await;
                }
            };
            check.timeout(TIMEOUT).await.unwrap();

            // Disable and drain the queue
            task::spawn_blocking(move || {
                println!("Pipeline Blocking [---]: disabling the queue");
                queue.disable();
                println!("Pipeline Blocking [---]: draining the queue");
                let _ = queue.drain();
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
    fn rx_pipeline_blocking() {
        fn schedule(queue: Arc<TransmissionPipeline>, counter: Arc<AtomicUsize>) {
            // Make sure to put only one message per batch: set the payload size
            // to half of the batch in such a way the serialized zenoh message
            // will be larger then half of the batch size (header + payload).
            let payload_size = (BATCH_SIZE / 2) as usize;

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
            let num_msg = *ZN_QUEUE_SIZE_CONTROL;
            for i in 0..num_msg {
                println!(
                    "Pipeline Blocking [>>>]: Scheduling message #{} with payload size of {} bytes",
                    i, payload_size
                );
                queue.push_zenoh_message(message.clone());
                let c = counter.fetch_add(1, Ordering::AcqRel);
                println!(
                    "Pipeline Blocking [>>>]: Scheduled message #{} with payload size of {} bytes",
                    c, payload_size
                );
            }
        }

        // Queue
        let batch_size = BATCH_SIZE;
        let is_streamed = true;
        let tct = TransportConduitTx::make(Priority::Control, SEQ_NUM_RES).unwrap();
        let conduit = vec![tct].into_boxed_slice();
        let queue = Arc::new(TransmissionPipeline::new(
            batch_size,
            is_streamed,
            conduit.into(),
        ));

        let counter = Arc::new(AtomicUsize::new(0));

        let c_counter = counter.clone();
        let c_queue = queue.clone();
        schedule(c_queue, c_counter);

        let c_queue = queue.clone();
        let h1 = task::spawn(async move {
            loop {
                if c_queue.pull().await.is_none() {
                    println!("Pipeline Blocking [---]: pull unblocked");
                    break;
                }
            }
        });

        task::block_on(async {
            // Wait to have sent enough messages and to have blocked
            println!(
                "Pipeline Blocking [---]: waiting to have {} messages being scheduled",
                *ZN_QUEUE_SIZE_CONTROL
            );
            let check = async {
                while counter.load(Ordering::Acquire) < *ZN_QUEUE_SIZE_CONTROL {
                    task::sleep(SLEEP).await;
                }
            };
            check.timeout(TIMEOUT).await.unwrap();

            // Disable and drain the queue
            task::spawn_blocking(move || {
                println!("Pipeline Blocking [---]: disabling the queue");
                queue.disable();
                println!("Pipeline Blocking [---]: draining the queue");
                let _ = queue.drain();
            })
            .timeout(TIMEOUT)
            .await
            .unwrap();

            // Make sure that the tasks scheduling have been unblocked
            println!("Pipeline Blocking [---]: waiting for consume to be unblocked");
            h1.timeout(TIMEOUT).await.unwrap();
        });
    }

    #[test]
    #[ignore]
    fn tx_pipeline_thr() {
        // Queue
        let batch_size = BATCH_SIZE;
        let is_streamed = true;
        let tct = TransportConduitTx::make(Priority::Control, SEQ_NUM_RES).unwrap();
        let conduit = vec![tct].into_boxed_slice();
        let pipeline = Arc::new(TransmissionPipeline::new(
            batch_size,
            is_streamed,
            conduit.into(),
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
                    let key = "/pipeline/thr".into();
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
                        c_pipeline.push_zenoh_message(message.clone());
                    }
                }
            }
        });

        let c_pipeline = pipeline;
        let c_count = count.clone();
        task::spawn(async move {
            loop {
                let (batch, priority) = c_pipeline.pull().await.unwrap();
                c_count.fetch_add(batch.len(), Ordering::AcqRel);
                task::sleep(Duration::from_nanos(100)).await;
                c_pipeline.refill(batch, priority);
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
