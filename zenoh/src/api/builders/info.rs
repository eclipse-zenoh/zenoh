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

#[cfg(feature = "unstable")]
use crate::api::cancellation::CancellationTokenBuilderTrait;
#[cfg(feature = "unstable")]
use crate::api::handlers::{locked, Callback, DefaultHandler, IntoHandler};
#[cfg(feature = "unstable")]
use crate::api::info::{Link, LinkEvent, Transport, TransportEvent};
use crate::net::runtime::DynamicRuntime;

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
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let transports = session.info().transports().await;
/// for transport in transports {
///     println!("Transport: zid={}, whatami={:?}", transport.zid(), transport.whatami());
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
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let links = session.info().links().await;
/// for link in links {
///     println!("Link: {} -> {}", link.src(), link.dst());
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

/// A builder returned by [`SessionInfo::transport_events()`](crate::session::SessionInfo::transport_events) that allows
/// subscribing to transport lifecycle events.
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
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
#[zenoh_macros::unstable]
pub struct TransportEventsBuilder<'a, Handler> {
    runtime: &'a DynamicRuntime,
    handler: Handler,
    history: bool,
    #[cfg(feature = "unstable")]
    cancellation_token: Option<crate::api::cancellation::CancellationToken>,
}

#[zenoh_macros::unstable]
impl<'a> TransportEventsBuilder<'a, DefaultHandler> {
    pub(crate) fn new(runtime: &'a DynamicRuntime) -> Self {
        Self {
            runtime,
            handler: DefaultHandler::default(),
            history: false,
            #[cfg(feature = "unstable")]
            cancellation_token: None,
        }
    }
}

#[zenoh_macros::unstable]
impl<'a, Handler> TransportEventsBuilder<'a, Handler> {
    /// Enable history - send events for existing transports before live events.
    pub fn history(mut self, enabled: bool) -> Self {
        self.history = enabled;
        self
    }

    /// Use a custom handler (channel, callback, etc.)
    pub fn with<H>(self, handler: H) -> TransportEventsBuilder<'a, H>
    where
        H: IntoHandler<TransportEvent>,
    {
        TransportEventsBuilder {
            runtime: self.runtime,
            handler,
            history: self.history,
            #[cfg(feature = "unstable")]
            cancellation_token: self.cancellation_token,
        }
    }

    /// Provide a callback to handle events
    pub fn callback<F>(self, callback: F) -> TransportEventsBuilder<'a, Callback<TransportEvent>>
    where
        F: Fn(TransportEvent) + Send + Sync + 'static,
    {
        self.with(Callback::from(callback))
    }

    /// Provide a mutable callback (internally synchronized)
    pub fn callback_mut<F>(
        self,
        callback: F,
    ) -> TransportEventsBuilder<'a, Callback<TransportEvent>>
    where
        F: FnMut(TransportEvent) + Send + Sync + 'static,
    {
        self.callback(locked(callback))
    }
}

#[cfg(feature = "unstable")]
#[zenoh_macros::internal_trait]
impl<Handler> CancellationTokenBuilderTrait for TransportEventsBuilder<'_, Handler> {
    /// Provide a cancellation token that ensures the callback is properly cleaned up.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::sample::SampleKind;
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let ct = zenoh::cancellation::CancellationToken::default();
    /// let events = session.info()
    ///     .transport_events()
    ///     .with(flume::bounded(32))
    ///     .cancellation_token(ct.clone())
    ///     .await;
    ///
    /// tokio::task::spawn(async move {
    ///     tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    ///     ct.cancel().await.unwrap();
    /// });
    ///
    /// while let Ok(event) = events.recv_async().await {
    ///     match event.kind() {
    ///         SampleKind::Put => println!("Transport opened: {}", event.transport().zid()),
    ///         SampleKind::Delete => println!("Transport closed"),
    ///     }
    /// }
    /// # }
    /// ```
    #[zenoh_macros::unstable_doc]
    fn cancellation_token(
        self,
        cancellation_token: crate::api::cancellation::CancellationToken,
    ) -> Self {
        Self {
            cancellation_token: Some(cancellation_token),
            ..self
        }
    }
}

