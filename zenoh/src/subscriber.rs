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

//! Subscribing primitives.
use crate::handlers::{locked, Callback, DefaultHandler, IntoCallbackReceiverPair};
use crate::key_expr::KeyExpr;
use crate::prelude::Locality;
use crate::sample::Sample;
use crate::Id;
use crate::Undeclarable;
use crate::{Result as ZResult, SessionRef};
use std::fmt;
use std::future::Ready;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use zenoh_core::{AsyncResolve, Resolvable, SyncResolve};
#[cfg(feature = "unstable")]
use zenoh_protocol::core::EntityGlobalId;
use zenoh_protocol::network::declare::{subscriber::ext::SubscriberInfo, Mode};

/// The kind of reliability.
pub use zenoh_protocol::core::Reliability;

pub(crate) struct SubscriberState {
    pub(crate) id: Id,
    pub(crate) remote_id: Id,
    pub(crate) key_expr: KeyExpr<'static>,
    pub(crate) scope: Option<KeyExpr<'static>>,
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

/// A subscriber that provides data through a callback.
///
/// CallbackSubscribers can be created from a zenoh [`Session`](crate::Session)
/// with the [`declare_subscriber`](crate::SessionDeclarations::declare_subscriber) function
/// and the [`callback`](SubscriberBuilder::callback) function
/// of the resulting builder.
///
/// Subscribers are automatically undeclared when dropped.
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// use zenoh::prelude::r#async::*;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// let subscriber = session
///     .declare_subscriber("key/expression")
///     .callback(|sample| { println!("Received: {} {:?}", sample.key_expr, sample.payload) })
///     .res()
///     .await
///     .unwrap();
/// # })
/// ```
#[derive(Debug)]
pub(crate) struct SubscriberInner<'a> {
    pub(crate) session: SessionRef<'a>,
    pub(crate) state: Arc<SubscriberState>,
    pub(crate) alive: bool,
}

impl<'a> SubscriberInner<'a> {
    /// Close a [`CallbackSubscriber`](CallbackSubscriber).
    ///
    /// `CallbackSubscribers` are automatically closed when dropped, but you may want to use this function to handle errors or
    /// close the `CallbackSubscriber` asynchronously.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// # fn data_handler(_sample: Sample) { };
    /// let subscriber = session
    ///     .declare_subscriber("key/expression")
    ///     .callback(data_handler)
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// subscriber.undeclare().res().await.unwrap();
    /// # })
    /// ```
    #[inline]
    pub fn undeclare(self) -> SubscriberUndeclaration<'a> {
        Undeclarable::undeclare_inner(self, ())
    }
}

impl<'a> Undeclarable<(), SubscriberUndeclaration<'a>> for SubscriberInner<'a> {
    fn undeclare_inner(self, _: ()) -> SubscriberUndeclaration<'a> {
        SubscriberUndeclaration { subscriber: self }
    }
}

/// A [`Resolvable`] returned when undeclaring a subscriber.
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// use zenoh::prelude::r#async::*;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// let subscriber = session
///     .declare_subscriber("key/expression")
///     .res()
///     .await
///     .unwrap();
/// subscriber.undeclare().res().await.unwrap();
/// # })
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
pub struct SubscriberUndeclaration<'a> {
    subscriber: SubscriberInner<'a>,
}

impl Resolvable for SubscriberUndeclaration<'_> {
    type To = ZResult<()>;
}

impl SyncResolve for SubscriberUndeclaration<'_> {
    fn res_sync(mut self) -> <Self as Resolvable>::To {
        self.subscriber.alive = false;
        self.subscriber
            .session
            .unsubscribe(self.subscriber.state.id)
    }
}

impl AsyncResolve for SubscriberUndeclaration<'_> {
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}

impl Drop for SubscriberInner<'_> {
    fn drop(&mut self) {
        if self.alive {
            let _ = self.session.unsubscribe(self.state.id);
        }
    }
}

/// The mode for push subscribers.
#[non_exhaustive]
#[derive(Debug, Clone, Copy)]
pub struct PushMode;

impl From<PushMode> for Mode {
    fn from(_: PushMode) -> Self {
        Mode::Push
    }
}

/// A builder for initializing a [`FlumeSubscriber`].
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// use zenoh::prelude::r#async::*;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// let subscriber = session
///     .declare_subscriber("key/expression")
///     .best_effort()
///     .res()
///     .await
///     .unwrap();
/// # })
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
#[derive(Debug)]
pub struct SubscriberBuilder<'a, 'b, Mode, Handler> {
    #[cfg(feature = "unstable")]
    pub session: SessionRef<'a>,
    #[cfg(not(feature = "unstable"))]
    pub(crate) session: SessionRef<'a>,

