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
use async_std::prelude::*;
use async_std::sync::{channel, Arc, Barrier, Mutex, Receiver, Sender, Weak};
use async_std::task;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::{Channel, KeepAliveEvent, LinkLeaseEvent, TransmissionQueue};

use crate::core::ZInt;
use crate::link::Link;
use crate::proto::{SeqNumGenerator, SessionMessage, ZenohMessage};

use zenoh_util::collections::{TimedEvent, TimedHandle, Timer};
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::zerror;

// Consume task
async fn consume_task(
    queue: Arc<TransmissionQueue>,
    link: Link,
    active: Arc<AtomicBool>,
    receiver: Receiver<()>,
    barrier: Arc<Barrier>,
) -> ZResult<()> {
    enum Action {
        Continue,
        Stop,
    }

    async fn consume(queue: &Arc<TransmissionQueue>, link: &Link) -> ZResult<Action> {
        // Pull a serialized batch from the queue
        let (batch, index) = queue.pull().await;
        // Send the buffer on the link
        link.send(batch.get_buffer()).await?;
        // Reinsert the batch into the queue
        queue.push_serialization_batch(batch, index).await;

        Ok(Action::Continue)
    }

    async fn signal(receiver: &Receiver<()>) -> ZResult<Action> {
        let res = receiver.recv().await;
        match res {
            Ok(_) => Ok(Action::Stop),
            Err(_) => zerror!(ZErrorKind::Other {
                descr: "Signal error".to_string()
            }),
        }
    }

    // Keep draining the queue while active
    while active.load(Ordering::Relaxed) {
        let action = consume(&queue, &link).race(signal(&receiver)).await?;
        match action {
            Action::Continue => continue,
            Action::Stop => break,
        }
    }

    // Drain what remains in the queue before exiting
    while let Some(batch) = queue.drain().await {
        link.send(batch.get_buffer()).await?;
    }

    // Synchronize with the close()
    barrier.wait().await;

    Ok(())
}

pub(super) struct LinkAlive {
    inner: AtomicBool,
}

impl LinkAlive {
    fn new() -> LinkAlive {
        LinkAlive {
            inner: AtomicBool::new(true),
        }
    }

    #[inline]
    pub(super) fn mark(&self) {
        self.inner.store(true, Ordering::Relaxed);
    }

    #[inline]
    pub(super) fn reset(&self) -> bool {
        self.inner.swap(false, Ordering::Relaxed)
    }
}

#[derive(Clone)]
pub(super) struct ChannelLink {
    link: Link,
    queue: Arc<TransmissionQueue>,
    active: Arc<AtomicBool>,
    alive: Arc<LinkAlive>,
    barrier: Arc<Barrier>,
    handles: Vec<TimedHandle>,
    signal: Sender<()>,
}

impl ChannelLink {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        ch: Weak<Channel>,
        link: Link,
        batch_size: usize,
        keep_alive: ZInt,
        lease: ZInt,
        sn_reliable: Arc<Mutex<SeqNumGenerator>>,
        sn_best_effort: Arc<Mutex<SeqNumGenerator>>,
        timer: Timer,
    ) -> ChannelLink {
        // The queue
        let queue = Arc::new(TransmissionQueue::new(
            batch_size.min(link.get_mtu()),
            link.is_streamed(),
            sn_reliable,
            sn_best_effort,
        ));

        // Control variables
        let active = Arc::new(AtomicBool::new(true));
        let alive = Arc::new(LinkAlive::new());

        // Barrier for sincronization
        let barrier = Arc::new(Barrier::new(2));

        // Keep alive event
        let event = KeepAliveEvent::new(queue.clone(), link.clone());
        // Keep alive interval is expressed in milliseconds
        let interval = Duration::from_millis(keep_alive as u64);
        let ka_event = TimedEvent::periodic(interval, event);
        // Get the handle of the periodic event
        let ka_handle = ka_event.get_handle();

        // Lease event
        let event = LinkLeaseEvent::new(ch, alive.clone(), link.clone());
        // Keep alive interval is expressed in milliseconds
        let interval = Duration::from_millis(lease as u64);
        let ll_event = TimedEvent::periodic(interval, event);
        // Get the handle of the periodic event
        let ll_handle = ll_event.get_handle();

        // Event handles
        let handles = vec![ka_handle, ll_handle];

        // Channel for signal the termination of consume task
        let (sender, receiver) = channel::<()>(1);

        // Spawn the timed events and the consume task
        let c_queue = queue.clone();
        let c_link = link.clone();
        let c_active = active.clone();
        let c_alive = alive.clone();
        let c_barrier = barrier.clone();
        let mut c_handles = handles.clone();
        task::spawn(async move {
            // Add the keep alive and lease events to the timer
            timer.add(ka_event).await;
            timer.add(ll_event).await;
            // Start the consume task
            let res = consume_task(
                c_queue,
                c_link.clone(),
                c_active.clone(),
                receiver,
                c_barrier,
            )
            .await;
            if res.is_err() {
                // Cleanup upon an error
                c_active.store(false, Ordering::Relaxed);
                c_alive.reset();
                // Drain the timed events
                for h in c_handles.drain(..) {
                    h.defuse();
                }
                // Close the underlying link
                let _ = c_link.close().await;
            }
        });

        ChannelLink {
            link,
            queue,
            active,
            alive,
            barrier,
            handles,
            signal: sender,
        }
    }
}

impl ChannelLink {
    #[inline]
    pub(super) fn get_link(&self) -> &Link {
        &self.link
    }

    #[inline]
    pub(super) fn mark_alive(&self) {
        self.alive.mark();
    }

    pub(super) async fn schedule_zenoh_message(&self, msg: ZenohMessage, priority: usize) {
        if self.active.load(Ordering::Relaxed) {
            self.queue.push_zenoh_message(msg, priority).await;
        }
    }

    pub(super) async fn schedule_session_message(&self, msg: SessionMessage, priority: usize) {
        if self.active.load(Ordering::Relaxed) {
            self.queue.push_session_message(msg, priority).await;
        }
    }

    pub(super) async fn close(mut self) -> ZResult<()> {
        // Deactivate the consume task if active
        if self.active.swap(false, Ordering::Relaxed) {
            // Send the signal
            let _ = self.signal.try_send(());
            // Defuse the timed events
            for h in self.handles.drain(..) {
                h.defuse();
            }
            // Wait for the task to exit
            self.barrier.wait().await;
            // Close the underlying link
            self.link.close().await
        } else {
            Ok(())
        }
    }
}
