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
use alloc::{boxed::Box, string::String, sync::Arc, vec::Vec};
use core::{
    fmt,
    hash::{Hash, Hasher},
    ops::Deref,
};
use std::net::SocketAddr;

use async_trait::async_trait;
use serde::Serialize;
use zenoh_protocol::{
    core::{EndPoint, Locator},
    transport::BatchSize,
};
use zenoh_result::ZResult;

pub type LinkManagerUnicast = Arc<dyn LinkManagerUnicastTrait>;
#[async_trait]
pub trait LinkManagerUnicastTrait: Send + Sync {
    async fn new_link(&self, endpoint: EndPoint) -> ZResult<LinkUnicast>;
    async fn new_listener(&self, endpoint: EndPoint) -> ZResult<Locator>;
    async fn del_listener(&self, endpoint: &EndPoint) -> ZResult<()>;
    async fn get_listeners(&self) -> Vec<EndPoint>;
    async fn get_locators(&self) -> Vec<Locator>;
}
pub type NewLinkChannelSender = flume::Sender<LinkUnicast>;

pub trait ConstructibleLinkManagerUnicast<T>: Sized {
    fn new(new_link_sender: NewLinkChannelSender, config: T) -> ZResult<Self>;
}

#[derive(Clone)]
pub struct LinkUnicast(pub Arc<dyn LinkUnicastTrait>);

#[async_trait]
pub trait LinkUnicastTrait: Send + Sync {
    fn get_mtu(&self) -> BatchSize;
    fn get_src(&self) -> &Locator;
    fn get_dst(&self) -> &Locator;
    fn is_reliable(&self) -> bool;
    fn is_streamed(&self) -> bool;
    fn get_interface_names(&self) -> Vec<String>;
    fn get_auth_id(&self) -> &LinkAuthId;
    async fn write(&self, buffer: &[u8]) -> ZResult<usize>;
    async fn write_all(&self, buffer: &[u8]) -> ZResult<()>;
    async fn read(&self, buffer: &mut [u8]) -> ZResult<usize>;
    async fn read_exact(&self, buffer: &mut [u8]) -> ZResult<()>;
    async fn close(&self) -> ZResult<()>;
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

pub fn get_ip_interface_names(addr: &SocketAddr) -> Vec<String> {
    match zenoh_util::net::get_interface_names_by_addr(addr.ip()) {
        Ok(interfaces) => {
            tracing::trace!("get_interface_names for {:?}: {:?}", addr.ip(), interfaces);
            interfaces
        }
        Err(e) => {
            tracing::debug!("get_interface_names for {:?} failed: {:?}", addr.ip(), e);
            vec![]
        }
    }
}

#[derive(Clone, Debug, Serialize, Hash, PartialEq, Eq)]
pub enum LinkAuthId {
    Tls(Option<String>),
    Quic(Option<String>),
    Tcp,
    Udp,
    Serial,
    Unixpipe,
    UnixsockStream,
    Vsock,
    Ws,
}

impl LinkAuthId {
    pub fn get_cert_common_name(&self) -> Option<&str> {
        match &self {
            LinkAuthId::Tls(n) => n.as_ref().map(|s| s.as_ref()),
            LinkAuthId::Quic(n) => n.as_ref().map(|s| s.as_ref()),
            LinkAuthId::Tcp => None,
            LinkAuthId::Udp => None,
            LinkAuthId::Serial => None,
            LinkAuthId::Unixpipe => None,
            LinkAuthId::UnixsockStream => None,
            LinkAuthId::Vsock => None,
            LinkAuthId::Ws => None,
        }
    }
}
