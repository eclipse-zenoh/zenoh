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
mod rx;
mod tx;

use super::super::super::link::Link;
use super::core;
use super::core::ZInt;
use super::io;
use super::io::{ArcSlice, RBuf};
use super::proto;
use super::proto::{SessionMessage, ZenohMessage};
use super::session;
use super::session::defaults::{QUEUE_PRIO_CTRL, RX_BUFF_SIZE};
use super::{SeqNumGenerator, SessionTransport};
use async_std::future::Future;
use async_std::pin::Pin;
use async_std::prelude::*;
use async_std::task;
use async_std::task::{Context, Poll};
use batch::*;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tx::*;
use zenoh_util::collections::RecyclingObjectPool;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::zerror;

#[derive(Clone)]
pub(crate) struct SessionTransportLink {
    // The underlying link
    pub(super) inner: Link,
    // The session lease in seconds
    pub(super) lease: ZInt,
    // Keep alive interval
    pub(super) keep_alive: ZInt,
    // The transport this link is associated to
    transport: SessionTransport,
    // The transmission pipeline
    pipeline: Arc<TransmissionPipeline>,
    // The signals to stop TX/RX tasks
    signal_tx: Arc<Mutex<bool>>,
    signal_rx: Arc<Mutex<bool>>,
}

impl SessionTransportLink {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        transport: SessionTransport,
        link: Link,
        batch_size: usize,
        keep_alive: ZInt,
        lease: ZInt,
        sn_reliable: Arc<Mutex<SeqNumGenerator>>,
        sn_best_effort: Arc<Mutex<SeqNumGenerator>>,
    ) -> SessionTransportLink {
        // The queue
        let pipeline = Arc::new(TransmissionPipeline::new(
            batch_size.min(link.get_mtu()),
            link.is_streamed(),
            sn_reliable,
            sn_best_effort,
        ));

        SessionTransportLink {
            transport,
            inner: link,
            lease,
            keep_alive,
            pipeline,
            signal_tx: Arc::new(Mutex::new(false)),
            signal_rx: Arc::new(Mutex::new(false)),
        }
    }
}

impl SessionTransportLink {
    pub(crate) fn start_tx(&self) {
        let mut guard = zlock!(self.signal_tx);
        if !*guard {
            // SessionTransport for signal the termination of TX task
            *guard = true;
            drop(guard);
            // Spawn the TX task
            let c_link = self.clone();
            let signal = Signal::new(self.signal_tx.clone());
            task::spawn(async move {
                // Start the consume task
                let res = tx_task(c_link.clone(), signal).await;
                if let Err(e) = res {
                    log::debug!("{}", e);
                    let _ = c_link.transport.del_link(&c_link.inner);
                }
            });
        }
    }

    pub(crate) fn stop_tx(&self) {
        let mut guard = zlock!(self.signal_tx);
        *guard = false;
    }

    pub(crate) fn start_rx(&self) {
        let mut guard = zlock!(self.signal_rx);
        if !*guard {
            *guard = true;
            drop(guard);
            // Spawn the RX task
            let c_link = self.clone();
            let signal = Signal::new(self.signal_rx.clone());
            task::spawn(async move {
                // Start the consume task
                let res = rx_task(c_link.clone(), signal).await;
                if let Err(e) = res {
                    log::debug!("{}", e);
                    let _ = c_link.transport.del_link(&c_link.inner);
                }
            });
        }
    }

    pub(crate) fn stop_rx(&self) {
        let mut guard = zlock!(self.signal_rx);
        *guard = false;
    }

    #[inline]
    pub(crate) fn get_link(&self) -> &Link {
        &self.inner
    }

    #[inline]
    pub(crate) fn schedule_zenoh_message(&self, msg: ZenohMessage, priority: usize) {
        self.pipeline.push_zenoh_message(msg, priority);
    }

    #[inline]
    pub(crate) fn schedule_session_message(&self, msg: SessionMessage, priority: usize) {
        self.pipeline.push_session_message(msg, priority);
    }

