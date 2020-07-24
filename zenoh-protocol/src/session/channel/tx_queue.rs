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
use async_std::sync::{Arc, Mutex};
use std::collections::VecDeque;

use super::SerializationBatch;

use crate::io::WBuf;
use crate::proto::{SeqNumGenerator, SessionMessage, ZenohMessage};
use crate::session::defaults::{
    // Constants
    QUEUE_NUM,
    QUEUE_PRIO_CTRL,
    QUEUE_PRIO_RETX,
    QUEUE_PRIO_DATA,
    // Configurable constants
    QUEUE_SIZE_CTRL,   
    QUEUE_SIZE_RETX,   
    QUEUE_SIZE_DATA,
    QUEUE_CONCURRENCY
};

use zenoh_util::zasynclock;
use zenoh_util::sync::Condition;


struct CircularBatchIn {
    priority: usize,
    batch_size: usize,
    inner: VecDeque<SerializationBatch>,
    sn_reliable: Arc<Mutex<SeqNumGenerator>>,
    sn_best_effort: Arc<Mutex<SeqNumGenerator>>,
    state_out: Arc<Mutex<Vec<CircularBatchOut>>>,
    state_empty: Arc<Mutex<CircularBatchEmpty>>,
    not_full: Arc<Condition>,
    not_empty: Arc<Condition>
}

macro_rules! zrefill {
    ($batch:expr) => {
        // Refill the batches
        let mut empty_guard = zasynclock!($batch.state_empty);
        if empty_guard.is_empty() {
            // Drop the guard and wait for the batches to be available
            $batch.not_full.wait(empty_guard).await;
            // We have been notified that there are batches available:
            // reacquire the lock on the state_empty
            empty_guard = zasynclock!($batch.state_empty);
        }
        // Drain all the empty batches
        while let Some(batch) = empty_guard.pull() {
            $batch.inner.push_back(batch);
        }
    };
}


impl CircularBatchIn {
    #[allow(clippy::too_many_arguments)]
    fn new(
        priority: usize,
        capacity: usize,
        batch_size: usize,
        is_streamed: bool,
        sn_reliable: Arc<Mutex<SeqNumGenerator>>,
        sn_best_effort: Arc<Mutex<SeqNumGenerator>>,
        state_out: Arc<Mutex<Vec<CircularBatchOut>>>,
        state_empty: Arc<Mutex<CircularBatchEmpty>>,
        not_full: Arc<Condition>,
        not_empty: Arc<Condition>
    ) -> CircularBatchIn {
        let mut inner = VecDeque::<SerializationBatch>::with_capacity(capacity);
        for _ in 0..capacity { 
            inner.push_back(SerializationBatch::new(
                batch_size, is_streamed, sn_reliable.clone(), sn_best_effort.clone()
            ));
        }

        CircularBatchIn {
            priority,
            batch_size,
            inner,
            sn_reliable,
            sn_best_effort,
            state_out,
            state_empty,
            not_full,
            not_empty
        }
    }    

    async fn try_serialize_session_message(&mut self, message: &SessionMessage) -> bool {
        loop {
            // Get the current serialization batch
            let batch = if let Some(batch) = self.inner.front_mut() {
                batch
            } else {
                // Refill the batches
                zrefill!(self);
                continue
            };

            // Try to serialize the message on the current batch
            return batch.serialize_session_message(&message).await
        }
    }
    
    async fn serialize_session_message(&mut self, message: SessionMessage) { 
        macro_rules! zserialize {
            ($message:expr) => {
                if self.try_serialize_session_message($message).await {
                    // Notify if needed
                    if self.not_empty.has_waiting_list() {
                        let guard = zasynclock!(self.state_out);
                        self.not_empty.notify(guard).await;
                    }
                    return
                }
            };
        }
        // Attempt the serialization on the current batch
        zserialize!(&message);

        // The first serialization attempt has failed. This means that the current
        // batch is full. Therefore:
        //   1) remove the current batch from the IN pipeline
        //   2) add the batch to the OUT pipeline        
        if let Some(batch) = self.pull() {
            // The previous batch wasn't empty
            let mut guard = zasynclock!(self.state_out);
            guard[self.priority].push(batch);
            // Notify if needed
            if self.not_empty.has_waiting_list() {
                self.not_empty.notify(guard).await;
            } else {
                drop(guard);
            }

            // Attempt the serialization on a new empty batch
            zserialize!(&message);
        }

        log::warn!("Session message dropped because it can not be fragmented: {:?}", message);
    }

