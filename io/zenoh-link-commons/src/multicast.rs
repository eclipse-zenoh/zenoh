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
use alloc::{borrow::Cow, boxed::Box, sync::Arc, vec::Vec};
use core::{
    fmt,
    hash::{Hash, Hasher},
    ops::Deref,
};

use async_trait::async_trait;
use zenoh_buffers::{reader::HasReader, writer::HasWriter};
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_protocol::{
    core::{EndPoint, Locator},
    transport::{BatchSize, TransportMessage},
};
use zenoh_result::{zerror, ZResult};

use crate::LinkAuthId;

/*************************************/
/*             MANAGER               */
/*************************************/
#[async_trait]
pub trait LinkManagerMulticastTrait: Send + Sync {
    async fn new_link(&self, endpoint: &EndPoint) -> ZResult<LinkMulticast>;
}

pub type LinkManagerMulticast = Arc<dyn LinkManagerMulticastTrait>;

/*************************************/
/*              LINK                 */
/*************************************/
#[derive(Clone)]
pub struct LinkMulticast(pub Arc<dyn LinkMulticastTrait>);

#[async_trait]
pub trait LinkMulticastTrait: Send + Sync {
    fn get_mtu(&self) -> BatchSize;
    fn get_src(&self) -> &Locator;
    fn get_dst(&self) -> &Locator;
    fn get_auth_id(&self) -> &LinkAuthId;
    fn is_reliable(&self) -> bool;
    async fn write(&self, buffer: &[u8]) -> ZResult<usize>;
    async fn write_all(&self, buffer: &[u8]) -> ZResult<()>;
    async fn read<'a>(&'a self, buffer: &mut [u8]) -> ZResult<(usize, Cow<'a, Locator>)>;
    async fn close(&self) -> ZResult<()>;
}

impl LinkMulticast {
    pub async fn send(&self, msg: &TransportMessage) -> ZResult<usize> {
        // Create the buffer for serializing the message
        let mut buff = Vec::new();

        let codec = Zenoh080::new();
        let mut writer = buff.writer();

        codec
            .write(&mut writer, msg)
            .map_err(|_| zerror!("Encoding error on link: {}", self))?;

        // Send the message on the link
        self.0.write_all(buff.as_slice()).await?;

        Ok(buff.len())
    }

    pub async fn recv(&self) -> ZResult<(TransportMessage, Locator)> {
        // Read the message
        let mut buffer = zenoh_buffers::vec::uninit(self.get_mtu() as usize);
        let (n, locator) = self.read(&mut buffer).await?;
        buffer.truncate(n);

        let codec = Zenoh080::new();
        let mut reader = buffer.reader();

        let msg: TransportMessage = codec
            .read(&mut reader)
            .map_err(|_| zerror!("Invalid Message: Decoding error on link: {}", self))?;

        Ok((msg, locator.into_owned()))
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
