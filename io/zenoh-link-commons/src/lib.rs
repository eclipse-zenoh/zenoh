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

//! ⚠️ WARNING ⚠️
//!
//! This crate is intended for Zenoh's internal use.
//!
//! [Click here for Zenoh's documentation](../zenoh/index.html)
#![no_std]
extern crate alloc;

// mod multicast;
mod unicast;

use alloc::{
    borrow::{Cow, ToOwned},
    boxed::Box,
    string::String,
    sync::Arc,
    vec::Vec,
};
use async_trait::async_trait;
use core::{
    cmp::PartialEq,
    fmt,
    hash::{Hash, Hasher},
    ops::Deref,
};
use serde::Serialize;
pub use unicast::*;
use zenoh_buffers::{
    reader::{HasReader, Reader},
    writer::HasWriter,
};
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_protocol::{
    core::{EndPoint, Locator},
    transport::TransportMessage,
};
use zenoh_result::{zerror, ZResult};

/*************************************/
/*            GENERAL                */
/*************************************/
#[derive(Clone, Debug, Serialize, Hash, PartialEq, Eq)]
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
    fn protocol(&self) -> &str;
    async fn is_multicast(&self, locator: &Locator) -> ZResult<bool>;
}

#[async_trait]
pub trait ConfigurationInspector<C>: Default {
    async fn inspect_config(&self, configuration: &C) -> ZResult<String>;
}

impl fmt::Display for Link {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} => {}", &self.src, &self.dst)
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
/*            MULTICAST              */
/*************************************/
#[async_trait]
pub trait LinkManagerMulticastTrait: Send + Sync {
    async fn new_link(&self, endpoint: &EndPoint) -> ZResult<LinkMulticast>;
}

pub type LinkManagerMulticast = Arc<dyn LinkManagerMulticastTrait>;

#[derive(Clone)]
pub struct LinkMulticast(pub Arc<dyn LinkMulticastTrait>);

#[async_trait]
pub trait LinkMulticastTrait: Send + Sync {
    fn get_mtu(&self) -> u16;
    fn get_src(&self) -> &Locator;
    fn get_dst(&self) -> &Locator;
    fn is_reliable(&self) -> bool;
    async fn write(&self, buffer: &[u8]) -> ZResult<usize>;
    async fn write_all(&self, buffer: &[u8]) -> ZResult<()>;
    async fn read<'a>(&'a self, buffer: &mut [u8]) -> ZResult<(usize, Cow<'a, Locator>)>;
    async fn close(&self) -> ZResult<()>;
}

impl LinkMulticast {
    pub async fn write_transport_message(&self, msg: &TransportMessage) -> ZResult<usize> {
        // Create the buffer for serializing the message
        let mut buff = Vec::new();
        let mut writer = buff.writer();
        let codec = Zenoh080::new();
        codec
            .write(&mut writer, msg)
            .map_err(|_| zerror!("Encoding error on link: {}", self))?;

        // Send the message on the link
        self.0.write_all(buff.as_slice()).await?;

        Ok(buff.len())
    }

    pub async fn read_transport_message(&self) -> ZResult<(Vec<TransportMessage>, Locator)> {
        // Read the message
        let mut buffer = zenoh_buffers::vec::uninit(self.get_mtu() as usize);
        let (n, locator) = self.read(&mut buffer).await?;
        buffer.truncate(n);

        let mut reader = buffer.reader();
        let codec = Zenoh080::new();

        let mut messages: Vec<TransportMessage> = Vec::with_capacity(1);
        while reader.can_read() {
            let msg: TransportMessage = codec
                .read(&mut reader)
                .map_err(|_| zerror!("Invalid Message: Decoding error on link: {}", self))?;
            messages.push(msg);
        }

        Ok((messages, locator.into_owned()))
    }
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