    async fn fragment_zenoh_message(&mut self, message: &ZenohMessage) {
        // Create an expandable buffer and serialize the totality of the message
        let mut wbuf = WBuf::new(self.batch_size, false); 
        wbuf.write_zenoh_message(&message);

        // Acquire the lock on the SN generator to ensure that we have all 
        // sequential sequence numbers for the fragments
        let mut guard = if message.is_reliable() {
            zasynclock!(self.sn_reliable)
        } else {
            zasynclock!(self.sn_best_effort)
        };

        // Fragment the whole message
        let mut to_write = wbuf.len();
        while to_write > 0 {
            // Get the current serialization batch
            let batch = if let Some(batch) = self.inner.front_mut() {
                batch
            } else {
                // Refill the batches
                zrefill!(self);
                continue
            };

            // Get the frame SN
            let sn = guard.get();

            // Serialize the message
            let written = batch.serialize_zenoh_fragment(
                message.is_reliable(), sn, &mut wbuf, to_write
            ).await;

            // Update the amount of bytes left to write
            to_write -= written;

            // 0 bytes written means error
            if written != 0 {
                // Move the serialization batch into the OUT pipeline
                let batch = self.inner.pop_front().unwrap();
                let mut out_guard = zasynclock!(self.state_out);
                out_guard[self.priority].push(batch);
                // Notify if needed
                if self.not_empty.has_waiting_list() {
                    self.not_empty.notify(out_guard).await;
                }
            } else {
                // Reinsert the SN back to the pool
                guard.set(sn);
                log::warn!("Zenoh message dropped because it can not be fragmented: {:?}", message);
                break
            }
        }
    }

    async fn try_serialize_zenoh_message(&mut self, message: &ZenohMessage) -> bool {
        loop {
            // Get the current serialization batch
            let batch = if let Some(batch) = self.inner.front_mut() {
                batch
            } else {
                // Refill the batches
                zrefill!(self);
                continue
            };

            // Try to serialize the message on the current batch
            return batch.serialize_zenoh_message(&message).await
        }
    }

    async fn serialize_zenoh_message(&mut self, message: ZenohMessage) { 
        macro_rules! zserialize {
            ($message:expr) => {
                if self.try_serialize_zenoh_message(&message).await {
                    // Notify if needed
                    if self.not_empty.has_waiting_list() {
                        let guard = zasynclock!(self.state_out);
                        self.not_empty.notify(guard).await;
                    }
                    return
                }
            };
        }

        // Attempt the serialization on the current batch
        zserialize!(&message);

        // The first serialization attempt has failed. This means that the current
        // batch is either full or the message is too large. In case of the former,
        // try to do:
        //   1) remove the current batch from the IN pipeline
        //   2) add the batch to the OUT pipeline        
        if let Some(batch) = self.pull() {
            // The previous batch wasn't empty, move it to the state OUT pipeline
            let mut guard = zasynclock!(self.state_out);
            guard[self.priority].push(batch);
            // Notify if needed
            if self.not_empty.has_waiting_list() {
                self.not_empty.notify(guard).await;
            } else {
                drop(guard);
            }

            // Attempt the serialization on a new empty batch
            zserialize!(&message);
        }

        // The second serialization attempt has failed. This means that the message is 
        // too large for the current batch size: we need to fragment.
        self.fragment_zenoh_message(&message).await;
    }

    fn pull(&mut self) -> Option<SerializationBatch> {
        if let Some(batch) = self.inner.front() {
            if !batch.is_empty() {
                // There is an incomplete batch, pop it
                return self.inner.pop_front()
            } 
        }
        None
    }
}

