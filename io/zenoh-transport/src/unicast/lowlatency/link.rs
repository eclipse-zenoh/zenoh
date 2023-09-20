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
use super::transport::TransportUnicastLowlatency;
#[cfg(feature = "stats")]
use crate::stats::TransportStats;
use crate::TransportExecutor;
use async_std::task;
use async_std::{prelude::FutureExt, sync::RwLock};
use zenoh_codec::*;
use zenoh_core::{zasyncread, zasyncwrite};

use std::sync::Arc;
use std::time::Duration;
use zenoh_buffers::{writer::HasWriter, ZSlice};
use zenoh_link::LinkUnicast;
use zenoh_protocol::transport::{
    BatchSize, KeepAlive, TransportBodyLowLatency, TransportMessageLowLatency,
};
use zenoh_result::{zerror, ZResult};
use zenoh_sync::RecyclingObjectPool;

pub(crate) async fn send_with_link(
    link: &LinkUnicast,
    msg: TransportMessageLowLatency,
    #[cfg(feature = "stats")] stats: &Arc<TransportStats>,
) -> ZResult<()> {
    let len;
    if link.is_streamed() {
        let mut buffer = vec![0, 0, 0, 0];
        let codec = Zenoh080::new();
        let mut writer = buffer.writer();
        codec
            .write(&mut writer, &msg)
            .map_err(|_| zerror!("Error serializing message {:?}", msg))?;

        len = (buffer.len() - 4) as u32;
        let le = len.to_le_bytes();

        buffer[0..4].copy_from_slice(&le);

        link.write_all(&buffer).await?;
    } else {
        let mut buffer = vec![];
        let codec = Zenoh080::new();
        let mut writer = buffer.writer();
        codec
            .write(&mut writer, &msg)
            .map_err(|_| zerror!("Error serializing message {:?}", msg))?;

        #[cfg(feature = "stats")]
        {
            len = buffer.len() as u32;
        }
        link.write_all(&buffer).await?;
    }
    log::trace!("Sent: {:?}", msg);

    #[cfg(feature = "stats")]
    {
        stats.inc_tx_t_msgs(1);
        stats.inc_tx_bytes(len as usize);
    }
    Ok(())
}

impl TransportUnicastLowlatency {
    pub(super) fn send(&self, msg: TransportMessageLowLatency) -> ZResult<()> {
        async_std::task::block_on(self.send_async(msg))
    }

    pub(super) async fn send_async(&self, msg: TransportMessageLowLatency) -> ZResult<()> {
        let guard = zasyncwrite!(self.link);
        send_with_link(
            &guard,
            msg,
            #[cfg(feature = "stats")]
            &self.stats,
        )
        .await
    }

    pub(super) fn start_keepalive(&self, executor: &TransportExecutor, keep_alive: Duration) {
        let mut guard = async_std::task::block_on(async { zasyncwrite!(self.handle_keepalive) });
        let c_transport = self.clone();
        let handle = executor.spawn(async move {
            let res = keepalive_task(
                c_transport.link.clone(),
                keep_alive,
                #[cfg(feature = "stats")]
                c_transport.stats.clone(),
            )
            .await;
            log::debug!(
                "[{}] Keepalive task finished with result {:?}",
                c_transport.manager.config.zid,
                res
            );
            if res.is_err() {
                log::debug!(
                    "[{}] <on keepalive exit> finalizing transport with peer: {}",
                    c_transport.manager.config.zid,
                    c_transport.config.zid
                );
                let _ = c_transport.finalize(0).await;
            }
        });
        *guard = Some(handle);
    }

    pub(super) async fn stop_keepalive(&self) {
        let zid = self.manager.config.zid;
        log::debug!("[{}] Stopping keepalive task...", zid,);
        let mut guard = zasyncwrite!(self.handle_keepalive);
        let handle = guard.take();
        drop(guard);

        if let Some(handle) = handle {
            let _ = handle.cancel().await;
            log::debug!("[{}] keepalive task stopped...", zid,);
        }
    }

