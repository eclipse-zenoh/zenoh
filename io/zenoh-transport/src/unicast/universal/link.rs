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
use std::{
    array,
    future::{poll_fn, Future},
    pin::Pin,
    task::Poll,
    time::Duration,
};

use tokio_util::{sync::CancellationToken, task::TaskTracker};
use zenoh_link::Link;
use zenoh_protocol::{
    core::Priority,
    transport::{KeepAlive, TransportMessage},
};
use zenoh_result::{zerror, ZResult};
use zenoh_sync::RecyclingObjectPool;
#[cfg(feature = "stats")]
use {crate::common::stats::TransportStats, std::sync::Arc};

use super::transport::TransportUnicastUniversal;
use crate::{
    common::{
        batch::{BatchConfig, RBatch},
        pipeline::{
            PipelineConsumer, TransmissionPipeline, TransmissionPipelineConf,
            TransmissionPipelineConsumer, TransmissionPipelineProducer,
        },
        priority::TransportPriorityTx,
    },
    unicast::link::{TransportLinkUnicast, TransportLinkUnicastRx, TransportLinkUnicastTx},
};

#[derive(Clone)]
pub(super) struct TransportLinkUnicastUniversal {
    // The underlying link
    pub(super) link: TransportLinkUnicast,
    // The transmission pipeline
    pub(super) pipeline: TransmissionPipelineProducer,
    // The task handling substruct
    tracker: TaskTracker,
    token: CancellationToken,
    #[cfg(feature = "stats")]
    pub(super) stats: Arc<TransportStats>,
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
            wait_before_drop: transport.manager.config.wait_before_drop,
            max_wait_before_drop_fragments: transport.manager.config.max_wait_before_drop_fragments,
            wait_before_close: transport.manager.config.wait_before_close,
            batching_enabled: transport.manager.config.batching,
            batching_time_limit: transport.manager.config.queue_backoff,
            queue_alloc: transport.manager.config.queue_alloc,
        };

        // The pipeline
        let (producer, consumer) =
            TransmissionPipeline::make(config, priority_tx, link.link.supports_priorities());

        let result = Self {
            link,
            pipeline: producer,
            tracker: TaskTracker::new(),
            token: CancellationToken::new(),
            #[cfg(feature = "stats")]
            stats: TransportStats::new(Some(Arc::downgrade(&transport.stats)), Default::default()),
        };

        (result, consumer)
    }

    pub(super) fn start_tx(
        &mut self,
        transport: TransportUnicastUniversal,
        consumer: TransmissionPipelineConsumer,
        keep_alive: Duration,
    ) {
        // Spawn the TX task
        let mut tx = self.link.tx();
        let token = self.token.clone();
        #[cfg(feature = "stats")]
        let stats = self.stats.clone();
        let task = async move {
            let res = tx_task(
                consumer,
                &mut tx,
                keep_alive,
                token,
                #[cfg(feature = "stats")]
                stats,
            )
            .await;

            if let Err(e) = res {
                tracing::debug!("TX task failed: {}", e);
                // Spawn a task to avoid a deadlock waiting for this same task
                // to finish in the close() joining its handle
                // TODO(yuyuan): do more study to check which ZRuntime should be used or refine the
                // termination
                zenoh_runtime::ZRuntime::Net
                    .spawn(async move { transport.del_link(tx.inner.link()).await });
            }
        };
        self.tracker.spawn_on(task, &zenoh_runtime::ZRuntime::TX);
    }

    pub(super) fn start_rx(&mut self, transport: TransportUnicastUniversal, lease: Duration) {
        let priorities = self.link.config.priorities.clone();
        let reliability = self.link.config.reliability;
        let mut rx = self.link.rx();
        let token = self.token.clone();
        #[cfg(feature = "stats")]
        let stats = self.stats.clone();
        let task = async move {
            // Start the consume task
            let res = rx_task(
                &mut rx,
                transport.clone(),
                lease,
                transport.manager.config.link_rx_buffer_size,
                token,
                #[cfg(feature = "stats")]
                stats,
            )
            .await;

            // TODO(yuyuan): improve this callback
            if let Err(e) = res {
                tracing::debug!("RX task failed: {}", e);

                // Spawn a task to avoid a deadlock waiting for this same task
                // to finish in the close() joining its handle
                // WARN: Must be spawned on RX

                zenoh_runtime::ZRuntime::RX.spawn(async move {
                    transport
                        .del_link(Link::new_unicast(&rx.link, priorities, reliability))
                        .await
                });

                // // WARN: This ZRuntime blocks
                // zenoh_runtime::ZRuntime::Net
                //     .spawn(async move { transport.del_link((&rx.link).into()).await });

                // // WARN: This cloud block
                // transport.del_link((&rx.link).into()).await;
            }
        };
        // WARN: If this is on ZRuntime::TX, a deadlock would occur.
        self.tracker.spawn_on(task, &zenoh_runtime::ZRuntime::RX);
    }

    pub(super) async fn close(self) -> ZResult<()> {
        tracing::trace!("{}: closing", self.link);

        self.tracker.close();
        self.token.cancel();
        self.pipeline.disable();
        self.tracker.wait().await;

        self.link.close(None).await
    }
}

