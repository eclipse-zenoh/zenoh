//
// Copyright (c) 2024 ZettaScale Technology
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

use std::future::{IntoFuture, Ready};

use zenoh_config::wrappers::ZenohId;
use zenoh_core::{Resolvable, Wait};
use zenoh_protocol::core::WhatAmI;

use crate::net::runtime::DynamicRuntime;

#[cfg(feature = "unstable")]
use crate::api::info::{Link, Transport};

/// A builder returned by [`SessionInfo::zid()`](crate::session::SessionInfo::zid) that allows
/// access to the [`ZenohId`] of the current zenoh [`Session`](crate::Session).
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let zid = session.info().zid().await;
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
pub struct ZenohIdBuilder<'a> {
    runtime: &'a DynamicRuntime,
}

impl<'a> ZenohIdBuilder<'a> {
    pub(crate) fn new(runtime: &'a DynamicRuntime) -> Self {
        Self { runtime }
    }
}

impl Resolvable for ZenohIdBuilder<'_> {
    type To = ZenohId;
}

impl Wait for ZenohIdBuilder<'_> {
    fn wait(self) -> Self::To {
        self.runtime.zid()
    }
}

impl IntoFuture for ZenohIdBuilder<'_> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

/// A builder returned by [`SessionInfo::routers_zid()`](crate::session::SessionInfo::routers_zid) that allows
/// access to the [`ZenohId`] of the zenoh routers this process is currently connected to,
/// or the [`ZenohId`] of the current router if this code is run from a router (plugin).
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
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
pub struct RoutersZenohIdBuilder<'a> {
    runtime: &'a DynamicRuntime,
}

impl<'a> RoutersZenohIdBuilder<'a> {
    pub(crate) fn new(runtime: &'a DynamicRuntime) -> Self {
        Self { runtime }
    }
}

impl Resolvable for RoutersZenohIdBuilder<'_> {
    type To = Box<dyn Iterator<Item = ZenohId> + Send + Sync>;
}

impl Wait for RoutersZenohIdBuilder<'_> {
    fn wait(self) -> Self::To {
        self.runtime.get_zids(WhatAmI::Router)
    }
}

impl IntoFuture for RoutersZenohIdBuilder<'_> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

/// A builder returned by [`SessionInfo::peers_zid()`](crate::session::SessionInfo::peers_zid) that allows
/// access to the [`ZenohId`] of the zenoh peers this process is currently connected to.
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
///
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let zid = session.info().zid().await;
/// let mut peers_zid = session.info().peers_zid().await;
/// while let Some(peer_zid) = peers_zid.next() {}
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
pub struct PeersZenohIdBuilder<'a> {
    runtime: &'a DynamicRuntime,
}

impl<'a> PeersZenohIdBuilder<'a> {
    pub(crate) fn new(runtime: &'a DynamicRuntime) -> Self {
        Self { runtime }
    }
}

impl Resolvable for PeersZenohIdBuilder<'_> {
    type To = Box<dyn Iterator<Item = ZenohId> + Send + Sync>;
}

impl Wait for PeersZenohIdBuilder<'_> {
    fn wait(self) -> <Self as Resolvable>::To {
        self.runtime.get_zids(WhatAmI::Peer)
    }
}

impl IntoFuture for PeersZenohIdBuilder<'_> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

/// A builder returned by [`SessionInfo::transports()`](crate::session::SessionInfo::transports) that allows
/// access to information about transports this session is connected to.
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
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
#[zenoh_macros::unstable]
pub struct TransportsBuilder<'a> {
    runtime: &'a DynamicRuntime,
}

#[zenoh_macros::unstable]
impl<'a> TransportsBuilder<'a> {
    pub(crate) fn new(runtime: &'a DynamicRuntime) -> Self {
        Self { runtime }
    }
}

#[zenoh_macros::unstable]
impl Resolvable for TransportsBuilder<'_> {
    type To = Box<dyn Iterator<Item = Transport> + Send + Sync>;
}

#[zenoh_macros::unstable]
impl Wait for TransportsBuilder<'_> {
    fn wait(self) -> Self::To {
        self.runtime.get_transports()
    }
}

#[zenoh_macros::unstable]
impl IntoFuture for TransportsBuilder<'_> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

/// A builder returned by [`SessionInfo::links()`](crate::session::SessionInfo::links) that allows
/// access to information about links across all transports.
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
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
#[zenoh_macros::unstable]
pub struct LinksBuilder<'a> {
    runtime: &'a DynamicRuntime,
}

#[zenoh_macros::unstable]
impl<'a> LinksBuilder<'a> {
    pub(crate) fn new(runtime: &'a DynamicRuntime) -> Self {
        Self { runtime }
    }
}

#[zenoh_macros::unstable]
impl Resolvable for LinksBuilder<'_> {
    type To = Box<dyn Iterator<Item = Link> + Send + Sync>;
}

#[zenoh_macros::unstable]
impl Wait for LinksBuilder<'_> {
    fn wait(self) -> Self::To {
        self.runtime.get_links()
    }
}

#[zenoh_macros::unstable]
impl IntoFuture for LinksBuilder<'_> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}
