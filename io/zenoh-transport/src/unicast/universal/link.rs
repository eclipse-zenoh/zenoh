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
        batch::{BatchConfig, RBatch},
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
use tokio::task;
use tokio::task::JoinHandle;

pub(super) struct Tasks {
    // The handlers to stop TX/RX tasks
    handle_tx: RwLock<Option<async_executor::Task<()>>>,
    signal_rx: Signal,
    handle_rx: RwLock<Option<JoinHandle<()>>>,
}

#[derive(Clone)]
pub(super) struct TransportLinkUnicastUniversal {
    // The underlying link
    pub(super) link: TransportLinkUnicast,
    // The transmission pipeline
    pub(super) pipeline: TransmissionPipelineProducer,
    // The task handling substruct
    tasks: Arc<Tasks>,
}

impl TransportLinkUnicastUniversal {
    pub(super) fn new(
        transport: &TransportUnicastUniversal,
        link: TransportLinkUnicast,
        priority_tx: &[TransportPriorityTx],
    ) -> (Self, TransmissionPipelineConsumer) {
        assert!(!priority_tx.is_empty());

        let config = TransmissionPipelineConf {
            batch: BatchConfig {
                mtu: link.config.batch.mtu,
                is_streamed: link.link.is_streamed(),
                #[cfg(feature = "transport_compression")]
                is_compression: link.config.batch.is_compression,
            },
            queue_size: transport.manager.config.queue_size,
            backoff: transport.manager.config.queue_backoff,
        };

        // The pipeline
        let (producer, consumer) = TransmissionPipeline::make(config, priority_tx);

        let tasks = Arc::new(Tasks {
            handle_tx: RwLock::new(None),
            signal_rx: Signal::new(),
            handle_rx: RwLock::new(None),
        });

        let result = Self {
            link,
            pipeline: producer,
            tasks,
        };

        (result, consumer)
    }
}

impl TransportLinkUnicastUniversal {
    pub(super) fn start_tx(
        &mut self,
        transport: TransportUnicastUniversal,
        consumer: TransmissionPipelineConsumer,
        executor: &TransportExecutor,
        keep_alive: Duration,
    ) {
        let mut guard = zwrite!(self.tasks.handle_tx);
        if guard.is_none() {
            // Spawn the TX task
            let mut tx = self.link.tx();
            let handle = executor.spawn(async move {
                let res = tx_task(
                    consumer,
                    &mut tx,
                    keep_alive,
                    #[cfg(feature = "stats")]
                    transport.stats.clone(),
                )
                .await;
                if let Err(e) = res {
                    log::debug!("{}", e);
                    // Spawn a task to avoid a deadlock waiting for this same task
                    // to finish in the close() joining its handle
                    task::spawn(async move { transport.del_link(tx.inner.link()).await });
                }
            });
            *guard = Some(handle);
        }
    }

    pub(super) fn stop_tx(&mut self) {
        self.pipeline.disable();
    }

    pub(super) fn start_rx(&mut self, transport: TransportUnicastUniversal, lease: Duration) {
        let mut guard = zwrite!(self.tasks.handle_rx);
        if guard.is_none() {
            // Spawn the RX task
            let mut rx = self.link.rx();
            let c_signal = self.tasks.signal_rx.clone();

            let handle = task::spawn(async move {
                // Start the consume task
                let res = rx_task(
                    &mut rx,
                    transport.clone(),
                    lease,
                    c_signal.clone(),
                    transport.manager.config.link_rx_buffer_size,
                )
                .await;
                c_signal.trigger();
                if let Err(e) = res {
                    log::debug!("{}", e);
                    // Spawn a task to avoid a deadlock waiting for this same task
                    // to finish in the close() joining its handle
                    task::spawn(async move { transport.del_link((&rx.link).into()).await });
                }
            });
            *guard = Some(handle);
        }
    }

    pub(super) fn stop_rx(&mut self) {
        self.tasks.signal_rx.trigger();
    }

    pub(super) async fn close(mut self) -> ZResult<()> {
        log::trace!("{}: closing", self.link);
        self.stop_rx();
        if let Some(handle) = self.handle_rx.take() {
            // SAFETY: it is safe to unwrap the Arc since we have the ownership of the whole link
            let handle_rx = Arc::try_unwrap(handle).unwrap();
            handle_rx.await?;
        }

        self.stop_tx();
        self.stop_rx();

        let handle_tx = zwrite!(self.tasks.handle_tx).take();
        if let Some(handle) = handle_tx {
            handle.await;
        }

        let handle_rx = zwrite!(self.tasks.handle_rx).take();
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
    let mtu = link.batch.max_buffer_size();
    let mut n = rx_buffer_size / mtu;
    if rx_buffer_size % mtu != 0 {
        n += 1;
    }

    let pool = RecyclingObjectPool::new(n, || vec![0_u8; mtu].into_boxed_slice());
    let l = (&link.link).into();
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
                transport.read_messages(batch, &l)?;
            }
            Action::Stop => break,
        }
    }

    Ok(())
}
