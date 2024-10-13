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

use crate::net::runtime::Runtime;

/// A builder returned by [`SessionInfo::zid()`](SessionInfo::zid) that allows
/// to access the [`ZenohId`] of the current zenoh [`Session`](crate::Session).
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
///
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let zid = session.info().zid().await;
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
pub struct ZenohIdBuilder<'a> {
    runtime: &'a Runtime,
}

impl<'a> ZenohIdBuilder<'a> {
    pub(crate) fn new(runtime: &'a Runtime) -> Self {
        Self { runtime }
    }
}

impl<'a> Resolvable for ZenohIdBuilder<'a> {
    type To = ZenohId;
}

impl<'a> Wait for ZenohIdBuilder<'a> {
    fn wait(self) -> Self::To {
        self.runtime.zid()
    }
}

impl<'a> IntoFuture for ZenohIdBuilder<'a> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

/// A builder returned by [`SessionInfo::routers_zid()`](SessionInfo::routers_zid) that allows
/// to access the [`ZenohId`] of the zenoh routers this process is currently connected to
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
    runtime: &'a Runtime,
}

impl<'a> RoutersZenohIdBuilder<'a> {
    pub(crate) fn new(runtime: &'a Runtime) -> Self {
        Self { runtime }
    }
}

impl<'a> Resolvable for RoutersZenohIdBuilder<'a> {
    type To = Box<dyn Iterator<Item = ZenohId> + Send + Sync>;
}

impl<'a> Wait for RoutersZenohIdBuilder<'a> {
    fn wait(self) -> Self::To {
        Box::new(
            zenoh_runtime::ZRuntime::Application
                .block_in_place(self.runtime.manager().get_transports_unicast())
                .into_iter()
                .filter_map(|s| {
                    s.get_whatami()
                        .ok()
                        .and_then(|what| (what == WhatAmI::Router).then_some(()))
                        .and_then(|_| s.get_zid().map(Into::into).ok())
                }),
        )
    }
}

impl<'a> IntoFuture for RoutersZenohIdBuilder<'a> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

/// A builder returned by [`SessionInfo::peers_zid()`](SessionInfo::peers_zid) that allows
/// to access the [`ZenohId`] of the zenoh peers this process is currently connected to.
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
    runtime: &'a Runtime,
}

impl<'a> PeersZenohIdBuilder<'a> {
    pub(crate) fn new(runtime: &'a Runtime) -> Self {
        Self { runtime }
    }
}

impl<'a> Resolvable for PeersZenohIdBuilder<'a> {
    type To = Box<dyn Iterator<Item = ZenohId> + Send + Sync>;
}

impl<'a> Wait for PeersZenohIdBuilder<'a> {
    fn wait(self) -> <Self as Resolvable>::To {
        Box::new(
            zenoh_runtime::ZRuntime::Application
                .block_in_place(self.runtime.manager().get_transports_unicast())
                .into_iter()
                .filter_map(|s| {
                    s.get_whatami()
                        .ok()
                        .and_then(|what| (what == WhatAmI::Peer).then_some(()))
                        .and_then(|_| s.get_zid().map(Into::into).ok())
                }),
        )
    }
}

impl<'a> IntoFuture for PeersZenohIdBuilder<'a> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}
