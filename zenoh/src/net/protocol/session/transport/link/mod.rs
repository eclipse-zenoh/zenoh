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
mod pipeline;

use super::super::super::link::Link;
use super::core;
use super::core::ZInt;
use super::io;
use super::io::{ArcSlice, RBuf};
use super::proto;
use super::proto::SessionMessage;
use super::session;
use super::session::defaults::{QUEUE_PRIO_CTRL, RX_BUFF_SIZE};
use super::{SeqNumGenerator, SessionTransport};
use async_std::prelude::*;
use async_std::task;
use async_std::task::JoinHandle;
use batch::*;
pub(crate) use pipeline::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use zenoh_util::collections::RecyclingObjectPool;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::sync::Signal;
use zenoh_util::zerror;

#[derive(Clone)]
pub(crate) struct SessionTransportLink {
    // The underlying link
    pub(super) inner: Link,
    // The transport this link is associated to
    transport: SessionTransport,
    // The transmission pipeline
    pipeline: Option<Arc<TransmissionPipeline>>,
    // The signals to stop TX/RX tasks
    handle_tx: Option<Arc<JoinHandle<()>>>,
    active_rx: Arc<AtomicBool>,
    signal_rx: Signal,
    handle_rx: Option<Arc<JoinHandle<()>>>,
}

impl SessionTransportLink {
    pub(crate) fn new(transport: SessionTransport, link: Link) -> SessionTransportLink {
        SessionTransportLink {
            transport,
            inner: link,
            pipeline: None,
            handle_tx: None,
            active_rx: Arc::new(AtomicBool::new(false)),
            signal_rx: Signal::new(),
            handle_rx: None,
        }
    }
}

impl SessionTransportLink {
    #[inline]
    pub(crate) fn get_link(&self) -> &Link {
        &self.inner
    }

    #[inline]
    pub(crate) fn get_pipeline(&self) -> Option<Arc<TransmissionPipeline>> {
        self.pipeline.clone()
    }

    pub(crate) fn start_tx(
        &mut self,
        keep_alive: ZInt,
        batch_size: usize,
        sn_reliable: Arc<Mutex<SeqNumGenerator>>,
        sn_best_effort: Arc<Mutex<SeqNumGenerator>>,
    ) {
        if self.handle_tx.is_none() {
            // The pipeline
            let pipeline = Arc::new(TransmissionPipeline::new(
                batch_size.min(self.inner.get_mtu()),
                self.inner.is_streamed(),
                sn_reliable,
                sn_best_effort,
            ));
            self.pipeline = Some(pipeline.clone());

            // Spawn the TX task
            let c_link = self.inner.clone();
            let c_transport = self.transport.clone();
            let handle = task::spawn(async move {
                let res = tx_task(pipeline, c_link.clone(), keep_alive).await;
                if let Err(e) = res {
                    log::debug!("{}", e);
                    let _ = c_transport.del_link(&c_link).await;
                }
            });
            self.handle_tx = Some(Arc::new(handle));
        }
    }

    pub(crate) fn stop_tx(&mut self) {
        if let Some(pipeline) = self.pipeline.take() {
            pipeline.disable();
        }
    }

    pub(crate) fn start_rx(&mut self, lease: ZInt) {
        if self.handle_rx.is_none() {
            self.active_rx.store(true, Ordering::Release);
            // Spawn the RX task
            let c_link = self.inner.clone();
            let c_transport = self.transport.clone();
            let c_signal = self.signal_rx.clone();
            let c_active = self.active_rx.clone();

            let handle = task::spawn(async move {
                // Start the consume task
                let res = rx_task(
                    c_link.clone(),
                    c_transport.clone(),
                    lease,
                    c_signal.clone(),
                    c_active.clone(),
                )
                .await;
                c_active.store(false, Ordering::Release);
                if let Err(e) = res {
                    log::debug!("{}", e);
                    let _ = c_transport.del_link(&c_link).await;
                }
            });
            self.handle_rx = Some(Arc::new(handle));
        }
    }

    pub(crate) fn stop_rx(&mut self) {
        self.active_rx.store(false, Ordering::Release);
        self.signal_rx.trigger();
    }

    pub(crate) async fn close(mut self) -> ZResult<()> {
        log::trace!("{}: closing", self.inner);
        self.stop_rx();
        if let Some(handle) = self.handle_rx.take() {
            // It is safe to unwrap the Arc since we have the ownership of the whole link
            let handle_rx = Arc::try_unwrap(handle).unwrap();
            handle_rx.await;
        }

        self.stop_tx();
        if let Some(handle) = self.handle_tx.take() {
            // It is safe to unwrap the Arc since we have the ownership of the whole link
            let handle_tx = Arc::try_unwrap(handle).unwrap();
            handle_tx.await;
        }

        self.inner.close().await
    }
}

