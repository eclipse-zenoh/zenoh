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
//! [Click here for Zenoh's documentation](https://docs.rs/zenoh/latest/zenoh)
extern crate alloc;

mod listener;
mod multicast;
#[cfg(feature = "tls")]
pub mod tls;
mod unicast;

use alloc::{borrow::ToOwned, boxed::Box, string::String, vec, vec::Vec};
use core::{cmp::PartialEq, fmt, hash::Hash};

use async_trait::async_trait;
pub use listener::*;
pub use multicast::*;
use serde::Serialize;
pub use unicast::*;
use zenoh_protocol::{core::Locator, transport::BatchSize};
use zenoh_result::ZResult;

/*************************************/
/*            GENERAL                */
/*************************************/

pub const BIND_INTERFACE: &str = "iface";

#[derive(Clone, Debug, Serialize, Hash, PartialEq, Eq)]
pub struct Link {
    pub src: Locator,
    pub dst: Locator,
    pub group: Option<Locator>,
    pub mtu: BatchSize,
    pub is_reliable: bool,
    pub is_streamed: bool,
    pub interfaces: Vec<String>,
    pub auth_identifier: LinkAuthId,
}

#[async_trait]
pub trait LocatorInspector: Default {
    fn protocol(&self) -> &str;
    async fn is_multicast(&self, locator: &Locator) -> ZResult<bool>;
}

pub trait ConfigurationInspector<C>: Default {
    fn inspect_config(&self, configuration: &C) -> ZResult<String>;
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
            interfaces: link.get_interface_names(),
            auth_identifier: link.get_auth_id().clone(),
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
            interfaces: vec![],
            auth_identifier: LinkAuthId::default(),
        }
    }
}

impl From<LinkMulticast> for Link {
    fn from(link: LinkMulticast) -> Link {
        Link::from(&link)
    }
}

impl PartialEq<LinkUnicast> for Link {
    fn eq(&self, other: &LinkUnicast) -> bool {
        self.src == *other.get_src() && self.dst == *other.get_dst()
    }
}

impl PartialEq<LinkMulticast> for Link {
    fn eq(&self, other: &LinkMulticast) -> bool {
        self.src == *other.get_src() && self.dst == *other.get_dst()
    }
}
