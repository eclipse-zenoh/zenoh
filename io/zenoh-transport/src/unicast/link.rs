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
use std::{fmt, sync::Arc};

use zenoh_buffers::{BBuf, ZSlice, ZSliceBuffer};
use zenoh_core::zcondfeat;
use zenoh_link::{Link, LinkUnicast};
use zenoh_protocol::{
    core::{PriorityRange, Reliability},
    transport::{BatchSize, Close, OpenAck, TransportMessage},
};
use zenoh_result::{zerror, ZResult};

use crate::common::batch::{BatchConfig, Decode, Encode, Finalize, RBatch, WBatch};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum TransportLinkUnicastDirection {
    Inbound,
    Outbound,
}

#[derive(Clone, Debug)]
pub(crate) struct TransportLinkUnicastConfig {
    // Inbound / outbound
    pub(crate) direction: TransportLinkUnicastDirection,
    pub(crate) batch: BatchConfig,
    pub(crate) priorities: Option<PriorityRange>,
    pub(crate) reliability: Option<Reliability>,
}

#[derive(Clone)]
pub(crate) struct TransportLinkUnicast {
    pub(crate) link: LinkUnicast,
    pub(crate) config: TransportLinkUnicastConfig,
}

impl TransportLinkUnicast {
    pub(crate) fn new(link: LinkUnicast, config: TransportLinkUnicastConfig) -> Self {
        Self::init(link, config)
    }

    pub(crate) fn reconfigure(self, new_config: TransportLinkUnicastConfig) -> Self {
        Self::init(self.link, new_config)
    }

    fn init(link: LinkUnicast, mut config: TransportLinkUnicastConfig) -> Self {
        config.batch.mtu = link.get_mtu().min(config.batch.mtu);
        Self { link, config }
    }

    pub(crate) fn link(&self) -> Link {
        Link::new_unicast(
            &self.link,
            self.config.priorities.clone(),
            self.config.reliability,
        )
    }

    pub(crate) fn tx(&self) -> TransportLinkUnicastTx {
        TransportLinkUnicastTx {
            inner: self.clone(),
            buffer: zcondfeat!(
                "transport_compression",
                self.config
                    .batch
                    .is_compression
                    .then_some(BBuf::with_capacity(
                        lz4_flex::block::get_maximum_output_size(self.config.batch.mtu as usize),
                    )),
                None
            ),
        }
    }

    pub(crate) fn rx(&self) -> TransportLinkUnicastRx {
        TransportLinkUnicastRx {
            link: self.link.clone(),
            config: self.config.clone(),
        }
    }

    pub(crate) async fn send(&self, msg: &TransportMessage) -> ZResult<usize> {
        let mut link = self.tx();
        link.send(msg).await
    }

    pub(crate) async fn recv(&self) -> ZResult<TransportMessage> {
        let mut link = self.rx();
        link.recv().await
    }

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
            .finish()
    }
}

impl PartialEq<Link> for TransportLinkUnicast {
    fn eq(&self, other: &Link) -> bool {
        &other.src == self.link.get_src() && &other.dst == self.link.get_dst()
    }
}

pub(crate) struct TransportLinkUnicastTx {
    pub(crate) inner: TransportLinkUnicast,
    pub(crate) buffer: Option<BBuf>,
}

impl TransportLinkUnicastTx {
    pub(crate) async fn send_batch(&mut self, batch: &mut WBatch) -> ZResult<()> {
        const ERR: &str = "Write error on link: ";

        // tracing::trace!("WBatch: {:?}", batch);

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

        // tracing::trace!("WBytes: {:02x?}", bytes);

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

impl fmt::Display for TransportLinkUnicastTx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl fmt::Debug for TransportLinkUnicastTx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TransportLinkUnicastRx")
            .field("link", &self.inner.link)
            .field("config", &self.inner.config)
            .field("buffer", &self.buffer.as_ref().map(|b| b.capacity()))
            .finish()
    }
}

pub(crate) struct TransportLinkUnicastRx {
    pub(crate) link: LinkUnicast,
    pub(crate) config: TransportLinkUnicastConfig,
}

