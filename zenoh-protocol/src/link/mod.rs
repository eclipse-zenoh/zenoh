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
#[cfg(feature = "transport_tcp")]
mod tcp;
// #[cfg(feature = "transport_udp")]
// mod udp;
// #[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
// mod unixsock_stream;

/* General imports */
use async_std::sync::Arc;
use async_trait::async_trait;
use std::cmp::PartialEq;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use zenoh_util::core::ZResult;

/*************************************/
/*              LINK                 */
/*************************************/
#[async_trait]
pub trait LinkTrait {
    fn get_mtu(&self) -> usize;
    fn get_src(&self) -> Locator;
    fn get_dst(&self) -> Locator;
    fn is_reliable(&self) -> bool;
    fn is_streamed(&self) -> bool;

    async fn write(&self, buffer: &[u8]) -> ZResult<usize>;
    async fn read(&self, buffer: &mut [u8]) -> ZResult<usize>;
    async fn read_exact(&self, buffer: &mut [u8]) -> ZResult<()>;
    async fn close(&self) -> ZResult<()>;
}

#[derive(Clone)]
pub struct Link(Arc<dyn LinkTrait + Send + Sync>);

impl Link {
    fn new(link: Arc<dyn LinkTrait + Send + Sync>) -> Link {
        Self(link)
    }
}

impl Deref for Link {
    type Target = Arc<dyn LinkTrait + Send + Sync>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Eq for Link {}

impl PartialEq for Link {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Hash for Link {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.get_src().hash(state);
        self.0.get_dst().hash(state);
    }
}

impl fmt::Display for Link {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} => {}", self.0.get_src(), self.0.get_dst())
    }
}

impl fmt::Debug for Link {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Link")
            .field("src", &self.0.get_src())
            .field("dst", &self.0.get_dst())
            .field("mtu", &self.0.get_mtu())
            .field("is_reliable", &self.0.is_reliable())
            .field("is_streamed", &self.0.is_streamed())
            .finish()
    }
}

/*************************************/
/*           LINK MANAGER            */
/*************************************/
#[async_trait]
pub trait LinkManagerTrait {
    async fn new_link(&self, dst: &Locator) -> ZResult<Link>;
    async fn new_listener(&self, locator: &Locator) -> ZResult<Locator>;
    async fn del_listener(&self, locator: &Locator) -> ZResult<()>;
    async fn get_listeners(&self) -> Vec<Locator>;
    async fn get_locators(&self) -> Vec<Locator>;
}

pub type LinkManager = Arc<dyn LinkManagerTrait + Send + Sync>;
