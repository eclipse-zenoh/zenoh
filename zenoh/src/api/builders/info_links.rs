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

#[zenoh_macros::unstable]
use std::future::{IntoFuture, Ready};

#[zenoh_macros::unstable]
use tracing::error;
#[zenoh_macros::unstable]
use zenoh_config::ZenohId;
#[zenoh_macros::unstable]
use zenoh_core::{Resolvable, Wait};
#[zenoh_macros::unstable]
use zenoh_result::ZResult;

#[zenoh_macros::unstable]
use crate::api::handlers::locked;
#[zenoh_macros::unstable]
use crate::api::info::{Link, LinkEvent};
#[zenoh_macros::unstable]
use crate::{
    api::session::UndeclarableSealed,
    handlers::{Callback, DefaultHandler, IntoHandler},
};
#[zenoh_macros::unstable]
use crate::{api::Id, net::runtime::DynamicRuntime};

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

#[zenoh_macros::unstable]
pub(crate) struct LinkEventsListenerInner {
    pub(crate) runtime: DynamicRuntime,
    pub(crate) id: Id,
    pub(crate) undeclare_on_drop: bool,
}

#[zenoh_macros::unstable]
impl std::fmt::Debug for LinkEventsListenerInner {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("LinkEventsListenerInner")
            .field("id", &self.id)
            .field("undeclare_on_drop", &self.undeclare_on_drop)
            .finish()
    }
}

/// A listener that sends notifications when link lifecycle events occur.
///
/// Call [`undeclare()`](LinkEventsListener::undeclare) to stop receiving events.
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
///     .await;
///
/// while let Ok(event) = listener.recv_async().await {
///     match event.kind() {
///         SampleKind::Put => println!("Link added: {} -> {}",
///             event.link().src(), event.link().dst()),
///         SampleKind::Delete => println!("Link removed"),
///     }
/// }
///
/// // Cleanup
/// listener.undeclare().await.unwrap();
/// # }
/// ```
#[zenoh_macros::unstable]
#[derive(Debug)]
pub struct LinkEventsListener<Handler> {
    pub(crate) inner: LinkEventsListenerInner,
    pub(crate) handler: Handler,
}

#[zenoh_macros::unstable]
impl<Handler> LinkEventsListener<Handler> {
    /// Undeclare the listener and stop receiving events.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let listener = session.info()
    ///     .link_events_listener()
    ///     .with(flume::bounded(32))
    ///     .await;
    /// listener.undeclare().await.unwrap();
    /// # }
    /// ```
    #[inline]
    pub fn undeclare(self) -> LinkEventsListenerUndeclaration<Handler>
    where
        Handler: Send,
    {
        self.undeclare_inner(())
    }

    fn undeclare_impl(&mut self) -> ZResult<()> {
        // Set flag first to avoid double panic
        self.inner.undeclare_on_drop = false;
        // Call runtime's cancel method with the stored id
        self.inner.runtime.cancel_link_events(self.inner.id);
        Ok(())
    }

    /// Returns a reference to this listener's handler.
    /// A handler is anything that implements [`IntoHandler`](crate::handlers::IntoHandler).
    /// The default handler is [`DefaultHandler`](crate::handlers::DefaultHandler).
    pub fn handler(&self) -> &Handler {
        &self.handler
    }

    /// Returns a mutable reference to this listener's handler.
    /// A handler is anything that implements [`IntoHandler`](crate::handlers::IntoHandler).
    /// The default handler is [`DefaultHandler`](crate::handlers::DefaultHandler).
    pub fn handler_mut(&mut self) -> &mut Handler {
        &mut self.handler
    }

    #[zenoh_macros::internal]
    pub fn set_background(&mut self, background: bool) {
        self.inner.undeclare_on_drop = !background;
    }
}

#[zenoh_macros::unstable]
impl<Handler> Drop for LinkEventsListener<Handler> {
    fn drop(&mut self) {
        if self.inner.undeclare_on_drop {
            if let Err(error) = self.undeclare_impl() {
                error!(error);
            }
        }
    }
}

#[zenoh_macros::unstable]
impl<Handler: Send> UndeclarableSealed<()> for LinkEventsListener<Handler> {
    type Undeclaration = LinkEventsListenerUndeclaration<Handler>;

    fn undeclare_inner(self, _: ()) -> Self::Undeclaration {
        LinkEventsListenerUndeclaration(self)
    }
}

#[zenoh_macros::unstable]
impl<Handler> std::ops::Deref for LinkEventsListener<Handler> {
    type Target = Handler;

    fn deref(&self) -> &Self::Target {
        &self.handler
    }
}

#[zenoh_macros::unstable]
impl<Handler> std::ops::DerefMut for LinkEventsListener<Handler> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.handler
    }
}

/// A [`Resolvable`] returned by [`LinkEventsListener::undeclare`]
#[zenoh_macros::unstable]
pub struct LinkEventsListenerUndeclaration<Handler>(LinkEventsListener<Handler>);

#[zenoh_macros::unstable]
impl<Handler> Resolvable for LinkEventsListenerUndeclaration<Handler> {
    type To = ZResult<()>;
}

#[zenoh_macros::unstable]
impl<Handler> Wait for LinkEventsListenerUndeclaration<Handler> {
    fn wait(mut self) -> <Self as Resolvable>::To {
        self.0.undeclare_impl()
    }
}

#[zenoh_macros::unstable]
impl<Handler> IntoFuture for LinkEventsListenerUndeclaration<Handler> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

/// A builder returned by [`SessionInfo::link_events_listener()`](crate::session::SessionInfo::link_events_listener) that allows
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
///     .links_events_listener()
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
    ///     .links_events_listener()
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
    type To = LinkEventsListener<Handler::Handler>;
}

#[zenoh_macros::unstable]
impl<Handler> Wait for LinkEventsListenerBuilder<'_, Handler>
where
    Handler: IntoHandler<LinkEvent> + Send,
    Handler::Handler: Send,
{
    fn wait(self) -> Self::To {
        let (callback, handler) = self.handler.into_handler();
        let id = self
            .runtime
            .links_events_listener(callback, self.history, self.transport_zid);

        LinkEventsListener {
            inner: LinkEventsListenerInner {
                runtime: self.runtime.clone(),
                id,
                undeclare_on_drop: true,
            },
            handler,
        }
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
