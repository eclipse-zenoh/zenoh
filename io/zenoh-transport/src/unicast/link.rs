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
use crate::common::batch::{BatchConfig, Decode, Encode, Finalize, RBatch, WBatch};
use std::fmt;
use std::sync::Arc;
use zenoh_buffers::{BBuf, ZSlice, ZSliceBuffer};
use zenoh_link::{Link, LinkUnicast};
use zenoh_protocol::transport::{BatchSize, Close, TransportMessage};
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

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct TransportLinkUnicast {
    pub(crate) link: LinkUnicast,
    pub(crate) config: TransportLinkUnicastConfig,
    #[cfg(feature = "transport_compression")]
    pub(crate) buffer: Option<BBuf>,
}

impl TransportLinkUnicast {
    pub fn new(link: LinkUnicast, config: TransportLinkUnicastConfig) -> Self {
        Self {
            link,
            config,
            #[cfg(feature = "transport_compression")]
            buffer: config.is_compression.then_some(BBuf::with_capacity(
                lz4_flex::block::get_maximum_output_size(config.mtu as usize),
            )),
        }
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

        // log::trace!("WBatch: {:?}", batch);

        let res = batch
            .finalize(
                #[cfg(feature = "transport_compression")]
                self.buffer.as_mut(),
            )
            .map_err(|_| zerror!("{ERR}{self}"))?;

        let bytes = match res {
            Finalize::Batch => batch.as_slice(),
            #[cfg(feature = "transport_compression")]
            Finalize::Buffer => self
                .buffer
                .as_ref()
                .ok_or_else(|| zerror!("Invalid buffer finalization"))?
                .as_slice(),
        };

        // log::trace!("WBytes: {:02x?}", bytes);

        // Send the message on the link
        if self.link.is_streamed() {
            let len: BatchSize = bytes
                .len()
                .try_into()
                .map_err(|_| zerror!("Invalid batch length"))?;
            let len = len.to_le_bytes();
            self.link.write_all(&len).await?;
        }
        self.link.write_all(bytes).await?;

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

        // log::trace!("RBytes: {:02x?}", &into.as_slice()[0..end]);

        let buffer = ZSlice::make(Arc::new(into), 0, end)
            .map_err(|_| zerror!("{ERR}{self}. ZSlice index(es) out of bounds"))?;
        let mut batch = RBatch::new(self.batch_config(), buffer);
        batch
            .initialize(buff)
            .map_err(|e| zerror!("{ERR}{self}. {e}."))?;

        // log::trace!("RBatch: {:?}", batch);

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

    pub async fn close(&mut self, reason: Option<u8>) -> ZResult<()> {
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

impl fmt::Display for TransportLinkUnicast {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.link)
    }
}

impl fmt::Debug for TransportLinkUnicast {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TransportLinkUnicast")
            .field("link", &self.link)
            .field("config", &self.config)
            .field("buffer", &self.buffer.as_ref().map(|b| b.capacity()))
            .finish()
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
