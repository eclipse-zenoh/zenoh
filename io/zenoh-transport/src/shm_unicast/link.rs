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
use super::TransportUnicastStatsAtomic;
use super::{oam_extensions::pack_oam_keepalive, transport::ShmTransportUnicastInner};
use crate::TransportExecutor;
use async_std::prelude::FutureExt;
use async_std::task;
use async_std::task::JoinHandle;
use zenoh_codec::*;

#[cfg(all(feature = "unstable", feature = "transport_compression"))]
use std::convert::TryInto;
use std::sync::Arc;
use std::time::Duration;
use zenoh_buffers::{writer::HasWriter, ZSlice};
use zenoh_link::{LinkUnicast, LinkUnicastDirection};
use zenoh_protocol::{network::NetworkMessage, transport::BatchSize};
use zenoh_result::{zerror, ZResult};
use zenoh_sync::RecyclingObjectPool;

#[cfg(all(feature = "unstable", feature = "transport_compression"))]
const HEADER_BYTES_SIZE: usize = 2;

#[cfg(all(feature = "unstable", feature = "transport_compression"))]
const COMPRESSION_BYTE_INDEX_STREAMED: usize = 2;

#[cfg(all(feature = "unstable", feature = "transport_compression"))]
const COMPRESSION_BYTE_INDEX: usize = 0;

#[cfg(all(feature = "unstable", feature = "transport_compression"))]
const COMPRESSION_ENABLED: u8 = 1_u8;

#[cfg(all(feature = "unstable", feature = "transport_compression"))]
const COMPRESSION_DISABLED: u8 = 0_u8;

#[cfg(all(feature = "unstable", feature = "transport_compression"))]
const BATCH_PAYLOAD_START_INDEX: usize = 1;

#[cfg(all(feature = "unstable", feature = "transport_compression"))]
const MAX_BATCH_SIZE: usize = u16::MAX as usize;

pub(super) struct ShmTransportLinkUnicast {
    // Inbound / outbound
    pub(super) direction: LinkUnicastDirection,
    // The underlying link
    pub(super) link: LinkUnicast,
    // The transport this link is associated to
    transport: ShmTransportUnicastInner,
    // The signals to stop TX/RX tasks
    handle_keepalive: Option<async_executor::Task<()>>,
    handle_rx: Option<JoinHandle<()>>,
}

impl ShmTransportLinkUnicast {
    pub(super) fn new(
        transport: ShmTransportUnicastInner,
        link: LinkUnicast,
        direction: LinkUnicastDirection,
    ) -> ShmTransportLinkUnicast {
        ShmTransportLinkUnicast {
            direction,
            transport,
            link,
            handle_keepalive: None,
            handle_rx: None,
        }
    }
}

async fn send_with_link(
    link: &LinkUnicast,
    msg: NetworkMessage,
    #[cfg(feature = "stats")] stats: &Arc<TransportUnicastStatsAtomic>,
) -> ZResult<()> {
    let mut buff = vec![];
    let codec = Zenoh080::new();
    let mut writer = buff.writer();
    codec
        .write(&mut writer, &msg)
        .map_err(|_| zerror!("Error serializing message {}", msg))?;

    // write len for streamed links only
    if link.is_streamed() {
        let len = buff.len();
        let ne = len.to_ne_bytes();
        link.write_all(&ne).await?;
    }
    link.write_all(&buff).await?;

    #[cfg(feature = "stats")]
    {
        self.transport.stats.inc_tx_t_msgs(1);
        self.transport.stats.inc_tx_bytes(buff.len() + 2);
    }
    Ok(())
}

impl ShmTransportLinkUnicast {
    pub(super) async fn send(&self, msg: NetworkMessage) -> ZResult<()> {
        send_with_link(
            &self.link,
            msg,
            #[cfg(feature = "stats")]
            &self.transport.stats,
        )
        .await
    }

