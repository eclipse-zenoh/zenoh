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
use zenoh_codec::*;
use zenoh_core::zasyncwrite;

use std::sync::Arc;
use std::time::Duration;
use zenoh_buffers::{writer::HasWriter, ZSlice};
use zenoh_link::LinkUnicast;
use zenoh_protocol::transport::TransportMessageLowLatency;
use zenoh_result::{zerror, ZResult};
use zenoh_runtime::ZRuntime;

pub(crate) async fn send_with_link(
    link: &TransportLinkUnicast,
    msg: TransportMessageLowLatency,
    #[cfg(feature = "stats")] stats: &Arc<TransportStats>,
) -> ZResult<()> {
    let len;
    let codec = Zenoh080::new();
    if link.is_streamed() {
        let mut buffer = vec![0; 4];
        let mut writer = buffer.writer();
        codec
            .write(&mut writer, &msg)
            .map_err(|_| zerror!("Error serializing message {:?}", msg))?;

        len = (buffer.len() - 4) as u32;
        let le = len.to_le_bytes();

        buffer[0..4].copy_from_slice(&le);

        link.link.write_all(&buffer).await?;
    } else {
        let mut buffer = vec![];
        let mut writer = buffer.writer();
        codec
            .write(&mut writer, &msg)
            .map_err(|_| zerror!("Error serializing message {:?}", msg))?;

        #[cfg(feature = "stats")]
        {
            len = buffer.len() as u32;
        }
        link.link.write_all(&buffer).await?;
    }
    log::trace!("Sent: {:?}", msg);

    #[cfg(feature = "stats")]
    {
        stats.inc_tx_t_msgs(1);
        stats.inc_tx_bytes(len as usize);
    }
    Ok(())
}

pub(crate) async fn read_with_link(link: &LinkUnicast, buffer: &mut [u8]) -> ZResult<usize> {
    if link.is_streamed() {
        // 16 bits for reading the batch length
        let mut length = [0_u8; 4];
        link.read_exact(&mut length).await?;
        let n = u32::from_le_bytes(length) as usize;
        link.read_exact(&mut buffer[0..n]).await?;
        Ok(n)
    } else {
        link.read(buffer).await
    }
}

impl TransportUnicastLowlatency {
    pub(super) async fn send_async(&self, msg: TransportMessageLowLatency) -> ZResult<()> {
        let guard = zasyncwrite!(self.link);
        let res = send_with_link(
            &guard,
            msg,
            #[cfg(feature = "stats")]
            &self.stats,
        )
        .await;

        // FIXME
        #[cfg(feature = "stats")]
        if res.is_ok() {
            self.stats.inc_tx_n_msgs(1);
        } else {
            self.stats.inc_tx_n_dropped(1);
        }

        res
    }

    pub(super) fn internal_start_rx(&self, lease: Duration, batch_size: u16) {
        let link = tokio::task::block_in_place(|| {
            ZRuntime::RX.block_on(async { self.link.read().await.clone() })
        });
        // self.link.blocking_read().clone());
        // The pool of buffers
        let pool = {
            let rx_buffer_size = self.manager.config.link_rx_buffer_size;
            let mtu = link.get_mtu().min(batch_size) as usize;
            let mut n = rx_buffer_size / mtu;
            if rx_buffer_size % mtu != 0 {
                n += 1;
            }
            zenoh_sync::RecyclingObjectPool::new(n, move || vec![0_u8; mtu].into_boxed_slice())
        };

        let token = self.cancellation_token.child_token();
        // TODO: This can be improved to minimal
        let transport = self.clone();
        let task = async move {
            loop {
                // Retrieve one buffer
                let mut buffer = pool.try_take().unwrap_or_else(|| pool.alloc());

                tokio::select! {
                    // Async read from the underlying link
                    res = tokio::time::timeout(lease, read_with_link(&link, &mut buffer)) => {
                        let bytes = res.map_err(|_| zerror!("{}: expired after {} milliseconds", link, lease.as_millis()))??;

                        #[cfg(feature = "stats")] {
                            let header_bytes = if link.is_streamed() { 2 } else { 0 };
                            transport.stats.inc_rx_bytes(header_bytes + bytes); // Account for the batch len encoding (16 bits)
                        }

                        // Deserialize all the messages from the current ZBuf
                        let zslice = ZSlice::make(Arc::new(buffer), 0, bytes).unwrap();
                        transport.read_messages(zslice, &link).await?;
                    }

                    _ = token.cancelled() => {
                        break;
                    }
                }
            }
            ZResult::Ok(())
        };

        self.task_tracker.spawn_on(task, &ZRuntime::RX);
    }
}
