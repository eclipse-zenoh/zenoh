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
    future::poll_fn,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    task::Poll,
    time::{Duration, Instant},
};

use crossbeam_utils::CachePadded;
use futures::{
    future::{select_all, OptionFuture},
    task::AtomicWaker,
    FutureExt,
};
use tokio::sync::Notify;
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use zenoh_link::Link;
#[cfg(feature = "unstable")]
use zenoh_protocol::core::Priority;
use zenoh_protocol::{
    core::Priority,
    transport::{KeepAlive, TransportMessage},
};
use zenoh_result::{bail, zerror, ZResult};
#[cfg(feature = "unstable")]
use zenoh_sync::{event, Notifier, Waiter};
use zenoh_sync::RecyclingObjectPool;

use super::transport::TransportUnicastUniversal;
#[cfg(feature = "stats")]
use crate::common::stats::TransportStats;
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
    #[cfg(feature = "unstable")]
    // Notifier for a BlockFirst message to be ready to be sent
    // (after the previous one has been sent)
    pub block_first_notifiers: [Notifier; Priority::NUM],
    #[cfg(feature = "unstable")]
    // Waiter for a BlockFirst message to be ready to be sent
    pub block_first_waiters: [Waiter; Priority::NUM],
    #[cfg(feature = "stats")]
    pub(super) stats: zenoh_stats::LinkStats,
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

        #[cfg(feature = "stats")]
        let stats = transport
            .stats
            .link_stats(link.link.get_src(), link.link.get_dst());

        #[cfg(feature = "unstable")]
        let mut block_first_notifiers = Vec::new();
        #[cfg(feature = "unstable")]
        let mut block_first_waiters = Vec::new();
        #[cfg(feature = "unstable")]
        for _ in 0..Priority::NUM {
            let (notifier, waiter) = event::new();
            // notify to be make the BlockFirst "slot" available
            notifier.notify().unwrap();
            block_first_notifiers.push(notifier);
            block_first_waiters.push(waiter);
        }

        let result = Self {
            link,
            pipeline: producer,
            tracker: TaskTracker::new(),
            token: CancellationToken::new(),
            #[cfg(feature = "unstable")]
            block_first_notifiers: block_first_notifiers.try_into().ok().unwrap(),
            #[cfg(feature = "unstable")]
            block_first_waiters: block_first_waiters.try_into().ok().unwrap(),
            #[cfg(feature = "stats")]
            stats,
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
        let latest_message_tracker = LatestMessageTracker::new::<{ Priority::NUM }>(keep_alive);
        let (res, _, _) = select_all(pipeline.split().into_iter().map(|pipeline| {
            let mut link = link.clone();
            let token = token.clone();
            let latest_message_tracker = latest_message_tracker.clone();
            #[cfg(feature = "stats")]
            let stats = stats.clone();
            zenoh_runtime::ZRuntime::TX.spawn(async move {
                write_loop(
                    pipeline.priority(),
                    pipeline,
                    &mut link,
                    latest_message_tracker,
                    token,
                    #[cfg(feature = "stats")]
                    stats,
                )
                .await
            })
        }))
        .await;
        res.unwrap()?;
    } else {
        write_loop(
            Priority::Control,
            pipeline,
            link,
            LatestMessageTracker::new::<1>(keep_alive),
            token,
            #[cfg(feature = "stats")]
            stats,
        )
        .await?;
    }
    Ok(())
}

