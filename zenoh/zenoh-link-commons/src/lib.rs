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

use async_std::sync::Arc;
use async_trait::async_trait;
use std::borrow::Cow;
use std::cmp::PartialEq;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use zenoh_buffers::{WBuf, ZBuf};
use zenoh_core::{bail, Result as ZResult};
use zenoh_protocol::proto::{MessageReader, MessageWriter, TransportMessage};
use zenoh_protocol_core::{endpoint, locator, EndPoint, Locator};

const WBUF_SIZE: usize = 64;

/*************************************/
/*            GENERAL                */
/*************************************/
#[derive(Clone, Debug)]
pub struct Link {
    pub src: Locator,
    pub dst: Locator,
    pub group: Option<Locator>,
    pub mtu: u16,
    pub is_reliable: bool,
    pub is_streamed: bool,
}

#[async_trait]
pub trait LocatorInspector: Default {
    fn protocol(&self) -> Cow<'static, str>;
    async fn is_multicast(&self, locator: &locator) -> ZResult<bool>;
}

impl fmt::Display for Link {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} => {}", &*self.src, &*self.dst)
    }
}

impl From<&LinkUnicast> for Link {
    fn from(link: &LinkUnicast) -> Link {
        Link {
            src: link.get_src().to_owned(),
            dst: link.get_dst().to_owned(),
            group: None,
            mtu: link.get_mtu(),
            is_reliable: link.is_reliable(),
            is_streamed: link.is_streamed(),
        }
    }
}

impl From<LinkUnicast> for Link {
    fn from(link: LinkUnicast) -> Link {
        Link::from(&link)
    }
}

impl From<&LinkMulticast> for Link {
    fn from(link: &LinkMulticast) -> Link {
        Link {
            src: link.get_src().to_owned(),
            dst: link.get_dst().to_owned(),
            group: Some(link.get_dst().to_owned()),
            mtu: link.get_mtu(),
            is_reliable: link.is_reliable(),
            is_streamed: false,
        }
    }
}

impl From<LinkMulticast> for Link {
    fn from(link: LinkMulticast) -> Link {
        Link::from(&link)
    }
}

/*************************************/
/*            UNICAST                */
/*************************************/

pub type LinkManagerUnicast = Arc<dyn LinkManagerUnicastTrait>;
#[async_trait]
pub trait LinkManagerUnicastTrait: Send + Sync {
    async fn new_link(&self, endpoint: EndPoint) -> ZResult<LinkUnicast>;
    async fn new_listener(&self, endpoint: EndPoint) -> ZResult<Locator>;
    async fn del_listener(&self, endpoint: &endpoint) -> ZResult<()>;
    fn get_listeners(&self) -> Vec<EndPoint>;
    fn get_locators(&self) -> Vec<Locator>;
}
pub type NewLinkChannelSender = flume::Sender<LinkUnicast>;
pub trait ConstructibleLinkManagerUnicast<T>: Sized {
    fn new(new_link_sender: NewLinkChannelSender, config: T) -> ZResult<Self>;
}

#[derive(Clone, PartialEq)]
pub enum LinkUnicastDirection {
    Inbound,
    Outbound,
}

#[derive(Clone)]
pub struct LinkUnicast(pub Arc<dyn LinkUnicastTrait>);

#[async_trait]
pub trait LinkUnicastTrait: Send + Sync {
    fn get_mtu(&self) -> u16;
    fn get_src(&self) -> &locator;
    fn get_dst(&self) -> &locator;
    fn is_reliable(&self) -> bool;
    fn is_streamed(&self) -> bool;
    async fn write(&self, buffer: &[u8]) -> ZResult<usize>;
    async fn write_all(&self, buffer: &[u8]) -> ZResult<()>;
    async fn read(&self, buffer: &mut [u8]) -> ZResult<usize>;
    async fn read_exact(&self, buffer: &mut [u8]) -> ZResult<()>;
    async fn close(&self) -> ZResult<()>;
}

impl LinkUnicast {
    pub async fn write_transport_message(&self, msg: &mut TransportMessage) -> ZResult<usize> {
        // Create the buffer for serializing the message
        let mut wbuf = WBuf::new(WBUF_SIZE, false);
        if self.is_streamed() {
            // Reserve 16 bits to write the length
            wbuf.write_bytes(&[0_u8, 0_u8]);
        }
        // Serialize the message
        wbuf.write_transport_message(msg);
        if self.is_streamed() {
            // Write the length on the first 16 bits
            let length: u16 = wbuf.len() as u16 - 2;
            let bits = wbuf.get_first_slice_mut(..2);
            bits.copy_from_slice(&length.to_le_bytes());
        }
        let mut buffer = vec![0_u8; wbuf.len()];
        wbuf.copy_into_slice(&mut buffer[..]);

        // Send the message on the link
        let _ = self.0.write_all(&buffer).await?;

        Ok(buffer.len())
    }

