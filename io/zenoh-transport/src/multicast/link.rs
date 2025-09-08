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
    convert::TryInto,
    fmt,
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::task::JoinHandle;
use zenoh_buffers::{BBuf, ZSlice, ZSliceBuffer};
use zenoh_core::{zcondfeat, zlock};
use zenoh_link::{LinkMulticast, Locator};
use zenoh_protocol::{
    core::{Bits, Priority, Resolution, WhatAmI, ZenohIdProto},
    transport::{
        join::ext::PatchType, BatchSize, Close, Join, PrioritySn, TransportMessage, TransportSn,
    },
};
use zenoh_result::{zerror, ZResult};
use zenoh_sync::{RecyclingObject, RecyclingObjectPool, Signal};

#[cfg(feature = "stats")]
use crate::stats::TransportStats;
use crate::{
    common::{
        batch::{BatchConfig, Encode, Finalize, RBatch, WBatch},
        pipeline::{
            TransmissionPipeline, TransmissionPipelineConf, TransmissionPipelineConsumer,
            TransmissionPipelineProducer,
        },
        priority::TransportPriorityTx,
    },
    multicast::transport::TransportMulticastInner,
};

/****************************/
/* TRANSPORT MULTICAST LINK */
/****************************/
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct TransportLinkMulticastConfig {
    pub(crate) batch: BatchConfig,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct TransportLinkMulticast {
    pub(crate) link: LinkMulticast,
    pub(crate) config: TransportLinkMulticastConfig,
}

impl TransportLinkMulticast {
    pub(crate) fn new(link: LinkMulticast, mut config: TransportLinkMulticastConfig) -> Self {
        config.batch.mtu = link.get_mtu().min(config.batch.mtu);
        config.batch.is_streamed = false;
        Self { link, config }
    }

    pub(crate) fn tx(&self) -> TransportLinkMulticastTx {
        TransportLinkMulticastTx {
            inner: self.clone(),
            buffer: zcondfeat!(
                "transport_compression",
                self.config
                    .batch
                    .is_compression
                    .then_some(BBuf::with_capacity(
                        lz4_flex::block::get_maximum_output_size(self.config.batch.mtu as usize),
                    )),
                None
            ),
        }
    }

    pub(crate) fn rx(&self) -> TransportLinkMulticastRx {
        TransportLinkMulticastRx {
            inner: self.clone(),
        }
    }

    pub(crate) async fn send(&self, msg: &TransportMessage) -> ZResult<usize> {
        let mut link = self.tx();
        link.send(msg).await
    }

    // pub(crate) async fn recv(&self) -> ZResult<(TransportMessage, Locator)> {
    //     let mut link = self.rx();
    //     link.recv().await
    // }

    pub(crate) async fn close(&self, reason: Option<u8>) -> ZResult<()> {
        if let Some(reason) = reason {
            // Build the close message
            let message: TransportMessage = Close {
                reason,
                session: false,
            }
            .into();
            // Send the close message on the link
            let _ = self.send(&message).await;
        }
        self.link.close().await
    }
}

impl fmt::Display for TransportLinkMulticast {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.link)
    }
}

impl fmt::Debug for TransportLinkMulticast {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TransportLinkMulticast")
            .field("link", &self.link)
            .field("config", &self.config)
            .finish()
    }
}

pub(crate) struct TransportLinkMulticastTx {
    pub(crate) inner: TransportLinkMulticast,
    pub(crate) buffer: Option<BBuf>,
}

impl TransportLinkMulticastTx {
    pub(crate) async fn send_batch(&mut self, batch: &mut WBatch) -> ZResult<()> {
        const ERR: &str = "Write error on link: ";

        let res = batch
            .finalize(self.buffer.as_mut())
            .map_err(|_| zerror!("{ERR}{self}"))?;

        let bytes = match res {
            Finalize::Batch => batch.as_slice(),
            Finalize::Buffer => self
                .buffer
                .as_ref()
                .ok_or_else(|| zerror!("Invalid buffer finalization"))?
                .as_slice(),
        };

        // Send the message on the link
        self.inner.link.write_all(bytes).await?;

        Ok(())
    }

    pub(crate) async fn send(&mut self, msg: &TransportMessage) -> ZResult<usize> {
        const ERR: &str = "Write error on link: ";

        // Create the batch for serializing the message
        let mut batch = WBatch::new(self.inner.config.batch);
        batch.encode(msg).map_err(|_| zerror!("{ERR}{self}"))?;
        let len = batch.len() as usize;
        self.send_batch(&mut batch).await?;
        Ok(len)
    }
}

