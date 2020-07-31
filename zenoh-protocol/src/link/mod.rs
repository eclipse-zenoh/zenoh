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
mod locator;
pub use locator::*;

mod manager;
pub use manager::*;

/* Import of Link modules */
mod tcp;
mod udp;

/* General imports */
use async_std::sync::{Arc, Weak};
use async_trait::async_trait;

use std::cmp::PartialEq;
use std::fmt;
use std::hash::{Hash, Hasher};

use crate::session::Transport;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::zweak;

/*************************************/
/*              LINK                 */
/*************************************/
const STR_ERR: &str = "Link not available";

#[derive(Clone)]
pub struct Link {
    mtu: usize,
    src: Locator,
    dst: Locator,
    is_reliable: bool,
    is_streamed: bool,
    inner: Weak<dyn LinkTrait + Send + Sync>,
}

impl Link {
    pub fn new(link: Arc<dyn LinkTrait + Send + Sync>) -> Link {
        Link {
            mtu: link.get_mtu(),
            src: link.get_src(),
            dst: link.get_dst(),
            is_reliable: link.is_reliable(),
            is_streamed: link.is_streamed(),
            inner: Arc::downgrade(&link),
        }
    }

    #[inline]
    pub async fn close(&self) -> ZResult<()> {
        let link = zweak!(self.inner, STR_ERR);
        link.close().await
    }

    #[inline]
    pub fn get_mtu(&self) -> usize {
        self.mtu
    }

    #[inline]
    pub fn get_src(&self) -> Locator {
        self.src.clone()
    }

    #[inline]
    pub fn get_dst(&self) -> Locator {
        self.dst.clone()
    }

    #[inline]
    pub fn is_reliable(&self) -> bool {
        self.is_reliable
    }

    #[inline]
    pub fn is_streamed(&self) -> bool {
        self.is_streamed
    }

    #[inline]
    pub async fn send(&self, buffer: &[u8]) -> ZResult<()> {
        let link = zweak!(self.inner, STR_ERR);
        link.send(buffer).await
    }

    #[inline]
    pub async fn start(&self) -> ZResult<()> {
        let link = zweak!(self.inner, STR_ERR);
        link.start().await
    }

    #[inline]
    pub async fn stop(&self) -> ZResult<()> {
        let link = zweak!(self.inner, STR_ERR);
        link.stop().await
    }
}

impl Eq for Link {}

impl PartialEq for Link {
    fn eq(&self, other: &Self) -> bool {
        self.inner.ptr_eq(&other.inner)
    }
}

impl Hash for Link {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.src.hash(state);
        self.dst.hash(state);
    }
}

impl fmt::Display for Link {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} => {}", self.get_src(), self.get_dst())
    }
}

impl fmt::Debug for Link {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status = self.inner.upgrade().is_some();
        f.debug_struct("Link")
            .field("src", &self.get_src())
            .field("dst", &self.get_dst())
            .field("mtu", &self.get_mtu())
            .field("is_reliable", &self.is_reliable())
            .field("is_streamed", &self.is_streamed())
            .field("active", &status)
            .finish()
    }
}

#[async_trait]
pub trait LinkTrait {
    async fn close(&self) -> ZResult<()>;

    fn get_mtu(&self) -> usize;

    fn get_src(&self) -> Locator;

    fn get_dst(&self) -> Locator;

    fn is_reliable(&self) -> bool;

    fn is_streamed(&self) -> bool;

    async fn send(&self, buffer: &[u8]) -> ZResult<()>;

    async fn start(&self) -> ZResult<()>;

    async fn stop(&self) -> ZResult<()>;
}

/*************************************/
/*           LINK MANAGER            */
/*************************************/
pub type LinkManager = Arc<dyn ManagerTrait + Send + Sync>;

#[async_trait]
pub trait ManagerTrait {
    async fn new_link(&self, dst: &Locator, transport: &Transport) -> ZResult<Link>;

    async fn get_link(&self, src: &Locator, dst: &Locator) -> ZResult<Link>;

    async fn del_link(&self, src: &Locator, dst: &Locator) -> ZResult<()>;

    async fn new_listener(&self, locator: &Locator) -> ZResult<Locator>;

    async fn del_listener(&self, locator: &Locator) -> ZResult<()>;

    async fn get_listeners(&self) -> Vec<Locator>;
}
