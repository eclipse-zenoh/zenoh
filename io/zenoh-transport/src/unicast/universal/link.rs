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
use super::transport::TransportUnicastUniversal;
#[cfg(feature = "stats")]
use crate::common::stats::TransportStats;
use crate::{
    common::{
        batch::RBatch,
        pipeline::{
            TransmissionPipeline, TransmissionPipelineConf, TransmissionPipelineConsumer,
            TransmissionPipelineProducer,
        },
        priority::TransportPriorityTx,
    },
    unicast::link::{TransportLinkUnicast, TransportLinkUnicastRx, TransportLinkUnicastTx},
    TransportExecutor,
};
use async_std::prelude::FutureExt;
use async_std::task;
use async_std::task::JoinHandle;
use std::{
    ops::Deref,
    sync::{Arc, RwLock},
    time::Duration,
};
use zenoh_buffers::ZSliceBuffer;
use zenoh_core::zwrite;
use zenoh_protocol::transport::{KeepAlive, TransportMessage};
use zenoh_result::{zerror, ZResult};
use zenoh_sync::{RecyclingObject, RecyclingObjectPool, Signal};

pub(super) struct TransportLinkUnicastUniversalInner {
    // The underlying link
    pub(super) link: TransportLinkUnicast,
    // The transmission pipeline
    pub(super) pipeline: TransmissionPipelineProducer,
    // The transport this link is associated to
    transport: TransportUnicastUniversal,
    // The signals to stop TX/RX tasks
    handle_tx: RwLock<Option<async_executor::Task<()>>>,
    signal_rx: Signal,
    handle_rx: RwLock<Option<JoinHandle<()>>>,
}

#[derive(Clone)]
pub(super) struct TransportLinkUnicastUniversal(Arc<TransportLinkUnicastUniversalInner>);

impl Deref for TransportLinkUnicastUniversal {
    type Target = Arc<TransportLinkUnicastUniversalInner>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TransportLinkUnicastUniversal {
    pub(super) fn new(
        transport: TransportUnicastUniversal,
        link: TransportLinkUnicast,
        priority_tx: &[TransportPriorityTx],
    ) -> (Self, TransmissionPipelineConsumer) {
        assert!(!priority_tx.is_empty());

        let config = TransmissionPipelineConf {
            is_streamed: link.link.is_streamed(),
            #[cfg(feature = "transport_compression")]
            is_compression: link.config.is_compression,
            batch_size: link.config.mtu,
            queue_size: transport.manager.config.queue_size,
            backoff: transport.manager.config.queue_backoff,
        };
        // The pipeline
        let (producer, consumer) = TransmissionPipeline::make(config, priority_tx);

        let inner = TransportLinkUnicastUniversalInner {
            link,
            pipeline: producer,
            transport,
            handle_tx: RwLock::new(None),
            signal_rx: Signal::new(),
            handle_rx: RwLock::new(None),
        };

        (Self(Arc::new(inner)), consumer)
    }
}

impl TransportLinkUnicastUniversal {
    pub(super) fn start_tx(
        &mut self,
        consumer: TransmissionPipelineConsumer,
        executor: &TransportExecutor,
        keep_alive: Duration,
    ) {
        let mut guard = zwrite!(self.handle_tx);
        if guard.is_none() {
            // Spawn the TX task
            let mut tx = self.link.tx();
            let c_transport = self.transport.clone();
            let handle = executor.spawn(async move {
                let res = tx_task(
                    consumer,
                    &mut tx,
                    keep_alive,
                    #[cfg(feature = "stats")]
                    c_transport.stats.clone(),
                )
                .await;
                if let Err(e) = res {
                    log::debug!("{}", e);
                    // Spawn a task to avoid a deadlock waiting for this same task
                    // to finish in the close() joining its handle
                    task::spawn(async move { c_transport.del_link(tx.inner.link()).await });
                }
            });
            *guard = Some(handle);
        }
    }

    pub(super) fn stop_tx(&mut self) {
        self.pipeline.disable();
    }

    pub(super) fn start_rx(&mut self, lease: Duration) {
        let mut guard = zwrite!(self.handle_rx);
        if guard.is_none() {
            // Spawn the RX task
            let mut rx = self.link.rx();
            let c_transport = self.transport.clone();
            let c_signal = self.signal_rx.clone();
            let c_rx_buffer_size = self.transport.manager.config.link_rx_buffer_size;

            let handle = task::spawn(async move {
                // Start the consume task
                let res = rx_task(
                    &mut rx,
                    c_transport.clone(),
                    lease,
                    c_signal.clone(),
                    c_rx_buffer_size,
                )
                .await;
                c_signal.trigger();
                if let Err(e) = res {
                    log::debug!("{}", e);
                    // Spawn a task to avoid a deadlock waiting for this same task
                    // to finish in the close() joining its handle
                    task::spawn(async move { c_transport.del_link(rx.inner.link()).await });
                }
            });
            *guard = Some(handle);
        }
    }