/*************************************/
/*              TASKS                */
/*************************************/
async fn tx_task(
    pipeline: TransmissionPipelineConsumer,
    link: &mut TransportLinkUnicastTx,
    keep_alive: Duration,
    token: CancellationToken,
    #[cfg(feature = "stats")] stats: Arc<TransportStats>,
) -> ZResult<()> {
    if link.inner.link.supports_priorities() {
        let tasks = pipeline
            .split()
            .into_iter()
            .map(|pipeline| {
                let mut link = link.clone();
                let token = token.clone();
                #[cfg(feature = "stats")]
                let stats = stats.clone();
                zenoh_runtime::ZRuntime::TX.spawn(async move {
                    write_loop(
                        pipeline.priority(),
                        pipeline,
                        &mut link,
                        keep_alive,
                        token,
                        #[cfg(feature = "stats")]
                        stats,
                    )
                    .await
                })
            })
            .collect::<Vec<_>>();
        for task in tasks {
            task.await.unwrap()?;
        }
    } else {
        write_loop(
            Priority::Control,
            pipeline,
            link,
            keep_alive,
            token,
            #[cfg(feature = "stats")]
            stats,
        )
        .await?;
    }
    Ok(())
}

async fn write_loop(
    keep_alive_prio: Priority,
    mut pipeline: impl PipelineConsumer,
    link: &mut TransportLinkUnicastTx,
    keep_alive: Duration,
    token: CancellationToken,
    #[cfg(feature = "stats")] stats: Arc<TransportStats>,
) -> ZResult<()> {
    loop {
        tokio::select! {
            res = tokio::time::timeout(keep_alive, pipeline.pull()) => {
                match res {
                    Ok(Some((mut batch, priority))) => {
                        link.send_batch(&mut batch, priority).await?;

                        #[cfg(feature = "stats")]
                        {
                            stats.inc_tx_t_msgs(batch.stats.t_msgs);
                            stats.inc_tx_bytes(batch.len() as usize);
                        }

                        // Reinsert the batch into the queue
                        pipeline.refill(batch, priority);
                    },
                    Ok(None) => {
                        // The queue has been disabled: break the tx loop, drain the queue, and exit
                        break;
                    },
                    Err(_) => {
                        // A timeout occurred, no control/data messages have been sent during
                        // the keep_alive period, we need to send a KeepAlive message
                        let message: TransportMessage = KeepAlive.into();

                        #[allow(unused_variables)] // Used when stats feature is enabled
                        let n = link.send(&message, keep_alive_prio).await?;

                        #[cfg(feature = "stats")]
                        {
                            stats.inc_tx_t_msgs(1);
                            stats.inc_tx_bytes(n);
                        }
                    }
                }
            },

            _ = token.cancelled() => break
        }
    }

    // Drain the transmission pipeline and write remaining bytes on the wire
    let mut batches = pipeline.drain();
    for (mut b, prio) in batches.drain(..) {
        tokio::time::timeout(keep_alive, link.send_batch(&mut b, prio))
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
    rx_buffer_size: usize,
    token: CancellationToken,
    #[cfg(feature = "stats")] stats: Arc<TransportStats>,
) -> ZResult<()> {
    // The pool of buffers
    let mtu = link.config.batch.mtu as usize;
    let mut n = rx_buffer_size / mtu;
    if n == 0 {
        tracing::debug!("RX configured buffer of {rx_buffer_size} bytes is too small for {link} that has an MTU of {mtu} bytes. Defaulting to {mtu} bytes for RX buffer.");
        n = 1;
    }
    let pool = RecyclingObjectPool::new(n, move || vec![0_u8; mtu].into_boxed_slice());

    if link.link.supports_priorities() {
        let mut tasks: [_; Priority::NUM] = array::from_fn(|prio| {
            let mut link = link.clone();
            let transport = transport.clone();
            let token = token.clone();
            #[cfg(feature = "stats")]
            let stats = stats.clone();
            let pool = pool.clone();
            zenoh_runtime::ZRuntime::RX.spawn(async move {
                read_loop(
                    Priority::try_from(prio as u8).unwrap(),
                    &mut link,
                    transport,
                    lease,
                    token,
                    #[cfg(feature = "stats")]
                    stats,
                    &pool,
                )
                .await
            })
        });
        poll_fn(|cx| {
            for task in &mut tasks {
                if let Poll::Ready(res) = Pin::new(task).poll(cx) {
                    return Poll::Ready(res.unwrap());
                }
            }
            Poll::Pending
        })
        .await
    } else {
        read_loop(
            Priority::Control,
            link,
            transport,
            lease,
            token,
            #[cfg(feature = "stats")]
            stats,
            &pool,
        )
        .await
    }
}

async fn read_loop<F: Fn() -> Box<[u8]>>(
    priority: Priority,
    link: &mut TransportLinkUnicastRx,
    transport: TransportUnicastUniversal,
    lease: Duration,
    token: CancellationToken,
    #[cfg(feature = "stats")] stats: Arc<TransportStats>,
    pool: &RecyclingObjectPool<Box<[u8]>, F>,
) -> ZResult<()> {
    async fn read<F: Fn() -> Box<[u8]>>(
        link: &mut TransportLinkUnicastRx,
        priority: Priority,
        pool: &RecyclingObjectPool<Box<[u8]>, F>,
    ) -> ZResult<RBatch> {
        let batch = link
            .recv_batch(|| pool.try_take().unwrap_or_else(|| pool.alloc()), priority)
            .await?;
        Ok(batch)
    }

    let l = Link::new_unicast(
        &link.link,
        link.config.priorities.clone(),
        link.config.reliability,
    );
    loop {
        tokio::select! {
            batch = tokio::time::timeout(lease, read(link, priority, pool)) => {
                let batch = batch.map_err(|_| zerror!("{}: expired after {} milliseconds", link, lease.as_millis()))??;
                #[cfg(feature = "stats")]
                {
                    stats.inc_rx_bytes(2 + batch.len()); // Account for the batch len encoding (16 bits)
                }
                transport.read_messages(batch, &l, #[cfg(feature = "stats")] &stats)?;
            }

            _ = token.cancelled() => return Ok(()),
        }
    }
}
