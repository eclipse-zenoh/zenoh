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

use std::future::{IntoFuture, Ready};

use zenoh_core::{Resolvable, Wait};

use crate::{
    handlers::{locked, Callback, DefaultHandler, IntoHandler},
    net::runtime::DynamicRuntime,
    session::{Transport, TransportEvent},
};

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

/// A builder returned by [`SessionInfo::transport_events_listener()`](crate::session::SessionInfo::transport_events_listener) that allows
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
///     .transport_events_listener()
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
pub struct TransportEventsListenerBuilder<'a, Handler> {
    runtime: &'a DynamicRuntime,
    handler: Handler,
    history: bool,
}

#[zenoh_macros::unstable]
impl<'a> TransportEventsListenerBuilder<'a, DefaultHandler> {
    pub(crate) fn new(runtime: &'a DynamicRuntime) -> Self {
        Self {
            runtime,
            handler: DefaultHandler::default(),
            history: false,
        }
    }
}

#[zenoh_macros::unstable]
impl<'a, Handler> TransportEventsListenerBuilder<'a, Handler> {
    /// Enable history - send events for existing transports before live events.
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
            runtime: self.runtime,
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
impl<Handler> Resolvable for TransportEventsListenerBuilder<'_, Handler>
where
    Handler: IntoHandler<TransportEvent> + Send,
    Handler::Handler: Send,
{
    type To = Handler::Handler;
}

#[zenoh_macros::unstable]
impl<Handler> Wait for TransportEventsListenerBuilder<'_, Handler>
where
    Handler: IntoHandler<TransportEvent> + Send,
    Handler::Handler: Send,
{
    fn wait(self) -> Self::To {
        let (callback, handler) = self.handler.into_handler();
        #[allow(unused_variables)] // id is only needed for unstable cancellation_token
        let id = self
            .runtime
            .transport_events_listener(callback, self.history);
        handler
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
