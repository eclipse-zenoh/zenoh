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

//! Callback handler trait.
use std::{
    sync::{Arc, Weak},
    time::{Duration, Instant},
};

use zenoh_collections::RingBuffer;
use zenoh_result::ZResult;

use super::{callback::Callback, IntoHandler};
use crate::api::session::API_DATA_RECEPTION_CHANNEL_SIZE;

/// A synchronous ring channel with a limited size that allows users to keep the last N data.
pub struct RingChannel {
    capacity: usize,
}

impl RingChannel {
    /// Initialize the RingBuffer with the capacity size.
    pub fn new(capacity: usize) -> Self {
        Self { capacity }
    }
}

impl Default for RingChannel {
    fn default() -> Self {
        Self::new(*API_DATA_RECEPTION_CHANNEL_SIZE)
    }
}

struct RingChannelInner<T> {
    ring: std::sync::Mutex<RingBuffer<T>>,
    not_empty_tx: flume::Sender<()>,
    not_empty_rx: flume::Receiver<()>,
}

pub(crate) struct RingChannelSender<T>(Arc<RingChannelInner<T>>);

impl<T> Clone for RingChannelSender<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> RingChannelSender<T> {
    pub(crate) fn send(&self, t: T) {
        match self.0.ring.lock() {
            Ok(mut g) => {
                // Eventually drop the oldest element.
                g.push_force(t);
                drop(g);
                let _ = self.0.not_empty_tx.try_send(());
            }
            Err(error) => tracing::error!(%error),
        }
    }
}

pub struct RingChannelHandler<T> {
    ring: Weak<RingChannelInner<T>>,
}

impl<T> RingChannelHandler<T> {
    /// Receive from the ring channel.
    ///
    /// If the ring channel is empty, this call will block until an element is available in the channel.
    pub fn recv(&self) -> ZResult<T> {
        let Some(channel) = self.ring.upgrade() else {
            bail!("The ringbuffer has been deleted.");
        };
        loop {
            if let Some(t) = channel.ring.lock().map_err(|e| zerror!("{}", e))?.pull() {
                return Ok(t);
            }
            channel.not_empty_rx.recv().map_err(|e| zerror!("{}", e))?;
        }
    }

    /// Receive from the ring channel with a deadline.
    ///
    /// If the ring channel is empty, this call will block until an element is available in the channel,
    /// or return `None` if the deadline has passed.
    pub fn recv_deadline(&self, deadline: Instant) -> ZResult<Option<T>> {
        let Some(channel) = self.ring.upgrade() else {
            bail!("The ringbuffer has been deleted.");
        };

        loop {
            if let Some(t) = channel.ring.lock().map_err(|e| zerror!("{}", e))?.pull() {
                return Ok(Some(t));
            }
            match channel.not_empty_rx.recv_deadline(deadline) {
                Ok(()) => {}
                Err(flume::RecvTimeoutError::Timeout) => return Ok(None),
                Err(err) => bail!("{}", err),
            }
        }
    }

    /// Receive from the ring channel with a timeout.
    ///
    /// If the ring channel is empty, this call will block until an element is available in the channel,
    /// or return `None` if the deadline has expired.
    pub fn recv_timeout(&self, timeout: Duration) -> ZResult<Option<T>> {
        let Some(channel) = self.ring.upgrade() else {
            bail!("The ringbuffer has been deleted.");
        };

        loop {
            if let Some(t) = channel.ring.lock().map_err(|e| zerror!("{}", e))?.pull() {
                return Ok(Some(t));
            }
            match channel.not_empty_rx.recv_timeout(timeout) {
                Ok(()) => {}
                Err(flume::RecvTimeoutError::Timeout) => return Ok(None),
                Err(err) => bail!("{}", err),
            }
        }
    }

    /// Receive from the ring channel.
    ///
    /// If the ring channel is empty, this call will block until an element is available in the channel.
    pub async fn recv_async(&self) -> ZResult<T> {
        let Some(channel) = self.ring.upgrade() else {
            bail!("The ringbuffer has been deleted.");
        };
        loop {
            if let Some(t) = channel.ring.lock().map_err(|e| zerror!("{}", e))?.pull() {
                return Ok(t);
            }
            channel
                .not_empty_rx
                .recv_async()
                .await
                .map_err(|e| zerror!("{}", e))?;
        }
    }

    /// Try to receive from the ring channel.
    ///
    /// If the ring channel is empty, this call will return immediately without blocking.
    pub fn try_recv(&self) -> ZResult<Option<T>> {
        let Some(channel) = self.ring.upgrade() else {
            bail!("The ringbuffer has been deleted.");
        };
        let mut guard = channel.ring.lock().map_err(|e| zerror!("{}", e))?;
        Ok(guard.pull())
    }
}

impl<T: Clone + Send + 'static> IntoHandler<T> for RingChannel {
    type Handler = RingChannelHandler<T>;

    fn into_handler(self) -> (Callback<T>, Self::Handler) {
        let (not_empty_tx, not_empty_rx) = flume::bounded(1);
        let inner = Arc::new(RingChannelInner {
            ring: std::sync::Mutex::new(RingBuffer::new(self.capacity)),
            not_empty_tx,
            not_empty_rx,
        });
        let receiver = RingChannelHandler {
            ring: Arc::downgrade(&inner),
        };
        (Callback::new_ring(RingChannelSender(inner)), receiver)
    }
}
