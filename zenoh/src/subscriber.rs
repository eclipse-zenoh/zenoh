//
// Copyright (c) 2022 ZettaScale Technology
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
use crate::prelude::{
    locked, Callback, DefaultHandler, Id, IntoCallbackReceiverPair, KeyExpr, Sample,
};
use crate::Undeclarable;
use crate::{Result as ZResult, SessionRef};
use std::fmt;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use zenoh_core::{AsyncResolve, Resolvable, Resolve, SyncResolve};
use zenoh_protocol_core::SubInfo;

/// The subscription mode.
pub use zenoh_protocol_core::SubMode;

/// The kind of reliability.
pub use zenoh_protocol_core::Reliability;

pub(crate) struct SubscriberState {
    pub(crate) id: Id,
    pub(crate) key_expr: KeyExpr<'static>,
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
/// with the [`declare_subscriber`](crate::Session::declare_subscriber) function
/// and the [`callback`](SubscriberBuilder::callback) function
/// of the resulting builder.
///
/// Subscribers are automatically undeclared when dropped.
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// use zenoh::prelude::*;
/// use r#async::AsyncResolve;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// let subscriber = session
///     .declare_subscriber("key/expression")
///     .callback(|sample| { println!("Received : {} {}", sample.key_expr, sample.value); })
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

/// A [`PullMode`] subscriber that provides data through a callback.
///
/// CallbackPullSubscribers only provide data when explicitely pulled by the
/// application with the [`pull`](CallbackPullSubscriber::pull) function.
/// CallbackPullSubscribers can be created from a zenoh [`Session`](crate::Session)
/// with the [`declare_subscriber`](crate::Session::declare_subscriber) function,
/// the [`callback`](SubscriberBuilder::callback) function
/// and the [`pull_mode`](SubscriberBuilder::pull_mode) function
/// of the resulting builder.
///
/// Subscribers are automatically undeclared when dropped.
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// use zenoh::prelude::*;
/// use r#async::AsyncResolve;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// let subscriber = session
///     .declare_subscriber("key/expression")
///     .callback(|sample| { println!("Received : {} {}", sample.key_expr, sample.value); })
///     .pull_mode()
///     .res()
///     .await
///     .unwrap();
/// subscriber.pull();
/// # })
/// ```
pub(crate) struct PullSubscriberInner<'a> {
    inner: SubscriberInner<'a>,
}

impl<'a> PullSubscriberInner<'a> {
    /// Pull available data for a [`CallbackPullSubscriber`].
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    /// use r#async::AsyncResolve;
    /// use zenoh::subscriber::SubMode;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let subscriber = session
    ///     .declare_subscriber("key/expression")
    ///     .callback(|sample| { println!("Received : {} {}", sample.key_expr, sample.value); })
    ///     .pull_mode()
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// subscriber.pull();
    /// # })
    /// ```
    #[inline]
    pub fn pull(&self) -> impl Resolve<ZResult<()>> + '_ {
        self.inner.session.pull(&self.inner.state.key_expr)
    }
    /// Close a [`CallbackPullSubscriber`](CallbackPullSubscriber).
    ///
    /// `CallbackPullSubscribers` are automatically closed when dropped, but you may want to use this function to handle errors or
    /// close the `CallbackPullSubscriber` asynchronously.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    /// use r#async::AsyncResolve;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// # fn data_handler(_sample: Sample) { };
    /// let subscriber = session
    ///     .declare_subscriber("key/expression")
    ///     .callback(data_handler)
    ///     .pull_mode()
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// subscriber.undeclare().res().await.unwrap();
    /// # })
    /// ```
    #[inline]
    pub fn undeclare(self) -> impl Resolve<ZResult<()>> + 'a {
        Undeclarable::undeclare(self.inner, ())
    }
}

impl<'l> SubscriberInner<'l> {
    /// Close a [`CallbackSubscriber`](CallbackSubscriber).
    ///
    /// `CallbackSubscribers` are automatically closed when dropped, but you may want to use this function to handle errors or
    /// close the `CallbackSubscriber` asynchronously.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    /// use r#async::AsyncResolve;
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
    pub fn undeclare(self) -> impl Resolve<ZResult<()>> + 'l {
        Undeclarable::undeclare(self, ())
    }
}

