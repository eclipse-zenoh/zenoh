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
use zenoh_config::{wrappers::ZenohId, WhatAmI};
#[cfg(feature = "unstable")]
use zenoh_core::{Resolve, ResolveClosure};
#[cfg(feature = "unstable")]
use zenoh_protocol::core::Locator;

#[cfg(feature = "unstable")]
use crate::api::{
    builders::info::{LinkEventsBuilder, TransportEventsBuilder},
    handlers::{CallbackParameter, DefaultHandler},
    sample::SampleKind,
};
use crate::{
    api::builders::info::{
        LinksBuilder, PeersZenohIdBuilder, RoutersZenohIdBuilder, TransportsBuilder, ZenohIdBuilder,
    },
    net::runtime::DynamicRuntime,
};
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
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let transports = session.info().transports().await;
    /// for transport in transports {
    ///     println!("Transport: zid={}, whatami={:?}", transport.zid(), transport.whatami());
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
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let links = session.info().links().await;
    /// for link in links {
    ///     println!("Link: {} -> {}", link.src(), link.dst());
    /// }
    /// # }
    /// ```
    #[zenoh_macros::unstable]
    pub fn links(&self) -> LinksBuilder<'_> {
        LinksBuilder::new(&self.runtime)
    }

    /// Subscribe to transport lifecycle events.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::sample::SampleKind;
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let events = session.info()
    ///     .transport_events()
    ///     .history(true)
    ///     .with(flume::bounded(32))
    ///     .await;
    ///
    /// while let Ok(event) = events.recv_async().await {
    ///     match event.kind() {
    ///         SampleKind::Put => println!("Transport opened: {}", event.transport().zid()),
    ///         SampleKind::Delete => println!("Transport closed"),
    ///     }
    /// }
    /// # }
    /// ```
    #[zenoh_macros::unstable]
    pub fn transport_events(&self) -> TransportEventsBuilder<'_, DefaultHandler> {
        TransportEventsBuilder::new(&self.runtime)
    }

    /// Subscribe to link lifecycle events.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::sample::SampleKind;
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let events = session.info()
    ///     .link_events()
    ///     .history(true)
    ///     .with(flume::bounded(32))
    ///     .await;
    ///
    /// while let Ok(event) = events.recv_async().await {
    ///     match event.kind() {
    ///         SampleKind::Put => println!("Link added: {} -> {}",
    ///             event.link().src(), event.link().dst()),
    ///         SampleKind::Delete => println!("Link removed"),
    ///     }
    /// }
    /// # }
    /// ```
    #[zenoh_macros::unstable]
    pub fn link_events(&self) -> LinkEventsBuilder<'_, DefaultHandler> {
        LinkEventsBuilder::new(&self.runtime)
    }
}

/// Represents a transport connection to a remote zenoh node.
#[zenoh_macros::unstable]
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "unstable", derive(serde::Serialize))]
pub struct Transport {
    pub(crate) zid: ZenohId,
    pub(crate) whatami: WhatAmI,
}

#[zenoh_macros::unstable]
impl Transport {
    /// Gets the ZenohId of the remote zenoh node.
    #[inline]
    pub fn zid(&self) -> &ZenohId {
        &self.zid
    }

    /// Gets the type of the remote zenoh node (Router, Peer, or Client).
    #[inline]
    pub fn whatami(&self) -> WhatAmI {
        self.whatami
    }
}

/// Represents a physical link within a transport.
#[zenoh_macros::unstable]
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "unstable", derive(serde::Serialize))]
pub struct Link {
    pub(crate) src: Locator,
    pub(crate) dst: Locator,
}

#[zenoh_macros::unstable]
impl Link {
    /// Gets the source locator (local endpoint).
    #[inline]
    pub fn src(&self) -> &Locator {
        &self.src
    }

    /// Gets the destination locator (remote endpoint).
    #[inline]
    pub fn dst(&self) -> &Locator {
        &self.dst
    }
}

/// Event emitted when a transport is opened or closed
#[zenoh_macros::unstable]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportEvent {
    pub(crate) kind: SampleKind, // Put = opened, Delete = closed
    pub(crate) transport: Transport,
}

#[zenoh_macros::unstable]
impl TransportEvent {
    /// Returns the kind of event (Put for opened, Delete for closed)
    pub fn kind(&self) -> SampleKind {
        self.kind
    }

    /// Returns a reference to the transport
    pub fn transport(&self) -> &Transport {
        &self.transport
    }

    /// Returns true if this is an "opened" event
    pub fn is_open(&self) -> bool {
        self.kind == SampleKind::Put
    }

    /// Returns true if this is a "closed" event
    pub fn is_closed(&self) -> bool {
        self.kind == SampleKind::Delete
    }
}

#[zenoh_macros::unstable]
impl CallbackParameter for TransportEvent {
    type Message<'a> = Self;
    fn from_message(msg: Self::Message<'_>) -> Self {
        msg
    }
}

/// Event emitted when a link is added or removed
#[zenoh_macros::unstable]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkEvent {
    pub(crate) kind: SampleKind, // Put = added, Delete = removed
    pub(crate) link: Link,
    pub(crate) transport_zid: ZenohId,
}

#[zenoh_macros::unstable]
impl LinkEvent {
    /// Returns the kind of event (Put for added, Delete for removed)
    pub fn kind(&self) -> SampleKind {
        self.kind
    }

    /// Returns a reference to the link
    pub fn link(&self) -> &Link {
        &self.link
    }

    /// Returns the ZenohId of the transport this link belongs to
    pub fn transport_zid(&self) -> &ZenohId {
        &self.transport_zid
    }

    /// Returns true if this is an "added" event
    pub fn is_added(&self) -> bool {
        self.kind == SampleKind::Put
    }

    /// Returns true if this is a "removed" event
    pub fn is_removed(&self) -> bool {
        self.kind == SampleKind::Delete
    }
}

#[zenoh_macros::unstable]
impl CallbackParameter for LinkEvent {
    type Message<'a> = Self;
    fn from_message(msg: Self::Message<'_>) -> Self {
        msg
    }
}
