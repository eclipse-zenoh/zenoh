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
use super::pipeline::TransmissionPipelineConf;
use super::protocol::core::Priority;
use super::protocol::io::WBuf;
use super::protocol::proto::{TransportMessage, ZenohMessage};
use backoff::ExponentialBackoff;
use crossbeam::queue::ArrayQueue;
use futures::future;
use futures::future::FutureExt;
use itertools::izip;
use owning_ref::ArcRef;
use std::mem;
use std::ops::{Deref, DerefMut};
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::sync::{Arc, Weak};
use std::task::Poll;
use tokio::sync::Notify;
use zenoh_core::zlock;
use zenoh_protocol::proto::MessageWriter;

pub struct TransmissionPipeline {
    inner: Option<Inner>,
}

impl TransmissionPipeline {
    /// Create a new link queue.
    pub(crate) fn new(
        config: TransmissionPipelineConf,
        conduit: Arc<[TransportConduitTx]>,
    ) -> Self {
        Self {
            inner: Some(Inner::new(config, conduit)),
        }
    }

    pub(crate) fn push_transport_message(&self, message: TransportMessage, priority: Priority) {
        self.inner().push_transport_message(message, priority)
    }

    pub(crate) fn push_zenoh_message(&self, message: ZenohMessage) -> bool {
        self.inner().push_zenoh_message(message)
    }

    pub(crate) async fn pull(&self) -> (SerializationBatchGuard, usize) {
        self.inner().pull().await
    }

    pub(crate) fn take(self) -> Vec<(SerializationBatch, usize)> {
        let inner: Inner = self.inner.unwrap();

        inner
            .take()
            .into_iter()
            .enumerate()
            .flat_map(|(priority, conduit)| {
                let Conduit {
                    stage_in,
                    stage_out,
                    ..
                } = conduit;
                let mut stage_in = stage_in.lock().unwrap();

                stage_out
                    .take()
                    .chain(stage_in.take())
                    .map(move |batch| (batch, priority))
            })
            .collect()
    }

    fn inner(&self) -> &Inner {
        self.inner.as_ref().unwrap()
    }
}

use inner::*;
mod inner {

    use super::*;

    pub(super) struct Inner {
        conduits: Vec<Conduit>,
    }

    impl Inner {
        /// Create a new link queue.
        pub(super) fn new(
            config: TransmissionPipelineConf,
            conduit: Arc<[TransportConduitTx]>,
        ) -> Self {
            let conduits: Vec<_> = izip!(0..conduit.len(), config.queue_size)
                .map(|(index, queue_size)| {
                    let conduit = ArcRef::new(conduit.clone()).map(|c| &c[index]);
                    let batch = SerializationBatch::new(config.batch_size, config.is_streamed);

                    Conduit {
                        conduit,
                        queue_size,
                        stage_in: Mutex::new(StageIn {
                            batch: Some(batch),
                            is_written: false,
                            fragbuf: WBuf::new(config.batch_size as usize, false),
                        }),
                        stage_out: StageOut::new(queue_size),
                        stage_refill: Arc::new(StageRefill::new(
                            queue_size - 1,
                            config.batch_size,
                            config.is_streamed,
                        )),
                    }
                })
                .collect();

            Self { conduits }
        }

        pub(super) fn push_transport_message(&self, message: TransportMessage, priority: Priority) {
            let priority = if self.is_qos() { priority as usize } else { 0 };
            self.conduits[priority]
                .lock_stage_in()
                .push_transport_message(message)
        }

        pub(super) fn push_zenoh_message(&self, mut message: ZenohMessage) -> bool {
            // get priority and queue for this message
            let queue = if self.is_qos() {
                message.channel.priority as usize
            } else {
                let priority = Priority::default();
                message.channel.priority = Priority::default();
                0
            };

            self.conduits[queue]
                .lock_stage_in()
                .push_zenoh_message(message)
        }

        pub(super) async fn pull(&self) -> (SerializationBatchGuard, usize) {
            let mut futures: Vec<_> = self
                .conduits
                .iter()
                .enumerate()
                .map(|(priority, conduit)| {
                    conduit.pull().map(move |batch| (batch, priority)).boxed()
                })
                .collect();

            future::poll_fn(|ctx| {
                for fut in &mut futures {
                    if let Poll::Ready(output) = fut.poll_unpin(ctx) {
                        return Poll::Ready(output);
                    }
                }

                Poll::Pending
            })
            .await
        }

        pub(super) fn is_qos(&self) -> bool {
            self.conduits.len() > 1
        }

        pub(super) fn take(self) -> Vec<Conduit> {
            self.conduits
        }
    }
}

use conduit::*;
mod conduit {
    use super::*;

