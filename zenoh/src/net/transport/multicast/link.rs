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
use super::super::defaults::ZN_RX_BUFF_SIZE;
use super::common::{conduit::TransportConduitTx, pipeline::TransmissionPipeline};
use super::protocol::io::{ZBuf, ZSlice};
use super::transport::TransportMulticastInner;
use crate::net::link::{LinkMulticast, Locator};
use async_std::prelude::*;
use async_std::task;
use async_std::task::JoinHandle;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use zenoh_util::collections::RecyclingObjectPool;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::sync::Signal;
use zenoh_util::zerror;

#[derive(Clone)]
pub(crate) struct TransportLinkMulticast {
    // The underlying link
    pub(super) inner: LinkMulticast,
    // The transport this link is associated to
    transport: TransportMulticastInner,
    // The transmission pipeline
    pipeline: Option<Arc<TransmissionPipeline>>,
    // The signals to stop TX/RX tasks
    handle_tx: Option<Arc<JoinHandle<()>>>,
    active_rx: Arc<AtomicBool>,
    signal_rx: Signal,
    handle_rx: Option<Arc<JoinHandle<()>>>,
}

impl TransportLinkMulticast {
    pub(crate) fn new(
        transport: TransportMulticastInner,
        link: LinkMulticast,
    ) -> TransportLinkMulticast {
        TransportLinkMulticast {
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

impl TransportLinkMulticast {
    #[inline]
    pub(crate) fn get_link(&self) -> &LinkMulticast {
        &self.inner
    }

    #[inline]
    pub(crate) fn get_pipeline(&self) -> Option<Arc<TransmissionPipeline>> {
        self.pipeline.clone()
    }

    pub(crate) fn start_tx(&mut self, batch_size: usize, conduit_tx: Arc<[TransportConduitTx]>) {
        if self.handle_tx.is_none() {
            // The pipeline
            let pipeline = Arc::new(TransmissionPipeline::new(
                batch_size.min(self.inner.get_mtu()),
                false,
                conduit_tx,
            ));
            self.pipeline = Some(pipeline.clone());

            // Spawn the TX task
            let c_link = self.inner.clone();
            let c_transport = self.transport.clone();
            let handle = task::spawn(async move {
                let res = tx_task(pipeline, c_link.clone()).await;
                if let Err(e) = res {
                    log::debug!("{}", e);
                    // Spawn a task to avoid a deadlock waiting for this same task
                    // to finish in the close() joining its handle
                    task::spawn(async move { c_transport.delete().await });
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

    pub(crate) fn start_rx(&mut self) {
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
                    c_signal.clone(),
                    c_active.clone(),
                )
                .await;
                c_active.store(false, Ordering::Release);
                if let Err(e) = res {
                    log::debug!("{}", e);
                    // Spawn a task to avoid a deadlock waiting for this same task
                    // to finish in the close() joining its handle
                    task::spawn(async move { c_transport.delete().await });
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
async fn tx_task(pipeline: Arc<TransmissionPipeline>, link: LinkMulticast) -> ZResult<()> {
    while let Some((batch, index)) = pipeline.pull().await {
        // Send the buffer on the link
        let _ = link.write_all(batch.as_bytes()).await?;
        // Reinsert the batch into the queue
        pipeline.refill(batch, index);
    }

    // Drain the transmission pipeline and write remaining bytes on the wire
    let mut batches = pipeline.drain();
    for (b, _) in batches.drain(..) {
        let _ = link.write_all(b.as_bytes()).await?;
    }

    Ok(())
}

async fn rx_task(
    link: LinkMulticast,
    transport: TransportMulticastInner,
    signal: Signal,
    active: Arc<AtomicBool>,
) -> ZResult<()> {
    enum Action {
        Read((usize, Locator)),
        Stop,
    }

    async fn read(link: &LinkMulticast, buffer: &mut [u8]) -> ZResult<Action> {
        let (n, loc) = link.read(buffer).await?;
        Ok(Action::Read((n, loc)))
    }

    async fn stop(signal: Signal) -> ZResult<Action> {
        signal.wait().await;
        Ok(Action::Stop)
    }

    // The ZBuf to read a message batch onto
    let mut zbuf = ZBuf::new();
    // The pool of buffers
    let n = 1 + (*ZN_RX_BUFF_SIZE / link.get_mtu());
    let pool = RecyclingObjectPool::new(n, || vec![0u8; link.get_mtu()].into_boxed_slice());
    while active.load(Ordering::Acquire) {
        // Clear the zbuf
        zbuf.clear();
        // Retrieve one buffer
        let mut buffer = pool.try_take().unwrap_or_else(|| pool.alloc());

        // Async read from the underlying link
        let action = read(&link, &mut buffer).race(stop(signal.clone())).await?;
        match action {
            Action::Read((n, loc)) => {
                if n == 0 {
                    // Reading 0 bytes means error
                    let e = format!("{}: zero bytes reading", link);
                    return zerror!(ZErrorKind::IoError { descr: e });
                }

                // Add the received bytes to the ZBuf for deserialization
                zbuf.add_zslice(ZSlice::new(buffer.into(), 0, n));

                // Deserialize all the messages from the current ZBuf
                while zbuf.can_read() {
                    match zbuf.read_transport_message() {
                        Some(msg) => transport.receive_message(msg, &loc)?,
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