impl TransportLinkUnicastRx {
    pub async fn recv_batch<C, T>(&mut self, buff: C) -> ZResult<RBatch>
    where
        C: Fn() -> T + Copy,
        T: AsMut<[u8]> + ZSliceBuffer + 'static,
    {
        const ERR: &str = "Read error from link: ";

        let mut into = (buff)();
        let end = if self.link.is_streamed() {
            // Read and decode the message length
            let mut len = BatchSize::MIN.to_le_bytes();
            self.link.read_exact(&mut len).await?;
            let l = BatchSize::from_le_bytes(len) as usize;

            // Read the bytes
            let slice = into
                .as_mut()
                .get_mut(len.len()..len.len() + l)
                .ok_or_else(|| zerror!("{ERR}{self}. Invalid batch length or buffer size."))?;
            self.link.read_exact(slice).await?;
            len.len() + l
        } else {
            // Read the bytes
            self.link.read(into.as_mut()).await?
        };

        // tracing::trace!("RBytes: {:02x?}", &into.as_slice()[0..end]);

        let buffer = ZSlice::new(Arc::new(into), 0, end)
            .map_err(|_| zerror!("{ERR}{self}. ZSlice index(es) out of bounds"))?;
        let mut batch = RBatch::new(self.config.batch, buffer);
        batch
            .initialize(buff)
            .map_err(|e| zerror!("{ERR}{self}. {e}."))?;

        // tracing::trace!("RBatch: {:?}", batch);

        Ok(batch)
    }

    pub async fn recv(&mut self) -> ZResult<TransportMessage> {
        let mtu = self.config.batch.mtu as usize;
        let mut batch = self
            .recv_batch(|| zenoh_buffers::vec::uninit(mtu).into_boxed_slice())
            .await?;
        let msg = batch
            .decode()
            .map_err(|_| zerror!("Decode error on link: {}", self))?;
        Ok(msg)
    }
}

impl fmt::Display for TransportLinkUnicastRx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{:?}", self.link, self.config)
    }
}

impl fmt::Debug for TransportLinkUnicastRx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TransportLinkUnicastRx")
            .field("link", &self.link)
            .field("config", &self.config)
            .finish()
    }
}

pub(crate) struct MaybeOpenAck {
    link: TransportLinkUnicastTx,
    open_ack: Option<OpenAck>,
}

impl MaybeOpenAck {
    pub(crate) fn new(link: &TransportLinkUnicast, open_ack: Option<OpenAck>) -> Self {
        Self {
            link: link.tx(),
            open_ack,
        }
    }

    pub(crate) async fn send_open_ack(mut self) -> ZResult<()> {
        if let Some(msg) = self.open_ack {
            zcondfeat!(
                "transport_compression",
                {
                    // !!! Workaround !!! as the state of the link is set with compression once the OpenSyn is received.
                    // Here we are disabling the compression just to send the OpenAck (that is not supposed to be compressed).
                    // Then then we re-enable it, in case it was enabled, after the OpenAck has been sent.
                    let compression = self.link.inner.config.batch.is_compression;
                    self.link.inner.config.batch.is_compression = false;
                    self.link.send(&msg.into()).await?;
                    self.link.inner.config.batch.is_compression = compression;
                },
                {
                    self.link.send(&msg.into()).await?;
                }
            )
        }
        Ok(())
    }

    pub(crate) fn link(&self) -> Link {
        self.link.inner.link()
    }
}

pub(crate) struct LinkUnicastWithOpenAck {
    pub(crate) link: TransportLinkUnicast,
    ack: Option<OpenAck>,
}

impl LinkUnicastWithOpenAck {
    pub(crate) fn new(link: TransportLinkUnicast, ack: Option<OpenAck>) -> Self {
        Self { link, ack }
    }

    pub(crate) fn inner_config(&self) -> &TransportLinkUnicastConfig {
        &self.link.config
    }

    pub(crate) fn unpack(self) -> (TransportLinkUnicast, MaybeOpenAck) {
        let ack = MaybeOpenAck::new(&self.link, self.ack);
        (self.link, ack)
    }

    pub(crate) fn fail(self) -> TransportLinkUnicast {
        self.link
    }
}

impl fmt::Display for LinkUnicastWithOpenAck {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.ack.as_ref() {
            Some(ack) => write!(f, "{}({:?})", self.link, ack),
            None => write!(f, "{}", self.link),
        }
    }
}