    pub(super) struct Conduit {
        pub(super) conduit: ArcRef<[TransportConduitTx], TransportConduitTx>,
        pub(super) queue_size: usize,
        pub(super) stage_in: Mutex<StageIn>,
        pub(super) stage_out: StageOut,
        pub(super) stage_refill: Arc<StageRefill>,
    }

    impl Conduit {
        pub(super) async fn pull(&self) -> SerializationBatchGuard {
            let batch = loop {
                // 1st try to take a batch from stage_out
                if let Some(batch) = self.stage_out.try_pop() {
                    break batch;
                };

                // this block wraps the lifetime of MutexGuard
                {
                    // If failed, lock stage_in
                    let mut stage_in = self.stage_in.lock().unwrap();

                    // A new batch can be concurrently pushed to stage_out in the mean time,
                    // so check stage_out again
                    if let Some(batch) = self.stage_out.try_pop() {
                        break batch;
                    };

                    // Reach here if stage_out has no more batches.
                    // Take a batch from stage_in instead.

                    // If stage_in batch is never written, return nothing
                    if stage_in.is_written {
                        let batch = stage_in.batch.take().unwrap();
                        stage_in.is_written = false;
                        break batch;
                    }
                }

                self.stage_out.notified().await;
            };

            SerializationBatchGuard {
                batch: Some(batch),
                stage_refill: Arc::downgrade(&self.stage_refill.clone()),
            }
        }

        pub(super) fn lock_stage_in(&self) -> ConduitGuard<'_> {
            ConduitGuard::new(self)
        }
    }
}

use conduit_guard::*;
mod conduit_guard {
    use super::*;

    pub(super) struct ConduitGuard<'a> {
        conduit: &'a TransportConduitTx,
        queue_size: usize,
        stage_refill: &'a StageRefill,
        stage_in: MutexGuard<'a, StageIn>,
        stage_out: &'a StageOut,
    }

    impl<'a> ConduitGuard<'a> {
        pub(super) fn new(conduit: &'a Conduit) -> Self {
            Self {
                conduit: &conduit.conduit,
                queue_size: conduit.queue_size,
                stage_refill: &conduit.stage_refill,
                stage_in: conduit.stage_in.lock().unwrap(),
                stage_out: &conduit.stage_out,
            }
        }

        fn shift_stage_in(&mut self, fallible: bool) -> Option<&mut SerializationBatch> {
            let Self {
                stage_refill,
                stage_in,
                stage_out,
                ..
            } = self;

            if !stage_in.is_written {
                debug_assert!(stage_in.batch.is_some());
                return stage_in.batch.as_mut();
            }

            let mut new = stage_refill.pop(fallible)?;
            new.clear();

            let orig = mem::replace(&mut stage_in.batch, Some(new));
            stage_in.is_written = false;

            if let Some(orig) = orig {
                stage_out.try_push(orig).unwrap();
            }

            stage_in.batch.as_mut()
        }

        fn fill_and_get_stage_in(&mut self, fallible: bool) -> Option<&mut SerializationBatch> {
            let Self {
                stage_refill,
                stage_in,
                ..
            } = self;

            if stage_in.batch.is_none() {
                let new_batch = match stage_refill.pop(fallible) {
                    Some(batch) => batch,
                    None => return None,
                };

                stage_in.batch = Some(new_batch);
                stage_in.is_written = false;
            }

            stage_in.batch.as_mut()
        }

        fn try_write_batch<F>(&mut self, mut f: F, fallible: bool) -> bool
        where
            F: FnMut(&mut SerializationBatch) -> bool,
        {
            let batch = match self.fill_and_get_stage_in(fallible) {
                Some(batch) => batch,
                None => return false,
            };

            // first try
            let ok = (f)(batch);
            if ok {
                self.stage_in.is_written = true;
                return true;
            }

            // if failed, rotate the batch
            let batch = match self.shift_stage_in(fallible) {
                Some(batch) => batch,
                None => return false,
            };

            // second try, must suceed
            let ok = (f)(batch);
            debug_assert!(ok);
            self.stage_in.is_written = true;

            true
        }

        fn fragment_zenoh_message(
            &mut self,
            mut message: ZenohMessage,
            channel: &mut TransportChannelTx,
        ) -> bool {
            if self.fill_and_get_stage_in(false).is_none() {
                return false;
            };

            let Self {
                stage_refill,
                stage_in,
                stage_out,
                ..
            } = self;
            let StageIn {
                batch: curr_batch,
                is_written,
                fragbuf,
            } = &mut **stage_in;

            let curr_batch = curr_batch.as_mut().unwrap();
            let mut out_batches = vec![];

            // Take the expandable buffer and serialize the totality of the message
            fragbuf.clear();
            fragbuf.write_zenoh_message(&mut message);

            // Fragment the whole message
            let mut remaining_len = fragbuf.len();
            let mut fragbuf_reader = fragbuf.reader();

            // Get the current serialization batch
            // Treat all messages as non-droppable once we start fragmenting
            let success = loop {
                if remaining_len == 0 {
                    break true;
                }

                // Serialize the message
                let written_len = curr_batch.serialize_zenoh_fragment(
                    message.channel.reliability,
                    message.channel.priority,
                    &mut channel.sn,
                    &mut fragbuf_reader,
                    remaining_len,
                );

                // 0 bytes written means error
                if written_len > 0 {
                    // Update the amount of bytes left to write
                    remaining_len -= written_len;

                    // bail out if # of written batches reaches the queue_size limit
                    if out_batches.len() == self.queue_size - 1 {
                        break false;
                    }

                    // Move the serialization batch into the OUT pipeline
                    let new = stage_refill.pop(false).unwrap();
                    let orig = mem::replace(curr_batch, new);
                    *is_written = false;
                    out_batches.push(orig);
                } else {
                    log::warn!(
                        "Zenoh message dropped because it can not be fragmented: {:?}",
                        message
                    );
                    break false;
                }
            };

            if success {
                out_batches.into_iter().for_each(|batch| {
                    stage_out.try_push(batch).unwrap();
                });
            } else {
                out_batches.into_iter().for_each(|batch| {
                    stage_refill.try_push(batch).unwrap();
                });

                curr_batch.clear();
                *is_written = false;
            }

            success
        }

        pub(super) fn push_transport_message(&mut self, mut message: TransportMessage) {
            let ok = self.try_write_batch(
                |batch| batch.serialize_transport_message(&mut message),
                false,
            );
            debug_assert!(ok);
        }

        pub(super) fn push_zenoh_message(&mut self, mut message: ZenohMessage) -> bool {
            // get conduit of specified priority
            let mut channel = if message.is_reliable() {
                zlock!(self.conduit.reliable)
            } else {
                zlock!(self.conduit.best_effort)
            };

            // try to write data
            let fallible = message.is_droppable();
            let priority = message.channel.priority;

            let ok = self.try_write_batch(
                |batch| batch.serialize_zenoh_message(&mut message, priority, &mut channel.sn),
                fallible,
            );

            if ok {
                true
            } else if !fallible {
                self.fragment_zenoh_message(message, &mut channel)
            } else {
                false
            }
        }
    }
}