    pub(super) fn stop_rx(&mut self) {
        self.signal_rx.trigger();
    }

    pub(super) async fn close(mut self) -> ZResult<()> {
        log::trace!("{}: closing", self.link);
        self.stop_tx();
        self.stop_rx();

        let handle_tx = zwrite!(self.handle_tx).take();
        if let Some(handle) = handle_tx {
            handle.await;
        }

        let handle_rx = zwrite!(self.handle_rx).take();
        if let Some(handle) = handle_rx {
            handle.await;
        }

        self.link.close(None).await
    }
}

/*************************************/
/*              TASKS                */
/*************************************/
async fn tx_task(
    mut pipeline: TransmissionPipelineConsumer,
    link: &mut TransportLinkUnicastTx,
    keep_alive: Duration,
    #[cfg(feature = "stats")] stats: Arc<TransportStats>,
) -> ZResult<()> {
    loop {
        match pipeline.pull().timeout(keep_alive).await {
            Ok(res) => match res {
                Some((mut batch, priority)) => {
                    link.send_batch(&mut batch).await?;

                    #[cfg(feature = "stats")]
                    {
                        stats.inc_tx_t_msgs(batch.stats.t_msgs);
                        stats.inc_tx_bytes(batch.len() as usize);
                    }

                    // Reinsert the batch into the queue
                    pipeline.refill(batch, priority);
                }
                None => break,
            },
            Err(_) => {
                let message: TransportMessage = KeepAlive.into();

                #[allow(unused_variables)] // Used when stats feature is enabled
                let n = link.send(&message).await?;
                #[cfg(feature = "stats")]
                {
                    stats.inc_tx_t_msgs(1);
                    stats.inc_tx_bytes(n);
                }
            }
        }
    }

    // Drain the transmission pipeline and write remaining bytes on the wire
    let mut batches = pipeline.drain();
    for (mut b, _) in batches.drain(..) {
        link.send_batch(&mut b)
            .timeout(keep_alive)
            .await
            .map_err(|_| zerror!("{}: flush failed after {} ms", link, keep_alive.as_millis()))??;

        #[cfg(feature = "stats")]
        {
            stats.inc_tx_t_msgs(b.stats.t_msgs);
            stats.inc_tx_bytes(b.len() as usize);
        }
    }

    Ok(())
}

async fn rx_task(
    link: &mut TransportLinkUnicastRx,
    transport: TransportUnicastUniversal,
    lease: Duration,
    signal: Signal,
    rx_buffer_size: usize,
) -> ZResult<()> {
    enum Action {
        Read(RBatch),
        Stop,
    }

    async fn read<T, F>(
        link: &mut TransportLinkUnicastRx,
        pool: &RecyclingObjectPool<T, F>,
    ) -> ZResult<Action>
    where
        T: ZSliceBuffer + 'static,
        F: Fn() -> T,
        RecyclingObject<T>: ZSliceBuffer,
    {
        let batch = link
            .recv_batch(|| pool.try_take().unwrap_or_else(|| pool.alloc()))
            .await?;
        Ok(Action::Read(batch))
    }

    async fn stop(signal: Signal) -> ZResult<Action> {
        signal.wait().await;
        Ok(Action::Stop)
    }

    // The pool of buffers
    let mtu = link.inner.config.mtu as usize;
    let mut n = rx_buffer_size / mtu;
    if rx_buffer_size % mtu != 0 {
        n += 1;
    }

    let pool = RecyclingObjectPool::new(n, || vec![0_u8; mtu].into_boxed_slice());
    while !signal.is_triggered() {
        // Async read from the underlying link
        let action = read(link, &pool)
            .race(stop(signal.clone()))
            .timeout(lease)
            .await
            .map_err(|_| zerror!("{}: expired after {} milliseconds", link, lease.as_millis()))??;
        match action {
            Action::Read(batch) => {
                #[cfg(feature = "stats")]
                {
                    transport.stats.inc_rx_bytes(2 + n); // Account for the batch len encoding (16 bits)
                }
                transport.read_messages(batch, &link.inner)?;
            }
            Action::Stop => break,
        }
    }

    Ok(())
}
