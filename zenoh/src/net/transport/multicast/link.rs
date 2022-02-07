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
use super::common::{conduit::TransportConduitTx, pipeline::TransmissionPipeline};
use super::protocol::io::{WBuf, ZBuf, ZSlice};
use super::protocol::message::extensions::{ZExt, ZExtPolicy};
use super::protocol::message::{Join, TransportProto, ZMessage};
use super::transport::TransportMulticastInner;
#[cfg(feature = "stats")]
use super::TransportMulticastStatsAtomic;
use crate::net::link::{LinkMulticast, Locator};
use crate::net::protocol::core::{ConduitSn, Priority, SeqNumBytes, WhatAmI, ZenohId};
use crate::net::protocol::message::KeepAlive;
use crate::net::protocol::VERSION;
use crate::net::transport::common::batch::SerializationBatch;
use async_std::prelude::*;
use async_std::task;
use async_std::task::JoinHandle;
use std::convert::TryInto;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use zenoh_util::collections::RecyclingObjectPool;
use zenoh_util::core::Result as ZResult;
use zenoh_util::sync::Signal;
use zenoh_util::zerror;

impl LinkMulticast {
    pub async fn send<T: ZMessage<Proto = TransportProto>>(&self, msg: &mut T) -> ZResult<usize> {
        const WBUF_SIZE: usize = 64;

        // Create the buffer for serializing the message
        let mut wbuf = WBuf::new(WBUF_SIZE, false);
        msg.write(&mut wbuf);
        let mut buffer = vec![0_u8; wbuf.len()];
        wbuf.copy_into_slice(&mut buffer[..]);

        // Send the message on the link
        let _ = self.0.write_all(&buffer).await;

        Ok(buffer.len())
    }

    //     pub(crate) async fn read_transport_message(&self) -> ZResult<(Vec<TransportMessage>, Locator)> {
    //         // Read the message
    //         let mut buffer = vec![0_u8; self.get_mtu()];
    //         let (n, locator) = self.read(&mut buffer).await?;
    //         buffer.truncate(n);

    //         let mut zbuf = ZBuf::from(buffer);
    //         let mut messages: Vec<TransportMessage> = Vec::with_capacity(1);
    //         while zbuf.can_read() {
    //             match zbuf.read_transport_message() {
    //                 Some(msg) => messages.push(msg),
    //                 None => {
    //                     let e = format!("Decoding error on link: {}", self);
    //                     return zerror!(ZErrorKind::InvalidMessage { descr: e });
    //                 }
    //             }
    //         }

    //         Ok((messages, locator))
    //     }
}

pub(super) struct TransportLinkMulticastConfig {
    pub(super) zid: ZenohId,
    pub(super) whatami: WhatAmI,
    pub(super) lease: Duration,
    pub(super) keep_alive: Duration,
    pub(super) join_interval: Duration,
    pub(super) sn_bytes: SeqNumBytes,
    pub(super) batch_size: u16,
}

#[derive(Clone)]
pub(super) struct TransportLinkMulticast {
    // The underlying link
    pub(super) link: LinkMulticast,
    // The transmission pipeline
    pub(super) pipeline: Option<Arc<TransmissionPipeline>>,
    // The transport this link is associated to
    transport: TransportMulticastInner,
    // The signals to stop TX/RX tasks
    handle_tx: Option<Arc<JoinHandle<()>>>,
    active_rx: Arc<AtomicBool>,
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
            active_rx: Arc::new(AtomicBool::new(false)),
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
            // The pipeline
            let pipeline = Arc::new(TransmissionPipeline::new(
                config.batch_size.min(self.link.get_mtu()),
                false,
                conduit_tx,
            ));
            self.pipeline = Some(pipeline.clone());

