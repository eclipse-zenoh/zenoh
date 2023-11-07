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
use crate::common::batch::{BatchConfig, Decode, Encode, RBatch, WBatch};
use std::fmt;
use std::sync::Arc;
use zenoh_buffers::{ZSlice, ZSliceBuffer};
use zenoh_link::{Link, LinkUnicast};
use zenoh_protocol::transport::{BatchSize, TransportMessage};
use zenoh_result::{zerror, ZResult};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum TransportLinkUnicastDirection {
    Inbound,
    Outbound,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct TransportLinkUnicastConfig {
    // Inbound / outbound
    pub(crate) direction: TransportLinkUnicastDirection,
    // MTU
    pub(crate) mtu: BatchSize,
    // Compression is active on the link
    #[cfg(feature = "transport_compression")]
    pub(crate) is_compression: bool,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct TransportLinkUnicast {
    pub(crate) link: LinkUnicast,
    pub(crate) config: TransportLinkUnicastConfig,
}

impl TransportLinkUnicast {
    pub fn new(link: LinkUnicast, config: TransportLinkUnicastConfig) -> Self {
        Self { link, config }
    }

    const fn batch_config(&self) -> BatchConfig {
        BatchConfig {
            mtu: self.config.mtu,
            #[cfg(feature = "transport_compression")]
            is_compression: self.config.is_compression,
        }
    }

    pub async fn send_batch(&mut self, batch: &mut WBatch) -> ZResult<()> {
        const ERR: &str = "Write error on link: ";

        batch.finalize().map_err(|_| zerror!("{ERR}{self}"))?;

        // Send the message on the link
        if self.link.is_streamed() {
            let len = batch.len().to_le_bytes();
            self.link.write_all(&len).await?;
        }
        self.link.write_all(batch.as_slice()).await?;

        Ok(())
    }

    pub async fn send(&mut self, msg: &TransportMessage) -> ZResult<usize> {
        const ERR: &str = "Write error on link: ";

        // Create the batch for serializing the message
        let mut batch = WBatch::new(self.batch_config());
        batch.encode(msg).map_err(|_| zerror!("{ERR}{self}"))?;
        let len = batch.len() as usize;
        self.send_batch(&mut batch).await?;
        Ok(len)
    }

    pub async fn recv_batch<C, T>(&mut self, buff: C) -> ZResult<RBatch>
    where
        C: Fn() -> T + Copy,
        T: ZSliceBuffer + 'static,
    {
        const ERR: &str = "Read error from link: ";

        let mut into = (buff)();
        let end = if self.link.is_streamed() {
            // Read and decode the message length
            let mut len = BatchSize::MIN.to_le_bytes();
            self.link.read_exact(&mut len).await?;
            let len = BatchSize::from_le_bytes(len) as usize;

            // Read the bytes
            let slice = into
                .as_mut_slice()
                .get_mut(..len)
                .ok_or_else(|| zerror!("{ERR}{self}. Invalid batch length or buffer size."))?;
            self.link.read_exact(slice).await?;
            len
        } else {
            // Read the bytes
            self.link.read(into.as_mut_slice()).await?
        };

        let buffer = ZSlice::make(Arc::new(into), 0, end)
            .map_err(|_| zerror!("{ERR}{self}. ZSlice index(es) out of bounds"))?;

        log::trace!("RBatch: {:?}", buffer);

        let mut batch = RBatch::new(self.batch_config(), buffer);
        batch
            .initialize(buff)
            .map_err(|e| zerror!("{ERR}{self}. {e}."))?;

        Ok(batch)
    }

    pub async fn recv(&mut self) -> ZResult<TransportMessage> {
        let mtu = self.link.get_mtu() as usize;
        let mut batch = self
            .recv_batch(|| zenoh_buffers::vec::uninit(mtu).into_boxed_slice())
            .await?;
        let msg = batch
            .decode()
            .map_err(|_| zerror!("Decode error on link: {}", self))?;
        Ok(msg)
    }

    pub async fn close(&self) -> ZResult<()> {
        self.link.close().await
    }
}

impl fmt::Display for TransportLinkUnicast {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.link)
    }
}

impl From<&TransportLinkUnicast> for Link {
    fn from(link: &TransportLinkUnicast) -> Self {
        Link::from(&link.link)
    }
}

impl From<TransportLinkUnicast> for Link {
    fn from(link: TransportLinkUnicast) -> Self {
        Link::from(link.link)
    }
}
