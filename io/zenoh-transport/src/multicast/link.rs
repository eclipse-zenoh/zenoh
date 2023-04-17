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
use super::common::{conduit::TransportConduitTx, pipeline::TransmissionPipeline};
use super::transport::TransportMulticastInner;
#[cfg(feature = "stats")]
use super::TransportMulticastStatsAtomic;
use crate::common::batch::WBatch;
use crate::common::pipeline::{
    TransmissionPipelineConf, TransmissionPipelineConsumer, TransmissionPipelineProducer,
};
use async_std::prelude::FutureExt;
use async_std::task;
use async_std::task::JoinHandle;
use std::convert::TryInto;
use std::sync::Arc;
use std::time::{Duration, Instant};
use zenoh_buffers::reader::{HasReader, Reader};
use zenoh_codec::{RCodec, Zenoh060};
use zenoh_core::zlock;
use zenoh_link::{LinkMulticast, Locator};
use zenoh_protocol::{
    core::{ConduitSn, ConduitSnList, Priority, WhatAmI, ZInt, ZenohId},
    transport::TransportMessage,
};
use zenoh_result::{bail, zerror, ZResult};
use zenoh_sync::RecyclingObjectPool;
use zenoh_sync::Signal;

pub(super) struct TransportLinkMulticastConfig {
    pub(super) version: u8,
    pub(super) zid: ZenohId,
    pub(super) whatami: WhatAmI,
    pub(super) lease: Duration,
    pub(super) keep_alive: usize,
    pub(super) join_interval: Duration,
    pub(super) sn_resolution: ZInt,
    pub(super) batch_size: u16,
}

#[derive(Clone)]
pub(super) struct TransportLinkMulticast {
    // The underlying link
    pub(super) link: LinkMulticast,
    // The transmission pipeline
    pub(super) pipeline: Option<TransmissionPipelineProducer>,
    // The transport this link is associated to
    transport: TransportMulticastInner,
    // The signals to stop TX/RX tasks
    handle_tx: Option<Arc<JoinHandle<()>>>,
    signal_rx: Signal,
    handle_rx: Option<Arc<JoinHandle<()>>>,
}

impl TransportLinkMulticast {
    pub(super) fn new(
        transport: TransportMulticastInner,
        link: LinkMulticast,
    ) -> TransportLinkMulticast {
        TransportLinkMulticast {
            transport,
            link,
            pipeline: None,
            handle_tx: None,
            signal_rx: Signal::new(),
            handle_rx: None,
        }
    }
}