impl fmt::Display for TransportLinkMulticastTx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl fmt::Debug for TransportLinkMulticastTx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = f.debug_struct("TransportLinkMulticastRx");
        s.field("link", &self.inner.link)
            .field("config", &self.inner.config);
        #[cfg(feature = "transport_compression")]
        {
            s.field("buffer", &self.buffer.as_ref().map(|b| b.capacity()));
        }
        s.finish()
    }
}

pub(crate) struct TransportLinkMulticastRx {
    pub(crate) inner: TransportLinkMulticast,
}

impl TransportLinkMulticastRx {
    pub async fn recv_batch<C, T>(&self, buff: C) -> ZResult<(RBatch, Locator)>
    where
        C: Fn() -> T + Copy,
        T: AsMut<[u8]> + ZSliceBuffer + 'static,
    {
        const ERR: &str = "Read error from link: ";

        let mut into = (buff)();
        let (n, locator) = self.inner.link.read(into.as_mut()).await?;
        let buffer = ZSlice::new(Arc::new(into), 0, n).map_err(|_| zerror!("Error"))?;
        let mut batch = RBatch::new(self.inner.config.batch, buffer);
        batch.initialize(buff).map_err(|_| zerror!("{ERR}{self}"))?;
        Ok((batch, locator.into_owned()))
    }

    // pub async fn recv(&mut self) -> ZResult<(TransportMessage, Locator)> {
    //     let mtu = self.inner.config.mtu as usize;
    //     let (mut batch, locator) = self
    //         .recv_batch(|| zenoh_buffers::vec::uninit(mtu).into_boxed_slice())
    //         .await?;
    //     let msg = batch
    //         .decode()
    //         .map_err(|_| zerror!("Decode error on link: {}", self))?;
    //     Ok((msg, locator))
    // }
}

impl fmt::Display for TransportLinkMulticastRx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl fmt::Debug for TransportLinkMulticastRx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TransportLinkMulticastRx")
            .field("link", &self.inner.link)
            .field("config", &self.inner.config)
            .finish()
    }
}

/**************************************/
/* TRANSPORT MULTICAST LINK UNIVERSAL */
/**************************************/
pub(super) struct TransportLinkMulticastConfigUniversal {
    pub(super) version: u8,
    pub(super) zid: ZenohIdProto,
    pub(super) whatami: WhatAmI,
    pub(super) lease: Duration,
    pub(super) join_interval: Duration,
    pub(super) sn_resolution: Bits,
    pub(super) batch_size: BatchSize,
}

// TODO(yuyuan): Introduce TaskTracker or JoinSet and retire handle_tx, handle_rx, and signal_rx.
#[derive(Clone)]
pub(super) struct TransportLinkMulticastUniversal {
    // The underlying link
    pub(super) link: TransportLinkMulticast,
    // The transmission pipeline
    pub(super) pipeline: Option<TransmissionPipelineProducer>,
    // The transport this link is associated to
    transport: TransportMulticastInner,
    // The signals to stop TX/RX tasks
    handle_tx: Option<Arc<JoinHandle<()>>>,
    signal_rx: Signal,
    handle_rx: Option<Arc<JoinHandle<()>>>,
}

impl TransportLinkMulticastUniversal {
    pub(super) fn new(
        transport: TransportMulticastInner,
        link: TransportLinkMulticast,
    ) -> TransportLinkMulticastUniversal {
        TransportLinkMulticastUniversal {
            transport,
            link,
            pipeline: None,
            handle_tx: None,
            signal_rx: Signal::new(),
            handle_rx: None,
        }
    }
}

