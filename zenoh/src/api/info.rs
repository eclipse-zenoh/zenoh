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

use zenoh_config::{wrappers::ZenohId, WhatAmI};
#[cfg(feature = "unstable")]
use zenoh_core::{Resolve, ResolveClosure};
use zenoh_link::LinkAuthId;
use zenoh_protocol::core::{Locator, Reliability};
use zenoh_transport::TransportPeer;

#[cfg(feature = "unstable")]
use crate::api::builders::info_links::{LinkEventsListenerBuilder, LinksBuilder};
#[cfg(feature = "unstable")]
use crate::api::builders::info_transport::{TransportEventsListenerBuilder, TransportsBuilder};
#[cfg(feature = "unstable")]
use crate::api::handlers::DefaultHandler;
use crate::api::{
    builders::info::{PeersZenohIdBuilder, RoutersZenohIdBuilder, ZenohIdBuilder},
    handlers::CallbackParameter,
    sample::SampleKind,
    session::WeakSession,
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
    pub(crate) session: WeakSession,
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
        ZenohIdBuilder::new(self.session.runtime())
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
        RoutersZenohIdBuilder::new(self.session.runtime())
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
        PeersZenohIdBuilder::new(self.session.runtime())
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
        ResolveClosure::new(|| self.session.runtime().get_locators())
    }

    /// Return information about currently opened transport sessions. Transport session is a connection to another zenoh node.
    ///
    /// # Examples
    /// ```no_run
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
        TransportsBuilder::new(self.session.runtime())
    }

    /// Return information about links across all transports.
    ///
    /// # Examples
    /// ```no_run
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
        LinksBuilder::new(&self.session)
    }

    /// Subscribe to transport lifecycle events.
    ///
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::sample::SampleKind;
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let events = session.info()
    ///     .transport_events_listener()
    ///     .history(true)
    ///     .with(flume::bounded(32))
    ///     .await
    ///     .expect("Failed to declare transport events listener");
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
    pub fn transport_events_listener(&self) -> TransportEventsListenerBuilder<'_, DefaultHandler> {
        TransportEventsListenerBuilder::new(&self.session)
    }

    /// Subscribe to link lifecycle events.
    ///
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::sample::SampleKind;
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let listener = session.info()
    ///     .link_events_listener()
    ///     .history(true)
    ///     .with(flume::bounded(32))
    ///     .await
    ///     .expect("Failed to declare link events listener");
    ///
    /// while let Ok(event) = listener.recv_async().await {
    ///     match event.kind() {
    ///         SampleKind::Put => println!("Link added: {} -> {}",
    ///             event.link().src(), event.link().dst()),
    ///         SampleKind::Delete => println!("Link removed"),
    ///     }
    /// }
    /// # }
    /// ```
    #[zenoh_macros::unstable]
    pub fn link_events_listener(&self) -> LinkEventsListenerBuilder<'_, DefaultHandler> {
        LinkEventsListenerBuilder::new(&self.session)
    }
}

/// Transport is a connection established to zenoh peer node.
/// Multiple transports to the same peer can exist. At this moment it's possible
/// to have both a unicast and a multicast transport to the same peer.
///
/// Each transport can have multiple corresponding [`Link`](crate::session::Link)s which represent
/// actual established data links with various protocols.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transport {
    pub(crate) zid: ZenohId,
    pub(crate) whatami: WhatAmI,
    pub(crate) is_qos: bool,
    pub(crate) is_multicast: bool,
    #[cfg(feature = "shared-memory")]
    pub(crate) is_shm: bool,
}

impl Transport {
    pub(crate) fn new(peer: &TransportPeer, is_multicast: bool) -> Self {
        Transport {
            zid: peer.zid.into(),
            whatami: peer.whatami,
            is_qos: peer.is_qos,
            is_multicast,
            #[cfg(feature = "shared-memory")]
            is_shm: peer.is_shm,
        }
    }

    /// Constructs an uninitialized empty Transport.
    #[zenoh_macros::internal]
    pub fn empty() -> Self {
        Transport {
            zid: ZenohId::default(),
            whatami: WhatAmI::default(),
            is_qos: false,
            is_multicast: false,
            #[cfg(feature = "shared-memory")]
            is_shm: false,
        }
    }
}

#[cfg(feature = "unstable")]
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

    /// Returns whether this transport supports QoS.
    #[inline]
    pub fn is_qos(&self) -> bool {
        self.is_qos
    }

    /// Returns whether this transport supports shared memory.
    #[cfg(feature = "shared-memory")]
    #[inline]
    pub fn is_shm(&self) -> bool {
        self.is_shm
    }

    /// Returns whether this transport is multicast.
    #[inline]
    pub fn is_multicast(&self) -> bool {
        self.is_multicast
    }
}

/// Describes a concrete link within a [`Transport`](crate::session::Transport).
/// Zenoh can establish multiple links to the same remote zenoh node using different protocols
/// (e.g., TCP, UDP, QUIC, etc.)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Link {
    pub(crate) zid: ZenohId,
    pub(crate) src: Locator,
    pub(crate) dst: Locator,
    pub(crate) group: Option<Locator>,
    pub(crate) mtu: u16,
    pub(crate) is_streamed: bool,
    pub(crate) interfaces: Vec<String>,
    pub(crate) auth_identifier: Option<String>,
    pub(crate) priorities: Option<(u8, u8)>,
    pub(crate) reliability: Option<Reliability>,
}

