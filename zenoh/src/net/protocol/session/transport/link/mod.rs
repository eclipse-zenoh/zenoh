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
use async_std::channel::{bounded, Receiver, Sender};
use async_std::prelude::*;
use async_std::sync::{Arc, Mutex};
use async_std::task;
use batch::*;
use std::time::Duration;
use tx::*;
use zenoh_util::collections::RecyclingObjectPool;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::{zasynclock, zerror};

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
    signal_tx: Arc<Mutex<Option<Sender<ZResult<()>>>>>,
    signal_rx: Arc<Mutex<Option<Sender<ZResult<()>>>>>,
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
            signal_tx: Arc::new(Mutex::new(None)),
            signal_rx: Arc::new(Mutex::new(None)),
        }
    }
}

impl SessionTransportLink {
    pub(crate) async fn start_tx(&self) {
        let mut guard = zasynclock!(self.signal_tx);
        if guard.is_none() {
            // SessionTransport for signal the termination of TX task
            let (sender_tx, receiver_tx) = bounded::<ZResult<()>>(1);
            *guard = Some(sender_tx);
            // Spawn the TX task
            let c_link = self.clone();
            task::spawn(async move {
                // Start the consume task
                let res = tx_task(c_link.clone(), receiver_tx).await;
                if let Err(e) = res {
                    log::debug!("{}", e);
                    let _ = c_link.transport.del_link(&c_link.inner).await;
                }
            });
        }
    }

    pub(crate) async fn stop_tx(&self) {
        let mut guard = zasynclock!(self.signal_tx);
        if let Some(signal_tx) = guard.take() {
            let _ = signal_tx.try_send(Ok(()));
        }
    }

    pub(crate) async fn start_rx(&self) {
        let mut guard = zasynclock!(self.signal_rx);
        if guard.is_none() {
            let (sender_rx, receiver_rx) = bounded::<ZResult<()>>(1);
            *guard = Some(sender_rx);
            // Spawn the RX task
            let c_link = self.clone();
            task::spawn(async move {
                // Start the consume task
                let res = rx_task(c_link.clone(), receiver_rx).await;
                if let Err(e) = res {
                    log::debug!("{}", e);
                    let _ = c_link.transport.del_link(&c_link.inner).await;
                }
            });
        }
    }

    pub(crate) async fn stop_rx(&self) {
        let mut guard = zasynclock!(self.signal_rx);
        if let Some(signal_rx) = guard.take() {
            let _ = signal_rx.try_send(Ok(()));
        }
    }

    #[inline]
    pub(crate) fn get_link(&self) -> &Link {
        &self.inner
    }

    #[inline]
    pub(crate) async fn schedule_zenoh_message(&self, msg: ZenohMessage, priority: usize) {
        self.pipeline.push_zenoh_message(msg, priority).await;
    }

    #[inline]
    pub(crate) async fn schedule_session_message(&self, msg: SessionMessage, priority: usize) {
        self.pipeline.push_session_message(msg, priority).await;
    }

    pub(crate) async fn close(self) -> ZResult<()> {
        // Send the TX stop signal
        let _ = self.stop_tx().await;
        // Drain what remains in the queue before exiting
        while let Some(batch) = self.pipeline.drain().await {
            let _ = self.inner.write_all(batch.get_buffer()).await;
        }

        // Send the RX stop signal
        let _ = self.stop_rx().await;
        // Close the underlying link
        self.inner.close().await
    }
}

/*************************************/
/*              TASKS                */
/*************************************/
async fn tx_task(link: SessionTransportLink, stop: Receiver<ZResult<()>>) -> ZResult<()> {
    let mut res: ZResult<()> = Ok(());

    // Keep draining the queue
    let consume = async {
        let keep_alive = Duration::from_millis(link.keep_alive);
        loop {
            // Pull a serialized batch from the queue
            match link.pipeline.pull().timeout(keep_alive).await {
                Ok((batch, index)) => {
                    // Send the buffer on the link
                    res = link.inner.write_all(batch.get_buffer()).await;
                    if res.is_err() {
                        break Ok(Ok(()));
                    }
                    // Reinsert the batch into the queue
                    link.pipeline.refill(batch, index).await;
                }
                Err(_) => {
                    let pid = None;
                    let attachment = None;
                    let message = SessionMessage::make_keep_alive(pid, attachment);
                    link.pipeline
                        .push_session_message(message, QUEUE_PRIO_CTRL)
                        .await;
                }
            }
        }
    };
    let _ = consume.race(stop.recv()).await;

    res
}

