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

use std::{
    fmt,
    future::{IntoFuture, Ready},
    ops::{Deref, DerefMut},
    sync::Arc,
};

use tracing::error;
use zenoh_core::{Resolvable, Wait};
use zenoh_protocol::network::declare::subscriber::ext::SubscriberInfo;
use zenoh_result::ZResult;
#[cfg(feature = "unstable")]
use {
    zenoh_config::wrappers::{EntityGlobalId, ZenohId},
    zenoh_protocol::core::EntityGlobalIdProto,
};

#[cfg(feature = "unstable")]
use crate::pubsub::Reliability;
use crate::{
    api::{
        handlers::{locked, Callback, DefaultHandler, IntoHandler},
        key_expr::KeyExpr,
        sample::{Locality, Sample},
        session::{UndeclarableSealed, WeakSession},
        Id,
    },
    Session,
};

pub(crate) struct SubscriberState {
    pub(crate) id: Id,
    pub(crate) remote_id: Id,
    pub(crate) key_expr: KeyExpr<'static>,
    pub(crate) origin: Locality,
    pub(crate) callback: Callback<'static, Sample>,
}

impl fmt::Debug for SubscriberState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Subscriber")
            .field("id", &self.id)
            .field("key_expr", &self.key_expr)
            .finish()
    }
}

#[derive(Debug)]
pub(crate) struct SubscriberInner {
    #[cfg(feature = "unstable")]
    pub(crate) session_id: ZenohId,
    pub(crate) session: WeakSession,
    pub(crate) state: Arc<SubscriberState>,
    pub(crate) kind: SubscriberKind,
    // Subscriber is undeclared on drop unless its handler is a ZST, i.e. it is callback-only
    pub(crate) undeclare_on_drop: bool,
}

/// A [`Resolvable`] returned when undeclaring a subscriber.
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
///
/// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
/// let subscriber = session
///     .declare_subscriber("key/expression")
///     .await
///     .unwrap();
/// subscriber.undeclare().await.unwrap();
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
pub struct SubscriberUndeclaration<Handler>(Subscriber<Handler>);

impl<Handler> Resolvable for SubscriberUndeclaration<Handler> {
    type To = ZResult<()>;
}

impl<Handler> Wait for SubscriberUndeclaration<Handler> {
    fn wait(mut self) -> <Self as Resolvable>::To {
        self.0.undeclare_impl()
    }
}

impl<Handler> IntoFuture for SubscriberUndeclaration<Handler> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

/// A builder for initializing a [`FlumeSubscriber`].
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
///
/// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
/// let subscriber = session
///     .declare_subscriber("key/expression")
///     .best_effort()
///     .await
///     .unwrap();
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
#[derive(Debug)]
pub struct SubscriberBuilder<'a, 'b, Handler> {
    #[cfg(feature = "unstable")]
    pub session: &'a Session,
    #[cfg(not(feature = "unstable"))]
    pub(crate) session: &'a Session,

    #[cfg(feature = "unstable")]
    pub key_expr: ZResult<KeyExpr<'b>>,
    #[cfg(not(feature = "unstable"))]
    pub(crate) key_expr: ZResult<KeyExpr<'b>>,

    #[cfg(feature = "unstable")]
    pub reliability: Reliability,

    #[cfg(feature = "unstable")]
    pub origin: Locality,
    #[cfg(not(feature = "unstable"))]
    pub(crate) origin: Locality,

    #[cfg(feature = "unstable")]
    pub handler: Handler,
    #[cfg(not(feature = "unstable"))]
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
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// let subscriber = session
    ///     .declare_subscriber("key/expression")
    ///     .callback(|sample| { println!("Received: {} {:?}", sample.key_expr(), sample.payload()); })
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
    #[inline]
    pub fn callback<Callback>(self, callback: Callback) -> SubscriberBuilder<'a, 'b, Callback>
    where
        Callback: Fn(Sample) + Send + Sync + 'static,
    {
        self.with(callback)
    }