    #[cfg(feature = "unstable")]
    pub key_expr: ZResult<KeyExpr<'b>>,
    #[cfg(not(feature = "unstable"))]
    pub(crate) key_expr: ZResult<KeyExpr<'b>>,

    #[cfg(feature = "unstable")]
    pub reliability: Reliability,
    #[cfg(not(feature = "unstable"))]
    pub(crate) reliability: Reliability,

    #[cfg(feature = "unstable")]
    pub mode: Mode,
    #[cfg(not(feature = "unstable"))]
    pub(crate) mode: Mode,

    #[cfg(feature = "unstable")]
    pub origin: Locality,
    #[cfg(not(feature = "unstable"))]
    pub(crate) origin: Locality,

    #[cfg(feature = "unstable")]
    pub handler: Handler,
    #[cfg(not(feature = "unstable"))]
    pub(crate) handler: Handler,
}

impl<'a, 'b, Mode> SubscriberBuilder<'a, 'b, Mode, DefaultHandler> {
    /// Receive the samples for this subscription with a callback.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let subscriber = session
    ///     .declare_subscriber("key/expression")
    ///     .callback(|sample| { println!("Received: {} {:?}", sample.key_expr, sample.payload); })
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// # })
    /// ```
    #[inline]
    pub fn callback<Callback>(self, callback: Callback) -> SubscriberBuilder<'a, 'b, Mode, Callback>
    where
        Callback: Fn(Sample) + Send + Sync + 'static,
    {
        let SubscriberBuilder {
            session,
            key_expr,
            reliability,
            mode,
            origin,
            handler: _,
        } = self;
        SubscriberBuilder {
            session,
            key_expr,
            reliability,
            mode,
            origin,
            handler: callback,
        }
    }

    /// Receive the samples for this subscription with a mutable callback.
    ///
    /// Using this guarantees that your callback will never be called concurrently.
    /// If your callback is also accepted by the [`callback`](SubscriberBuilder::callback) method, we suggest you use it instead of `callback_mut`
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let mut n = 0;
    /// let subscriber = session
    ///     .declare_subscriber("key/expression")
    ///     .callback_mut(move |_sample| { n += 1; })
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// # })
    /// ```
    #[inline]
    pub fn callback_mut<CallbackMut>(
        self,
        callback: CallbackMut,
    ) -> SubscriberBuilder<'a, 'b, Mode, impl Fn(Sample) + Send + Sync + 'static>
    where
        CallbackMut: FnMut(Sample) + Send + Sync + 'static,
    {
        self.callback(locked(callback))
    }

    /// Receive the samples for this subscription with a [`Handler`](crate::prelude::IntoCallbackReceiverPair).
    ///
    /// # Examples
    /// ```no_run
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let subscriber = session
    ///     .declare_subscriber("key/expression")
    ///     .with(flume::bounded(32))
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// while let Ok(sample) = subscriber.recv_async().await {
    ///     println!("Received: {} {:?}", sample.key_expr, sample.payload);
    /// }
    /// # })
    /// ```
    #[inline]
    pub fn with<Handler>(self, handler: Handler) -> SubscriberBuilder<'a, 'b, Mode, Handler>
    where
        Handler: crate::prelude::IntoCallbackReceiverPair<'static, Sample>,
    {
        let SubscriberBuilder {
            session,
            key_expr,
            reliability,
            mode,
            origin,
            handler: _,
        } = self;
        SubscriberBuilder {
            session,
            key_expr,
            reliability,
            mode,
            origin,
            handler,
        }
    }
}
impl<'a, 'b, Mode, Handler> SubscriberBuilder<'a, 'b, Mode, Handler> {
    /// Change the subscription reliability.
    #[inline]
    pub fn reliability(mut self, reliability: Reliability) -> Self {
        self.reliability = reliability;
        self
    }

    /// Change the subscription reliability to `Reliable`.
    #[inline]
    pub fn reliable(mut self) -> Self {
        self.reliability = Reliability::Reliable;
        self
    }

    /// Change the subscription reliability to `BestEffort`.
    #[inline]
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

    /// Change the subscription mode to Push.
    #[inline]
    pub fn push_mode(self) -> SubscriberBuilder<'a, 'b, PushMode, Handler> {
        let SubscriberBuilder {
            session,
            key_expr,
            reliability,
            mode: _,
            origin,
            handler,
        } = self;
        SubscriberBuilder {
            session,
            key_expr,
            reliability,
            mode: PushMode,
            origin,
            handler,
        }
    }
}

