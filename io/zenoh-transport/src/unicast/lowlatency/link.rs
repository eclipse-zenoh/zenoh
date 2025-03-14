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
use std::{sync::Arc, time::Duration};

use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use zenoh_buffers::{writer::HasWriter, ZSlice};
use zenoh_codec::*;
use zenoh_core::{zasyncread, zasyncwrite};
use zenoh_link::LinkUnicast;
use zenoh_protocol::transport::{
    KeepAlive, TransportBodyLowLatencyRef, TransportMessageLowLatencyRef,
};
use zenoh_result::{zerror, ZResult};
use zenoh_runtime::ZRuntime;

use super::transport::TransportUnicastLowlatency;
#[cfg(feature = "stats")]
use crate::stats::TransportStats;
use crate::unicast::link::{TransportLinkUnicast, TransportLinkUnicastRx};

pub(crate) async fn send_with_link(
    link: &LinkUnicast,
    msg: TransportMessageLowLatencyRef<'_>,
    #[cfg(feature = "stats")] stats: &Arc<TransportStats>,
) -> ZResult<()> {
    let len;
    let codec = Zenoh080::new();
    if link.is_streamed() {
        let mut buffer = vec![0, 0, 0, 0];
        let mut writer = buffer.writer();
        codec
            .write(&mut writer, msg)
            .map_err(|_| zerror!("Error serializing message {:?}", msg))?;

        len = (buffer.len() - 4) as u32;
        let le = len.to_le_bytes();

        buffer[0..4].copy_from_slice(&le);

        link.write_all(&buffer).await?;
    } else {
        let mut buffer = vec![];
        let mut writer = buffer.writer();
        codec
            .write(&mut writer, msg)
            .map_err(|_| zerror!("Error serializing message {:?}", msg))?;

        #[cfg(feature = "stats")]
        {
            len = buffer.len() as u32;
        }
        link.write_all(&buffer).await?;
    }
    tracing::trace!("Sent: {:?}", msg);

    #[cfg(feature = "stats")]
    {
        stats.inc_tx_t_msgs(1);
        stats.inc_tx_bytes(len as usize);
    }
    Ok(())
}

pub(crate) async fn read_with_link(
    link: &TransportLinkUnicastRx,
    buffer: &mut [u8],
    is_streamed: bool,
) -> ZResult<usize> {
    if is_streamed {
        // 16 bits for reading the batch length
        let mut length = [0_u8; 4];
        link.link.read_exact(&mut length).await?;
        let n = u32::from_le_bytes(length) as usize;
        let len = buffer.len();
        let b = buffer.get_mut(0..n).ok_or_else(|| {
            zerror!("Batch len is invalid. Received {n} but negotiated max len is {len}.")
        })?;
        link.link.read_exact(b).await?;
        Ok(n)
    } else {
        link.link.read(buffer).await
    }
}

impl TransportUnicastLowlatency {
    pub(super) fn send(&self, msg: TransportMessageLowLatencyRef) -> ZResult<()> {
        zenoh_runtime::ZRuntime::TX.block_in_place(self.send_async(msg))
    }

    pub(super) async fn send_async(&self, msg: TransportMessageLowLatencyRef<'_>) -> ZResult<()> {
        let guard = zasyncwrite!(self.link);
        let link = &guard.as_ref().ok_or_else(|| zerror!("No link"))?.link;
        send_with_link(
            link,
            msg,
            #[cfg(feature = "stats")]
            &self.stats,
        )
        .await
    }

    pub(super) fn start_keepalive(&self, keep_alive: Duration) {
        let c_transport = self.clone();
        let token = self.token.child_token();
        let task = async move {
            let res = keepalive_task(
                c_transport.link.clone(),
                keep_alive,
                token,
                #[cfg(feature = "stats")]
                c_transport.stats.clone(),
            )
            .await;
            tracing::debug!(
                "[{}] Keepalive task finished with result {:?}",
                c_transport.manager.config.zid,
                res
            );
            if res.is_err() {
                tracing::debug!(
                    "[{}] <on keepalive exit> finalizing transport with peer: {}",
                    c_transport.manager.config.zid,
                    c_transport.config.zid
                );

                // Spawn a task to avoid a deadlock waiting for this same task
                // to finish in the close() joining its handle
                // WARN: Must be spawned on RX
                zenoh_runtime::ZRuntime::RX.spawn(async move { c_transport.finalize(0).await });
            }
        };
        self.tracker.spawn_on(task, &ZRuntime::TX);
    }