async fn read_stream(link: SessionTransportLink) -> ZResult<()> {
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
        let _ = match link.inner.read_exact(&mut length).timeout(lease).await {
            Ok(res) => res?,
            Err(_) => {
                // Link lease has expired
                let e = format!(
                    "Link has expired after {} milliseconds: {}",
                    link.lease, link.inner
                );
                return zerror!(ZErrorKind::IoError { descr: e });
            }
        };

        let to_read = u16::from_le_bytes(length) as usize;

        // Retrieve one buffer
        let mut buffer = if let Some(buffer) = pool.try_take() {
            buffer
        } else {
            pool.alloc()
        };

        let _ = match link
            .inner
            .read_exact(&mut buffer[0..to_read])
            .timeout(lease)
            .await
        {
            Ok(res) => res?,
            Err(_) => {
                // Link lease has expired
                let e = format!(
                    "Link has expired after {} milliseconds: {}",
                    link.lease, link.inner
                );
                return zerror!(ZErrorKind::IoError { descr: e });
            }
        };

        rbuf.add_slice(ArcSlice::new(buffer.into(), 0, to_read));

        while rbuf.can_read() {
            match rbuf.read_session_message() {
                Some(msg) => link.receive_message(msg).await,
                None => {
                    let e = format!("Decoding error on link: {}", link.inner);
                    return zerror!(ZErrorKind::IoError { descr: e });
                }
            }
        }
    }
}

async fn read_dgram(link: SessionTransportLink) -> ZResult<()> {
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
        let mut buffer = if let Some(buffer) = pool.try_take() {
            buffer
        } else {
            pool.alloc()
        };

        // Async read from the underlying link
        let res = match link.inner.read(&mut buffer).timeout(lease).await {
            Ok(res) => res,
            Err(_) => {
                // Link lease has expired
                let e = format!(
                    "Link has expired after {} milliseconds: {}",
                    link.lease, link.inner
                );
                return zerror!(ZErrorKind::IoError { descr: e });
            }
        };

        // Wait for incoming connections
        match res {
            Ok(n) => {
                if n == 0 {
                    // Reading 0 bytes means error
                    let e = format!("Zero bytes reading on link: {}", link.inner);
                    return zerror!(ZErrorKind::IoError { descr: e });
                }

                // Add the received bytes to the RBuf for deserialization
                rbuf.add_slice(ArcSlice::new(buffer.into(), 0, n));

                // Deserialize all the messages from the current RBuf
                while rbuf.can_read() {
                    match rbuf.read_session_message() {
                        Some(msg) => link.receive_message(msg).await,
                        None => {
                            let e = format!("Decoding error on link: {}", link.inner);
                            return zerror!(ZErrorKind::IoError { descr: e });
                        }
                    }
                }
            }
            Err(e) => {
                let e = format!("Reading error on link {}: {}", link.inner, e);
                return zerror!(ZErrorKind::IoError { descr: e });
            }
        };
    }
}

async fn rx_task(link: SessionTransportLink, stop: Receiver<ZResult<()>>) -> ZResult<()> {
    let read_loop = async {
        let res = if link.inner.is_streamed() {
            read_stream(link).await
        } else {
            read_dgram(link).await
        };
        match res {
            Ok(_) => Ok(Ok(())),
            Err(e) => Ok(Err(e)),
        }
    };

    // Execute the read loop
    let race_res = read_loop.race(stop.recv()).await;
    match race_res {
        Ok(read_res) => read_res,
        Err(_) => Ok(()),
    }
}