async fn write_loop(
    priority: Priority,
    mut pipeline: impl PipelineConsumer,
    link: &mut TransportLinkUnicastTx,
    latest_message_tracker: Arc<LatestMessageTracker>,
    token: CancellationToken,
    #[cfg(feature = "stats")] stats: zenoh_stats::LinkStats,
) -> ZResult<()> {
    loop {
        tokio::select! {
            pull = pipeline.pull() => {
                let Some((mut batch, priority)) = pull else {
                    // The queue has been disabled: break the tx loop, drain the queue, and exit
                    break
                };
                link.send_batch(&mut batch, priority).await?;
                // inform the latest message tracker that a message has been sent
                latest_message_tracker.set(priority);

                #[cfg(feature = "stats")]
                {
                    stats.inc_bytes(zenoh_stats::Tx, batch.len() as u64);
                    stats.inc_transport_message(zenoh_stats::Tx, batch.stats.t_msgs as u64);
                }

                // Reinsert the batch into the queue
                pipeline.refill(batch, priority);
            },
            _ = latest_message_tracker.wait_timeout(priority) => {
                // A timeout occurred, no control/data messages have been sent during
                // the keep_alive period, we need to send a KeepAlive message
                let message: TransportMessage = KeepAlive.into();

                #[allow(unused_variables)] // Used when stats feature is enabled
                let n = link.send(&message, Priority::Control).await?;

                #[cfg(feature = "stats")]
                {
                    stats.inc_bytes(zenoh_stats::Tx, n as u64);
                    stats.inc_transport_message(zenoh_stats::Tx, 1);
                }
            }
            _ = token.cancelled() => break
        }
    }

    // Drain the transmission pipeline and write remaining bytes on the wire
    let mut batches = pipeline.drain();
    for (mut b, prio) in batches.drain(..) {
        tokio::time::timeout(
            latest_message_tracker.timeout(),
            link.send_batch(&mut b, prio),
        )
        .await
        .map_err(|_| {
            zerror!(
                "{link}: flush failed after {} ms",
                latest_message_tracker.timeout().as_millis()
            )
        })??;

        #[cfg(feature = "stats")]
        {
            stats.inc_bytes(zenoh_stats::Tx, b.len() as u64);
            stats.inc_transport_message(zenoh_stats::Tx, b.stats.t_msgs as u64);
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
    #[cfg(feature = "stats")] stats: zenoh_stats::LinkStats,
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
        let latest_message_tracker = LatestMessageTracker::new::<{ Priority::NUM }>(lease);
        let (res, _, _) = select_all((Priority::MAX as u8..Priority::MIN as u8).map(|prio| {
            let mut link = link.clone();
            let transport = transport.clone();
            let token = token.clone();
            let latest_message_tracker = latest_message_tracker.clone();
            #[cfg(feature = "stats")]
            let stats = stats.clone();
            let pool = pool.clone();
            zenoh_runtime::ZRuntime::RX.spawn(async move {
                read_loop(
                    Priority::try_from(prio).unwrap(),
                    &mut link,
                    transport,
                    latest_message_tracker,
                    token,
                    #[cfg(feature = "stats")]
                    stats,
                    &pool,
                )
                .await
            })
        }))
        .await;
        res.unwrap()
    } else {
        read_loop(
            Priority::Control,
            link,
            transport,
            LatestMessageTracker::new::<1>(lease),
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
    latest_message_tracker: Arc<LatestMessageTracker>,
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
            batch = read(link, priority, pool) => {
                let batch = batch?;
                #[cfg(feature = "stats")]
                {
                    let header_bytes = if l.is_streamed { 2 } else { 0 };
                    stats.inc_bytes(zenoh_stats::Rx, header_bytes + batch.len() as u64);
                }
                transport.read_messages(batch, &l, #[cfg(feature = "stats")] &stats)?;
            }
            _ = latest_message_tracker.wait_timeout(priority) => {
                bail!("{link}: expired after {} milliseconds", latest_message_tracker.timeout().as_millis());
            }
            _ = token.cancelled() => return Ok(()),
        }
    }
}

struct LatestMessageTracker<T: ?Sized = [CachePadded<Mutex<Instant>>]> {
    timeout: Duration,
    priority: CachePadded<AtomicUsize>,
    waker: AtomicWaker,
    has_timed_out: AtomicBool,
    latest_message: T,
}

impl LatestMessageTracker {
    fn new<const PRIORITIES: usize>(timeout: Duration) -> Arc<LatestMessageTracker> {
        let now = Instant::now();
        let this = Arc::new(LatestMessageTracker {
            timeout,
            priority: CachePadded::new(AtomicUsize::new(0)),
            waker: AtomicWaker::new(),
            has_timed_out: AtomicBool::new(false),
            latest_message: array::from_fn::<_, PRIORITIES, _>(|_| {
                CachePadded::new(Mutex::new(now))
            }),
        });
        let tracker = this.clone();
        tokio::spawn(async move {
            let mut latest_message = now;
            loop {
                tokio::time::sleep_until((latest_message + tracker.timeout).into()).await;
                let prev = latest_message;
                let latest_priority = tracker.priority.load(Ordering::Acquire);
                latest_message = *tracker.latest_message[latest_priority].lock().unwrap();
                if latest_message <= prev {
                    tracker.has_timed_out.store(true, Ordering::Release);
                    tracker.waker.wake();
                }
            }
        });
        this
    }

    fn timeout(&self) -> Duration {
        self.timeout
    }

    fn set(&self, priority: Priority) {
        *self.latest_message[priority as usize].lock().unwrap() = Instant::now();
        self.priority.store(priority as usize, Ordering::Release);
    }

    async fn wait_timeout(&self, priority: Priority) {
        poll_fn(|cx| {
            if priority != Priority::Control {
                return Poll::Pending;
            }
            self.waker.register(cx.waker());
            if self.has_timed_out.load(Ordering::Acquire) {
                self.has_timed_out.store(false, Ordering::Release);
                return Poll::Ready(());
            }
            Poll::Pending
        })
        .await
    }

    fn get(&self) -> Instant {
        *self.latest_message[self.priority.load(Ordering::Acquire)]
            .lock()
            .unwrap()
    }
}
