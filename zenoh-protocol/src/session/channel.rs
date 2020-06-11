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
use async_std::sync::{Arc, Barrier, Mutex, RwLock, Weak};
use async_std::task;
use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::iter::FromIterator;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::core::{PeerId, ZInt};
use crate::io::WBuf;
use crate::link::Link;
use crate::proto::{FramePayload, SessionBody, SessionMessage, SeqNum, SeqNumGenerator, WhatAmI, ZenohMessage, smsg};
use crate::session::{Action, MsgHandler, SessionManagerInner, TransportTrait};
use crate::session::defaults::{
    // Control buffer
    QUEUE_PRIO_CTRL,
    QUEUE_SIZE_CTRL,
    QUEUE_CRED_CTRL,
    // Retransmission buffer
    // QUEUE_PRIO_RETX,
    QUEUE_SIZE_RETX,
    QUEUE_CRED_RETX,
    // Data buffer
    QUEUE_PRIO_DATA,
    QUEUE_SIZE_DATA,
    QUEUE_CRED_DATA,
    // Queue size
    QUEUE_SIZE_TOT,
    QUEUE_CONCURRENCY,
    // Default slice size when serializing a message that needs to be fragmented
    // WRITE_MSG_SLICE_SIZE
};

use zenoh_util::{zasynclock, zasyncread, zasyncwrite, zerror};
use zenoh_util::collections::{CreditBuffer, CreditQueue, Timed, TimedEvent, TimedHandle, Timer};
use zenoh_util::collections::credit_queue::Drain as CreditQueueDrain;
use zenoh_util::core::{ZResult, ZError, ZErrorKind};



#[derive(Clone, Debug)]
enum MessageInner {
    Session(SessionMessage),
    Zenoh(ZenohMessage),
    Stop
}

#[derive(Clone, Debug)]
struct MessageTx {
    // The inner message to transmit
    inner: MessageInner,
    // The preferred link to transmit the Message on
    link: Option<Link>
}

/*************************************/
/*           CHANNEL TASK            */
/*************************************/

// Always send on the first link for the time being
const DEFAULT_LINK_INDEX: usize = 0; 
const TWO_BYTES: [u8; 2] = [0u8, 0u8];

#[derive(Debug)]
struct SerializedBatch {
    // The buffer to perform the batching on
    buffer: WBuf,
    // The link this batch is associated to
    link: Link
}

impl SerializedBatch {
    fn new(link: Link, size: usize) -> SerializedBatch {        
        // Create the buffer
        let mut buffer = WBuf::new(size, true);
        // Reserve two bytes if the link is streamed
        if link.is_streamed() {
            buffer.write_bytes(&TWO_BYTES);
        }

        SerializedBatch {
            buffer,
            link
        }
    }

    fn clear(&mut self) {
        self.buffer.clear();
        if self.link.is_streamed() {
            self.buffer.write_bytes(&TWO_BYTES);
        }
    }

    async fn transmit(&mut self) -> ZResult<()> {
        let mut length: u16 = self.buffer.len() as u16;
        if self.link.is_streamed() {
            // Remove from the total the 16 bits used for the length
            length -= 2;            
            // Write the length on the first 16 bits
            let bits = self.buffer.get_first_slice_mut(..2);
            bits.copy_from_slice(&length.to_le_bytes());
        }

        if length > 0 {
            let res = self.link.send(self.buffer.get_first_slice(..)).await;
            self.clear();            
            return res
        }

        Ok(())
    }
}

fn map_messages_on_links(    
    drain: &mut CreditQueueDrain<'_, MessageTx>,
    batches: &[SerializedBatch],
    messages: &mut Vec<Vec<MessageInner>>
) {    
    // Drain all the messages from the queue and map them on the links
    for msg in drain {   
        log::trace!("Scheduling: {:?}", msg.inner);   
        // Find the right index for the message
        let index = if let Some(link) = &msg.link {
            // Check if the target link exists, otherwise drop it            
            if let Some(index) = batches.iter().position(|x| &x.link == link) {
                index
            } else {
                log::debug!("Message dropped because link {} does not exist: {:?}", link, msg.inner);
                // Silently drop the message            
                continue
            }
        } else {
            DEFAULT_LINK_INDEX
        };
        // Add the message to the right link
        messages[index].push(msg.inner);
    }
}