    /// Receive the samples for this subscription with a mutable callback.
    ///
    /// Using this guarantees that your callback will never be called concurrently.
    /// If your callback is also accepted by the [`callback`](SubscriberBuilder::callback) method, we suggest you use it instead of `callback_mut`
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// let mut n = 0;
    /// let subscriber = session
    ///     .declare_subscriber("key/expression")
    ///     .callback_mut(move |_sample| { n += 1; })
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
    #[inline]
    pub fn callback_mut<CallbackMut>(
        self,
        callback: CallbackMut,
    ) -> SubscriberBuilder<'a, 'b, impl Fn(Sample) + Send + Sync + 'static>
    where
        CallbackMut: FnMut(Sample) + Send + Sync + 'static,
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
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
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
        Handler: IntoHandler<'static, Sample>,
    {
        let SubscriberBuilder {
            session,
            key_expr,
            #[cfg(feature = "unstable")]
            reliability,
            origin,
            handler: _,
        } = self;
        SubscriberBuilder {
            session,
            key_expr,
            #[cfg(feature = "unstable")]
            reliability,
            origin,
            handler,
        }
    }
}

impl<'a, 'b, Handler> SubscriberBuilder<'a, 'b, Handler> {
    /// Change the subscription reliability.
    #[inline]
    #[zenoh_macros::unstable]
    pub fn reliability(mut self, reliability: Reliability) -> Self {
        self.reliability = reliability;
        self
    }

    /// Change the subscription reliability to `Reliable`.
    #[inline]
    #[zenoh_macros::unstable]
    pub fn reliable(mut self) -> Self {
        self.reliability = Reliability::Reliable;
        self
    }

    /// Change the subscription reliability to `BestEffort`.
    #[inline]
    #[zenoh_macros::unstable]
    pub fn best_effort(mut self) -> Self {
        self.reliability = Reliability::BestEffort;
        self
    }

    /// Restrict the matching publications that will be receive by this [`Subscriber`]
    /// to the ones that have the given [`Locality`](crate::prelude::Locality).
    #[zenoh_macros::unstable]
    #[inline]
    pub fn allowed_origin(mut self, origin: Locality) -> Self {
        self.origin = origin;
        self
    }
}

// Push mode
impl<'a, Handler> Resolvable for SubscriberBuilder<'a, '_, Handler>
where
    Handler: IntoHandler<'static, Sample> + Send,
    Handler::Handler: Send,
{
    type To = ZResult<Subscriber<Handler::Handler>>;
}

impl<'a, Handler> Wait for SubscriberBuilder<'a, '_, Handler>
where
    Handler: IntoHandler<'static, Sample> + Send,
    Handler::Handler: Send,
{
    fn wait(self) -> <Self as Resolvable>::To {
        let key_expr = self.key_expr?;
        let session = self.session;
        let (callback, receiver) = self.handler.into_handler();
        session
            .0
            .declare_subscriber_inner(
                &key_expr,
                self.origin,
                callback,
                #[cfg(feature = "unstable")]
                &SubscriberInfo {
                    reliability: self.reliability,
                },
                #[cfg(not(feature = "unstable"))]
                &SubscriberInfo::default(),
            )
            .map(|sub_state| Subscriber {
                inner: SubscriberInner {
                    #[cfg(feature = "unstable")]
                    session_id: session.zid(),
                    session: session.downgrade(),
                    state: sub_state,
                    kind: SubscriberKind::Subscriber,
                    undeclare_on_drop: !Handler::BACKGROUND,
                },
                handler: receiver,
            })
    }
}

