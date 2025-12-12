//
// Copyright (c) 2025 ZettaScale Technology
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

use zenoh_config::ZenohId;
use zenoh_core::{Resolvable, Wait};

use crate::handlers::locked;
use crate::session::{Link, LinkEvent};
use crate::{
    handlers::{Callback, DefaultHandler, IntoHandler},
    net::runtime::DynamicRuntime,
};

/// A builder returned by [`SessionInfo::links()`](crate::session::SessionInfo::links) that allows
/// access to information about links across all transport sessions. Multiple links can be established
/// between two zenoh nodes (e.g., using different interfaces).
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
    transport_zid: Option<ZenohId>,
}

#[zenoh_macros::unstable]
impl<'a> LinksBuilder<'a> {
    pub(crate) fn new(runtime: &'a DynamicRuntime) -> Self {
        Self {
            runtime,
            transport_zid: None,
        }
    }

    /// Filter links by transport ZenohId.
    ///
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let transport_zid = /* some ZenohId */;
    /// let links = session.info().links().transport(transport_zid).await;
    /// for link in links {
    ///     println!("Link: {} -> {}", link.src(), link.dst());
    /// }
    /// # }
    /// ```
    pub fn transport(mut self, zid: ZenohId) -> Self {
        self.transport_zid = Some(zid);
        self
    }
}

#[zenoh_macros::unstable]
impl Resolvable for LinksBuilder<'_> {
    type To = Box<dyn Iterator<Item = Link> + Send + Sync>;
}

#[zenoh_macros::unstable]
impl Wait for LinksBuilder<'_> {
    fn wait(self) -> Self::To {
        self.runtime.get_links(self.transport_zid)
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


/// A builder returned by [`SessionInfo::linkl_events_listener()`](crate::session::SessionInfo::linkl_events_listener) that allows
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
///     .linkl_events_listener()
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
pub struct LinkEventsListenerBuilder<'a, Handler> {
    runtime: &'a DynamicRuntime,
    handler: Handler,
    history: bool,
    transport_zid: Option<ZenohId>,
}

#[zenoh_macros::unstable]
impl<'a> LinkEventsListenerBuilder<'a, DefaultHandler> {
    pub(crate) fn new(runtime: &'a DynamicRuntime) -> Self {
        Self {
            runtime,
            handler: DefaultHandler::default(),
            history: false,
            transport_zid: None,
        }
    }
}

#[zenoh_macros::unstable]
impl<'a, Handler> LinkEventsListenerBuilder<'a, Handler> {
    /// Enable history - send events for existing links before live events.
    pub fn history(mut self, enabled: bool) -> Self {
        self.history = enabled;
        self
    }

    /// Use a custom handler (channel, callback, etc.)
    pub fn with<H>(self, handler: H) -> LinkEventsListenerBuilder<'a, H>
    where
        H: IntoHandler<LinkEvent>,
    {
        LinkEventsListenerBuilder {
            runtime: self.runtime,
            handler,
            history: self.history,
            transport_zid: self.transport_zid,
        }
    }

    /// Filter link events by transport ZenohId.
    ///
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let transport_zid = /* some ZenohId */;
    /// let events = session.info()
    ///     .linkl_events_listener()
    ///     .transport(transport_zid)
    ///     .with(flume::bounded(32))
    ///     .await;
    /// # }
    /// ```
    pub fn transport(mut self, zid: ZenohId) -> Self {
        self.transport_zid = Some(zid);
        self
    }

    /// Provide a callback to handle events
    pub fn callback<F>(self, callback: F) -> LinkEventsListenerBuilder<'a, Callback<LinkEvent>>
    where
        F: Fn(LinkEvent) + Send + Sync + 'static,
    {
        self.with(Callback::from(callback))
    }

    /// Provide a mutable callback which is never called concurrently. If the callback can be accepted by
    /// [`callback`](Self::callback), prefer using that instead for better performance.
    pub fn callback_mut<F>(self, callback: F) -> LinkEventsListenerBuilder<'a, Callback<LinkEvent>>
    where
        F: FnMut(LinkEvent) + Send + Sync + 'static,
    {
        self.callback(locked(callback))
    }
}

#[zenoh_macros::unstable]
impl<Handler> Resolvable for LinkEventsListenerBuilder<'_, Handler>
where
    Handler: IntoHandler<LinkEvent> + Send,
    Handler::Handler: Send,
{
    type To = Handler::Handler;
}

#[zenoh_macros::unstable]
impl<Handler> Wait for LinkEventsListenerBuilder<'_, Handler>
where
    Handler: IntoHandler<LinkEvent> + Send,
    Handler::Handler: Send,
{
    fn wait(self) -> Self::To {
        let (callback, handler) = self.handler.into_handler();
        #[allow(unused_variables)] // id is only needed for unstable cancellation_token
        let id = self
            .runtime
            .linkl_events_listener(callback, self.history, self.transport_zid);
        handler
    }
}

#[zenoh_macros::unstable]
impl<Handler> IntoFuture for LinkEventsListenerBuilder<'_, Handler>
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