async fn batch_fragment_transmit(
    inner: &mut ChannelInnerTx,
    messages: &mut Vec<MessageInner>,
    batch: &mut SerializedBatch
) -> bool {  
    enum CurrentFrame {
        Reliable,
        BestEffort,
        None
    }
    
    let mut current_frame = CurrentFrame::None;
    
    // Process all the messages
    for msg in messages.drain(..) {
        log::trace!("Serializing: {:?}", msg);

        let mut has_failed = false;
        let mut current_sn = None;
        let mut is_first = true;

        // The loop works as follows:
        // - first iteration tries to serialize the message on the current batch;
        // - second iteration tries to serialize the message on a new batch if the first iteration failed
        // - third iteration fragments the message if the second iteration failed
        loop {
            // Mark the write operation       
            batch.buffer.mark();
            // Try to serialize the message on the current batch
            let res = match &msg {
                MessageInner::Zenoh(m) => {
                    // The message is reliable or not
                    let reliable = m.is_reliable();
                    // Eventually update the current frame and sn based on the current status
                    match current_frame {
                        CurrentFrame::Reliable => {
                            if !reliable {
                                // A new best-effort frame needs to be started
                                current_frame = CurrentFrame::BestEffort;
                                current_sn = Some(inner.sn.best_effort.get());
                                is_first = true;
                            }
                        }, 
                        CurrentFrame::BestEffort => {
                            if reliable {    
                                // A new reliable frame needs to be started
                                current_frame = CurrentFrame::Reliable;
                                current_sn = Some(inner.sn.reliable.get());
                                is_first = true;
                            }
                        },
                        CurrentFrame::None => {
                            if !has_failed || !is_first {
                                if reliable {
                                    // A new reliable frame needs to be started
                                    current_frame = CurrentFrame::Reliable;
                                    current_sn = Some(inner.sn.reliable.get());
                                } else {
                                    // A new best-effort frame needs to be started
                                    current_frame = CurrentFrame::BestEffort;
                                    current_sn = Some(inner.sn.best_effort.get());
                                }
                                is_first = true;
                            }
                        }
                    }

                    // If a new sequence number has been provided, it means we are in the case we need 
                    // to start a new frame. Write a new frame header.
                    if let Some(sn) = current_sn {                        
                        // Serialize the new frame and the zenoh message
                        batch.buffer.write_frame_header(reliable, sn, None, None)
                        && batch.buffer.write_zenoh_message(&m)
                    } else {
                        is_first = false;
                        batch.buffer.write_zenoh_message(&m)
                    }                    
                },
                MessageInner::Session(m) => {
                    current_frame = CurrentFrame::None;
                    batch.buffer.write_session_message(&m)
                },
                MessageInner::Stop => {
                    let _ = batch.transmit().await;
                    return false
                }
            };

            // We have been succesfull in writing on the current batch: breaks the loop
            // and tries to write more message on the same batch
            if res {
                break
            }

            // An error occured, revert the batch buffer
            batch.buffer.revert();
            // Reset the current frame but not the sn which is carried over the next iteration
            current_frame = CurrentFrame::None; 

            // This is the second time that the serialization fails, we should fragment
            if has_failed {
                // @TODO: Implement fragmentation here
                //        Drop the message for the time being
                log::warn!("Message dropped because fragmentation is not yet implemented: {:?}", msg);
                batch.clear();
                break
            }

            // Send the batch
            let _ = batch.transmit().await;

            // Mark the serialization attempt as failed
            has_failed = true;
        }
    }

    true
}

