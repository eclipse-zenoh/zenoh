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
mod batch;
mod events;
mod rx;
mod tx;

use crate::core::ZInt;
use crate::link::Link;
use crate::proto::{SessionMessage, ZenohMessage};
use crate::session::transport::{SeqNumGenerator, SessionTransport};
use async_std::channel::{bounded, Receiver, Sender};
use async_std::prelude::*;
use async_std::sync::{Arc, Mutex, Weak};
use async_std::task;
use batch::*;
use events::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
pub(crate) use tx::*;
use zenoh_util::collections::{TimedEvent, TimedHandle, Timer};
use zenoh_util::core::ZResult;

// Consume task
async fn consume_task(
    queue: Arc<TransmissionPipeline>,
    link: Link,
    receiver: Receiver<()>,
) -> ZResult<()> {
    let mut result: ZResult<()> = Ok(());

    // Keep draining the queue
    let consume = async {
        loop {
            // Pull a serialized batch from the queue
            let (batch, index) = queue.pull().await;
            // Send the buffer on the link
            result = link.send(batch.get_buffer()).await;
            if result.is_err() {
                break Ok(());
            }
            // Reinsert the batch into the queue
            queue.refill(batch, index).await;
        }
    };
    let _ = consume.race(receiver.recv()).await;

    result
}

pub(crate) struct LinkAlive {
    inner: AtomicBool,
}

impl LinkAlive {
    fn new() -> LinkAlive {
        LinkAlive {
            inner: AtomicBool::new(true),
        }
    }

    #[inline]
    pub(crate) fn mark(&self) {
        self.inner.store(true, Ordering::Relaxed);
    }

    #[inline]
    pub(crate) fn reset(&self) -> bool {
        self.inner.swap(false, Ordering::Relaxed)
    }
}

#[derive(Clone)]
pub(crate) struct SessionTransportLink {
    link: Link,
    queue: Arc<TransmissionPipeline>,
    alive: Arc<LinkAlive>,
    handles: Vec<TimedHandle>,
    signal: Sender<()>,
}

impl SessionTransportLink {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        ch: Weak<SessionTransport>,
        link: Link,
        batch_size: usize,
        keep_alive: ZInt,
        lease: ZInt,
        sn_reliable: Arc<Mutex<SeqNumGenerator>>,
        sn_best_effort: Arc<Mutex<SeqNumGenerator>>,
        timer: Timer,
    ) -> SessionTransportLink {
        // The queue
        let queue = Arc::new(TransmissionPipeline::new(
            batch_size.min(link.get_mtu()),
            link.is_streamed(),
            sn_reliable,
            sn_best_effort,
        ));

        // Control variables
        let alive = Arc::new(LinkAlive::new());

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

        // SessionTransport for signal the termination of consume task
        let (sender, receiver) = bounded::<()>(1);

        // Spawn the timed events and the consume task
        let c_queue = queue.clone();
        let c_link = link.clone();
        let c_alive = alive.clone();
        let mut c_handles = handles.clone();
        task::spawn(async move {
            // Add the keep alive and lease events to the timer
            timer.add(ka_event).await;
            timer.add(ll_event).await;
            // Start the consume task
            let res = consume_task(c_queue, c_link.clone(), receiver).await;
            if res.is_err() {
                // Cleanup upon an error
                c_alive.reset();

                // Drain the timed events
                for h in c_handles.drain(..) {
                    h.defuse();
                }

                // Close the underlying link
                let _ = c_link.close().await;
            }
        });

        SessionTransportLink {
            link,
            queue,
            alive,
            handles,
            signal: sender,
        }
    }
}

impl SessionTransportLink {
    #[inline]
    pub(crate) fn get_link(&self) -> &Link {
        &self.link
    }

    #[inline]
    pub(crate) fn mark_alive(&self) {
        self.alive.mark();
    }

    #[inline]
    pub(crate) async fn schedule_zenoh_message(&self, msg: ZenohMessage, priority: usize) {
        self.queue.push_zenoh_message(msg, priority).await;
    }

    #[inline]
    pub(crate) async fn schedule_session_message(&self, msg: SessionMessage, priority: usize) {
        self.queue.push_session_message(msg, priority).await;
    }

    pub(crate) async fn close(mut self) -> ZResult<()> {
        // Send the signal
        let _ = self.signal.send(()).await;

        // Drain what remains in the queue before exiting
        while let Some(batch) = self.queue.drain().await {
            self.link.send(batch.get_buffer()).await?;
        }

        // Defuse the timed events
        for h in self.handles.drain(..) {
            h.defuse();
        }

        // Close the underlying link
        self.link.close().await
    }
}