#[zenoh_macros::unstable]
impl<Handler> Resolvable for TransportEventsBuilder<'_, Handler>
where
    Handler: IntoHandler<TransportEvent> + Send,
    Handler::Handler: Send,
{
    type To = Handler::Handler;
}

#[zenoh_macros::unstable]
impl<Handler> Wait for TransportEventsBuilder<'_, Handler>
where
    Handler: IntoHandler<TransportEvent> + Send,
    Handler::Handler: Send,
{
    fn wait(self) -> Self::To {
        #[allow(unused_mut)] // mut is needed only for unstable cancellation_token
        let (mut callback, handler) = self.handler.into_handler();
        #[cfg(feature = "unstable")]
        if let Some(ct) = self.cancellation_token {
            if let Some(notifier) = ct.notifier() {
                callback.set_on_drop_notifier(notifier);
            } else {
                // Token already cancelled, return handler without registering
                return handler;
            }
        }
        self.runtime.transport_events(callback, self.history);
        handler
    }
}

#[zenoh_macros::unstable]
impl<Handler> IntoFuture for TransportEventsBuilder<'_, Handler>
where
    Handler: IntoHandler<TransportEvent> + Send,
    Handler::Handler: Send,
{
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

/// A builder returned by [`SessionInfo::link_events()`](crate::session::SessionInfo::link_events) that allows
/// subscribing to link lifecycle events.
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
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
#[zenoh_macros::unstable]
pub struct LinkEventsBuilder<'a, Handler> {
    runtime: &'a DynamicRuntime,
    handler: Handler,
    history: bool,
}

#[zenoh_macros::unstable]
impl<'a> LinkEventsBuilder<'a, DefaultHandler> {
    pub(crate) fn new(runtime: &'a DynamicRuntime) -> Self {
        Self {
            runtime,
            handler: DefaultHandler::default(),
            history: false,
        }
    }
}

#[zenoh_macros::unstable]
impl<'a, Handler> LinkEventsBuilder<'a, Handler> {
    /// Enable history - send events for existing links before live events.
    pub fn history(mut self, enabled: bool) -> Self {
        self.history = enabled;
        self
    }

    /// Use a custom handler (channel, callback, etc.)
    pub fn with<H>(self, handler: H) -> LinkEventsBuilder<'a, H>
    where
        H: IntoHandler<LinkEvent>,
    {
        LinkEventsBuilder {
            runtime: self.runtime,
            handler,
            history: self.history,
        }
    }

    /// Provide a callback to handle events
    pub fn callback<F>(self, callback: F) -> LinkEventsBuilder<'a, Callback<LinkEvent>>
    where
        F: Fn(LinkEvent) + Send + Sync + 'static,
    {
        self.with(Callback::from(callback))
    }

    /// Provide a mutable callback (internally synchronized)
    pub fn callback_mut<F>(self, callback: F) -> LinkEventsBuilder<'a, Callback<LinkEvent>>
    where
        F: FnMut(LinkEvent) + Send + Sync + 'static,
    {
        self.callback(locked(callback))
    }
}

#[zenoh_macros::unstable]
impl<Handler> Resolvable for LinkEventsBuilder<'_, Handler>
where
    Handler: IntoHandler<LinkEvent> + Send,
    Handler::Handler: Send,
{
    type To = Handler::Handler;
}

#[zenoh_macros::unstable]
impl<Handler> Wait for LinkEventsBuilder<'_, Handler>
where
    Handler: IntoHandler<LinkEvent> + Send,
    Handler::Handler: Send,
{
    fn wait(self) -> Self::To {
        let (callback, handler) = self.handler.into_handler();
        self.runtime.link_events(callback, self.history);
        handler
    }
}

#[zenoh_macros::unstable]
impl<Handler> IntoFuture for LinkEventsBuilder<'_, Handler>
where
    Handler: IntoHandler<LinkEvent> + Send,
    Handler::Handler: Send,
{
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}