// Task for draining the queue
async fn drain_queue(
    ch: Arc<Channel>,
    mut batches: Vec<SerializedBatch>,    
    mut messages: Vec<Vec<MessageInner>>
) {
    // @TODO: Implement reliability
    // @TODO: Implement fragmentation

    // Keep the lock on the inner transmission structure
    let mut inner = zasynclock!(ch.tx);

    // Control variable
    let mut active = true; 

    while active {    
        // log::trace!("Waiting for messages in the transmission queue of channel: {}", ch.get_peer());
        // Get a Drain iterator for the queue
        // drain() waits for the queue to be non-empty
        let mut drain = ch.queue.drain().await;
        
        // Try to always fill the batch
        while active {
            // Map the messages on the links. This operation drains messages from the Drain iterator
            map_messages_on_links(&mut drain, &batches, &mut messages);
            
            // The drop() on Drain object needs to be manually called since an async
            // destructor is not yet supported in Rust. More information available at:
            // https://internals.rust-lang.org/t/asynchronous-destructors/11127/47 
            drain.drop().await;
            
            // Concurrently send on all the selected links
            for (i, mut batch) in batches.iter_mut().enumerate() {
                // active = batch_fragment_transmit(link, &mut inner, &mut context[i]);
                // Check if the batch is ready to send      
                let res = batch_fragment_transmit(&mut inner, &mut messages[i], &mut batch).await;
                if !res {
                    // There was an error while transmitting. Exit.
                    active = false;
                    break
                }
            }

            // Try to drain messages from the queue
            // try_drain does not wait for the queue to be non-empty
            drain = ch.queue.try_drain().await;
            // Check if we can drain from the Drain iterator
            let (min, _) = drain.size_hint();
            if min == 0 {
                // The drop() on Drain object needs to be manually called since an async
                // destructor is not yet supported in Rust. More information available at:
                // https://internals.rust-lang.org/t/asynchronous-destructors/11127/47
                drain.drop().await;
                break
            }
        }

        // Send any leftover on the batches
        for batch in batches.iter_mut() {
            // Check if the batch is ready to send      
            let _ = batch.transmit().await;        
        }

        // Deschedule the task to allow other tasks to be scheduled and eventually push on the queue
        task::yield_now().await;
    }
}

// Consume task
async fn consume_task(ch: Arc<Channel>) -> ZResult<()> {
    // Acquire the lock on the links
    let guard = zasyncread!(ch.links);

    // Check if we have links to send on
    if guard.links.is_empty() {
        let e = "Unable to start the consume task, no links available".to_string();
        log::debug!("{}", e);
        return zerror!(ZErrorKind::Other {
            descr: e
        })
    }

    // Allocate batches and messages buffers
    let mut batches: Vec<SerializedBatch> = Vec::with_capacity(guard.links.len());
    let mut messages: Vec<Vec<MessageInner>> = Vec::with_capacity(guard.links.len());

    // Initialize the batches based on the current links parameters      
    for link in guard.get().drain(..) {
        let size = guard.batch_size.min(link.get_mtu());
        batches.push(SerializedBatch::new(link, size));
        messages.push(Vec::with_capacity(*QUEUE_SIZE_TOT));
    }

    // Drop the mutex guard
    drop(guard);

    // Drain the queue until a Stop signal is received
    drain_queue(ch.clone(), batches, messages).await;

    // Barrier to synchronize with the stop()
    ch.barrier.wait().await;

    log::trace!("Exiting consume task for channel: {}", ch.get_peer());

    Ok(())
}

/*************************************/
/*          TIMED EVENTS            */
/*************************************/
struct KeepAliveEvent {
    ch: Weak<Channel>,
    link: Link
}

impl KeepAliveEvent {
    fn new(ch: Weak<Channel>, link: Link) -> KeepAliveEvent {
        KeepAliveEvent {
            ch,
            link
        }
    }
}

#[async_trait]
impl Timed for KeepAliveEvent {
    async fn run(&mut self) {
        log::trace!("Schedule KEEP_ALIVE messages for link: {}", self.link);        
        if let Some(ch) = self.ch.upgrade() {
            // Create the KEEP_ALIVE message
            let pid = None;
            let attachment = None;
            let message = MessageTx {
                inner: MessageInner::Session(SessionMessage::make_keep_alive(pid, attachment)),
                link: Some(self.link.clone())
            };

            // Push the KEEP_ALIVE messages on the queue
            ch.queue.push(message, *QUEUE_PRIO_CTRL).await;
        }
    }
}

struct LeaseEvent {
    ch: Weak<Channel>
}

impl LeaseEvent {
    fn new(ch: Weak<Channel>) -> LeaseEvent {
        LeaseEvent {
            ch
        }
    }
}