    pub(super) fn start_keepalive(&mut self, executor: &TransportExecutor, keep_alive: Duration) {
        if self.handle_keepalive.is_none() {
            // Spawn the keepalive task
            let c_link = self.link.clone();
            let c_transport = self.transport.clone();
            let handle = executor.spawn(async move {
                let res = keepalive_task(
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
            self.handle_keepalive = Some(handle);
        }
    }

    pub(super) async fn stop_keepalive(&mut self) {
        if let Some(handle_keepalive) = self.handle_keepalive.take() {
            handle_keepalive.cancel().await;
        }
    }

    pub(super) fn start_rx(&mut self, lease: Duration, batch_size: u16) {
        if self.handle_rx.is_none() {
            // Spawn the RX task
            let c_link = self.link.clone();
            let c_transport = self.transport.clone();
            let c_rx_buffer_size = self.transport.manager.config.link_rx_buffer_size;

            let handle = task::spawn(async move {
                // Start the consume task
                let res = rx_task(
                    c_link.clone(),
                    c_transport.clone(),
                    lease,
                    batch_size,
                    c_rx_buffer_size,
                )
                .await;
                if let Err(e) = res {
                    log::debug!("{}", e);
                    // Spawn a task to avoid a deadlock waiting for this same task
                    // to finish in the close() joining its handle
                    task::spawn(async move { c_transport.del_link(&c_link).await });
                }
            });
            self.handle_rx = Some(handle);
        }
    }

    pub(super) async fn stop_rx(&mut self) {
        if let Some(handle_rx) = self.handle_rx.take() {
            handle_rx.cancel().await;
        }
    }

    pub(super) async fn close(mut self) -> ZResult<()> {
        log::trace!("{}: closing", self.link);
        self.stop_rx();
        self.stop_keepalive();
        self.link.close().await
    }
}

/*************************************/
/*              TASKS                */
/*************************************/
async fn keepalive_task(
    link: LinkUnicast,
    keep_alive: Duration,
    #[cfg(feature = "stats")] stats: Arc<TransportUnicastStatsAtomic>,
) -> ZResult<()> {
    loop {
        async_std::task::sleep(keep_alive).await;

        send_with_link(
            &link,
            pack_oam_keepalive(),
            #[cfg(feature = "stats")]
            &stats,
        )
        .await?;
    }
}

async fn rx_task_stream(
    link: LinkUnicast,
    transport: ShmTransportUnicastInner,
    lease: Duration,
    rx_batch_size: BatchSize,
    rx_buffer_size: usize,
) -> ZResult<()> {
    enum Action {
        Read(usize),
        Stop,
    }

    async fn read(link: &LinkUnicast, buffer: &mut [u8]) -> ZResult<Action> {
        // 16 bits for reading the batch length
        let mut length = [0_u8, 0_u8];
        link.read_exact(&mut length).await?;
        let n = BatchSize::from_le_bytes(length) as usize;

        link.read_exact(&mut buffer[0..n]).await?;
        Ok(Action::Read(n))
    }

    // The pool of buffers
    let mtu = link.get_mtu().min(rx_batch_size) as usize;
    let mut n = rx_buffer_size / mtu;
    if rx_buffer_size % mtu != 0 {
        n += 1;
    }

    let pool = RecyclingObjectPool::new(n, || vec![0_u8; mtu].into_boxed_slice());
    loop {
        // Retrieve one buffer
        let mut buffer = pool.try_take().unwrap_or_else(|| pool.alloc());

        // Async read from the underlying link
        let action = read(&link, &mut buffer)
            .timeout(lease)
            .await
            .map_err(|_| zerror!("{}: expired after {} milliseconds", link, lease.as_millis()))??;
        match action {
            Action::Read(n) => {
                #[cfg(feature = "stats")]
                transport.stats.inc_rx_bytes(2 + n); // Account for the batch len encoding (16 bits)

                // Deserialize all the messages from the current ZBuf
                let zslice = ZSlice::make(Arc::new(buffer), 0, n).unwrap();
                transport.read_messages(zslice, &link)?;
            }
            Action::Stop => break,
        }
    }
    Ok(())
}

async fn rx_task_dgram(
    link: LinkUnicast,
    transport: ShmTransportUnicastInner,
    lease: Duration,
    rx_batch_size: BatchSize,
    rx_buffer_size: usize,
) -> ZResult<()> {
    enum Action {
        Read(usize),
        Stop,
    }

    async fn read(link: &LinkUnicast, buffer: &mut [u8]) -> ZResult<Action> {
        let n = link.read(buffer).await?;
        Ok(Action::Read(n))
    }

    // The pool of buffers
    let mtu = link.get_mtu().min(rx_batch_size) as usize;
    let mut n = rx_buffer_size / mtu;
    if rx_buffer_size % mtu != 0 {
        n += 1;
    }

    let pool = RecyclingObjectPool::new(n, || vec![0_u8; mtu].into_boxed_slice());
    loop {
        // Retrieve one buffer
        let mut buffer = pool.try_take().unwrap_or_else(|| pool.alloc());

        // Async read from the underlying link
        let action = read(&link, &mut buffer)
            .timeout(lease)
            .await
            .map_err(|_| zerror!("{}: expired after {} milliseconds", link, lease.as_millis()))??;
        match action {
            Action::Read(n) => {
                #[cfg(feature = "stats")]
                transport.stats.inc_rx_bytes(n);

                // Deserialize all the messages from the current ZBuf
                let zslice = ZSlice::make(Arc::new(buffer), 0, n).unwrap();
                transport.read_messages(zslice, &link)?;
            }
            Action::Stop => break,
        }
    }
    Ok(())
}

async fn rx_task(
    link: LinkUnicast,
    transport: ShmTransportUnicastInner,
    lease: Duration,
    rx_batch_size: u16,
    rx_buffer_size: usize,
) -> ZResult<()> {
    if link.is_streamed() {
        rx_task_stream(link, transport, lease, rx_batch_size, rx_buffer_size).await
    } else {
        rx_task_dgram(link, transport, lease, rx_batch_size, rx_buffer_size).await
    }
}
