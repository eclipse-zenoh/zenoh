//
// Copyright (c) 2025 ZettaScale TechnologyA
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
use zenoh_core::{Resolvable, Wait};
#[zenoh_macros::unstable]
use zenoh_result::ZResult;

#[zenoh_macros::unstable]
use crate::api::handlers::locked;
#[zenoh_macros::unstable]
use crate::api::info::{Transport, TransportEvent};
#[zenoh_macros::unstable]
use crate::{
    api::cancellation::SyncGroup,
    api::session::{UndeclarableSealed, WeakSession},
    api::Id,
    handlers::{Callback, DefaultHandler, IntoHandler},
    net::runtime::DynamicRuntime,
};

/// A builder returned by [`SessionInfo::transports()`](crate::session::SessionInfo::transports).
///
/// The builder creates an iterator over all currently active [`Transport`](crate::session::Transport) instances.
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

#[zenoh_macros::unstable]
pub(crate) struct TransportEventsListenerInner {
    pub(crate) session: WeakSession,
    pub(crate) id: Id,
    pub(crate) undeclare_on_drop: bool,
}

#[zenoh_macros::unstable]
impl std::fmt::Debug for TransportEventsListenerInner {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("TransportEventsListenerInner")
            .field("id", &self.id)
            .field("undeclare_on_drop", &self.undeclare_on_drop)
            .finish()
    }
}

/// A listener that sends notifications when transport lifecycle events occur.
///
/// Call [`undeclare()`](TransportEventsListener::undeclare) to stop receiving events.
///
/// # Examples
/// ```no_run
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh::sample::SampleKind;
///
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let listener = session.info()
///     .transport_events_listener()
///     .history(true)
///     .with(flume::bounded(32))
///     .await
///     .expect("Failed to declare transport events listener");
///
/// while let Ok(event) = listener.recv_async().await {
///     match event.kind() {
///         SampleKind::Put => println!("Transport opened: {}", event.transport().zid()),
///         SampleKind::Delete => println!("Transport closed"),
///     }
/// }
///
/// // Cleanup
/// listener.undeclare().await.unwrap();
/// # }
/// ```
#[zenoh_macros::unstable]
pub struct TransportEventsListener<Handler> {
    pub(crate) inner: TransportEventsListenerInner,
    pub(crate) handler: Handler,
    #[cfg(feature = "unstable")]
    pub(crate) callback_sync_group: SyncGroup,
}

#[zenoh_macros::unstable]
impl<Handler> TransportEventsListener<Handler> {
    /// Undeclare the listener and stop receiving events.
    ///
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let listener = session.info()
    ///     .transport_events_listener()
    ///     .with(flume::bounded(32))
    ///     .await
    ///     .expect("Failed to declare transport events listener");
    /// listener.undeclare().await.unwrap();
    /// # }
    /// ```
    #[inline]
    pub fn undeclare(self) -> TransportEventsListenerUndeclaration<Handler>
    where
        Handler: Send,
    {
        self.undeclare_inner(())
    }

    fn undeclare_impl(&mut self) -> ZResult<()> {
        // Set flag first to avoid double panic
        self.inner.undeclare_on_drop = false;
        // Call session's undeclare method with the stored id
        self.inner
            .session
            .undeclare_transport_events_listener_inner(self.inner.id)
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
impl<Handler> Drop for TransportEventsListener<Handler> {
    fn drop(&mut self) {
        if self.inner.undeclare_on_drop {
            if let Err(error) = self.undeclare_impl() {
                error!(error);
            }
        }
    }
}

#[zenoh_macros::unstable]
impl<Handler: Send> UndeclarableSealed<()> for TransportEventsListener<Handler> {
    type Undeclaration = TransportEventsListenerUndeclaration<Handler>;

    fn undeclare_inner(self, _: ()) -> Self::Undeclaration {
        TransportEventsListenerUndeclaration {
            listener: self,
            #[cfg(feature = "unstable")]
            wait_callbacks: false,
        }
    }
}

#[zenoh_macros::unstable]
impl<Handler> std::ops::Deref for TransportEventsListener<Handler> {
    type Target = Handler;

    fn deref(&self) -> &Self::Target {
        &self.handler
    }
}

#[zenoh_macros::unstable]
impl<Handler> std::ops::DerefMut for TransportEventsListener<Handler> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.handler
    }
}

/// A [`Resolvable`] returned by [`TransportEventsListener::undeclare`]
#[zenoh_macros::unstable]
pub struct TransportEventsListenerUndeclaration<Handler> {
    listener: TransportEventsListener<Handler>,
    #[cfg(feature = "unstable")]
    wait_callbacks: bool,
}

#[zenoh_macros::unstable]
impl<Handler> TransportEventsListenerUndeclaration<Handler> {
    /// Block in undeclare operation until all currently running instances of transport events listener callback (if any) return.
    #[zenoh_macros::unstable]
    pub fn wait_callbacks(mut self) -> Self {
        self.wait_callbacks = true;
        self
    }
}

#[zenoh_macros::unstable]
impl<Handler> Resolvable for TransportEventsListenerUndeclaration<Handler> {
    type To = ZResult<()>;
}