#[async_trait]
impl Timed for LeaseEvent {
    async fn run(&mut self) {        
        if let Some(ch) = self.ch.upgrade() {
            log::trace!("Verify session lease for peer: {}", ch.get_peer());

            // Create the set of current links
            let links: HashSet<Link> = HashSet::from_iter(ch.get_links().await.drain(..));
            // Get and reset the current status of active links
            let alive: HashSet<Link> = HashSet::from_iter(zasynclock!(ch.rx).alive.drain());
            // Create the difference set
            let mut difference: HashSet<Link> = HashSet::from_iter(links.difference(&alive).cloned());

            if links == difference {
                // We have no links left or all the links have expired: close the whole session
                log::warn!("Session with peer {} has expired", ch.get_peer());                
                let _ = ch.close(smsg::close_reason::EXPIRED).await;
            } else {
                // Remove only the links with expired lease
                for l in difference.drain() {
                    log::warn!("Link with peer {} has expired: {}", ch.get_peer(), l);
                    let _ = ch.close_link(&l, smsg::close_reason::EXPIRED).await;                
                }
            }            
        }
    }
}

/*************************************/
/*      CHANNEL INNER TX STRUCT      */
/*************************************/
// Structs to manage the sequence numbers of channels
struct SeqNumTx {
    reliable: SeqNumGenerator,
    best_effort: SeqNumGenerator,
}

impl SeqNumTx {
    fn new(sn_resolution: ZInt, initial_sn: ZInt) -> SeqNumTx {
        SeqNumTx {
            reliable: SeqNumGenerator::make(initial_sn, sn_resolution).unwrap(),
            best_effort: SeqNumGenerator::make(initial_sn, sn_resolution).unwrap(),
        }
    }
}

// Store the mutable data that need to be used for reception
struct ChannelInnerTx {
    sn: SeqNumTx
    // Add reliability queue
    // Add message to fragment
}

impl ChannelInnerTx {
    fn new(sn_resolution: ZInt, initial_sn: ZInt) -> ChannelInnerTx {
        ChannelInnerTx {
            sn: SeqNumTx::new(sn_resolution, initial_sn)
        } 
    }
}


/*************************************/
/*              LINKS                */
/*************************************/
// Store the mutable data that need to be used for transmission
#[derive(Clone)]
struct ChannelLinks {
    batch_size: usize,
    links: HashMap<Link, TimedHandle>
}

impl ChannelLinks {
    fn new(batch_size: usize) -> ChannelLinks {
        ChannelLinks {
            batch_size,
            links: HashMap::new()
        }
    }

    pub(super) fn add(&mut self, link: Link, handle: TimedHandle) -> ZResult<()> {
        // Check if this link is not already present
        if self.links.contains_key(&link) {
            let e = format!("Can not add the link to the channel because it is already associated: {}", link);
            log::trace!("{}", e);
            return zerror!(ZErrorKind::InvalidLink {
                descr: e
            })
        }

        // Add the link to the channel
        self.links.insert(link, handle);

        Ok(())
    }

    pub(super) fn get(&self) -> Vec<Link> {
        self.links.iter().map(|(l, _)| l.clone()).collect()
    }

    pub(super) fn del(&mut self, link: &Link) -> ZResult<TimedHandle> {
        // Remove the link from the channel
        if let Some(handle) = self.links.remove(link) {
            Ok(handle)
        } else {
            let e = format!("Can not delete the link from the channel because it does not exist: {}", link);
            log::trace!("{}", e);
            zerror!(ZErrorKind::InvalidLink {
                descr: format!("{}", link)
            })
        }
    }    

    pub(super) fn clear(&mut self) {
        self.links.clear()
    }
}


/*************************************/
/*     CHANNEL INNER RX STRUCT       */
/*************************************/

// Structs to manage the sequence numbers of channels
struct SeqNumRx {
    reliable: SeqNum,
    best_effort: SeqNum,
}

impl SeqNumRx {
    fn new(sn_resolution: ZInt, initial_sn: ZInt) -> SeqNumRx {
        // Set the sequence number in the state as it had 
        // received a message with initial_sn - 1
        let initial_sn = if initial_sn == 0 {
            sn_resolution
        } else {
            initial_sn - 1
        };
        SeqNumRx {
            reliable: SeqNum::make(initial_sn, sn_resolution).unwrap(),
            best_effort: SeqNum::make(initial_sn, sn_resolution).unwrap(),
        }
    }
}