impl<'a, Handler> IntoFuture for SubscriberBuilder<'a, '_, Handler>
where
    Handler: IntoHandler<'static, Sample> + Send,
    Handler::Handler: Send,
{
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

/// A subscriber that provides data through a [`Handler`](crate::handlers::IntoHandler).
///
/// Subscribers can be created from a zenoh [`Session`](crate::Session)
/// with the [`declare_subscriber`](crate::Session::declare_subscriber) function.
///
/// Callback subscribers will run in background until the session is closed,
/// or until it is undeclared.
/// On the other hand, subscribers with a handler are automatically undeclared when dropped.
///
/// # Examples
///
/// Using callback:
/// ```no_run
/// # #[tokio::main]
/// # async fn main() {
///
/// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
/// session
///     .declare_subscriber("key/expression")
///     .callback(|sample| { println!("Received: {} {:?}", sample.key_expr(), sample.payload()) })
///     .await
///     .unwrap();
/// // subscriber run in background until the session is closed
/// # }
/// ```
///
/// Using channel handler:
/// ```no_run
/// # #[tokio::main]
/// # async fn main() {
///
/// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
/// let subscriber = session
///     .declare_subscriber("key/expression")
///     .with(flume::bounded(32))
///     .await
///     .unwrap();
/// while let Ok(sample) = subscriber.recv_async().await {
///     println!("Received: {} {:?}", sample.key_expr(), sample.payload());
/// }
/// // subscriber is undeclared at the end of the scope
/// # }
/// ```
#[non_exhaustive]
#[derive(Debug)]
pub struct Subscriber<Handler> {
    pub(crate) inner: SubscriberInner,
    pub(crate) handler: Handler,
}

impl<Handler> Subscriber<Handler> {
    /// Returns the [`EntityGlobalId`] of this Subscriber.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// let subscriber = session.declare_subscriber("key/expression")
    ///     .await
    ///     .unwrap();
    /// let subscriber_id = subscriber.id();
    /// # }
    /// ```
    #[zenoh_macros::unstable]
    pub fn id(&self) -> EntityGlobalId {
        EntityGlobalIdProto {
            zid: self.inner.session_id.into(),
            eid: self.inner.state.id,
        }
        .into()
    }

    /// Returns the [`KeyExpr`] this Subscriber subscribes to.
    pub fn key_expr(&self) -> &KeyExpr<'static> {
        &self.inner.state.key_expr
    }

    /// Returns a reference to this subscriber's handler.
    /// An handler is anything that implements [`IntoHandler`].
    /// The default handler is [`DefaultHandler`].
    pub fn handler(&self) -> &Handler {
        &self.handler
    }

    /// Returns a mutable reference to this subscriber's handler.
    /// An handler is anything that implements [`IntoHandler`].
    /// The default handler is [`DefaultHandler`].
    pub fn handler_mut(&mut self) -> &mut Handler {
        &mut self.handler
    }

    /// Undeclare the [`Subscriber`].
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// let subscriber = session.declare_subscriber("key/expression")
    ///     .await
    ///     .unwrap();
    /// subscriber.undeclare().await.unwrap();
    /// # }
    /// ```
    #[inline]
    pub fn undeclare(self) -> SubscriberUndeclaration<Handler>
    where
        Handler: Send,
    {
        self.undeclare_inner(())
    }

    fn undeclare_impl(&mut self) -> ZResult<()> {
        // set the flag first to avoid double panic if this function panic
        self.inner.undeclare_on_drop = false;
        self.inner
            .session
            .undeclare_subscriber_inner(self.inner.state.id, self.inner.kind)
    }
}

impl<Handler> Drop for Subscriber<Handler> {
    fn drop(&mut self) {
        if self.inner.undeclare_on_drop {
            if let Err(error) = self.undeclare_impl() {
                error!(error);
            }
        }
    }
}

impl<Handler: Send> UndeclarableSealed<()> for Subscriber<Handler> {
    type Undeclaration = SubscriberUndeclaration<Handler>;

    fn undeclare_inner(self, _: ()) -> Self::Undeclaration {
        SubscriberUndeclaration(self)
    }
}

impl<Handler> Deref for Subscriber<Handler> {
    type Target = Handler;

    fn deref(&self) -> &Self::Target {
        self.handler()
    }
}
impl<Handler> DerefMut for Subscriber<Handler> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.handler_mut()
    }
}

/// A [`Subscriber`] that provides data through a `flume` channel.
pub type FlumeSubscriber = Subscriber<flume::Receiver<Sample>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SubscriberKind {
    Subscriber,
    LivelinessSubscriber,
}