#[zenoh_macros::unstable]
impl<Handler> Wait for TransportEventsListenerUndeclaration<Handler> {
    fn wait(mut self) -> <Self as Resolvable>::To {
        self.listener.undeclare_impl()?;
        #[cfg(feature = "unstable")]
        if self.wait_callbacks {
            self.listener.callback_sync_group.wait();
        }
        Ok(())
    }
}

#[zenoh_macros::unstable]
impl<Handler> IntoFuture for TransportEventsListenerUndeclaration<Handler> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

/// A builder returned by [`SessionInfo::transport_events_listener()`](crate::session::SessionInfo::transport_events_listener) that allows subscribing to transport lifecycle events.
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
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
#[zenoh_macros::unstable]
pub struct TransportEventsListenerBuilder<'a, Handler, const BACKGROUND: bool = false> {
    session: &'a WeakSession,
    handler: Handler,
    history: bool,
}

#[zenoh_macros::unstable]
impl<'a> TransportEventsListenerBuilder<'a, DefaultHandler> {
    pub(crate) fn new(session: &'a WeakSession) -> Self {
        Self {
            session,
            handler: DefaultHandler::default(),
            history: false,
        }
    }
}

#[zenoh_macros::unstable]
impl<'a, Handler> TransportEventsListenerBuilder<'a, Handler> {
    /// Enable history.
    ///
    /// Send events for existing transports before live events.
    pub fn history(mut self, enabled: bool) -> Self {
        self.history = enabled;
        self
    }

    /// Use a custom handler (channel, callback, etc.)
    pub fn with<H>(self, handler: H) -> TransportEventsListenerBuilder<'a, H>
    where
        H: IntoHandler<TransportEvent>,
    {
        TransportEventsListenerBuilder {
            session: self.session,
            handler,
            history: self.history,
        }
    }

    /// Provide a callback to handle events
    pub fn callback<F>(
        self,
        callback: F,
    ) -> TransportEventsListenerBuilder<'a, Callback<TransportEvent>>
    where
        F: Fn(TransportEvent) + Send + Sync + 'static,
    {
        self.with(Callback::from(callback))
    }

    /// Provide a mutable callback which is never called concurrently. If the callback can be accepted by
    /// [`callback`](Self::callback), prefer using that instead for better performance.
    pub fn callback_mut<F>(
        self,
        callback: F,
    ) -> TransportEventsListenerBuilder<'a, Callback<TransportEvent>>
    where
        F: FnMut(TransportEvent) + Send + Sync + 'static,
    {
        self.callback(locked(callback))
    }
}

#[zenoh_macros::unstable]
impl<'a> TransportEventsListenerBuilder<'a, Callback<TransportEvent>> {
    /// Run the listener in the background, automatically dropping the handler when done.
    ///
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::sample::SampleKind;
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// // no need to assign and keep a variable for a background listener
    /// session.info()
    ///     .transport_events_listener()
    ///     .callback(|event| {
    ///         match event.kind() {
    ///             SampleKind::Put => println!("Transport opened: {}", event.transport().zid()),
    ///             SampleKind::Delete => println!("Transport closed"),
    ///         }
    ///     })
    ///     .background()
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
    pub fn background(self) -> TransportEventsListenerBuilder<'a, Callback<TransportEvent>, true> {
        TransportEventsListenerBuilder {
            session: self.session,
            handler: self.handler,
            history: self.history,
        }
    }
}

#[zenoh_macros::unstable]
impl<Handler> Resolvable for TransportEventsListenerBuilder<'_, Handler>
where
    Handler: IntoHandler<TransportEvent> + Send,
    Handler::Handler: Send,
{
    type To = ZResult<TransportEventsListener<Handler::Handler>>;
}

#[zenoh_macros::unstable]
impl<Handler> Wait for TransportEventsListenerBuilder<'_, Handler>
where
    Handler: IntoHandler<TransportEvent> + Send,
    Handler::Handler: Send,
{
    fn wait(self) -> Self::To {
        #[cfg(feature = "unstable")]
        let callback_sync_group = SyncGroup::default();
        let (callback, handler) = self.handler.into_handler();
        let state = self.session.declare_transport_events_listener_inner(
            callback,
            self.history,
            callback_sync_group.notifier(),
        )?;

        Ok(TransportEventsListener {
            inner: TransportEventsListenerInner {
                session: self.session.clone(),
                id: state.id,
                undeclare_on_drop: true,
            },
            handler,
            callback_sync_group,
        })
    }
}

#[zenoh_macros::unstable]
impl<Handler> IntoFuture for TransportEventsListenerBuilder<'_, Handler>
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
#[zenoh_macros::unstable]
impl Resolvable for TransportEventsListenerBuilder<'_, Callback<TransportEvent>, true> {
    type To = ZResult<()>;
}

#[zenoh_macros::unstable]
impl Wait for TransportEventsListenerBuilder<'_, Callback<TransportEvent>, true> {
    fn wait(self) -> <Self as Resolvable>::To {
        let state = self.session.declare_transport_events_listener_inner(
            self.handler,
            self.history,
            #[cfg(feature = "unstable")]
            None,
        )?;
        // Set the listener to not undeclare on drop (background mode)
        // Note: We can't access the listener to set background flag, so we just don't keep a reference
        // The listener will live until explicitly undeclared or session closes
        drop(state);
        Ok(())
    }
}

#[zenoh_macros::unstable]
impl IntoFuture for TransportEventsListenerBuilder<'_, Callback<TransportEvent>, true> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}