impl<'a> Undeclarable<()> for SubscriberInner<'a> {
    type Output = ZResult<()>;
    type Undeclaration = SubscriberUndeclaration<'a>;
    fn undeclare(self, _: ()) -> Self::Undeclaration {
        SubscriberUndeclaration { subscriber: self }
    }
}

/// A [`Resolvable`] returned when undeclaring a subscriber.
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// use zenoh::prelude::*;
/// use r#async::AsyncResolve;
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
pub struct SubscriberUndeclaration<'a> {
    subscriber: SubscriberInner<'a>,
}

impl Resolvable for SubscriberUndeclaration<'_> {
    type Output = ZResult<()>;
}

impl SyncResolve for SubscriberUndeclaration<'_> {
    fn res_sync(mut self) -> Self::Output {
        self.subscriber.alive = false;
        self.subscriber
            .session
            .unsubscribe(self.subscriber.state.id)
    }
}

impl AsyncResolve for SubscriberUndeclaration<'_> {
    type Future = futures::future::Ready<Self::Output>;
    fn res_async(self) -> Self::Future {
        futures::future::ready(self.res_sync())
    }
}

impl Drop for SubscriberInner<'_> {
    fn drop(&mut self) {
        if self.alive {
            let _ = self.session.unsubscribe(self.state.id);
        }
    }
}

/// The mode for pull subscribers.
#[non_exhaustive]
#[derive(Debug, Clone, Copy)]
pub struct PullMode;

impl From<PullMode> for SubMode {
    fn from(_: PullMode) -> Self {
        SubMode::Pull
    }
}

/// The mode for push subscribers.
#[non_exhaustive]
#[derive(Debug, Clone, Copy)]
pub struct PushMode;

impl From<PushMode> for SubMode {
    fn from(_: PushMode) -> Self {
        SubMode::Push
    }
}

/// A builder for initializing a [`FlumeSubscriber`].
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// use zenoh::prelude::*;
/// use r#async::AsyncResolve;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// let subscriber = session
///     .declare_subscriber("key/expression")
///     .best_effort()
///     .pull_mode()
///     .res()
///     .await
///     .unwrap();
/// # })
/// ```
#[derive(Debug)]
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
pub struct SubscriberBuilder<'a, 'b, Mode, Handler> {
    pub(crate) session: SessionRef<'a>,
    pub(crate) key_expr: ZResult<KeyExpr<'b>>,
    pub(crate) reliability: Reliability,
    pub(crate) mode: Mode,
    pub(crate) local: bool,
    pub(crate) handler: Handler,
}

impl<'a, 'b, Mode> SubscriberBuilder<'a, 'b, Mode, DefaultHandler> {
    /// Receive the samples for this subscription with a callback.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    /// use r#async::AsyncResolve;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let subscriber = session
    ///     .declare_subscriber("key/expression")
    ///     .callback(|sample| { println!("Received : {} {}", sample.key_expr, sample.value); })
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
            local,
            handler: _,
        } = self;
        SubscriberBuilder {
            session,
            key_expr,
            reliability,
            mode,
            local,
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
    /// use zenoh::prelude::*;
    /// use r#async::AsyncResolve;
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
    /// use zenoh::prelude::*;
    /// use r#async::AsyncResolve;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let subscriber = session
    ///     .declare_subscriber("key/expression")
    ///     .with(flume::bounded(32))
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// while let Ok(sample) = subscriber.recv_async().await {
    ///     println!("Received : {} {}", sample.key_expr, sample.value);
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
            local,
            handler: _,
        } = self;
        SubscriberBuilder {
            session,
            key_expr,
            reliability,
            mode,
            local,
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

    /// Make the subscription local only.
    #[inline]
    pub fn local(mut self) -> Self {
        self.local = true;
        self
    }
}