// Store the mutable data that need to be used for transmission
struct ChannelInnerRx {    
    sn: SeqNumRx,
    alive: HashSet<Link>,
    callback: Option<Arc<dyn MsgHandler + Send + Sync>>
}

impl ChannelInnerRx {
    fn new(
        sn_resolution: ZInt,
        initial_sn: ZInt
    ) -> ChannelInnerRx {
        ChannelInnerRx {
            sn: SeqNumRx::new(sn_resolution, initial_sn),
            alive: HashSet::new(),
            callback: None
        }
    }
}


/*************************************/
/*           CHANNEL STRUCT          */
/*************************************/
pub(super) struct Channel {
    // The manager this channel is associated to
    manager: Arc<SessionManagerInner>,
    // The remote peer id
    pid: PeerId,
    // The session lease in seconds
    lease: ZInt,
    // Keep alive interval
    keep_alive: ZInt,
    // The SN resolution 
    sn_resolution: ZInt,
    // Keep track whether the consume task is active
    active: Mutex<bool>,
    // The callback has been set or not
    has_callback: AtomicBool,
    // The message queue
    queue: CreditQueue<MessageTx>,
    // The links associated to the channel
    links: RwLock<ChannelLinks>,
    // The mutable data struct for transmission
    tx: Mutex<ChannelInnerTx>,
    // The mutable data struct for reception
    rx: Mutex<ChannelInnerRx>,
    // The internal timer
    timer: Timer,
    // Barrier for syncrhonizing the stop() with the consume_task
    barrier: Arc<Barrier>,
    // Weak reference to self
    w_self: RwLock<Option<Weak<Self>>>
}