impl TransportLinkMulticastUniversal {
    pub(super) fn start_tx(
        &mut self,
        config: TransportLinkMulticastConfigUniversal,
        priority_tx: Arc<[TransportPriorityTx]>,
    ) {
        let initial_sns: Vec<PrioritySn> = priority_tx
            .iter()
            .map(|x| PrioritySn {
                reliable: {
                    let sn = zlock!(x.reliable).sn.now();
                    if sn == 0 {
                        config.sn_resolution.mask() as TransportSn
                    } else {
                        sn - 1
                    }
                },
                best_effort: {
                    let sn = zlock!(x.best_effort).sn.now();
                    if sn == 0 {
                        config.sn_resolution.mask() as TransportSn
                    } else {
                        sn - 1
                    }
                },
            })
            .collect();

        if self.handle_tx.is_none() {
            let tpc = TransmissionPipelineConf {
                batch: self.link.config.batch,
                queue_size: self.transport.manager.config.queue_size,
                wait_before_drop: self.transport.manager.config.wait_before_drop,
                max_wait_before_drop_fragments: self
                    .transport
                    .manager
                    .config
                    .max_wait_before_drop_fragments,
                wait_before_close: self.transport.manager.config.wait_before_close,
                batching_enabled: self.transport.manager.config.batching,
                batching_time_limit: self.transport.manager.config.queue_backoff,
                queue_alloc: self.transport.manager.config.queue_alloc,
            };
            // The pipeline
            let (producer, consumer) = TransmissionPipeline::make(tpc, &priority_tx);
            self.pipeline = Some(producer);

            // Spawn the TX task
            let c_link = self.link.clone();
            let c_transport = self.transport.clone();

            let handle = zenoh_runtime::ZRuntime::TX.spawn(async move {
                let res = tx_task(
                    consumer,
                    c_link.tx(),
                    config,
                    initial_sns,
                    #[cfg(feature = "stats")]
                    c_transport.stats.clone(),
                )
                .await;
                if let Err(e) = res {
                    tracing::debug!("TX task failed: {}", e);
                    // Spawn a task to avoid a deadlock waiting for this same task
                    // to finish in the close() joining its handle
                    zenoh_runtime::ZRuntime::Net.spawn(async move { c_transport.delete().await });
                }
            });
            self.handle_tx = Some(Arc::new(handle));
        }
    }

    pub(super) fn stop_tx(&mut self) {
        if let Some(pipeline) = self.pipeline.as_ref() {
            pipeline.disable();
        }
    }

    pub(super) fn start_rx(&mut self, batch_size: BatchSize) {
        if self.handle_rx.is_none() {
            // Spawn the RX task
            let c_link = self.link.clone();
            let c_transport = self.transport.clone();
            let c_signal = self.signal_rx.clone();
            let c_rx_buffer_size = self.transport.manager.config.link_rx_buffer_size;

            let handle = zenoh_runtime::ZRuntime::RX.spawn(async move {
                // Start the consume task
                let res = rx_task(
                    c_link.rx(),
                    c_transport.clone(),
                    c_signal.clone(),
                    c_rx_buffer_size,
                    batch_size,
                )
                .await;
                c_signal.trigger();
                if let Err(e) = res {
                    tracing::debug!("RX task failed: {}", e);
                    // Spawn a task to avoid a deadlock waiting for this same task
                    // to finish in the close() joining its handle
                    zenoh_runtime::ZRuntime::Net.spawn(async move { c_transport.delete().await });
                }
            });
            self.handle_rx = Some(Arc::new(handle));
        }
    }

    pub(super) fn stop_rx(&mut self) {
        self.signal_rx.trigger();
    }

    pub(super) async fn close(mut self) -> ZResult<()> {
        tracing::trace!("{}: closing", self.link);
        self.stop_rx();
        if let Some(handle) = self.handle_rx.take() {
            // It is safe to unwrap the Arc since we have the ownership of the whole link
            let handle_rx = Arc::try_unwrap(handle).unwrap();
            handle_rx.await?;
        }

        self.stop_tx();
        if let Some(handle) = self.handle_tx.take() {
            // It is safe to unwrap the Arc since we have the ownership of the whole link
            let handle_tx = Arc::try_unwrap(handle).unwrap();
            handle_tx.await?;
        }

        self.link.close(None).await
    }
}

