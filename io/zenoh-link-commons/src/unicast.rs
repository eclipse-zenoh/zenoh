//
// Copyright (c) 2022 ZettaScale Technology
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
use alloc::{boxed::Box, sync::Arc, vec::Vec};
use async_trait::async_trait;
use core::{
    convert::TryFrom,
    fmt,
    hash::{Hash, Hasher},
    ops::Deref,
};
use zenoh_buffers::{
    reader::HasReader,
    writer::{HasWriter, Writer},
};
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_protocol::{
    core::{EndPoint, Locator},
    transport::TransportMessage,
};
use zenoh_result::{zerror, ZResult};

pub type LinkManagerUnicast = Arc<dyn LinkManagerUnicastTrait>;
#[async_trait]
pub trait LinkManagerUnicastTrait: Send + Sync {
    async fn new_link(&self, endpoint: EndPoint) -> ZResult<LinkUnicast>;
    async fn new_listener(&self, endpoint: EndPoint) -> ZResult<Locator>;
    async fn del_listener(&self, endpoint: &EndPoint) -> ZResult<()>;
    fn get_listeners(&self) -> Vec<EndPoint>;
    fn get_locators(&self) -> Vec<Locator>;
}
pub type NewLinkChannelSender = flume::Sender<LinkUnicast>;
pub trait ConstructibleLinkManagerUnicast<T>: Sized {
    fn new(new_link_sender: NewLinkChannelSender, config: T) -> ZResult<Self>;
}

#[derive(Clone, PartialEq, Eq)]
pub enum LinkUnicastDirection {
    Inbound,
    Outbound,
}

#[derive(Clone)]
pub struct LinkUnicast(pub Arc<dyn LinkUnicastTrait>);

#[async_trait]
pub trait LinkUnicastTrait: Send + Sync {
    fn get_mtu(&self) -> u16;
    fn get_src(&self) -> &Locator;
    fn get_dst(&self) -> &Locator;
    fn is_reliable(&self) -> bool;
    fn is_streamed(&self) -> bool;
    async fn write(&self, buffer: &[u8]) -> ZResult<usize>;
    async fn write_all(&self, buffer: &[u8]) -> ZResult<()>;
    async fn read(&self, buffer: &mut [u8]) -> ZResult<usize>;
    async fn read_exact(&self, buffer: &mut [u8]) -> ZResult<()>;
    async fn close(&self) -> ZResult<()>;
}

impl LinkUnicast {
    pub async fn send(&self, msg: &TransportMessage) -> ZResult<usize> {
        const ERR: &str = "Write error on link: ";

        // Create the buffer for serializing the message
        let mut buff = Vec::new();
        let mut writer = buff.writer();
        let codec = Zenoh080::new();

        // Reserve 16 bits to write the length
        if self.is_streamed() {
            writer
                .write_exact(u16::MIN.to_le_bytes().as_slice())
                .map_err(|_| zerror!("{ERR}{self}"))?;
        }
        // Serialize the message
        codec
            .write(&mut writer, msg)
            .map_err(|_| zerror!("{ERR}{self}"))?;

        // Write the length
        if self.is_streamed() {
            let num = u16::MIN.to_le_bytes().len();
            let len = u16::try_from(writer.len() - num).map_err(|_| zerror!("{ERR}{self}"))?;
            buff[..num].copy_from_slice(len.to_le_bytes().as_slice());
        }

        // Send the message on the link
        self.0.write_all(buff.as_slice()).await?;

        Ok(buff.len())
    }

    pub async fn recv(&self) -> ZResult<TransportMessage> {
        // Read from the link
        let buffer = if self.is_streamed() {
            // Read and decode the message length
            let mut length_bytes = [0_u8; 2];
            self.read_exact(&mut length_bytes).await?;
            let to_read = u16::from_le_bytes(length_bytes) as usize;
            // Read the message
            let mut buffer = zenoh_buffers::vec::uninit(to_read);
            self.read_exact(&mut buffer).await?;
            buffer
        } else {
            // Read the message
            let mut buffer = zenoh_buffers::vec::uninit(self.get_mtu() as usize);
            let n = self.read(&mut buffer).await?;
            buffer.truncate(n);
            buffer
        };

        let mut reader = buffer.reader();
        let codec = Zenoh080::new();

        let msg: TransportMessage = codec
            .read(&mut reader)
            .map_err(|_| zerror!("Read error on link: {}", self))?;

        Ok(msg)
    }
}

impl Deref for LinkUnicast {
    type Target = Arc<dyn LinkUnicastTrait>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Eq for LinkUnicast {}

impl PartialEq for LinkUnicast {
    fn eq(&self, other: &Self) -> bool {
        self.get_src() == other.get_src() && self.get_dst() == other.get_dst()
    }
}

impl Hash for LinkUnicast {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.get_src().hash(state);
        self.get_dst().hash(state);
    }
}

impl fmt::Display for LinkUnicast {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} => {}", self.get_src(), self.get_dst())
    }
}

impl fmt::Debug for LinkUnicast {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Link")
            .field("src", &self.get_src())
            .field("dst", &self.get_dst())
            .field("mtu", &self.get_mtu())
            .field("is_reliable", &self.is_reliable())
            .field("is_streamed", &self.is_streamed())
            .finish()
    }
}

impl From<Arc<dyn LinkUnicastTrait>> for LinkUnicast {
    fn from(link: Arc<dyn LinkUnicastTrait>) -> LinkUnicast {
        LinkUnicast(link)
    }
}