impl<'a, 'b, Handler> SubscriberBuilder<'a, 'b, PushMode, Handler> {
    /// Change the subscription mode to Pull.
    #[inline]
    pub fn pull_mode(self) -> SubscriberBuilder<'a, 'b, PullMode, Handler> {
        let SubscriberBuilder {
            session,
            key_expr,
            reliability,
            mode: _,
            local,
            handler,
        } = self;
        SubscriberBuilder {
            session,
            key_expr,
            reliability,
            mode: PullMode,
            local,
            handler,
        }
    }
}
impl<'a, 'b, Handler> SubscriberBuilder<'a, 'b, PullMode, Handler> {
    /// Change the subscription mode to Push.
    #[inline]
    pub fn push_mode(self) -> SubscriberBuilder<'a, 'b, PushMode, Handler> {
        let SubscriberBuilder {
            session,
            key_expr,
            reliability,
            mode: _,
            local,
            handler,
        } = self;
        SubscriberBuilder {
            session,
            key_expr,
            reliability,
            mode: PushMode,
            local,
            handler,
        }
    }
}

impl<'a, Handler: IntoCallbackReceiverPair<'static, Sample>> Resolvable
    for SubscriberBuilder<'a, '_, PushMode, Handler>
{
    type Output = ZResult<Subscriber<'a, Handler::Receiver>>;
}

impl<'a, Handler: IntoCallbackReceiverPair<'static, Sample>> Resolvable
    for SubscriberBuilder<'a, '_, PullMode, Handler>
{
    type Output = ZResult<PullSubscriber<'a, Handler::Receiver>>;
}

impl<'a, Handler: IntoCallbackReceiverPair<'static, Sample>> SyncResolve
    for SubscriberBuilder<'a, '_, PushMode, Handler>
where
    Handler::Receiver: Send,
{
    fn res_sync(self) -> Self::Output {
        let (callback, receiver) = self.handler.into_cb_receiver_pair();
        let key_expr = self.key_expr?;
        let session = self.session;
        let inner = if self.local {
            session
                .declare_local_subscriber(&key_expr, Box::new(callback))
                .map(|sub_state| SubscriberInner {
                    session,
                    state: sub_state,
                    alive: true,
                })
        } else {
            session
                .declare_subscriber_inner(
                    &key_expr,
                    Box::new(callback),
                    &SubInfo {
                        reliability: self.reliability,
                        mode: self.mode.into(),
                    },
                )
                .map(|sub_state| SubscriberInner {
                    session,
                    state: sub_state,
                    alive: true,
                })
        };
        Ok(Subscriber {
            subscriber: inner?,
            receiver,
        })
    }
}
impl<'a, Handler: IntoCallbackReceiverPair<'static, Sample>> AsyncResolve
    for SubscriberBuilder<'a, '_, PushMode, Handler>
where
    Handler::Receiver: Send,
{
    type Future = futures::future::Ready<Self::Output>;
    fn res_async(self) -> Self::Future {
        futures::future::ready(self.res_sync())
    }
}
impl<'a, Handler: IntoCallbackReceiverPair<'static, Sample>> SyncResolve
    for SubscriberBuilder<'a, '_, PullMode, Handler>
where
    Handler::Receiver: Send,
{
    fn res_sync(self) -> Self::Output {
        let (callback, receiver) = self.handler.into_cb_receiver_pair();
        let key_expr = self.key_expr?;
        let session = self.session;
        let inner = if self.local {
            session
                .declare_local_subscriber(&key_expr, Box::new(callback))
                .map(|sub_state| PullSubscriberInner {
                    inner: SubscriberInner {
                        session,
                        state: sub_state,
                        alive: true,
                    },
                })
        } else {
            session
                .declare_subscriber_inner(
                    &key_expr,
                    Box::new(callback),
                    &SubInfo {
                        reliability: self.reliability,
                        mode: self.mode.into(),
                    },
                )
                .map(|sub_state| PullSubscriberInner {
                    inner: SubscriberInner {
                        session,
                        state: sub_state,
                        alive: true,
                    },
                })
        };
        Ok(PullSubscriber {
            subscriber: inner?,
            receiver,
        })
    }
}
impl<'a, Handler: IntoCallbackReceiverPair<'static, Sample>> AsyncResolve
    for SubscriberBuilder<'a, '_, PullMode, Handler>
where
    Handler::Receiver: Send,
{
    type Future = futures::future::Ready<Self::Output>;
    fn res_async(self) -> Self::Future {
        futures::future::ready(self.res_sync())
    }
}

