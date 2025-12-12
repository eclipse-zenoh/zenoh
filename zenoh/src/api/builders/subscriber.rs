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

use zenoh_core::{Resolvable, Wait};
use zenoh_result::ZResult;

use crate::{
    api::{
        handlers::{locked, Callback, DefaultHandler, IntoHandler},
        key_expr::KeyExpr,
        sample::{Locality, Sample},
        subscriber::{Subscriber, SubscriberInner, SubscriberKind},
    },
    Session,
};

/// A builder for initializing a [`Subscriber`](crate::pubsub::Subscriber).
/// Returned by the
/// [`Session::declare_subscriber`](crate::Session::declare_subscriber) method.
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
///
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let subscriber = session
///     .declare_subscriber("key/expression")
///     .await
///     .unwrap();
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
#[derive(Debug)]
pub struct SubscriberBuilder<'a, 'b, Handler, const BACKGROUND: bool = false> {
    #[cfg(feature = "internal")]
    pub session: &'a Session,
    #[cfg(not(feature = "internal"))]
    pub(crate) session: &'a Session,

    #[cfg(feature = "internal")]
    pub key_expr: ZResult<KeyExpr<'b>>,
    #[cfg(not(feature = "internal"))]
    pub(crate) key_expr: ZResult<KeyExpr<'b>>,

    #[cfg(feature = "internal")]
    pub origin: Locality,
    #[cfg(not(feature = "internal"))]
    pub(crate) origin: Locality,

    #[cfg(feature = "internal")]
    pub handler: Handler,
    #[cfg(not(feature = "internal"))]
    pub(crate) handler: Handler,
}

impl<'a, 'b> SubscriberBuilder<'a, 'b, DefaultHandler> {
    /// Receive the samples for this subscription with a callback.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let subscriber = session
    ///     .declare_subscriber("key/expression")
    ///     .callback(|sample| { println!("Received: {} {:?}", sample.key_expr(), sample.payload()); })
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
    #[inline]
    pub fn callback<F>(self, callback: F) -> SubscriberBuilder<'a, 'b, Callback<Sample>>
    where
        F: Fn(Sample) + Send + Sync + 'static,
    {
        self.with(Callback::from(callback))
    }

    /// Receive the samples for this subscription with a mutable callback.
    ///
    /// Using this guarantees that your callback will never be called concurrently.
    /// If your callback is also accepted by the [`callback`](SubscriberBuilder::callback) method, we suggest you use it instead of `callback_mut`.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let mut n = 0;
    /// let subscriber = session
    ///     .declare_subscriber("key/expression")
    ///     .callback_mut(move |_sample| { n += 1; })
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
    #[inline]
    pub fn callback_mut<F>(self, callback: F) -> SubscriberBuilder<'a, 'b, Callback<Sample>>
    where
        F: FnMut(Sample) + Send + Sync + 'static,
    {
        self.callback(locked(callback))
    }

    /// Receive the samples for this subscription with a [`Handler`](crate::handlers::IntoHandler).
    ///
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let subscriber = session
    ///     .declare_subscriber("key/expression")
    ///     .with(flume::bounded(32))
    ///     .await
    ///     .unwrap();
    /// while let Ok(sample) = subscriber.recv_async().await {
    ///     println!("Received: {} {:?}", sample.key_expr(), sample.payload());
    /// }
    /// # }
    /// ```
    #[inline]
    pub fn with<Handler>(self, handler: Handler) -> SubscriberBuilder<'a, 'b, Handler>
    where
        Handler: IntoHandler<Sample>,
    {
        let SubscriberBuilder {
            session,
            key_expr,
            origin,
            handler: _,
        } = self;
        SubscriberBuilder {
            session,
            key_expr,
            origin,
            handler,
        }
    }
}

impl<'a, 'b> SubscriberBuilder<'a, 'b, Callback<Sample>> {
    /// Make the subscriber run in the background until the session is closed.
    ///
    /// The background builder doesn't return a `Subscriber` object anymore.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// // no need to assign and keep a variable with a background subscriber
    /// session
    ///     .declare_subscriber("key/expression")
    ///     .callback(|sample| { println!("Received: {} {:?}", sample.key_expr(), sample.payload()); })
    ///     .background()
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
    pub fn background(self) -> SubscriberBuilder<'a, 'b, Callback<Sample>, true> {
        SubscriberBuilder {
            session: self.session,
            key_expr: self.key_expr,
            origin: self.origin,
            handler: self.handler,
        }
    }
}

impl<Handler, const BACKGROUND: bool> SubscriberBuilder<'_, '_, Handler, BACKGROUND> {
    /// Changes the [`Locality`](crate::sample::Locality) of received publications.
    ///
    /// Restricts the matching publications that will be received by this [`Subscriber`] to the ones
    /// that have the given [`Locality`](crate::sample::Locality).
    #[inline]
    pub fn allowed_origin(mut self, origin: Locality) -> Self {
        self.origin = origin;
        self
    }
}

impl<Handler> Resolvable for SubscriberBuilder<'_, '_, Handler>
where
    Handler: IntoHandler<Sample> + Send,
    Handler::Handler: Send,
{
    type To = ZResult<Subscriber<Handler::Handler>>;
}

impl<Handler> Wait for SubscriberBuilder<'_, '_, Handler>
where
    Handler: IntoHandler<Sample> + Send,
    Handler::Handler: Send,
{
    fn wait(self) -> <Self as Resolvable>::To {
        let key_expr = self.key_expr?;
        let session = self.session;
        let (callback, receiver) = self.handler.into_handler();
        session
            .0
            .declare_subscriber_inner(&key_expr, self.origin, callback)
            .map(|sub_state| Subscriber {
                inner: SubscriberInner {
                    session: session.downgrade(),
                    id: sub_state.id,
                    key_expr: sub_state.key_expr.clone(),
                    kind: SubscriberKind::Subscriber,
                    undeclare_on_drop: true,
                },
                handler: receiver,
            })
    }
}

impl<Handler> IntoFuture for SubscriberBuilder<'_, '_, Handler>
where
    Handler: IntoHandler<Sample> + Send,
    Handler::Handler: Send,
{
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

impl Resolvable for SubscriberBuilder<'_, '_, Callback<Sample>, true> {
    type To = ZResult<()>;
}

impl Wait for SubscriberBuilder<'_, '_, Callback<Sample>, true> {
    fn wait(self) -> <Self as Resolvable>::To {
        self.session
            .0
            .declare_subscriber_inner(&self.key_expr?, self.origin, self.handler)?;
        Ok(())
    }
}

impl IntoFuture for SubscriberBuilder<'_, '_, Callback<Sample>, true> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}