/*************************************/
/*              TASKS                */
/*************************************/
async fn tx_task(
    mut pipeline: TransmissionPipelineConsumer,
    mut link: TransportLinkMulticastTx,
    config: TransportLinkMulticastConfigUniversal,
    mut last_sns: Vec<PrioritySn>,
    #[cfg(feature = "stats")] stats: Arc<TransportStats>,
) -> ZResult<()> {
    async fn join(last_join: Instant, join_interval: Duration) {
        let now = Instant::now();
        let target = last_join + join_interval;
        if now < target {
            let left = target - now;
            tokio::time::sleep(left).await;
        }
    }

    let mut last_join = Instant::now().checked_sub(config.join_interval).unwrap();
    loop {
        tokio::select! {
            res = pipeline.pull() => {
                match res {
                    Some((mut batch, priority)) => {
                        // Send the buffer on the link
                        link.send_batch(&mut batch).await?;
                        // Keep track of next SNs
                        if let Some(sn) = batch.codec.latest_sn.reliable {
                            last_sns[priority as usize].reliable = sn;
                        }
                        if let Some(sn) = batch.codec.latest_sn.best_effort {
                            last_sns[priority as usize].best_effort = sn;
                        }
                        #[cfg(feature = "stats")]
                        {
                            stats.inc_tx_t_msgs(batch.stats.t_msgs);
                            stats.inc_tx_bytes(batch.len() as usize);
                        }
                        // Reinsert the batch into the queue
                        pipeline.refill(batch, priority);
                    }
                    None => {
                        // Drain the transmission pipeline and write remaining bytes on the wire
                        let mut batches = pipeline.drain();
                        for (mut b, _) in batches.drain(..) {
                            tokio::time::timeout(config.join_interval, link.send_batch(&mut b))
                                .await
                                .map_err(|_| {
                                    zerror!(
                                        "{}: flush failed after {} ms",
                                        link,
                                        config.join_interval.as_millis()
                                    )
                                })??;

                            #[cfg(feature = "stats")]
                            {
                                stats.inc_tx_t_msgs(b.stats.t_msgs);
                                stats.inc_tx_bytes(b.len() as usize);
                            }
                        }
                        break;
                    }

                }
            }

            _ = join(last_join, config.join_interval) => {
                let next_sns = last_sns
                    .iter()
                    .map(|c| PrioritySn {
                        reliable: (1 + c.reliable) & config.sn_resolution.mask() as TransportSn,
                        best_effort: (1 + c.best_effort)
                            & config.sn_resolution.mask() as TransportSn,
                    })
                    .collect::<Vec<PrioritySn>>();
                let (next_sn, ext_qos) = if next_sns.len() == Priority::NUM {
                    let tmp: [PrioritySn; Priority::NUM] = next_sns.try_into().unwrap();
                    (PrioritySn::DEFAULT, Some(Box::new(tmp)))
                } else {
                    (next_sns[0], None)
                };
                let message: TransportMessage = Join {
                    version: config.version,
                    whatami: config.whatami,
                    zid: config.zid,
                    resolution: Resolution::default(),
                    batch_size: config.batch_size,
                    lease: config.lease,
                    next_sn,
                    ext_qos,
                    ext_shm: None,
                    ext_patch: PatchType::CURRENT
                }
                .into();

                #[allow(unused_variables)] // Used when stats feature is enabled
                let n = link.send(&message).await?;
                #[cfg(feature = "stats")]
                {
                    stats.inc_tx_t_msgs(1);
                    stats.inc_tx_bytes(n);
                }

                last_join = Instant::now();

            }
        }
    }

    Ok(())
}

async fn rx_task(
    mut link: TransportLinkMulticastRx,
    transport: TransportMulticastInner,
    signal: Signal,
    rx_buffer_size: usize,
    batch_size: BatchSize,
) -> ZResult<()> {
    async fn read<T, F>(
        link: &mut TransportLinkMulticastRx,
        pool: &RecyclingObjectPool<T, F>,
    ) -> ZResult<(RBatch, Locator)>
    where
        T: ZSliceBuffer + 'static,
        F: Fn() -> T,
        RecyclingObject<T>: AsMut<[u8]> + ZSliceBuffer,
    {
        let (rbatch, locator) = link
            .recv_batch(|| pool.try_take().unwrap_or_else(|| pool.alloc()))
            .await?;
        Ok((rbatch, locator))
    }

    // The pool of buffers
    let mtu = link.inner.config.batch.mtu as usize;
    let mut n = rx_buffer_size / mtu;
    if n == 0 {
        tracing::debug!("RX configured buffer of {rx_buffer_size} bytes is too small for {link} that has an MTU of {mtu} bytes. Defaulting to {mtu} bytes for RX buffer.");
        n = 1;
    }

    let pool = RecyclingObjectPool::new(n, || vec![0_u8; mtu].into_boxed_slice());
    loop {
        tokio::select! {
            _ = signal.wait() => break,
            res = read(&mut link, &pool) => {
                let (batch, locator) = res?;

                #[cfg(feature = "stats")]
                transport.stats.inc_rx_bytes(batch.len());

                // Deserialize all the messages from the current ZBuf
                transport.read_messages(
                    batch,
                    locator,
                    batch_size,
                    #[cfg(feature = "stats")]
                    &transport,
                )?;
            }
        }
    }
    Ok(())
}