            // Spawn the TX task
            let c_link = self.link.clone();
            let c_transport = self.transport.clone();
            let handle = task::spawn(async move {
                let res = tx_task(
                    pipeline.clone(),
                    c_link.clone(),
                    config,
                    initial_sns,
                    #[cfg(feature = "stats")]
                    c_transport.stats.clone(),
                )
                .await;
                pipeline.disable();
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
        if let Some(pipeline) = self.pipeline.take() {
            pipeline.disable();
        }
    }

    pub(super) fn start_rx(&mut self) {
        if self.handle_rx.is_none() {
            self.active_rx.store(true, Ordering::Release);
            // Spawn the RX task
            let c_link = self.link.clone();
            let c_transport = self.transport.clone();
            let c_signal = self.signal_rx.clone();
            let c_active = self.active_rx.clone();
            let c_rx_buff_size = self.transport.manager.config.link_rx_buff_size;

            let handle = task::spawn(async move {
                // Start the consume task
                let res = rx_task(
                    c_link.clone(),
                    c_transport.clone(),
                    c_signal.clone(),
                    c_active.clone(),
                    c_rx_buff_size,
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

    pub(super) fn stop_rx(&mut self) {
        self.active_rx.store(false, Ordering::Release);
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
    pipeline: Arc<TransmissionPipeline>,
    link: LinkMulticast,
    config: TransportLinkMulticastConfig,
    mut next_sns: Vec<ConduitSn>,
    #[cfg(feature = "stats")] stats: Arc<TransportMulticastStatsAtomic>,
) -> ZResult<()> {
    enum Action {
        Pull((SerializationBatch, usize)),
        Join,
        KeepAlive,
        Stop,
    }

    async fn pull(pipeline: &TransmissionPipeline, keep_alive: Duration) -> Action {
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

    let mut last_join = Instant::now() - config.join_interval;
    loop {
        match pull(&pipeline, config.keep_alive)
            .race(join(last_join, config.join_interval))
            .await
        {
            Action::Pull((batch, priority)) => {
                // Send the buffer on the link
                let bytes = batch.as_bytes();
                let _ = link.write_all(bytes).await?;
                // Keep track of next SNs
                if let Some(sn) = batch.sn.reliable {
                    next_sns[priority].reliable = sn.next;
                }
                if let Some(sn) = batch.sn.best_effort {
                    next_sns[priority].best_effort = sn.next;
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
                let mut msg = Join::new(
                    VERSION,
                    config.whatami,
                    config.zid,
                    config.sn_bytes,
                    config.lease,
                    next_sns[0],
                );
                if next_sns.len() == Priority::NUM {
                    let sns: [ConduitSn; Priority::NUM] = next_sns.clone().try_into().unwrap();
                    msg.exts.qos = Some(ZExt::new(sns, ZExtPolicy::Ignore));
                }

                #[allow(unused_variables)] // Used when stats feature is enabled
                let n = link.send(&mut msg).await?;
                #[cfg(feature = "stats")]
                {
                    stats.inc_tx_t_msgs(1);
                    stats.inc_tx_bytes(n);
                }

                last_join = Instant::now();
            }
            Action::KeepAlive => {
                let message = KeepAlive::new();
                pipeline.push_transport_message(message, Priority::Background);
            }
            Action::Stop => {
                // Drain the transmission pipeline and write remaining bytes on the wire
                let mut batches = pipeline.drain();
                for (b, _) in batches.drain(..) {
                    let _ = link
                        .write_all(b.as_bytes())
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
                        stats.inc_tx_bytes(b.len());
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
    active: Arc<AtomicBool>,
    rx_buff_size: usize,
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
    let mtu = link.get_mtu() as usize;
    let n = 1 + (rx_buff_size / mtu);
    let pool = RecyclingObjectPool::new(n, || vec![0_u8; mtu].into_boxed_slice());
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
                    bail!("{}: zero bytes reading", link);
                }

                #[cfg(feature = "stats")]
                transport.stats.inc_rx_bytes(n);

                // Add the received bytes to the ZBuf for deserialization
                let zs = ZSlice::make(buffer.into(), 0, n)
                    .map_err(|_| zerror!("{}: decoding error", link))?;
                zbuf.add_zslice(zs);

                // Deserialize all the messages from the current ZBuf
                transport.deserialize(&mut zbuf, &loc)?;
            }
            Action::Stop => break,
        }
    }
    Ok(())
}