impl Link {
    pub(crate) fn new(zid: ZenohId, link: &zenoh_link_commons::Link, is_qos: bool) -> Self {
        let auth_identifier = match &link.auth_identifier {
            LinkAuthId::Tls(Some(s)) | LinkAuthId::Quic(Some(s)) => Some(s.clone()),
            LinkAuthId::Tls(None)
            | LinkAuthId::Quic(None)
            | LinkAuthId::Tcp
            | LinkAuthId::Udp
            | LinkAuthId::Serial
            | LinkAuthId::Unixpipe
            | LinkAuthId::UnixsockStream
            | LinkAuthId::Vsock
            | LinkAuthId::Ws => None, // avoid using _ wildcard to ensure that new protocols are correctly handled
        };
        let priorities = if is_qos {
            link.priorities
                .as_deref()
                .map(|p| (*p.start() as u8, *p.end() as u8))
                .or(Some((
                    // max priority is the lowest numerical value
                    zenoh_protocol::core::Priority::MAX as u8,
                    zenoh_protocol::core::Priority::MIN as u8,
                )))
        } else {
            None
        };
        // Link reliability is always known - it's either specified explicitly in
        // the config or inferred from the protocol
        let reliability = if is_qos {
            Some(link.reliability.unwrap_or_else(|| {
                let inspector = zenoh_link::LocatorInspector::default();
                // TODO: do we need to check both src and dst?
                if inspector.is_reliable(&link.src).unwrap_or(false)
                    && inspector.is_reliable(&link.dst).unwrap_or(false)
                {
                    Reliability::Reliable
                } else {
                    Reliability::BestEffort
                }
            }))
        } else {
            None
        };
        Link {
            zid,
            src: link.src.clone(),
            dst: link.dst.clone(),
            group: link.group.clone(),
            mtu: link.mtu,
            is_streamed: link.is_streamed,
            interfaces: link.interfaces.clone(),
            auth_identifier,
            priorities,
            reliability,
        }
    }

    /// Constructs an uninitialized empty Link.
    #[zenoh_macros::internal]
    pub fn empty() -> Self {
        Link {
            zid: ZenohId::default(),
            src: Locator::empty(),
            dst: Locator::empty(),
            group: None,
            mtu: 0,
            is_streamed: false,
            interfaces: Vec::new(),
            auth_identifier: None,
            priorities: None,
            reliability: None,
        }
    }
}

#[cfg(feature = "unstable")]
impl Link {
    /// Gets the ZenohId of the transport this link belongs to.
    #[inline]
    pub fn zid(&self) -> &ZenohId {
        &self.zid
    }

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

    /// Gets the optional group locator (destination if link is for multicast).
    #[inline]
    pub fn group(&self) -> Option<&Locator> {
        self.group.as_ref()
    }

    /// Gets the maximum transmission unit in bytes (MTU) of the link.
    #[inline]
    pub fn mtu(&self) -> u16 {
        self.mtu
    }

    /// Returns whether the link is streamed.
    #[inline]
    pub fn is_streamed(&self) -> bool {
        self.is_streamed
    }

    /// Gets the network interfaces associated with the link.
    #[inline]
    pub fn interfaces(&self) -> &[String] {
        &self.interfaces
    }

    /// Gets the authentication identifier used for the link (if any).
    #[inline]
    pub fn auth_identifier(&self) -> Option<&str> {
        self.auth_identifier.as_deref()
    }

    /// Gets the optional priority range (min, max) associated with the link.
    /// The numeric priority values corresponds [`Priority`](crate::qos::Priority)
    /// but can also contain value 0 (Control) not exposed in the public enum.
    /// Returns None if the transport does not support QoS.
    #[inline]
    pub fn priorities(&self) -> Option<(u8, u8)> {
        self.priorities
    }

    /// Gets the reliability level of the link.
    /// Returns None if the transport does not support QoS.
    #[inline]
    pub fn reliability(&self) -> Option<Reliability> {
        self.reliability
    }
}

/// Event emitted when a transport is opened or closed
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportEvent {
    pub(crate) kind: SampleKind, // Put = opened, Delete = closed
    pub(crate) transport: Transport,
}

impl TransportEvent {
    /// Constructs an uninitialized empty TransportEvent.
    #[zenoh_macros::internal]
    pub fn empty() -> Self {
        TransportEvent {
            kind: SampleKind::Put,
            transport: Transport::empty(),
        }
    }
}

#[cfg(feature = "unstable")]
impl TransportEvent {
    /// Returns the kind of event (Put for opened, Delete for closed)
    pub fn kind(&self) -> SampleKind {
        self.kind
    }

    /// Returns a reference to the transport
    pub fn transport(&self) -> &Transport {
        &self.transport
    }

    /// Returns a mutable reference to the transport
    pub fn transport_mut(&mut self) -> &mut Transport {
        &mut self.transport
    }
}

impl CallbackParameter for TransportEvent {
    type Message<'a> = Self;
    fn from_message(msg: Self::Message<'_>) -> Self {
        msg
    }
}

/// Event emitted when a link is added or removed
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkEvent {
    pub(crate) kind: SampleKind, // Put = added, Delete = removed
    pub(crate) link: Link,
}

impl LinkEvent {
    /// Constructs an uninitialized empty LinkEvent.
    #[zenoh_macros::internal]
    pub fn empty() -> Self {
        LinkEvent {
            kind: SampleKind::Put,
            link: Link::empty(),
        }
    }
}

#[cfg(feature = "unstable")]
impl LinkEvent {
    /// Returns the kind of event (Put for added, Delete for removed)
    pub fn kind(&self) -> SampleKind {
        self.kind
    }

    /// Returns a reference to the link
    pub fn link(&self) -> &Link {
        &self.link
    }

    /// Returns a mutable reference to the link
    pub fn link_mut(&mut self) -> &mut Link {
        &mut self.link
    }
}

impl CallbackParameter for LinkEvent {
    type Message<'a> = Self;
    fn from_message(msg: Self::Message<'_>) -> Self {
        msg
    }
}