    pub(crate) fn close(self) {
        log::trace!("{}: closing", self.inner);
        self.stop_tx();
        self.stop_rx();
        let tmp = self;
        task::spawn(async move { tmp.flush().await });
    }

    pub(crate) async fn flush(self) -> ZResult<()> {
        let duration = Duration::from_millis(self.lease);
        // Drain what remains in the queue before exiting
        while let Some(batch) = self.pipeline.drain().await {
            log::trace!("Draining {}: {:?}", self.inner, batch.as_bytes());
            let _ = self
                .inner
                .write_all(batch.as_bytes())
                .timeout(duration)
                .await
                .map_err(|_| {
                    let e = format!(
                        "{}: failed to flush after {} milliseconds",
                        self.inner, self.lease
                    );
                    zerror2!(ZErrorKind::IoError { descr: e })
                })??;
        }
        // Close the underlying link
        self.inner.close().timeout(duration).await.map_err(|_| {
            let e = format!(
                "{}: failed to close after {} milliseconds",
                self.inner, self.lease
            );
            zerror2!(ZErrorKind::IoError { descr: e })
        })?
    }
}

/*************************************/
/*              TASKS                */
/*************************************/
#[derive(Clone)]
struct Signal(Arc<Mutex<bool>>);

impl Signal {
    fn new(signal: Arc<Mutex<bool>>) -> Signal {
        Signal(signal)
    }
}

impl Future for Signal {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _ctx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.0.try_lock() {
            Ok(guard) => match *guard {
                true => Poll::Pending,
                false => Poll::Ready(()),
            },
            Err(_) => Poll::Pending,
        }
    }
}

async fn tx_task(link: SessionTransportLink, signal: Signal) -> ZResult<()> {
    enum Action {
        Pull((SerializationBatch, usize)),
        Timeout,
        Stop,
    }

    async fn pull(link: &SessionTransportLink, duration: Duration) -> Action {
        match link.pipeline.pull().timeout(duration).await {
            Ok((batch, index)) => Action::Pull((batch, index)),
            Err(_) => Action::Timeout,
        }
    }

    async fn stop(signal: Signal) -> Action {
        signal.await;
        Action::Stop
    }

    let keep_alive = Duration::from_millis(link.keep_alive);
    loop {
        match pull(&link, keep_alive).race(stop(signal.clone())).await {
            Action::Pull((batch, index)) => {
                // Send the buffer on the link
                link.inner.write_all(batch.as_bytes()).await?;
                // Reinsert the batch into the queue
                link.pipeline.refill(batch, index);
            }
            Action::Timeout => {
                let pid = None;
                let attachment = None;
                let message = SessionMessage::make_keep_alive(pid, attachment);
                link.pipeline.push_session_message(message, QUEUE_PRIO_CTRL);
            }
            Action::Stop => return Ok(()),
        }
    }
}