impl TransportLinkMulticast {
    pub(super) fn start_tx(
        &mut self,
        config: TransportLinkMulticastConfig,
        conduit_tx: Arc<[TransportConduitTx]>,
    ) {
        let initial_sns: Vec<ConduitSn> = conduit_tx
            .iter()
            .map(|x| ConduitSn {
                reliable: zlock!(x.reliable).sn.now(),
                best_effort: zlock!(x.best_effort).sn.now(),
            })
            .collect();

        if self.handle_tx.is_none() {
            let tpc = TransmissionPipelineConf {
                is_streamed: false,
                batch_size: config.batch_size.min(self.link.get_mtu()),
                queue_size: self.transport.manager.config.queue_size,
                backoff: self.transport.manager.config.queue_backoff,
            };
            // The pipeline
            let (producer, consumer) = TransmissionPipeline::make(tpc, &conduit_tx);
            self.pipeline = Some(producer);

            // Spawn the TX task
            let c_link = self.link.clone();
            let c_transport = self.transport.clone();
            let handle = task::spawn(async move {
                let res = tx_task(
                    consumer,
                    c_link.clone(),
                    config,
                    initial_sns,
                    #[cfg(feature = "stats")]
                    c_transport.stats.clone(),
                )
                .await;
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

    pub(super) fn stop_tx(&mut self) {
        if let Some(pipeline) = self.pipeline.as_ref() {
            pipeline.disable();
        }
    }

    pub(super) fn start_rx(&mut self) {
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
                    c_signal.clone(),
                    c_rx_buffer_size,
                )
                .await;
                c_signal.trigger();
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

        self.link.close().await
    }
}

/*************************************/
/*              TASKS                */
/*************************************/
async fn tx_task(
    mut pipeline: TransmissionPipelineConsumer,
    link: LinkMulticast,
    config: TransportLinkMulticastConfig,
    mut last_sns: Vec<ConduitSn>,
    #[cfg(feature = "stats")] stats: Arc<TransportMulticastStatsAtomic>,
) -> ZResult<()> {
    enum Action {
        Pull((WBatch, usize)),
        Join,
        KeepAlive,
        Stop,
    }

    async fn pull(pipeline: &mut TransmissionPipelineConsumer, keep_alive: Duration) -> Action {
        match pipeline.pull().timeout(keep_alive).await {
            Ok(res) => match res {
                Some(sb) => Action::Pull(sb),
                None => Action::Stop,
            },
            Err(_) => Action::KeepAlive,
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

    let keep_alive = config.join_interval / config.keep_alive as u32;
    let mut last_join = Instant::now().checked_sub(config.join_interval).unwrap();
    loop {
        match pull(&mut pipeline, keep_alive)
            .race(join(last_join, config.join_interval))
            .await
        {
            Action::Pull((batch, priority)) => {
                // Send the buffer on the link
                let bytes = batch.as_bytes();
                link.write_all(bytes).await?;
                // Keep track of next SNs
                if let Some(sn) = batch.latest_sn.reliable {
                    last_sns[priority].reliable = sn;
                }
                if let Some(sn) = batch.latest_sn.best_effort {
                    last_sns[priority].best_effort = sn;
                }
                #[cfg(feature = "stats")]
                {
                    stats.inc_tx_t_msgs(batch.stats.t_msgs);
                    stats.inc_tx_bytes(bytes.len());
                }
                // Reinsert the batch into the queue
                pipeline.refill(batch, priority);
            }
            Action::Join => {
                let attachment = None;
                let next_sns = last_sns
                    .iter()
                    .map(|c| ConduitSn {
                        reliable: (1 + c.reliable) % config.sn_resolution,
                        best_effort: (1 + c.best_effort) % config.sn_resolution,
                    })
                    .collect::<Vec<ConduitSn>>();
                let next_sns = if next_sns.len() == Priority::NUM {
                    let tmp: [ConduitSn; Priority::NUM] = next_sns.try_into().unwrap();
                    ConduitSnList::QoS(tmp.into())
                } else {
                    assert_eq!(next_sns.len(), 1);
                    ConduitSnList::Plain(next_sns[0])
                };
                let message = TransportMessage::make_join(
                    config.version,
                    config.whatami,
                    config.zid,
                    config.lease,
                    config.sn_resolution,
                    next_sns,
                    attachment,
                );

                #[allow(unused_variables)] // Used when stats feature is enabled
                let n = link.write_transport_message(&message).await?;
                #[cfg(feature = "stats")]
                {
                    stats.inc_tx_t_msgs(1);
                    stats.inc_tx_bytes(n);
                }

                last_join = Instant::now();
            }
            Action::KeepAlive => {
                let zid = Some(config.zid);
                let attachment = None;
                let message = TransportMessage::make_keep_alive(zid, attachment);

                #[allow(unused_variables)] // Used when stats feature is enabled
                let n = link.write_transport_message(&message).await?;
                #[cfg(feature = "stats")]
                {
                    stats.inc_tx_t_msgs(1);
                    stats.inc_tx_bytes(n);
                }
            }
            Action::Stop => {
                // Drain the transmission pipeline and write remaining bytes on the wire
                let mut batches = pipeline.drain();
                for (b, _) in batches.drain(..) {
                    link.write_all(b.as_bytes())
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
    link: LinkMulticast,
    transport: TransportMulticastInner,
    signal: Signal,
    rx_buffer_size: usize,
) -> ZResult<()> {
    enum Action {
        Read((usize, Locator)),
        Stop,
    }

    async fn read(link: &LinkMulticast, buffer: &mut [u8]) -> ZResult<Action> {
        let (n, loc) = link.read(buffer).await?;
        Ok(Action::Read((n, loc.into_owned())))
    }

    async fn stop(signal: Signal) -> ZResult<Action> {
        signal.wait().await;
        Ok(Action::Stop)
    }

    // The codec
    let codec = Zenoh060::default();

    // The pool of buffers
    let mtu = link.get_mtu() as usize;
    let mut n = rx_buffer_size / mtu;
    if rx_buffer_size % mtu != 0 {
        n += 1;
    }
    let pool = RecyclingObjectPool::new(n, || vec![0_u8; mtu].into_boxed_slice());
    while !signal.is_triggered() {
        // Retrieve one buffer
        let mut buffer = pool.try_take().unwrap_or_else(|| pool.alloc());
        // Async read from the underlying link
        let action = read(&link, &mut buffer).race(stop(signal.clone())).await?;
        match action {
            Action::Read((n, loc)) => {
                if n == 0 {
                    // Reading 0 bytes means error
                    bail!("{}: zero bytes reading", link);
                }

                #[cfg(feature = "stats")]
                transport.stats.inc_rx_bytes(n);

                // Deserialize all the messages from the current ZBuf
                let mut reader = buffer[0..n].reader();
                while reader.can_read() {
                    let msg: TransportMessage = codec
                        .read(&mut reader)
                        .map_err(|_| zerror!("{}: decoding error", link))?;

                    #[cfg(feature = "stats")]
                    transport.stats.inc_rx_t_msgs(1);

                    transport.receive_message(msg, &loc)?
                }
            }
            Action::Stop => break,
        }
    }
    Ok(())
}
