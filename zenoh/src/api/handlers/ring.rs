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
    collections::VecDeque,
    sync::Arc,
    time::{Duration, Instant},
};

use zenoh_result::ZResult;

use crate::api::{
    handlers::{callback::Callback, IntoHandler},
    session::API_DATA_RECEPTION_CHANNEL_SIZE,
};

/// A synchronous ring channel with a limited size that allows users to keep the last N data.
///
/// [`RingChannel`] implements FIFO semantics with a dropping strategy when full.
/// The oldest elements will be dropped when newer arrive.
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
    ring: std::sync::Mutex<Option<VecDeque<T>>>,
    capacity: usize,
    not_empty_rx: flume::Receiver<()>,
}

pub struct RingChannelHandler<T>(Arc<RingChannelInner<T>>);

impl<T> RingChannelHandler<T> {
    /// Receive from the ring channel.
    ///
    /// If the ring channel is empty, this call will block until an element is available in the channel.
    pub fn recv(&self) -> ZResult<T> {
        loop {
            if let Some(t) = self.try_recv()? {
                return Ok(t);
            }
            self.0.not_empty_rx.recv()?
        }
    }

    /// Receive from the ring channel with a deadline.
    ///
    /// If the ring channel is empty, this call will block until an element is available in the channel,
    /// or return `None` if the deadline has passed.
    pub fn recv_deadline(&self, deadline: Instant) -> ZResult<Option<T>> {
        loop {
            if let Some(t) = self.try_recv()? {
                return Ok(Some(t));
            }
            match self.0.not_empty_rx.recv_deadline(deadline) {
                Ok(()) => continue,
                Err(flume::RecvTimeoutError::Timeout) => return Ok(None),
                Err(err) => return Err(err.into()),
            }
        }
    }

    /// Receive from the ring channel with a timeout.
    ///
    /// If the ring channel is empty, this call will block until an element is available in the channel,
    /// or return `None` if the deadline has expired.
    pub fn recv_timeout(&self, timeout: Duration) -> ZResult<Option<T>> {
        self.recv_deadline(Instant::now() + timeout)
    }

    /// Receive from the ring channel.
    ///
    /// If the ring channel is empty, this call will block until an element is available in the channel.
    pub async fn recv_async(&self) -> ZResult<T> {
        loop {
            if let Some(t) = self.try_recv()? {
                return Ok(t);
            }
            self.0.not_empty_rx.recv_async().await?
        }
    }

    /// Try to receive from the ring channel.
    ///
    /// If the ring channel is empty, this call will return immediately without blocking.
    pub fn try_recv(&self) -> ZResult<Option<T>> {
        let mut opt_buffer = self.0.ring.lock().unwrap();
        let buffer = opt_buffer
            .as_mut()
            .ok_or_else(|| zerror!("The ringbuffer has been deleted."))?;
        Ok(buffer.pop_front())
    }
}

struct RingChannelCallback<T> {
    inner: Arc<RingChannelInner<T>>,
    not_empty_tx: flume::Sender<()>,
}

impl<T> RingChannelCallback<T> {
    fn push(&self, value: T) {
        let mut guard = self.inner.ring.lock().unwrap();
        let buffer = guard.as_mut().unwrap();
        if buffer.len() == self.inner.capacity {
            buffer.pop_front();
        }
        buffer.push_back(value);
        let _ = self.not_empty_tx.try_send(());
    }
}

impl<T> Drop for RingChannelCallback<T> {
    fn drop(&mut self) {
        self.inner.ring.lock().unwrap().take();
    }
}

impl<T: Send + 'static> IntoHandler<T> for RingChannel {
    type Handler = RingChannelHandler<T>;

    fn into_handler(self) -> (Callback<T>, Self::Handler) {
        let (not_empty_tx, not_empty_rx) = flume::bounded(1);
        let inner = Arc::new(RingChannelInner {
            ring: std::sync::Mutex::new(Some(VecDeque::with_capacity(self.capacity))),
            capacity: self.capacity,
            not_empty_rx,
        });
        let handler = RingChannelHandler(inner.clone());
        let callback = RingChannelCallback {
            inner,
            not_empty_tx,
        };
        (Callback::new(Arc::new(move |t| callback.push(t))), handler)
    }
}
