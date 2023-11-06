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
    unicast::link::TransportLinkUnicast,
    TransportExecutor,
};
use async_std::prelude::FutureExt;
use async_std::task;
use async_std::task::JoinHandle;
use std::{sync::Arc, time::Duration};
use zenoh_buffers::ZSliceBuffer;
use zenoh_protocol::transport::{BatchSize, KeepAlive, TransportMessage};
use zenoh_result::{zerror, ZResult};
use zenoh_sync::{RecyclingObject, RecyclingObjectPool, Signal};

#[derive(Clone)]
pub(super) struct TransportLinkUnicastUniversal {
    // The underlying link
    pub(super) link: TransportLinkUnicast,
    // The transmission pipeline
    pub(super) pipeline: Option<TransmissionPipelineProducer>,
    // The transport this link is associated to
    transport: TransportUnicastUniversal,
    // The signals to stop TX/RX tasks
    handle_tx: Option<Arc<async_executor::Task<()>>>,
    signal_rx: Signal,
    handle_rx: Option<Arc<JoinHandle<()>>>,
}

impl TransportLinkUnicastUniversal {
    pub(super) fn new(transport: TransportUnicastUniversal, link: TransportLinkUnicast) -> Self {
        Self {
            link,
            pipeline: None,
            transport,
            handle_tx: None,
            signal_rx: Signal::new(),
            handle_rx: None,
        }
    }
}

impl TransportLinkUnicastUniversal {
    pub(super) fn start_tx(
        &mut self,
        executor: &TransportExecutor,
        keep_alive: Duration,
        batch_size: BatchSize,
        priority_tx: &[TransportPriorityTx],
    ) {
        if self.handle_tx.is_none() {
            let config = TransmissionPipelineConf {
                is_streamed: self.link.link.is_streamed(),
                #[cfg(feature = "transport_compression")]
                is_compression: self.link.config.is_compression,
                batch_size: batch_size.min(self.link.link.get_mtu()),
                queue_size: self.transport.manager.config.queue_size,
                backoff: self.transport.manager.config.queue_backoff,
            };

            // The pipeline
            let (producer, consumer) = TransmissionPipeline::make(config, priority_tx);
            self.pipeline = Some(producer);

            // Spawn the TX task
            let c_link = self.link.clone();
            let c_transport = self.transport.clone();
            let handle = executor.spawn(async move {
                let res = tx_task(
                    consumer,
                    c_link.clone(),
                    keep_alive,
                    #[cfg(feature = "stats")]
                    c_transport.stats.clone(),
                )
                .await;
                if let Err(e) = res {
                    log::debug!("{}", e);
                    // Spawn a task to avoid a deadlock waiting for this same task
                    // to finish in the close() joining its handle
                    task::spawn(async move { c_transport.del_link(&c_link).await });
                }
            });
            self.handle_tx = Some(Arc::new(handle));
        }
    }

    pub(super) fn stop_tx(&mut self) {
        if let Some(pl) = self.pipeline.as_ref() {
            pl.disable();
        }
    }

    pub(super) fn start_rx(&mut self, lease: Duration, batch_size: u16) {
        if self.handle_rx.is_none() {
            // Spawn the RX task
            let c_link = self.link.clone();
            let c_transport = self.transport.clone();
            let c_signal = self.signal_rx.clone();
            let c_rx_buffer_size = self.transport.manager.config.link_rx_buffer_size;

            let handle = task::spawn(async move {
                // Start the consume task
                let res = rx_task(
                    c_link.clone(),
                    c_transport.clone(),
                    lease,
                    c_signal.clone(),
                    batch_size,
                    c_rx_buffer_size,
                )
                .await;
                c_signal.trigger();
                if let Err(e) = res {
                    log::debug!("{}", e);
                    // Spawn a task to avoid a deadlock waiting for this same task
                    // to finish in the close() joining its handle
                    task::spawn(async move { c_transport.del_link(&c_link).await });
                }
            });
            self.handle_rx = Some(Arc::new(handle));
        }
    }

    pub(super) fn stop_rx(&mut self) {
        self.signal_rx.trigger();
    }

    pub(super) async fn close(mut self) -> ZResult<()> {
        log::trace!("{}: closing", self.link);
        self.stop_rx();
        if let Some(handle) = self.handle_rx.take() {
            // SAFETY: it is safe to unwrap the Arc since we have the ownership of the whole link
            let handle_rx = Arc::try_unwrap(handle).unwrap();
            handle_rx.await;
        }

        self.stop_tx();
        if let Some(handle) = self.handle_tx.take() {
            // SAFETY: it is safe to unwrap the Arc since we have the ownership of the whole link
            let handle_tx = Arc::try_unwrap(handle).unwrap();
            handle_tx.await;
        }

        self.link.close().await
    }
}

/*************************************/
/*              TASKS                */
/*************************************/
async fn tx_task(
    mut pipeline: TransmissionPipelineConsumer,
    link: TransportLinkUnicast,
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
                        stats.inc_tx_bytes(bytes.len());
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
    link: TransportLinkUnicast,
    transport: TransportUnicastUniversal,
    lease: Duration,
    signal: Signal,
    rx_batch_size: BatchSize,
    rx_buffer_size: usize,
) -> ZResult<()> {
    enum Action {
        Read(RBatch),
        Stop,
    }

    async fn read<T, F>(
        link: &TransportLinkUnicast,
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
    let mtu = link.link.get_mtu().min(rx_batch_size) as usize;
    let mut n = rx_buffer_size / mtu;
    if rx_buffer_size % mtu != 0 {
        n += 1;
    }

    let pool = RecyclingObjectPool::new(n, || vec![0_u8; mtu].into_boxed_slice());
    while !signal.is_triggered() {
        // Async read from the underlying link
        let action = read(&link, &pool)
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
                transport.read_messages(batch, &link)?;
            }
            Action::Stop => break,
        }
    }

    Ok(())
}