use stage_in::*;
mod stage_in {
    use super::*;

    pub(super) struct StageIn {
        pub(super) batch: Option<SerializationBatch>,
        pub(super) is_written: bool,
        pub(super) fragbuf: WBuf,
    }

    impl StageIn {
        pub(super) fn take(&mut self) -> Option<SerializationBatch> {
            self.is_written.then(|| {
                self.is_written = false;
                self.batch.take().unwrap()
            })
        }
    }
}

use stage_out::*;
mod stage_out {
    use std::iter;

    use super::*;

    pub(super) struct StageOut {
        queue: ArrayQueue<SerializationBatch>,
        notify: Notify,
    }

    impl StageOut {
        pub(super) fn new(queue_size: usize) -> Self {
            Self {
                queue: ArrayQueue::new(queue_size),
                notify: Notify::new(),
            }
        }

        pub(super) fn try_pop(&self) -> Option<SerializationBatch> {
            self.queue.pop()
        }

        pub(super) fn try_push(&self, batch: SerializationBatch) -> Result<(), SerializationBatch> {
            self.queue.push(batch)?;
            self.notify.notify_one();
            Ok(())
        }

        pub(super) async fn pop(&self) -> SerializationBatch {
            loop {
                if let Some(batch) = self.try_pop() {
                    break batch;
                }
                self.notify.notified().await;
            }
        }

        pub(super) async fn notified(&self) {
            self.notify.notified().await;
        }

        pub(super) fn take(self) -> impl Iterator<Item = SerializationBatch> {
            iter::from_fn(move || self.queue.pop())
        }
    }
}

use stage_refill::*;
mod stage_refill {

    use super::*;

    pub(super) struct StageRefill {
        queue: ArrayQueue<SerializationBatch>,
        backoff: ExponentialBackoff,
    }

    impl StageRefill {
        pub(super) fn new(queue_size: usize, batch_size: u16, is_streamed: bool) -> Self {
            let queue = {
                let queue = ArrayQueue::new(queue_size);
                (0..(queue_size)).for_each(|_| {
                    queue
                        .push(SerializationBatch::new(batch_size, is_streamed))
                        .unwrap();
                });
                queue
            };

            Self {
                queue,
                backoff: ExponentialBackoff::default(), // TODO
            }
        }

