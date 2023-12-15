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
    TransportExecutor,
};
use async_executor::Task;
use async_std::{
    prelude::FutureExt,
    task::{self, JoinHandle},
};
use std::{
    convert::TryInto,
    fmt,
    sync::Arc,
    time::{Duration, Instant},
};
use zenoh_buffers::{BBuf, ZSlice, ZSliceBuffer};
use zenoh_core::{zcondfeat, zlock};
use zenoh_link::{Link, LinkMulticast, Locator};
use zenoh_protocol::{
    core::{Bits, Priority, Resolution, WhatAmI, ZenohId},
    transport::{BatchSize, Close, Join, PrioritySn, TransportMessage, TransportSn},
};
use zenoh_result::{zerror, ZResult};
use zenoh_sync::{RecyclingObject, RecyclingObjectPool, Signal};

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
                        lz4_flex::block::get_maximum_output_size(
                            self.config.batch.max_buffer_size()
                        ),
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

impl From<&TransportLinkMulticast> for Link {
    fn from(link: &TransportLinkMulticast) -> Self {
        Link::from(&link.link)
    }
}

impl From<TransportLinkMulticast> for Link {
    fn from(link: TransportLinkMulticast) -> Self {
        Link::from(link.link)
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
        T: ZSliceBuffer + 'static,
    {
        const ERR: &str = "Read error from link: ";

        let mut into = (buff)();
        let (n, locator) = self.inner.link.read(into.as_mut_slice()).await?;
        let buffer = ZSlice::make(Arc::new(into), 0, n).map_err(|_| zerror!("Error"))?;
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
    pub(super) zid: ZenohId,
    pub(super) whatami: WhatAmI,
    pub(super) lease: Duration,
    pub(super) join_interval: Duration,
    pub(super) sn_resolution: Bits,
    pub(super) batch_size: BatchSize,
}

#[derive(Clone)]
pub(super) struct TransportLinkMulticastUniversal {
    // The underlying link
    pub(super) link: TransportLinkMulticast,
    // The transmission pipeline
    pub(super) pipeline: Option<TransmissionPipelineProducer>,
    // The transport this link is associated to
    transport: TransportMulticastInner,
    // The signals to stop TX/RX tasks
    handle_tx: Option<Arc<Task<()>>>,
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
        executor: &TransportExecutor,
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
                backoff: self.transport.manager.config.queue_backoff,
            };
            // The pipeline
            let (producer, consumer) = TransmissionPipeline::make(tpc, &priority_tx);
            self.pipeline = Some(producer);

            // Spawn the TX task
            let c_link = self.link.clone();
            let ctransport = self.transport.clone();
            let handle = executor.spawn(async move {
                let res = tx_task(
                    consumer,
                    c_link.tx(),
                    config,
                    initial_sns,
                    #[cfg(feature = "stats")]
                    ctransport.stats.clone(),
                )
                .await;
                if let Err(e) = res {
                    log::debug!("{}", e);
                    // Spawn a task to avoid a deadlock waiting for this same task
                    // to finish in the close() joining its handle
                    task::spawn(async move { ctransport.delete().await });
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
            let ctransport = self.transport.clone();
            let c_signal = self.signal_rx.clone();
            let c_rx_buffer_size = self.transport.manager.config.link_rx_buffer_size;

            let handle = task::spawn(async move {
                // Start the consume task
                let res = rx_task(
                    c_link.rx(),
                    ctransport.clone(),
                    c_signal.clone(),
                    c_rx_buffer_size,
                    batch_size,
                )
                .await;
                c_signal.trigger();
                if let Err(e) = res {
                    log::debug!("{}", e);
                    // Spawn a task to avoid a deadlock waiting for this same task
                    // to finish in the close() joining its handle
                    task::spawn(async move { ctransport.delete().await });
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
    enum Action {
        Pull((WBatch, usize)),
        Join,
        Stop,
    }

    async fn pull(pipeline: &mut TransmissionPipelineConsumer) -> Action {
        match pipeline.pull().await {
            Some(sb) => Action::Pull(sb),
            None => Action::Stop,
        }
    }

    async fn join(last_join: Instant, join_interval: Duration) -> Action {
        let now = Instant::now();
        let target = last_join + join_interval;
        if now < target {
            let left = target - now;
            task::sleep(left).await;
        }
        Action::Join
    }

    let mut last_join = Instant::now().checked_sub(config.join_interval).unwrap();
    loop {
        match pull(&mut pipeline)
            .race(join(last_join, config.join_interval))
            .await
        {
            Action::Pull((mut batch, priority)) => {
                // Send the buffer on the link
                link.send_batch(&mut batch).await?;
                // Keep track of next SNs
                if let Some(sn) = batch.codec.latest_sn.reliable {
                    last_sns[priority].reliable = sn;
                }
                if let Some(sn) = batch.codec.latest_sn.best_effort {
                    last_sns[priority].best_effort = sn;
                }
                #[cfg(feature = "stats")]
                {
                    stats.inc_tx_t_msgs(batch.stats.t_msgs);
                    stats.inc_tx_bytes(batch.len() as usize);
                }
                // Reinsert the batch into the queue
                pipeline.refill(batch, priority);
            }
            Action::Join => {
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
                    (PrioritySn::default(), Some(Box::new(tmp)))
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
            Action::Stop => {
                // Drain the transmission pipeline and write remaining bytes on the wire
                let mut batches = pipeline.drain();
                for (mut b, _) in batches.drain(..) {
                    link.send_batch(&mut b)
                        .timeout(config.join_interval)
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

    Ok(())
}

async fn rx_task(
    mut link: TransportLinkMulticastRx,
    transport: TransportMulticastInner,
    signal: Signal,
    rx_buffer_size: usize,
    batch_size: BatchSize,
) -> ZResult<()> {
    enum Action {
        Read((RBatch, Locator)),
        Stop,
    }

    async fn read<T, F>(
        link: &mut TransportLinkMulticastRx,
        pool: &RecyclingObjectPool<T, F>,
    ) -> ZResult<Action>
    where
        T: ZSliceBuffer + 'static,
        F: Fn() -> T,
        RecyclingObject<T>: ZSliceBuffer,
    {
        let (rbatch, locator) = link
            .recv_batch(|| pool.try_take().unwrap_or_else(|| pool.alloc()))
            .await?;
        Ok(Action::Read((rbatch, locator)))
    }

    async fn stop(signal: Signal) -> ZResult<Action> {
        signal.wait().await;
        Ok(Action::Stop)
    }

    // The pool of buffers
    let mtu = link.inner.config.batch.max_buffer_size();
    let mut n = rx_buffer_size / mtu;
    if rx_buffer_size % mtu != 0 {
        n += 1;
    }

    let pool = RecyclingObjectPool::new(n, || vec![0_u8; mtu].into_boxed_slice());
    while !signal.is_triggered() {
        // Async read from the underlying link
        let action = read(&mut link, &pool).race(stop(signal.clone())).await?;
        match action {
            Action::Read((batch, locator)) => {
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
            Action::Stop => break,
        }
    }
    Ok(())
}
