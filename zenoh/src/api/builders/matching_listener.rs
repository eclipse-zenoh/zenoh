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
#[cfg(feature = "unstable")]
use std::future::{IntoFuture, Ready};

#[cfg(feature = "unstable")]
use zenoh_core::{Resolvable, Wait};
#[cfg(feature = "unstable")]
use zenoh_result::ZResult;
#[cfg(feature = "unstable")]
use {
    crate::api::{
        handlers::{Callback, DefaultHandler, IntoHandler},
        publisher::{MatchingListener, MatchingListenerInner, MatchingStatus, Publisher},
    },
    std::sync::Arc,
};

/// A builder for initializing a [`MatchingListener`].
#[zenoh_macros::unstable]
#[derive(Debug)]
pub struct MatchingListenerBuilder<'a, 'b, Handler, const BACKGROUND: bool = false> {
    pub(crate) publisher: &'a Publisher<'b>,
    pub handler: Handler,
}

#[zenoh_macros::unstable]
impl<'a, 'b> MatchingListenerBuilder<'a, 'b, DefaultHandler> {
    /// Receive the MatchingStatuses for this listener with a callback.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let publisher = session.declare_publisher("key/expression").await.unwrap();
    /// let matching_listener = publisher
    ///     .matching_listener()
    ///     .callback(|matching_status| {
    ///         if matching_status.matching_subscribers() {
    ///             println!("Publisher has matching subscribers.");
    ///         } else {
    ///             println!("Publisher has NO MORE matching subscribers.");
    ///         }
    ///     })
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
    #[inline]
    #[zenoh_macros::unstable]
    pub fn callback<F>(
        self,
        callback: F,
    ) -> MatchingListenerBuilder<'a, 'b, Callback<MatchingStatus>>
    where
        F: Fn(MatchingStatus) + Send + Sync + 'static,
    {
        self.with(Callback::new(Arc::new(callback)))
    }

    /// Receive the MatchingStatuses for this listener with a mutable callback.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let mut n = 0;
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let publisher = session.declare_publisher("key/expression").await.unwrap();
    /// let matching_listener = publisher
    ///     .matching_listener()
    ///     .callback_mut(move |_matching_status| { n += 1; })
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
    #[inline]
    #[zenoh_macros::unstable]
    pub fn callback_mut<F>(
        self,
        callback: F,
    ) -> MatchingListenerBuilder<'a, 'b, Callback<MatchingStatus>>
    where
        F: FnMut(MatchingStatus) + Send + Sync + 'static,
    {
        self.callback(crate::api::handlers::locked(callback))
    }

    /// Receive the MatchingStatuses for this listener with a [`Handler`](IntoHandler).
    ///
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let publisher = session.declare_publisher("key/expression").await.unwrap();
    /// let matching_listener = publisher
    ///     .matching_listener()
    ///     .with(flume::bounded(32))
    ///     .await
    ///     .unwrap();
    /// while let Ok(matching_status) = matching_listener.recv_async().await {
    ///     if matching_status.matching_subscribers() {
    ///         println!("Publisher has matching subscribers.");
    ///     } else {
    ///         println!("Publisher has NO MORE matching subscribers.");
    ///     }
    /// }
    /// # }
    /// ```
    #[inline]
    #[zenoh_macros::unstable]
    pub fn with<Handler>(self, handler: Handler) -> MatchingListenerBuilder<'a, 'b, Handler>
    where
        Handler: IntoHandler<MatchingStatus>,
    {
        let MatchingListenerBuilder {
            publisher,
            handler: _,
        } = self;
        MatchingListenerBuilder { publisher, handler }
    }
}

#[zenoh_macros::unstable]
impl<'a, 'b> MatchingListenerBuilder<'a, 'b, Callback<MatchingStatus>> {
    /// Register the listener callback to be run in background until the publisher is undeclared.
    ///
    /// Background builder doesn't return a `MatchingListener` object anymore.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let publisher = session.declare_publisher("key/expression").await.unwrap();
    /// // no need to assign and keep a variable with a background listener
    /// publisher
    ///     .matching_listener()
    ///     .callback(|matching_status| {
    ///         if matching_status.matching_subscribers() {
    ///             println!("Publisher has matching subscribers.");
    ///         } else {
    ///             println!("Publisher has NO MORE matching subscribers.");
    ///         }
    ///     })
    ///     .background()
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
    pub fn background(self) -> MatchingListenerBuilder<'a, 'b, Callback<MatchingStatus>, true> {
        MatchingListenerBuilder {
            publisher: self.publisher,
            handler: self.handler,
        }
    }
}

#[zenoh_macros::unstable]
impl<Handler> Resolvable for MatchingListenerBuilder<'_, '_, Handler>
where
    Handler: IntoHandler<MatchingStatus> + Send,
    Handler::Handler: Send,
{
    type To = ZResult<MatchingListener<Handler::Handler>>;
}

#[zenoh_macros::unstable]
impl<Handler> Wait for MatchingListenerBuilder<'_, '_, Handler>
where
    Handler: IntoHandler<MatchingStatus> + Send,
    Handler::Handler: Send,
{
    #[zenoh_macros::unstable]
    fn wait(self) -> <Self as Resolvable>::To {
        let (callback, handler) = self.handler.into_handler();
        let state = self
            .publisher
            .session
            .declare_matches_listener_inner(self.publisher, callback)?;
        zlock!(self.publisher.matching_listeners).insert(state.id);
        Ok(MatchingListener {
            inner: MatchingListenerInner {
                session: self.publisher.session.clone(),
                matching_listeners: self.publisher.matching_listeners.clone(),
                id: state.id,
                undeclare_on_drop: true,
            },
            handler,
        })
    }
}

#[zenoh_macros::unstable]
impl<Handler> IntoFuture for MatchingListenerBuilder<'_, '_, Handler>
where
    Handler: IntoHandler<MatchingStatus> + Send,
    Handler::Handler: Send,
{
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    #[zenoh_macros::unstable]
    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

#[zenoh_macros::unstable]
impl Resolvable for MatchingListenerBuilder<'_, '_, Callback<MatchingStatus>, true> {
    type To = ZResult<()>;
}

#[zenoh_macros::unstable]
impl Wait for MatchingListenerBuilder<'_, '_, Callback<MatchingStatus>, true> {
    #[zenoh_macros::unstable]
    fn wait(self) -> <Self as Resolvable>::To {
        let state = self
            .publisher
            .session
            .declare_matches_listener_inner(self.publisher, self.handler)?;
        zlock!(self.publisher.matching_listeners).insert(state.id);
        Ok(())
    }
}

#[zenoh_macros::unstable]
impl IntoFuture for MatchingListenerBuilder<'_, '_, Callback<MatchingStatus>, true> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    #[zenoh_macros::unstable]
    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}