// Push mode
impl<'a, Handler> Resolvable for SubscriberBuilder<'a, '_, PushMode, Handler>
where
    Handler: IntoCallbackReceiverPair<'static, Sample> + Send,
    Handler::Receiver: Send,
{
    type To = ZResult<Subscriber<'a, Handler::Receiver>>;
}

impl<'a, Handler> SyncResolve for SubscriberBuilder<'a, '_, PushMode, Handler>
where
    Handler: IntoCallbackReceiverPair<'static, Sample> + Send,
    Handler::Receiver: Send,
{
    fn res_sync(self) -> <Self as Resolvable>::To {
        let key_expr = self.key_expr?;
        let session = self.session;
        let (callback, receiver) = self.handler.into_cb_receiver_pair();
        session
            .declare_subscriber_inner(
                &key_expr,
                &None,
                self.origin,
                callback,
                &SubscriberInfo {
                    reliability: self.reliability,
                    mode: self.mode.into(),
                },
            )
            .map(|sub_state| Subscriber {
                subscriber: SubscriberInner {
                    session,
                    state: sub_state,
                    alive: true,
                },
                receiver,
            })
    }
}

impl<'a, Handler> AsyncResolve for SubscriberBuilder<'a, '_, PushMode, Handler>
where
    Handler: IntoCallbackReceiverPair<'static, Sample> + Send,
    Handler::Receiver: Send,
{
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}

/// A subscriber that provides data through a [`Handler`](crate::prelude::IntoCallbackReceiverPair).
///
/// Subscribers can be created from a zenoh [`Session`](crate::Session)
/// with the [`declare_subscriber`](crate::SessionDeclarations::declare_subscriber) function
/// and the [`with`](SubscriberBuilder::with) function
/// of the resulting builder.
///
/// Subscribers are automatically undeclared when dropped.
///
/// # Examples
/// ```no_run
/// # async_std::task::block_on(async {
/// use zenoh::prelude::r#async::*;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// let subscriber = session
///     .declare_subscriber("key/expression")
///     .with(flume::bounded(32))
///     .res()
///     .await
///     .unwrap();
/// while let Ok(sample) = subscriber.recv_async().await {
///     println!("Received: {} {:?}", sample.key_expr, sample.payload);
/// }
/// # })
/// ```
#[non_exhaustive]
#[derive(Debug)]
pub struct Subscriber<'a, Receiver> {
    pub(crate) subscriber: SubscriberInner<'a>,
    pub receiver: Receiver,
}

impl<'a, Receiver> Subscriber<'a, Receiver> {
    /// Returns the [`EntityGlobalId`] of this Subscriber.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let subscriber = session.declare_subscriber("key/expression")
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// let subscriber_id = subscriber.id();
    /// # })
    /// ```
    #[zenoh_macros::unstable]
    pub fn id(&self) -> EntityGlobalId {
        EntityGlobalId {
            zid: self.subscriber.session.zid(),
            eid: self.subscriber.state.id,
        }
    }

    /// Returns the [`KeyExpr`] this Subscriber subscribes to.
    pub fn key_expr(&self) -> &KeyExpr<'static> {
        &self.subscriber.state.key_expr
    }

    /// Close a [`Subscriber`].
    ///
    /// Subscribers are automatically closed when dropped, but you may want to use this function to handle errors or
    /// close the Subscriber asynchronously.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let subscriber = session.declare_subscriber("key/expression")
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// subscriber.undeclare().res().await.unwrap();
    /// # })
    /// ```
    #[inline]
    pub fn undeclare(self) -> SubscriberUndeclaration<'a> {
        self.subscriber.undeclare()
    }
}

impl<'a, T> Undeclarable<(), SubscriberUndeclaration<'a>> for Subscriber<'a, T> {
    fn undeclare_inner(self, _: ()) -> SubscriberUndeclaration<'a> {
        Undeclarable::undeclare_inner(self.subscriber, ())
    }
}

impl<Receiver> Deref for Subscriber<'_, Receiver> {
    type Target = Receiver;

    fn deref(&self) -> &Self::Target {
        &self.receiver
    }
}
impl<Receiver> DerefMut for Subscriber<'_, Receiver> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.receiver
    }
}

/// A [`Subscriber`] that provides data through a `flume` channel.
pub type FlumeSubscriber<'a> = Subscriber<'a, flume::Receiver<Sample>>;
