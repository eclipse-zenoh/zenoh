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

//! Tools to access information about the current zenoh [`Session`](crate::Session).

#[cfg(feature = "unstable")]
use zenoh_core::{Resolve, ResolveClosure};
#[cfg(feature = "unstable")]
use zenoh_protocol::core::Locator;

use crate::{
    api::builders::info::{PeersZenohIdBuilder, RoutersZenohIdBuilder, LinksBuilder, TransportsBuilder, ZenohIdBuilder},
    net::runtime::DynamicRuntime,
};

#[cfg(feature = "unstable")]
use zenoh_config::{wrappers::ZenohId, WhatAmI};
/// Struct returned by [`Session::info()`](crate::Session::info) that allows
/// access to information about the current zenoh [`Session`](crate::Session).
///
/// This information includes the [`ZenohId`](crate::session::ZenohId) identifier of the current session,
/// and the identifiers of the connected routers and peers (see also [`WhatAmI`](crate::config::WhatAmI)
/// for more information about peers and routers).
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let info = session.info();
/// let zid = info.zid().await;
/// # }
/// ```
pub struct SessionInfo {
    pub(crate) runtime: DynamicRuntime,
}

impl SessionInfo {
    /// Return the [`ZenohId`](crate::session::ZenohId) of the current zenoh [`Session`](crate::Session).
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let zid = session.info().zid().await;
    /// # }
    /// ```
    pub fn zid(&self) -> ZenohIdBuilder<'_> {
        ZenohIdBuilder::new(&self.runtime)
    }

    /// Return the [`ZenohId`](crate::session::ZenohId) of the zenoh routers this process is currently connected to,
    /// or the [`ZenohId`](crate::session::ZenohId) of the current router if this code is run from a router (plugin).
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let mut routers_zid = session.info().routers_zid().await;
    /// while let Some(router_zid) = routers_zid.next() {}
    /// # }
    /// ```
    pub fn routers_zid(&self) -> RoutersZenohIdBuilder<'_> {
        RoutersZenohIdBuilder::new(&self.runtime)
    }

    /// Return the [`ZenohId`](crate::session::ZenohId) of the zenoh peers this process is currently connected to.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let mut peers_zid = session.info().peers_zid().await;
    /// while let Some(peer_zid) = peers_zid.next() {}
    /// # }
    /// ```
    pub fn peers_zid(&self) -> PeersZenohIdBuilder<'_> {
        PeersZenohIdBuilder::new(&self.runtime)
    }

    /// Return the locators on which the current zenoh [`Session`](crate::Session) is listening to.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// println!("{:?}", session.info().locators().await);
    /// // print ["tcp/127.0.0.1:7447"]
    /// # }
    /// ```
    #[zenoh_macros::unstable]
    pub fn locators(&self) -> impl Resolve<Vec<Locator>> + '_ {
        ResolveClosure::new(|| self.runtime.get_locators())
    }

    /// Return information about transports this session is connected to.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let mut transports = session.info().transports().await;
    /// while let Some(transport) = transports.next() {
    ///     println!("Transport: zid={}, whatami={:?}", transport.zid, transport.whatami);
    /// }
    /// # }
    /// ```
    #[zenoh_macros::unstable]
    pub fn transports(&self) -> TransportsBuilder<'_> {
        TransportsBuilder::new(&self.runtime)
    }

    /// Return information about links across all transports.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let mut links = session.info().links().await;
    /// while let Some(link) = links.next() {
    ///     println!("Link: {} -> {}", link.src, link.dst);
    /// }
    /// # }
    /// ```
    #[zenoh_macros::unstable]
    pub fn links(&self) -> LinksBuilder<'_> {
        LinksBuilder::new(&self.runtime)
    }
}

/// Represents a transport connection to a remote zenoh node.
#[zenoh_macros::unstable]
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "unstable", derive(serde::Serialize))]
pub struct Transport {
    /// The ZenohId of the remote zenoh node
    pub zid: ZenohId,
    /// The type of the remote zenoh node (Router, Peer, or Client)
    pub whatami: WhatAmI,
}

/// Represents a physical link within a transport.
#[zenoh_macros::unstable]
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "unstable", derive(serde::Serialize))]
pub struct Link {
    /// Source locator (local endpoint)
    pub src: Locator,
    /// Destination locator (remote endpoint)
    pub dst: Locator,
}
