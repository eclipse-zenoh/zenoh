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

//! ⚠️ WARNING ⚠️
//!
//! This crate is intended for Zenoh's internal use.
//!
//! [Click here for Zenoh's documentation](../zenoh/index.html)
#![no_std]
extern crate alloc;

mod multicast;
mod unicast;

use alloc::{borrow::ToOwned, boxed::Box, string::String};
use async_trait::async_trait;
use core::{cmp::PartialEq, fmt, hash::Hash};
pub use multicast::*;
use serde::Serialize;
pub use unicast::*;
use zenoh_protocol::core::Locator;
use zenoh_result::ZResult;

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