    pub(super) fn internal_start_rx(&self, lease: Duration) {
        let rx_buffer_size = self.manager.config.link_rx_buffer_size;
        let token = self.token.child_token();

        let c_transport = self.clone();
        let rx_task = async move {
            let guard = zasyncread!(c_transport.link);
            let link_rx = guard.as_ref().unwrap().rx();
            drop(guard);

            let is_streamed = link_rx.link.is_streamed();

            // The pool of buffers
            let pool = {
                let mtu = link_rx.config.batch.mtu as usize;
                let mut n = rx_buffer_size / mtu;
                if n == 0 {
                    tracing::debug!("RX configured buffer of {rx_buffer_size} bytes is too small for {link_rx} that has an MTU of {mtu} bytes. Defaulting to {mtu} bytes for RX buffer.");
                    n = 1;
                }
                zenoh_sync::RecyclingObjectPool::new(n, move || vec![0_u8; mtu].into_boxed_slice())
            };

            loop {
                // Retrieve one buffer
                let mut buffer = pool.try_take().unwrap_or_else(|| pool.alloc());

                tokio::select! {
                    // Async read from the underlying link
                    res = tokio::time::timeout(lease, read_with_link(&link_rx, &mut buffer, is_streamed)) => {
                        let bytes = res.map_err(|_| zerror!("{}: expired after {} milliseconds", link_rx, lease.as_millis()))??;

                        #[cfg(feature = "stats")] {
                            let header_bytes = if is_streamed { 2 } else { 0 };
                            c_transport.stats.inc_rx_bytes(header_bytes + bytes); // Account for the batch len encoding (16 bits)
                        }

                        // Deserialize all the messages from the current ZBuf
                        let zslice = ZSlice::new(Arc::new(buffer), 0, bytes).unwrap();
                        c_transport.read_messages(zslice, &link_rx.link).await?;
                    }

                    _ = token.cancelled() => {
                        break ZResult::Ok(());
                    }
                }
            }
        };

        let c_transport = self.clone();
        self.tracker.spawn_on(
            async move {
                let res = rx_task.await;
                tracing::debug!(
                    "[{}] Rx task finished with result {:?}",
                    c_transport.manager.config.zid,
                    res
                );
                if res.is_err() {
                    tracing::debug!(
                        "[{}] <on rx exit> finalizing transport with peer: {}",
                        c_transport.manager.config.zid,
                        c_transport.config.zid
                    );

                    // Spawn a task to avoid a deadlock waiting for this same task
                    // to finish in the close() joining its handle
                    // WARN: Must be spawned on RX
                    zenoh_runtime::ZRuntime::RX.spawn(async move { c_transport.finalize(0).await });
                }
            },
            &ZRuntime::RX,
        );
    }
}

/*************************************/
/*              TASKS                */
/*************************************/
async fn keepalive_task(
    link: Arc<RwLock<Option<TransportLinkUnicast>>>,
    keep_alive: Duration,
    token: CancellationToken,
    #[cfg(feature = "stats")] stats: Arc<TransportStats>,
) -> ZResult<()> {
    let mut interval =
        tokio::time::interval_at(tokio::time::Instant::now() + keep_alive, keep_alive);

    loop {
        tokio::select! {
            _ = interval.tick() => {
                let keepailve = TransportMessageLowLatencyRef {
                    body: TransportBodyLowLatencyRef::KeepAlive(KeepAlive),
                };

                let guard = zasyncwrite!(link);
                let link = &guard.as_ref().ok_or_else(|| zerror!("No link"))?.link;
                let _ = send_with_link(
                    link,
                    keepailve,
                    #[cfg(feature = "stats")]
                    &stats,
                )
                .await;
                drop(guard);
            }

            _ = token.cancelled() => break,
        }
    }
    Ok(())
}