/// A subscriber that provides data through a [`Handler`](crate::prelude::IntoCallbackReceiverPair).
///
/// Subscribers can be created from a zenoh [`Session`](crate::Session)
/// with the [`declare_subscriber`](crate::Session::declare_subscriber) function
/// and the [`with`](SubscriberBuilder::with) function
/// of the resulting builder.
///
/// Subscribers are automatically undeclared when dropped.
///
/// # Examples
/// ```no_run
/// # async_std::task::block_on(async {
/// use zenoh::prelude::*;
/// use r#async::AsyncResolve;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// let subscriber = session
///     .declare_subscriber("key/expression")
///     .with(flume::bounded(32))
///     .res()
///     .await
///     .unwrap();
/// while let Ok(sample) = subscriber.recv_async().await {
///     println!("Received : {} {}", sample.key_expr, sample.value);
/// }
/// # })
/// ```
#[non_exhaustive]
#[derive(Debug)]
pub struct Subscriber<'a, Receiver> {
    pub(crate) subscriber: SubscriberInner<'a>,
    pub receiver: Receiver,
}

/// A [`PullMode`] subscriber that provides data through a [`Handler`](crate::prelude::IntoCallbackReceiverPair).
///
/// PullSubscribers only provide data when explicitely pulled by the
/// application with the [`pull`](PullSubscriber::pull) function.
/// PullSubscribers can be created from a zenoh [`Session`](crate::Session)
/// with the [`declare_subscriber`](crate::Session::declare_subscriber) function,
/// the [`with`](SubscriberBuilder::with) function
/// and the [`pull_mode`](SubscriberBuilder::pull_mode) function
/// of the resulting builder.
///
/// Subscribers are automatically undeclared when dropped.
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// use zenoh::prelude::*;
/// use r#async::AsyncResolve;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// let subscriber = session
///     .declare_subscriber("key/expression")
///     .with(flume::bounded(32))
///     .pull_mode()
///     .res()
///     .await
///     .unwrap();
/// subscriber.pull();
/// # })
/// ```
#[non_exhaustive]
pub struct PullSubscriber<'a, Receiver> {
    pub(crate) subscriber: PullSubscriberInner<'a>,
    pub receiver: Receiver,
}

impl<'a, Receiver> Deref for PullSubscriber<'a, Receiver> {
    type Target = Receiver;
    fn deref(&self) -> &Self::Target {
        &self.receiver
    }
}

impl<'a, Receiver> DerefMut for PullSubscriber<'a, Receiver> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.receiver
    }
}

impl<'a, Receiver> PullSubscriber<'a, Receiver> {
    /// Pull available data for a [`PullSubscriber`].
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    /// use r#async::AsyncResolve;
    /// use zenoh::subscriber::SubMode;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let subscriber = session
    ///     .declare_subscriber("key/expression")
    ///     .with(flume::bounded(32))
    ///     .pull_mode()
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// subscriber.pull();
    /// # })
    /// ```
    #[inline]
    pub fn pull(&self) -> impl Resolve<ZResult<()>> + '_ {
        self.subscriber.pull()
    }
    /// Close a [`PullSubscriber`].
    ///
    /// Subscribers are automatically closed when dropped, but you may want to use this function to handle errors or
    /// close the Subscriber asynchronously.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    /// use r#async::AsyncResolve;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let subscriber = session.declare_subscriber("key/expression").pull_mode().res().await.unwrap();
    /// subscriber.undeclare().res().await.unwrap();
    /// # })
    /// ```
    #[inline]
    pub fn undeclare(self) -> impl Resolve<ZResult<()>> + 'a {
        self.subscriber.undeclare()
    }
}

impl<'a, Receiver> Subscriber<'a, Receiver> {
    /// Close a [`Subscriber`].
    ///
    /// Subscribers are automatically closed when dropped, but you may want to use this function to handle errors or
    /// close the Subscriber asynchronously.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    /// use r#async::AsyncResolve;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let subscriber = session.declare_subscriber("key/expression").res().await.unwrap();
    /// subscriber.undeclare().res().await.unwrap();
    /// # })
    /// ```
    #[inline]
    pub fn undeclare(self) -> impl Resolve<ZResult<()>> + 'a {
        self.subscriber.undeclare()
    }
}
impl<'a, T> Undeclarable<()> for Subscriber<'a, T> {
    type Output = ZResult<()>;
    type Undeclaration = SubscriberUndeclaration<'a>;
    fn undeclare(self, _: ()) -> Self::Undeclaration {
        Undeclarable::undeclare(self.subscriber, ())
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