    pub(super) fn internal_start_rx(&self, lease: Duration, batch_size: u16) {
        let mut guard = async_std::task::block_on(async { zasyncwrite!(self.handle_rx) });
        let c_transport = self.clone();
        let handle = task::spawn(async move {
            let guard = zasyncread!(c_transport.link);
            let link = guard.clone();
            drop(guard);
            let rx_buffer_size = c_transport.manager.config.link_rx_buffer_size;

            // Start the rx task
            let res = rx_task(link, c_transport.clone(), lease, batch_size, rx_buffer_size).await;
            log::debug!(
                "[{}] Rx task finished with result {:?}",
                c_transport.manager.config.zid,
                res
            );
            if res.is_err() {
                log::debug!(
                    "[{}] <on rx exit> finalizing transport with peer: {}",
                    c_transport.manager.config.zid,
                    c_transport.config.zid
                );
                let _ = c_transport.finalize(0).await;
            }
        });
        *guard = Some(handle);
    }

    pub(super) async fn stop_rx(&self) {
        let zid = self.manager.config.zid;
        log::debug!("[{}] Stopping rx task...", zid,);
        let mut guard = zasyncwrite!(self.handle_rx);
        let handle = guard.take();
        drop(guard);

        if let Some(handle) = handle {
            let _ = handle.cancel().await;
            log::debug!("[{}] rx task stopped...", zid,);
        }
    }
}

/*************************************/
/*              TASKS                */
/*************************************/
async fn keepalive_task(
    link: Arc<RwLock<LinkUnicast>>,
    keep_alive: Duration,
    #[cfg(feature = "stats")] stats: Arc<TransportStats>,
) -> ZResult<()> {
    loop {
        async_std::task::sleep(keep_alive).await;

        let keepailve = TransportMessageLowLatency {
            body: TransportBodyLowLatency::KeepAlive(KeepAlive),
        };

        let guard = zasyncwrite!(link);
        let _ = send_with_link(
            &guard,
            keepailve,
            #[cfg(feature = "stats")]
            &stats,
        )
        .await;
        drop(guard);
    }
}

async fn rx_task_stream(
    link: LinkUnicast,
    transport: TransportUnicastLowlatency,
    lease: Duration,
    rx_batch_size: BatchSize,
    rx_buffer_size: usize,
) -> ZResult<()> {
    async fn read(link: &LinkUnicast, buffer: &mut [u8]) -> ZResult<usize> {
        // 16 bits for reading the batch length
        let mut length = [0_u8, 0_u8, 0_u8, 0_u8];
        link.read_exact(&mut length).await?;
        let n = u32::from_le_bytes(length) as usize;

        link.read_exact(&mut buffer[0..n]).await?;
        Ok(n)
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
        let bytes = read(&link, &mut buffer)
            .timeout(lease)
            .await
            .map_err(|_| zerror!("{}: expired after {} milliseconds", link, lease.as_millis()))??;
        #[cfg(feature = "stats")]
        transport.stats.inc_rx_bytes(2 + bytes); // Account for the batch len encoding (16 bits)

        // Deserialize all the messages from the current ZBuf
        let zslice = ZSlice::make(Arc::new(buffer), 0, bytes).unwrap();
        transport.read_messages(zslice, &link).await?;
    }
}

async fn rx_task_dgram(
    link: LinkUnicast,
    transport: TransportUnicastLowlatency,
    lease: Duration,
    rx_batch_size: BatchSize,
    rx_buffer_size: usize,
) -> ZResult<()> {
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
        let bytes =
            link.read(&mut buffer).timeout(lease).await.map_err(|_| {
                zerror!("{}: expired after {} milliseconds", link, lease.as_millis())
            })??;

        #[cfg(feature = "stats")]
        transport.stats.inc_rx_bytes(bytes);

        // Deserialize all the messages from the current ZBuf
        let zslice = ZSlice::make(Arc::new(buffer), 0, bytes).unwrap();
        transport.read_messages(zslice, &link).await?;
    }
}

async fn rx_task(
    link: LinkUnicast,
    transport: TransportUnicastLowlatency,
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