struct CircularBatchOut {
    inner: VecDeque<SerializationBatch>,
    state_in: Option<Arc<Mutex<CircularBatchIn>>>
}

impl CircularBatchOut {
    fn new(capacity: usize) -> CircularBatchOut {
        CircularBatchOut {
            inner: VecDeque::<SerializationBatch>::with_capacity(capacity),
            state_in: None
        }
    }
    
    fn initialize(&mut self, state_in: Arc<Mutex<CircularBatchIn>>) {
        self.state_in = Some(state_in);
    }

    #[inline]
    fn push(&mut self, batch: SerializationBatch) {        
        self.inner.push_back(batch);        
    }

    fn pull(&mut self) -> Option<SerializationBatch> {
        if let Some(mut batch) = self.inner.pop_front() {
            batch.write_len();
            return Some(batch)
        } else {
            // Check if an incomplete (non-empty) batch is available in the state IN pipeline.            
            if let Some(mut guard) = self.state_in.as_ref().unwrap().try_lock() {
                if let Some(mut batch) = guard.pull() {
                    batch.write_len();
                    // Send the incomplete batch
                    return Some(batch)
                }
            }
        }     
        None
    }
}

struct CircularBatchEmpty {
    inner: VecDeque<SerializationBatch>,
}

impl CircularBatchEmpty {
    fn new(capacity: usize) -> CircularBatchEmpty {
        CircularBatchEmpty {            
            inner: VecDeque::<SerializationBatch>::with_capacity(capacity)
        }
    }

    #[inline]
    fn push(&mut self, mut batch: SerializationBatch) {
        batch.clear();
        self.inner.push_back(batch);
    }

