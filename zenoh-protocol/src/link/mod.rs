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

/* General imports */
use async_std::sync::Arc;
use async_trait::async_trait;

use std::cmp::PartialEq;
use std::fmt;
use std::hash::{Hash, Hasher};

use crate::session::Transport;
use zenoh_util::core::ZResult;


/*************************************/
/*              LINK                 */
/*************************************/
pub type Link = Arc<dyn LinkTrait + Send + Sync>;

#[async_trait]
pub trait LinkTrait {
    async fn close(&self) -> ZResult<()>;

    fn get_mtu(&self) -> usize;

    fn get_src(&self) -> Locator;

    fn get_dst(&self) -> Locator;

    fn is_ordered(&self) -> bool;

    fn is_reliable(&self) -> bool;

    fn is_streamed(&self) -> bool;

    async fn send(&self, buffer: &[u8]) -> ZResult<()>;

    async fn start(&self) -> ZResult<()>;

    async fn stop(&self) -> ZResult<()>;
}

impl Eq for dyn LinkTrait + Send + Sync {}

impl PartialEq for dyn LinkTrait + Send + Sync {
    fn eq(&self, other: &Self) -> bool {
        self.get_src() == other.get_src() && self.get_dst() == other.get_dst()
    }
}

impl Hash for dyn LinkTrait + Send + Sync {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.get_src().hash(state);
        self.get_dst().hash(state);
    }
}

impl fmt::Display for dyn LinkTrait + Send + Sync {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} => {}", self.get_src(), self.get_dst())?;
        Ok(())
    }
}

impl fmt::Debug for dyn LinkTrait + Send + Sync {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Link")
            .field("src", &self.get_src())
            .field("dst", &self.get_dst())
            .field("mtu", &self.get_mtu())
            .field("is_ordered", &self.is_ordered())
            .field("is_reliable", &self.is_reliable())
            .field("is_streamed", &self.is_streamed())
            .finish()
    }
}

/*************************************/
/*           LINK MANAGER            */
/*************************************/
pub type LinkManager = Arc<dyn ManagerTrait + Send + Sync>;

#[async_trait]
pub trait ManagerTrait {
    async fn new_link(&self, dst: &Locator, transport: &Transport) -> ZResult<Link>;

    async fn del_link(&self, src: &Locator, dst: &Locator) -> ZResult<Link>;

    async fn get_link(&self, src: &Locator, dst: &Locator) -> ZResult<Link>;

    async fn new_listener(&self, locator: &Locator) -> ZResult<()>;

    async fn del_listener(&self, locator: &Locator) -> ZResult<()>;

    async fn get_listeners(&self) -> Vec<Locator>;
}