/*************************************/
/*              TASKS                */
/*************************************/
async fn tx_task(pipeline: Arc<TransmissionPipeline>, link: Link, keep_alive: ZInt) -> ZResult<()> {
    let keep_alive = Duration::from_millis(keep_alive);
    loop {
        match pipeline.pull().timeout(keep_alive).await {
            Ok(res) => match res {
                Some((batch, index)) => {
                    // Send the buffer on the link
                    let _ = link.write_all(batch.as_bytes()).await?;
                    // Reinsert the batch into the queue
                    pipeline.refill(batch, index);
                }
                None => break,
            },
            Err(_) => {
                let pid = None;
                let attachment = None;
                let message = SessionMessage::make_keep_alive(pid, attachment);
                pipeline.push_session_message(message, QUEUE_PRIO_CTRL);
            }
        }
    }

    // Drain the transmission pipeline and write remaining bytes on the wire
    let mut batches = pipeline.drain();
    for b in batches.drain(..) {
        let _ = link
            .write_all(b.as_bytes())
            .timeout(keep_alive)
            .await
            .map_err(|_| {
                let e = format!("{}: flush failed after {} ms", link, keep_alive.as_millis());
                zerror2!(ZErrorKind::IoError { descr: e })
            })??;
    }

    Ok(())
}

async fn rx_task_stream(
    link: Link,
    transport: SessionTransport,
    lease: ZInt,
    signal: Signal,
    active: Arc<AtomicBool>,
) -> ZResult<()> {
    enum Action {
        Read(usize),
        Stop,
    }

    async fn read(link: &Link, buffer: &mut [u8]) -> ZResult<Action> {
        // 16 bits for reading the batch length
        let mut length = [0u8, 0u8];
        link.read_exact(&mut length).await?;
        let n = u16::from_le_bytes(length) as usize;
        link.read_exact(&mut buffer[0..n]).await?;
        Ok(Action::Read(n))
    }

    async fn stop(signal: Signal) -> ZResult<Action> {
        signal.wait().await;
        Ok(Action::Stop)
    }

    let lease = Duration::from_millis(lease);
    // The RBuf to read a message batch onto
    let mut rbuf = RBuf::new();
    // The pool of buffers
    let n = 1 + (*RX_BUFF_SIZE / link.get_mtu());
    let pool = RecyclingObjectPool::new(n, || vec![0u8; link.get_mtu()].into_boxed_slice());
    while active.load(Ordering::Acquire) {
        // Clear the RBuf
        rbuf.clear();

        // Retrieve one buffer
        let mut buffer = pool.try_take().unwrap_or_else(|| pool.alloc());

        // Async read from the underlying link
        let action = read(&link, &mut buffer)
            .race(stop(signal.clone()))
            .timeout(lease)
            .await
            .map_err(|_| {
                let e = format!("{}: expired after {} milliseconds", link, lease.as_millis());
                zerror2!(ZErrorKind::IoError { descr: e })
            })??;
        match action {
            Action::Read(n) => {
                rbuf.add_slice(ArcSlice::new(buffer.into(), 0, n));

                while rbuf.can_read() {
                    match rbuf.read_session_message() {
                        Some(msg) => transport.receive_message(msg, &link),
                        None => {
                            let e = format!("{}: decoding error", link);
                            return zerror!(ZErrorKind::IoError { descr: e });
                        }
                    }
                }
            }
            Action::Stop => break,
        }
    }
    Ok(())
}

async fn rx_task_dgram(
    link: Link,
    transport: SessionTransport,
    lease: ZInt,
    signal: Signal,
    active: Arc<AtomicBool>,
) -> ZResult<()> {
    enum Action {
        Read(usize),
        Stop,
    }

    async fn read(link: &Link, buffer: &mut [u8]) -> ZResult<Action> {
        let n = link.read(buffer).await?;
        Ok(Action::Read(n))
    }

    async fn stop(signal: Signal) -> ZResult<Action> {
        signal.wait().await;
        Ok(Action::Stop)
    }

    let lease = Duration::from_millis(lease);
    // The RBuf to read a message batch onto
    let mut rbuf = RBuf::new();
    // The pool of buffers
    let n = 1 + (*RX_BUFF_SIZE / link.get_mtu());
    let pool = RecyclingObjectPool::new(n, || vec![0u8; link.get_mtu()].into_boxed_slice());
    while active.load(Ordering::Acquire) {
        // Clear the rbuf
        rbuf.clear();
        // Retrieve one buffer
        let mut buffer = pool.try_take().unwrap_or_else(|| pool.alloc());

        // Async read from the underlying link
        let action = read(&link, &mut buffer, lease)
            .race(stop(signal.clone()))
            .timeout(lease)
            .await
            .map_err(|_| {
                let e = format!("{}: expired after {} milliseconds", link, lease.as_millis());
                zerror2!(ZErrorKind::IoError { descr: e })
            })??;
        match action {
            Action::Read(n) => {
                if n == 0 {
                    // Reading 0 bytes means error
                    let e = format!("{}: zero bytes reading", link);
                    return zerror!(ZErrorKind::IoError { descr: e });
                }

                // Add the received bytes to the RBuf for deserialization
                rbuf.add_slice(ArcSlice::new(buffer.into(), 0, n));

                // Deserialize all the messages from the current RBuf
                while rbuf.can_read() {
                    match rbuf.read_session_message() {
                        Some(msg) => transport.receive_message(msg, &link),
                        None => {
                            let e = format!("{}: decoding error", link);
                            return zerror!(ZErrorKind::IoError { descr: e });
                        }
                    }
                }
            }
            Action::Stop => break,
        }
    }
    Ok(())
}

async fn rx_task(
    link: Link,
    transport: SessionTransport,
    lease: ZInt,
    signal: Signal,
    active: Arc<AtomicBool>,
) -> ZResult<()> {
    if link.is_streamed() {
        rx_task_stream(link, transport, lease, signal, active).await
    } else {
        rx_task_dgram(link, transport, lease, signal, active).await
    }
}