    #[inline]
    fn pull(&mut self) -> Option<SerializationBatch> {
        self.inner.pop_front()
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

/// Link queue
pub struct TransmissionQueue {
    // Each priority queue has its own Mutex
    state_in: Vec<Arc<Mutex<CircularBatchIn>>>,
    // Each priority queue has its own Mutex
    state_empty: Vec<Arc<Mutex<CircularBatchEmpty>>>,
    // A single Mutex for all the priority queues
    state_out: Arc<Mutex<Vec<CircularBatchOut>>>,
    // Each priority queue has its own Conditional variable
    // The conditional variable requires a MutexGuard from state_empty
    not_full: Vec<Arc<Condition>>,
    // A signle confitional variable for all the priority queues
    // The conditional variable requires a MutexGuard from state_out
    not_empty: Arc<Condition>
}

impl TransmissionQueue {
    /// Create a new link queue.
    pub fn new(
        batch_size: usize,
        is_streamed: bool,
        sn_reliable: Arc<Mutex<SeqNumGenerator>>,
        sn_best_effort: Arc<Mutex<SeqNumGenerator>>
    ) -> TransmissionQueue {
        // Conditional variables        
        let not_full = vec![Arc::new(Condition::new(*QUEUE_CONCURRENCY)); QUEUE_NUM];
        let not_empty = Arc::new(Condition::new(*QUEUE_CONCURRENCY));

        // Build the state EMPTY
        let mut state_empty = Vec::with_capacity(QUEUE_NUM);
        state_empty.push(Arc::new(Mutex::new(CircularBatchEmpty::new(*QUEUE_SIZE_CTRL))));
        state_empty.push(Arc::new(Mutex::new(CircularBatchEmpty::new(*QUEUE_SIZE_RETX))));
        state_empty.push(Arc::new(Mutex::new(CircularBatchEmpty::new(*QUEUE_SIZE_DATA))));

        // Build the state OUT
        let mut state_out = Vec::with_capacity(QUEUE_NUM);
        state_out.push(CircularBatchOut::new(*QUEUE_SIZE_CTRL));
        state_out.push(CircularBatchOut::new(*QUEUE_SIZE_RETX));
        state_out.push(CircularBatchOut::new(*QUEUE_SIZE_DATA));
        let state_out = Arc::new(Mutex::new(state_out));

        // Build the state IN
        let mut state_in = Vec::with_capacity(QUEUE_NUM);
        state_in.push(Arc::new(Mutex::new(CircularBatchIn::new(
            QUEUE_PRIO_CTRL, *QUEUE_SIZE_CTRL, batch_size, is_streamed, sn_reliable.clone(),
            sn_best_effort.clone(), state_out.clone(), state_empty[QUEUE_PRIO_CTRL].clone(), 
            not_full[QUEUE_PRIO_CTRL].clone(), not_empty.clone()
        )))); 
        state_in.push(Arc::new(Mutex::new(CircularBatchIn::new(
            QUEUE_PRIO_RETX, *QUEUE_SIZE_RETX, batch_size, is_streamed, sn_reliable.clone(), 
            sn_best_effort.clone(), state_out.clone(), state_empty[QUEUE_PRIO_RETX].clone(), 
            not_full[QUEUE_PRIO_RETX].clone(), not_empty.clone()
        ))));
        state_in.push(Arc::new(Mutex::new(CircularBatchIn::new(
            QUEUE_PRIO_DATA, *QUEUE_SIZE_DATA, batch_size, is_streamed, sn_reliable, 
            sn_best_effort, state_out.clone(), state_empty[QUEUE_PRIO_DATA].clone(), 
            not_full[QUEUE_PRIO_DATA].clone(), not_empty.clone()
        ))));

        // Initialize the state OUT
        let mut guard = state_out.try_lock().unwrap();
        guard[QUEUE_PRIO_CTRL].initialize(state_in[QUEUE_PRIO_CTRL].clone());
        guard[QUEUE_PRIO_RETX].initialize(state_in[QUEUE_PRIO_RETX].clone());
        guard[QUEUE_PRIO_DATA].initialize(state_in[QUEUE_PRIO_DATA].clone());
        drop(guard);
         
        TransmissionQueue { 
            state_in,
            state_out,
            state_empty,            
            not_full,
            not_empty
        }
    }

    pub(super) async fn push_session_message(&self, message: SessionMessage, priority: usize) {
        zasynclock!(self.state_in[priority]).serialize_session_message(message).await;
    }

    pub(super) async fn push_zenoh_message(&self, message: ZenohMessage, priority: usize) {
        zasynclock!(self.state_in[priority]).serialize_zenoh_message(message).await;
    }

    pub(super) async fn push_serialization_batch(&self, batch: SerializationBatch, priority: usize) {
        let mut guard = zasynclock!(self.state_empty[priority]);
        guard.push(batch);
        if self.not_full[priority].has_waiting_list() {
            self.not_full[priority].notify(guard).await;
        }
    }

    pub(super) async fn pull(&self) -> (SerializationBatch, usize) {
        loop {
            let mut guard = zasynclock!(self.state_out);
            for priority in 0usize..guard.len() {
                if let Some(batch) = guard[priority].pull() {
                    return (batch, priority)
                }
            }
            self.not_empty.wait(guard).await;
        }
    }

    pub(super) async fn drain(&self) -> Option<SerializationBatch> {
        // First try to drain the state OUT pipeline
        let mut guard = zasynclock!(self.state_out);
        for priority in 0usize..guard.len() {
            if let Some(batch) = guard[priority].pull() {
                return Some(batch)
            }
        }
        drop(guard);
        
        // Then try to drain what left in the state IN pipeline
        for priority in 0usize..self.state_in.len() {
            let mut guard = zasynclock!(self.state_in[priority]);
            if let Some(batch) = guard.pull() {
                return Some(batch)
            }
        }        

        None
    }
}

#[cfg(test)]
mod tests {
    use async_std::prelude::*;
    use async_std::sync::{Arc, Barrier, Mutex};
    use async_std::task;
    use std::convert::TryFrom;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use crate::core::{ResKey, ZInt};
    use crate::io::RBuf;
    use crate::proto::{FramePayload, SeqNumGenerator, SessionBody, Frame, ZenohMessage};
    use crate::session::defaults::{QUEUE_PRIO_DATA, SESSION_BATCH_SIZE, SESSION_SEQ_NUM_RESOLUTION};

    use super::*;


    const TIMEOUT: Duration = Duration::from_secs(60);

    #[test]
    fn tx_queue() {        
        async fn schedule(
            num_msg: usize,
            payload_size: usize,
            queue: &Arc<TransmissionQueue>
        ) {
            // Send reliable messages
            let reliable = true;
            let key = ResKey::RName("test".to_string());
            let info = None;
            let payload = RBuf::from(vec![0u8; payload_size]);
            let reply_context = None;
            let attachment = None;

            let message = ZenohMessage::make_data(
                reliable, key, info, payload, reply_context, attachment
            );
            
            println!(">>> Sending {} messages with payload size: {}", num_msg, payload_size);
            for _ in 0..num_msg {
                queue.push_zenoh_message(message.clone(), QUEUE_PRIO_DATA).await;
            }            
        }

        async fn consume(
            queue: Arc<TransmissionQueue>,                     
            c_threshold: Arc<AtomicUsize>,
            c_barrier: Arc<Barrier>
        ) {
            let mut batches: usize = 0;
            let mut bytes: usize = 0;
            let mut msgs: usize = 0;            
            let mut fragments: usize = 0;

            loop {
                let (batch, priority) = queue.pull().await;   
                batches += 1;
                bytes += batch.len();           
                // Create a RBuf for deserialization starting from the batch
                let mut rbuf: RBuf = batch.get_serialized_messages().into();
                // Deserialize the messages
                while let Ok(msg) = rbuf.read_session_message() {
                    match msg.body {
                        SessionBody::Frame(Frame { payload, .. }) => match payload {
                            FramePayload::Messages { messages } => {
                                msgs += messages.len();
                            },
                            FramePayload::Fragment { is_final, .. } => {
                                fragments += 1;
                                if is_final {
                                    msgs += 1;
                                }
                            }
                        },
                        _ => { msgs += 1; }
                    }
                    // Synchronize for this test
                    if msgs == c_threshold.load(Ordering::SeqCst) {
                        println!("    Received {} messages, {} bytes, {} batches, {} fragments", msgs, bytes, batches, fragments);
                        let res = c_barrier.wait().timeout(TIMEOUT).await;
                        assert!(res.is_ok());
                        // Reset counters
                        batches = 0;
                        bytes = 0;
                        msgs = 0;            
                        fragments = 0;
                    }   
                }
                // Reinsert the batch
                queue.push_serialization_batch(batch, priority).await;
            }     
        }

        // Queue
        let batch_size = *SESSION_BATCH_SIZE;
        let is_streamed = true;
        let sn_reliable = Arc::new(Mutex::new(
            SeqNumGenerator::new(0, *SESSION_SEQ_NUM_RESOLUTION)
        ));
        let sn_best_effort = Arc::new(Mutex::new(
            SeqNumGenerator::new(0, *SESSION_SEQ_NUM_RESOLUTION)
        ));
        let queue = Arc::new(TransmissionQueue::new(
            batch_size, is_streamed, sn_reliable, sn_best_effort
        ));       

        // Synch variables
        let barrier = Arc::new(Barrier::new(2));
        let threshold = Arc::new(AtomicUsize::new(0));

        // Consume task
        let c_queue = queue.clone();
        let c_barrier = barrier.clone();
        let c_threshold = threshold.clone();
        task::spawn(async move {
            consume(c_queue, c_threshold, c_barrier).await;
        });
                
        // Total amount of bytes to send in each test
        let bytes: usize = 100_000_000;
        let max_msgs: usize = 1_000;
        // Paylod size of the messages
        let payload_sizes = [8, 64, 512, 4_096, 8_192, 32_768, 262_144, 2_097_152];
        
        task::block_on(async {
            for ps in payload_sizes.iter() {
                if ZInt::try_from(*ps).is_err() {
                    break
                }
                
                let num_msg = max_msgs.min(bytes / ps);
                threshold.store(num_msg, Ordering::SeqCst);
                
                schedule(num_msg, *ps, &queue).await;

                let res = barrier.wait().timeout(TIMEOUT).await;
                assert!(res.is_ok()); 
            }    
        }); 
    }
}