impl Channel {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        manager: Arc<SessionManagerInner>,
        pid: PeerId, 
        _whatami: WhatAmI,
        lease: ZInt,
        keep_alive: ZInt,
        sn_resolution: ZInt, 
        initial_sn_tx: ZInt,
        initial_sn_rx: ZInt,
        batch_size: usize        
    ) -> Channel {
        // The buffer to send the Control messages. High priority
        let ctrl = CreditBuffer::<MessageTx>::new(
            *QUEUE_SIZE_CTRL,
            *QUEUE_CRED_CTRL,
            CreditBuffer::<MessageTx>::spending_policy(|_msg| 0isize),
        );
        // The buffer to send the retransmission of messages. Medium priority
        let retx = CreditBuffer::<MessageTx>::new(
            *QUEUE_SIZE_RETX,
            *QUEUE_CRED_RETX,
            CreditBuffer::<MessageTx>::spending_policy(|_msg| 0isize),
        );
        // The buffer to send the Data messages. Low priority
        let data = CreditBuffer::<MessageTx>::new(
            *QUEUE_SIZE_DATA,
            *QUEUE_CRED_DATA,
            // @TODO: Once the reliability queue is implemented, update the spending policy
            CreditBuffer::<MessageTx>::spending_policy(|_msg| 0isize),
        );
        // Build the vector of buffer for the transmission queue.
        // A lower index in the vector means higher priority in the queue.
        // The buffer with index 0 has the highest priority.
        let queue_tx = vec![ctrl, retx, data];

        Channel {
            manager,
            pid,
            lease,
            keep_alive,
            sn_resolution,
            has_callback: AtomicBool::new(false),
            queue: CreditQueue::new(queue_tx, *QUEUE_CONCURRENCY),
            active: Mutex::new(false),
            links: RwLock::new(ChannelLinks::new(batch_size)),
            tx: Mutex::new(ChannelInnerTx::new(sn_resolution, initial_sn_tx)),
            rx: Mutex::new(ChannelInnerRx::new(sn_resolution, initial_sn_rx)),
            timer: Timer::new(),
            barrier: Arc::new(Barrier::new(2)),
            w_self: RwLock::new(None)
        }        
    }

    pub(super) async fn initialize(&self, w_self: Weak<Self>) {
        // Initialize the weak reference to self
        *zasyncwrite!(self.w_self) = Some(w_self.clone());
        // Initialize the lease event
        let event = LeaseEvent::new(w_self);
        let interval = Duration::from_millis(self.lease);
        let event = TimedEvent::periodic(interval, event);
        self.timer.add(event).await;
    }


    /*************************************/
    /*            ACCESSORS              */
    /*************************************/
    pub(super) fn get_peer(&self) -> PeerId {
        self.pid.clone()
    }

    pub(super) fn get_lease(&self) -> ZInt {
        self.lease
    }

    pub(super) fn get_keep_alive(&self) -> ZInt {
        self.keep_alive
    }

    pub(super) fn get_sn_resolution(&self) -> ZInt {
        self.sn_resolution
    }

    pub(super) fn has_callback(&self) -> bool {
        self.has_callback.load(Ordering::Relaxed)
    }

    pub(super) async fn set_callback(&self, callback: Arc<dyn MsgHandler + Send + Sync>) {        
        let mut guard = zasynclock!(self.rx);
        self.has_callback.store(true, Ordering::Relaxed);
        guard.callback = Some(callback);
    }


    /*************************************/
    /*               TASK                */
    /*************************************/
    async fn start(&self) -> ZResult<()> {
        let mut guard = zasynclock!(self.active);   
        // If not already active, start the transmission loop
        if !*guard {    
            // Get the Arc to the channel
            let ch = zasyncread!(self.w_self).as_ref().unwrap().upgrade().unwrap();

            // Spawn the transmission loop
            task::spawn(async move {
                let _ = consume_task(ch.clone()).await;
                // Mark the task as non-active
                let mut guard = zasynclock!(ch.active);
                *guard = false;
            });

            // Mark that now the task can be stopped
            *guard = true;

            return Ok(())
        }

        zerror!(ZErrorKind::Other {
            descr: format!("Can not start channel with peer {} because it is already active", self.get_peer())
        })
    }

    async fn stop(&self, priority: usize) -> ZResult<()> {  
        let mut guard = zasynclock!(self.active);         
        if *guard {
            let msg = MessageTx {
                inner: MessageInner::Stop,
                link: None
            };
            self.queue.push(msg, priority).await;
            self.barrier.wait().await;

            // Mark that now the task can be started
            *guard = false;

            return Ok(())
        }

        zerror!(ZErrorKind::Other {
            descr: format!("Can not stop channel with peer {} because it is already inactive", self.get_peer())
        })
    }


    /*************************************/
    /*           TERMINATION             */
    /*************************************/
    async fn delete(&self) {
        // Stop the consume task with the lowest priority to give enough
        // time to send all the messages still present in the queue
        let _ = self.stop(*QUEUE_PRIO_DATA).await;

        // Delete the session on the manager
        let _ = self.manager.del_session(&self.pid).await;            

        // Notify the callback
        if let Some(callback) = &zasynclock!(self.rx).callback {
            callback.close().await;
        }
        
        // Close all the links
        // NOTE: del_link() is meant to be used thorughout the lifetime
        //       of the session and not for its termination.
        for l in self.get_links().await.drain(..) {
            let _ = l.close().await;
        }

        // Remove all the reference to the links
        zasyncwrite!(self.links).clear();
    }

    pub(super) async fn close_link(&self, link: &Link, reason: u8) -> ZResult<()> {     
        log::trace!("Closing link {} with peer: {}", link, self.get_peer());

        // Close message to be sent on the target link
        let peer_id = Some(self.manager.config.pid.clone());
        let reason_id = reason;              
        let link_only = true;  // This is should always be true when closing a link              
        let attachment = None;  // No attachment here
        let msg = SessionMessage::make_close(peer_id, reason_id, link_only, attachment);

        let close = MessageTx {
            inner: MessageInner::Session(msg),
            link: Some(link.clone())
        };

        // NOTE: del_link() stops the consume task with priority QUEUE_PRIO_CTRL.
        //       The close message must be pushed with the same priority before
        //       the link is deleted
        self.queue.push(close, *QUEUE_PRIO_CTRL).await;

        // Remove the link from the channel
        self.del_link(&link).await?;

        // Close the underlying link
        link.close().await
    }

    pub(super) async fn close(&self, reason: u8) -> ZResult<()> {
        log::trace!("Closing session with peer: {}", self.get_peer());

        // Atomically push the messages on the queue
        let mut messages: Vec<MessageTx> = Vec::new();

        // Close message to be sent on all the links
        let peer_id = Some(self.manager.config.pid.clone());
        let reason_id = reason;              
        let link_only = false;  // This is should always be false for user-triggered close              
        let attachment = None;  // No attachment here
        let msg = SessionMessage::make_close(peer_id, reason_id, link_only, attachment);

        for link in self.get_links().await.drain(..) {
            let close = MessageTx {
                inner: MessageInner::Session(msg.clone()),
                link: Some(link)
            };
            messages.push(close);
        }

        // Atomically push the close and stop messages to the queue
        self.queue.push_batch(messages, *QUEUE_PRIO_DATA).await;

        // Terminate and clean up the session
        self.delete().await;
        
        Ok(())
    }

    /*************************************/
    /*        SCHEDULE AND SEND TX       */
    /*************************************/
    /// Schedule a Zenoh message on the transmission queue
    pub(super) async fn schedule(&self, message: ZenohMessage, link: Option<Link>) {
        let message = MessageTx {
            inner: MessageInner::Zenoh(message),
            link
        };
        // Wait for the queue to have space for the message
        self.queue.push(message, *QUEUE_PRIO_DATA).await;
    }

    /// Schedule a batch of Zenoh messages on the transmission queue
    pub(super) async fn schedule_batch(&self, mut messages: Vec<ZenohMessage>, link: Option<Link>) {
        let messages = messages.drain(..).map(|x| {
            MessageTx {
                inner: MessageInner::Zenoh(x),
                link: link.clone(),
            }
        }).collect();
        // Wait for the queue to have space for the message
        self.queue.push_batch(messages, *QUEUE_PRIO_DATA).await;        
    }

    /*************************************/
    /*               LINK                */
    /*************************************/
    pub(super) async fn add_link(&self, link: Link) -> ZResult<()> {
        // Get the Arc to the channel
        let ch = zasyncread!(self.w_self).as_ref().unwrap().upgrade().unwrap();
        
        // Initialize the event for periodically sending KEEP_ALIVE messages on the link
        let event = KeepAliveEvent::new(Arc::downgrade(&ch), link.clone());
        // Keep alive interval is expressed in millisesond
        let interval = Duration::from_millis(self.keep_alive);
        let event = TimedEvent::periodic(interval, event);
        // Get the handle of the periodic event
        let handle = event.get_handle();

        // Add the link along with the handle for defusing the KEEP_ALIVE messages
        zasyncwrite!(self.links).add(link.clone(), handle)?;

        // Add the link to the set of alive links
        zasynclock!(self.rx).alive.insert(link);

        // Add the periodic event to the timer
        ch.timer.add(event).await;

        // Stop the consume task to get the updated view on the links
        let _ = self.stop(*QUEUE_PRIO_CTRL).await;
        // Start the consume task with the new view on the links
        let _ = self.start().await;
        
        Ok(())
    }

    pub(super) async fn del_link(&self, link: &Link) -> ZResult<()> { 
        // Try to remove the link
        let mut guard = zasyncwrite!(self.links);
        let handle = guard.del(link)?;
        let is_empty = guard.links.is_empty();
        // Drop the guard before doing any other operation
        drop(guard);
        
        // Defuse the periodic sending of KEEP_ALIVE messages on this link
        handle.defuse();
                
        // Stop the consume task to get the updated view on the links
        let _ = self.stop(*QUEUE_PRIO_CTRL).await;
        // Don't restart the consume task if there are no links left
        if !is_empty {
            // Start the consume task with the new view on the links
            let _ = self.start().await;
        }

        Ok(())    
    }

    pub(super) async fn get_links(&self) -> Vec<Link> {
        zasyncread!(self.links).get()
    }


    /*************************************/
    /*   MESSAGE RECEIVED FROM THE LINK  */
    /*************************************/
    async fn process_reliable_frame(&self, sn: ZInt, payload: FramePayload) -> Action {
        // @TODO: Implement the reordering and reliability. Wait for missing messages.
        let mut guard = zasynclock!(self.rx);        
        if !(guard.sn.reliable.precedes(sn) && guard.sn.reliable.set(sn).is_ok()) {
            log::warn!("Reliable frame with invalid SN dropped: {}", sn);
            return Action::Read
        }

        let callback = if let Some(callback) = &guard.callback {
            callback
        } else {
            log::error!("Reliable frame dropped because callback is unitialized: {:?}", payload);
            return Action::Read
        };

        match payload {
            FramePayload::Fragment { .. } => {
                unimplemented!("Fragmentation not implemented");
            },
            FramePayload::Messages { mut messages } => {
                for msg in messages.drain(..) {
                    log::trace!("Session: {}. Message: {:?}", self.get_peer(), msg);
                    let _ = callback.handle_message(msg).await;
                }
            }
        }
        
        Action::Read
    }

    async fn process_best_effort_frame(&self, sn: ZInt, payload: FramePayload) -> Action {
        let mut guard = zasynclock!(self.rx);
        if !(guard.sn.best_effort.precedes(sn) && guard.sn.best_effort.set(sn).is_ok()) {
            log::warn!("Best-effort frame with invalid SN dropped: {}", sn);
            return Action::Read
        }

        let callback = if let Some(callback) = &guard.callback {
            callback
        } else {
            log::error!("Best-effort frame dropped because callback is unitialized: {:?}", payload);
            return Action::Read
        };

        match payload {
            FramePayload::Fragment { .. } => {
                unimplemented!("Fragmentation not implemented");
            },
            FramePayload::Messages { mut messages } => {
                for msg in messages.drain(..) {
                    log::trace!("Session: {}. Message: {:?}", self.get_peer(), msg);
                    let _ = callback.handle_message(msg).await;
                }              
            }
        }
        
        Action::Read
    }

    async fn process_close(&self, link: &Link, pid: Option<PeerId>, reason: u8, link_only: bool) -> Action {
        // Check if the PID is correct when provided
        if let Some(pid) = pid {
            if pid != self.pid {
                log::warn!("Received an invalid Close on link {} from peer {} with reason: {}. Ignoring.", link, pid, reason);
                return Action::Read
            }
        }        
        
        if link_only {
            // Delete only the link but keep the session open
            let _ = self.del_link(link).await;
            // Close the link
            let _ = link.close().await;
        } else { 
            // Close the whole session 
            self.delete().await;
        }
        
        Action::Close
    }

    async fn process_keep_alive(&self, link: &Link, pid: Option<PeerId>) -> Action {
        // Check if the PID is correct when provided
        if let Some(pid) = pid {
            if pid != self.pid {
                log::warn!("Received an invalid KeepAlive on link {} from peer: {}. Ignoring.", link, pid);
                return Action::Read
            }
        } 

        let mut guard = zasynclock!(self.rx);
        // Add the link to the list of alive links
        guard.alive.insert(link.clone());

        Action::Read
    }
}

