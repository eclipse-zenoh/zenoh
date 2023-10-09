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
use zenoh_buffers::ZSliceBuffer;
use zenoh_link::{Link, LinkUnicast};
use zenoh_protocol::transport::TransportMessage;
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

    pub fn batch_config(&self) -> BatchConfig {
        BatchConfig {
            is_streamed: self.link.is_streamed(),
            #[cfg(feature = "transport_compression")]
            is_compression: self.config.is_compression,
        }
    }

    pub async fn send_batch(&self, batch: &mut WBatch) -> ZResult<()> {
        const ERR: &str = "Write error on link: ";
        batch.finalize().map_err(|_| zerror!("{ERR}{self}"))?;
        // Send the message on the link
        self.link.write_all(batch.as_slice()).await?;

        Ok(())
    }

    pub async fn send(&self, msg: &TransportMessage) -> ZResult<usize> {
        const ERR: &str = "Write error on link: ";
        // Create the batch for serializing the message
        let mut batch = WBatch::with_capacity(self.batch_config(), self.link.get_mtu());
        batch.encode(msg).map_err(|_| zerror!("{ERR}{self}"))?;
        let len = batch.len() as usize;
        self.send_batch(&mut batch).await?;
        Ok(len)
    }

    pub async fn recv_batch<C, T>(&self, buff: C) -> ZResult<RBatch>
    where
        C: Fn() -> T + Copy,
        T: ZSliceBuffer + 'static,
    {
        const ERR: &str = "Read error from link: ";
        let mut batch = RBatch::read_unicast(self.batch_config(), &self.link, buff).await?;
        batch.finalize(buff).map_err(|_| zerror!("{ERR}{self}"))?;
        Ok(batch)
    }

    pub async fn recv(&self) -> ZResult<TransportMessage> {
        let mut batch = self
            .recv_batch(|| {
                zenoh_buffers::vec::uninit(self.link.get_mtu() as usize).into_boxed_slice()
            })
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