    pub async fn read_transport_message(&self) -> ZResult<Vec<TransportMessage>> {
        // Read from the link
        let buffer = if self.is_streamed() {
            // Read and decode the message length
            let mut length_bytes = [0_u8; 2];
            let _ = self.read_exact(&mut length_bytes).await?;
            let to_read = u16::from_le_bytes(length_bytes) as usize;
            // Read the message
            let mut buffer = vec![0_u8; to_read];
            let _ = self.read_exact(&mut buffer).await?;
            buffer
        } else {
            // Read the message
            let mut buffer = vec![0_u8; self.get_mtu() as usize];
            let n = self.read(&mut buffer).await?;
            buffer.truncate(n);
            buffer
        };

        let mut zbuf = ZBuf::from(buffer);
        let mut messages: Vec<TransportMessage> = Vec::with_capacity(1);
        while zbuf.can_read() {
            match zbuf.read_transport_message() {
                Some(msg) => messages.push(msg),
                None => {
                    bail!("Invalid Message: Decoding error on link: {}", self);
                }
            }
        }

        Ok(messages)
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

/*************************************/
/*            MULTICAST              */
/*************************************/

#[async_trait]
pub trait LinkManagerMulticastTrait: Send + Sync {
    async fn new_link(&self, endpoint: &endpoint) -> ZResult<LinkMulticast>;
}

pub type LinkManagerMulticast = Arc<dyn LinkManagerMulticastTrait>;

#[derive(Clone)]
pub struct LinkMulticast(pub Arc<dyn LinkMulticastTrait>);

#[async_trait]
pub trait LinkMulticastTrait: Send + Sync {
    fn get_mtu(&self) -> u16;
    fn get_src(&self) -> &locator;
    fn get_dst(&self) -> &locator;
    fn is_reliable(&self) -> bool;
    async fn write(&self, buffer: &[u8]) -> ZResult<usize>;
    async fn write_all(&self, buffer: &[u8]) -> ZResult<()>;
    async fn read<'a>(&'a self, buffer: &mut [u8]) -> ZResult<(usize, Cow<'a, locator>)>;
    async fn close(&self) -> ZResult<()>;
}

impl LinkMulticast {
    pub async fn write_transport_message(&self, msg: &mut TransportMessage) -> ZResult<usize> {
        // Create the buffer for serializing the message
        let mut wbuf = WBuf::new(WBUF_SIZE, false);
        wbuf.write_transport_message(msg);
        let mut buffer = vec![0_u8; wbuf.len()];
        wbuf.copy_into_slice(&mut buffer[..]);

        // Send the message on the link
        let _ = self.0.write_all(&buffer).await;

        Ok(buffer.len())
    }

    // pub async fn read_transport_message(&self) -> ZResult<(Vec<TransportMessage>, Locator)> {
    //     // Read the message
    //     let mut buffer = vec![0_u8; self.get_mtu()];
    //     let (n, locator) = self.read(&mut buffer).await?;
    //     buffer.truncate(n);

    //     let mut zbuf = ZBuf::from(buffer);
    //     let mut messages: Vec<TransportMessage> = Vec::with_capacity(1);
    //     while zbuf.can_read() {
    //         match zbuf.read_transport_message() {
    //             Some(msg) => messages.push(msg),
    //             None => {
    //                 let e = format!("Decoding error on link: {}", self);
    //                 return zerror!(ZErrorKind::InvalidMessage { descr: e });
    //             }
    //         }
    //     }

    //     Ok((messages, locator))
    // }
}

impl Deref for LinkMulticast {
    type Target = Arc<dyn LinkMulticastTrait>;
    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Eq for LinkMulticast {}

impl PartialEq for LinkMulticast {
    fn eq(&self, other: &Self) -> bool {
        self.get_src() == other.get_src() && self.get_dst() == other.get_dst()
    }
}

impl Hash for LinkMulticast {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.get_src().hash(state);
        self.get_dst().hash(state);
    }
}

impl fmt::Display for LinkMulticast {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} => {}", self.get_src(), self.get_dst())
    }
}

impl fmt::Debug for LinkMulticast {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Link")
            .field("src", &self.get_src())
            .field("dst", &self.get_dst())
            .field("mtu", &self.get_mtu())
            .field("is_reliable", &self.is_reliable())
            .finish()
    }
}

impl From<Arc<dyn LinkMulticastTrait>> for LinkMulticast {
    fn from(link: Arc<dyn LinkMulticastTrait>) -> LinkMulticast {
        LinkMulticast(link)
    }
}