#[async_trait]
impl TransportTrait for Channel {
    async fn receive_message(&self, link: &Link, message: SessionMessage) -> Action {
        log::trace!("Received from peer {} on link {}: {:?}", self.get_peer(), link, message);
        match message.body {
            SessionBody::Frame { ch, sn, payload } => {
                match ch {
                    true => self.process_reliable_frame(sn, payload).await,
                    false => self.process_best_effort_frame(sn, payload).await
                }
            },
            SessionBody::AckNack { .. } => {
                unimplemented!("Handling of AckNack Messages not yet implemented!");
            },
            SessionBody::Close { pid, reason, link_only } => {
                self.process_close(link, pid, reason, link_only).await
            },
            SessionBody::Hello { .. } => {
                unimplemented!("Handling of Hello Messages not yet implemented!");
            },
            SessionBody::KeepAlive { pid } => {
                self.process_keep_alive(link, pid).await
            },            
            SessionBody::Ping { .. } => {
                unimplemented!("Handling of Ping Messages not yet implemented!");
            },
            SessionBody::Pong { .. } => {
                unimplemented!("Handling of Pong Messages not yet implemented!");
            },
            SessionBody::Scout { .. } => {
                unimplemented!("Handling of Scout Messages not yet implemented!");
            },
            SessionBody::Sync { .. } => {
                unimplemented!("Handling of Sync Messages not yet implemented!");
            },            
            SessionBody::Open { .. } |
            SessionBody::Accept { .. } => {
                log::debug!("Unexpected Open/Accept message received in an already established session\
                             Closing the link: {}", link);
                Action::Close
            }
        }        
    }

    async fn link_err(&self, link: &Link) {
        log::warn!("Unexpected error on link {} with peer: {}", link, self.get_peer());
        let _ = self.del_link(link).await;
        let _ = link.close().await;
    }
}