        pub(super) fn try_push(&self, batch: SerializationBatch) -> Result<(), SerializationBatch> {
            self.queue.push(batch)?;
            Ok(())
        }

        pub(super) fn pop(&self, fallible: bool) -> Option<SerializationBatch> {
            use backoff::Error as E;

            backoff::retry(self.backoff.clone(), || {
                if let Some(batch) = self.queue.pop() {
                    Ok(batch)
                } else if fallible {
                    Err(E::Permanent(()))
                } else {
                    Err(E::Transient {
                        err: (),
                        retry_after: None,
                    })
                }
            })
            .ok()
        }
    }
}

use serialization_batch_guard::*;
mod serialization_batch_guard {
    use super::*;

    pub(crate) struct SerializationBatchGuard {
        pub(super) batch: Option<SerializationBatch>,
        pub(super) stage_refill: Weak<StageRefill>,
    }

    impl Deref for SerializationBatchGuard {
        type Target = SerializationBatch;

        fn deref(&self) -> &Self::Target {
            self.batch.as_ref().unwrap()
        }
    }

    impl DerefMut for SerializationBatchGuard {
        fn deref_mut(&mut self) -> &mut Self::Target {
            self.batch.as_mut().unwrap()
        }
    }

    impl Drop for SerializationBatchGuard {
        fn drop(&mut self) {
            if let Some(stage_refill) = self.stage_refill.upgrade() {
                let batch = self.batch.take().unwrap();
                stage_refill.try_push(batch).unwrap();
            }
        }
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
        is_streamed: false,
        batch_size: u16::MAX,
        queue_size: [1; Priority::NUM],
        backoff: Duration::from_nanos(100),
    };

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
            for i in 0..num_msg {
                println!("Pipeline Flow [>>>]: Pushed {} msgs", i + 1);
                queue.push_zenoh_message(message.clone());
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
                // Drop to return the batch to pipeline
                drop(batch);
            }

            println!(
                "Pipeline Flow [<<<]: Received {} messages, {} bytes, {} batches, {} fragments",
                msgs, bytes, batches, fragments
            );
        }

        // Pipeline
        let tct = TransportConduitTx::make(SEQ_NUM_RES).unwrap();
        let conduit = vec![tct].into_boxed_slice();
        let queue = Arc::new(TransmissionPipeline::new(
            TransmissionPipelineConf::default(),
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
        let conduit = vec![tct].into_boxed_slice();
        let queue = Arc::new(TransmissionPipeline::new(
            TransmissionPipelineConf::default(),
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
                println!("Pipeline Blocking [---]: disable and drain the queue");
                // TODO: take the owner from Arc
                let _ = queue.take();
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
        fn schedule(queue: &TransmissionPipeline, counter: Arc<AtomicUsize>) {
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
            let num_msg = CONFIG.queue_size[Priority::MAX as usize];
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
        let tct = TransportConduitTx::make(SEQ_NUM_RES).unwrap();
        let conduit = vec![tct].into_boxed_slice();
        let queue = Arc::new(TransmissionPipeline::new(CONFIG, conduit.into()));

        let counter = Arc::new(AtomicUsize::new(0));

        let c_counter = counter.clone();
        schedule(&queue, c_counter);

        let c_queue = queue.clone();
        let h1 = task::spawn(async move {
            loop {
                c_queue.pull().await;
            }
        });

        task::block_on(async {
            // Wait to have sent enough messages and to have blocked
            println!(
                "Pipeline Blocking [---]: waiting to have {} messages being scheduled",
                CONFIG.queue_size[Priority::MAX as usize]
            );

            async {
                while counter.load(Ordering::Acquire) < CONFIG.queue_size[Priority::MAX as usize] {
                    task::sleep(SLEEP).await;
                }
            }
            .timeout(TIMEOUT)
            .await
            .unwrap();

            // Disable and drain the queue
            task::spawn_blocking(move || {
                println!("Pipeline Blocking [---]: disable and drain the queue");
                // TODO: take the owner from Arc
                let _ = queue.take();
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
        let tct = TransportConduitTx::make(SEQ_NUM_RES).unwrap();
        let conduit = vec![tct].into_boxed_slice();
        let pipeline = Arc::new(TransmissionPipeline::new(CONFIG, conduit.into()));
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
                let (batch, priority) = c_pipeline.pull().await;
                c_count.fetch_add(batch.len(), Ordering::AcqRel);
                task::sleep(Duration::from_nanos(100)).await;
                drop(batch);
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