async fn read_stream(link: SessionTransportLink, signal: Signal) -> ZResult<()> {
    enum Action {
        Read,
        Stop,
    }

    async fn read(
        link: &SessionTransportLink,
        buffer: &mut [u8],
        duration: Duration,
    ) -> ZResult<Action> {
        link.inner
            .read_exact(buffer)
            .timeout(duration)
            .await
            .map_err(|_| {
                let e = format!("{}: expired after {} milliseconds", link.inner, link.lease);
                zerror2!(ZErrorKind::IoError { descr: e })
            })??;
        Ok(Action::Read)
    }

    async fn stop(signal: Signal) -> ZResult<Action> {
        signal.await;
        Ok(Action::Stop)
    }

    let lease = Duration::from_millis(link.lease);
    // The RBuf to read a message batch onto
    let mut rbuf = RBuf::new();
    // 16 bits for reading the batch length
    let mut length = [0u8, 0u8];
    // The pool of buffers
    let n = 1 + (*RX_BUFF_SIZE / link.inner.get_mtu());
    // let pool = RecyclingBufferPool::new(n, link.inner.get_mtu());
    let pool = RecyclingObjectPool::new(n, || vec![0u8; link.inner.get_mtu()].into_boxed_slice());
    loop {
        // Clear the RBuf
        rbuf.clear();

        // Async read from the underlying link
        log::trace!("{} waiting to read length", link.inner);
        let action = read(&link, &mut length, lease)
            .race(stop(signal.clone()))
            .await?;
        let to_read = match action {
            Action::Read => u16::from_le_bytes(length) as usize,
            Action::Stop => return Ok(()),
        };

        // Retrieve one buffer
        let mut buffer = pool.try_take().unwrap_or_else(|| pool.alloc());

        log::trace!("{} waiting to read {} bytes", link.inner, to_read);
        let action = read(&link, &mut buffer[0..to_read], lease)
            .race(stop(signal.clone()))
            .await?;
        match action {
            Action::Read => {
                rbuf.add_slice(ArcSlice::new(buffer.into(), 0, to_read));

                while rbuf.can_read() {
                    match rbuf.read_session_message() {
                        Some(msg) => link.receive_message(msg),
                        None => {
                            let e = format!("{}: decoding error", link.inner);
                            return zerror!(ZErrorKind::IoError { descr: e });
                        }
                    }
                }
            }
            Action::Stop => return Ok(()),
        }
    }
}

async fn read_dgram(link: SessionTransportLink, signal: Signal) -> ZResult<()> {
    enum Action {
        Read(usize),
        Stop,
    }

    async fn read(
        link: &SessionTransportLink,
        buffer: &mut [u8],
        duration: Duration,
    ) -> ZResult<Action> {
        let n = link
            .inner
            .read(buffer)
            .timeout(duration)
            .await
            .map_err(|_| {
                let e = format!("{}: expired after {} milliseconds", link.inner, link.lease);
                zerror2!(ZErrorKind::IoError { descr: e })
            })??;
        Ok(Action::Read(n))
    }

    async fn stop(signal: Signal) -> ZResult<Action> {
        signal.await;
        Ok(Action::Stop)
    }

    let lease = Duration::from_millis(link.lease);
    // The RBuf to read a message batch onto
    let mut rbuf = RBuf::new();
    // The pool of buffers
    let n = 1 + (*RX_BUFF_SIZE / link.inner.get_mtu());
    // let pool = RecyclingBufferPool::new(n, link.inner.get_mtu());
    let pool = RecyclingObjectPool::new(n, || vec![0u8; link.inner.get_mtu()].into_boxed_slice());
    loop {
        // Clear the rbuf
        rbuf.clear();
        // Retrieve one buffer
        let mut buffer = pool.try_take().unwrap_or_else(|| pool.alloc());

        // Async read from the underlying link
        let action = read(&link, &mut buffer, lease)
            .race(stop(signal.clone()))
            .await?;
        match action {
            Action::Read(n) => {
                if n == 0 {
                    // Reading 0 bytes means error
                    let e = format!("{}: zero bytes reading", link.inner);
                    return zerror!(ZErrorKind::IoError { descr: e });
                }

                // Add the received bytes to the RBuf for deserialization
                rbuf.add_slice(ArcSlice::new(buffer.into(), 0, n));

                // Deserialize all the messages from the current RBuf
                while rbuf.can_read() {
                    match rbuf.read_session_message() {
                        Some(msg) => link.receive_message(msg),
                        None => {
                            let e = format!("{}: decoding error", link.inner);
                            return zerror!(ZErrorKind::IoError { descr: e });
                        }
                    }
                }
            }
            Action::Stop => return Ok(()),
        }
    }
}

async fn rx_task(link: SessionTransportLink, signal: Signal) -> ZResult<()> {
    if link.inner.is_streamed() {
        read_stream(link, signal).await
    } else {
        read_dgram(link, signal).await
    }
}
