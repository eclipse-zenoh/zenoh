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
use crate::prelude::{locked, Callback, Id, KeyExpr, Sample};
use crate::{Result as ZResult, SessionRef};
use crate::{Undeclarable, API_DATA_RECEPTION_CHANNEL_SIZE};
use std::fmt;
use std::marker::PhantomData;
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
    pub(crate) callback: Callback<Sample>,
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
pub struct CallbackSubscriber<'a> {
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
pub struct CallbackPullSubscriber<'a> {
    inner: CallbackSubscriber<'a>,
}

impl<'a> CallbackPullSubscriber<'a> {
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

impl<'l> CallbackSubscriber<'l> {
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

impl<'a> Undeclarable<()> for CallbackSubscriber<'a> {
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
    subscriber: CallbackSubscriber<'a>,
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

impl Drop for CallbackSubscriber<'_> {
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
pub struct SubscriberBuilder<'a, 'b, Mode> {
    pub(crate) session: SessionRef<'a>,
    pub(crate) key_expr: ZResult<KeyExpr<'b>>,
    pub(crate) reliability: Reliability,
    pub(crate) mode: Mode,
    pub(crate) local: bool,
}

impl<'a, 'b, Mode> SubscriberBuilder<'a, 'b, Mode> {
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
    pub fn callback<Callback>(
        self,
        callback: Callback,
    ) -> CallbackSubscriberBuilder<'a, 'b, Mode, Callback>
    where
        Callback: Fn(Sample) + Send + Sync + 'static,
    {
        CallbackSubscriberBuilder {
            builder: self,
            callback,
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
    ) -> CallbackSubscriberBuilder<'a, 'b, Mode, impl Fn(Sample) + Send + Sync + 'static>
    where
        CallbackMut: FnMut(Sample) + Send + Sync + 'static,
    {
        self.callback(locked(callback))
    }

    /// Receive the samples for this subscription with a [`Handler`](crate::prelude::Handler).
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
    pub fn with<IntoHandler, Receiver>(
        self,
        handler: IntoHandler,
    ) -> HandlerSubscriberBuilder<'a, 'b, Mode, IntoHandler, Receiver>
    where
        IntoHandler: crate::prelude::IntoHandler<Sample, Receiver>,
    {
        HandlerSubscriberBuilder {
            builder: self,
            handler,
            receiver: PhantomData,
        }
    }

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

impl<'a, 'b> SubscriberBuilder<'a, 'b, PushMode> {
    /// Change the subscription mode to Pull.
    #[inline]
    pub fn pull_mode(self) -> SubscriberBuilder<'a, 'b, PullMode> {
        let SubscriberBuilder {
            session,
            key_expr,
            reliability,
            mode: _,
            local,
        } = self;
        SubscriberBuilder {
            session,
            key_expr,
            reliability,
            mode: PullMode,
            local,
        }
    }
}
impl<'a, 'b> SubscriberBuilder<'a, 'b, PullMode> {
    /// Change the subscription mode to Push.
    #[inline]
    pub fn push_mode(self) -> SubscriberBuilder<'a, 'b, PushMode> {
        let SubscriberBuilder {
            session,
            key_expr,
            reliability,
            mode: _,
            local,
        } = self;
        SubscriberBuilder {
            session,
            key_expr,
            reliability,
            mode: PushMode,
            local,
        }
    }
}

impl<'a> Resolvable for SubscriberBuilder<'a, '_, PushMode> {
    type Output = ZResult<HandlerSubscriber<'a, flume::Receiver<Sample>>>;
}

impl<'a> Resolvable for SubscriberBuilder<'a, '_, PullMode> {
    type Output = ZResult<HandlerPullSubscriber<'a, flume::Receiver<Sample>>>;
}

impl<'a> SyncResolve for SubscriberBuilder<'a, '_, PushMode> {
    fn res_sync(self) -> Self::Output {
        HandlerSubscriberBuilder {
            builder: self,
            handler: flume::bounded(*API_DATA_RECEPTION_CHANNEL_SIZE),
            receiver: PhantomData,
        }
        .res_sync()
    }
}
impl<'a> AsyncResolve for SubscriberBuilder<'a, '_, PushMode> {
    type Future = futures::future::Ready<Self::Output>;
    fn res_async(self) -> Self::Future {
        futures::future::ready(self.res_sync())
    }
}
impl<'a> SyncResolve for SubscriberBuilder<'a, '_, PullMode> {
    fn res_sync(self) -> Self::Output {
        HandlerSubscriberBuilder {
            builder: self,
            handler: flume::bounded(*API_DATA_RECEPTION_CHANNEL_SIZE),
            receiver: PhantomData,
        }
        .res_sync()
    }
}
impl<'a> AsyncResolve for SubscriberBuilder<'a, '_, PullMode> {
    type Future = futures::future::Ready<Self::Output>;
    fn res_async(self) -> Self::Future {
        futures::future::ready(self.res_sync())
    }
}

/// A builder for initializing a [`CallbackSubscriber`].
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
///     .best_effort()
///     .pull_mode()
///     .res()
///     .await
///     .unwrap();
/// # })
/// ```
#[derive(Debug)]
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
pub struct CallbackSubscriberBuilder<'a, 'b, Mode, Callback>
where
    Callback: Fn(Sample) + Send + Sync + 'static,
{
    pub(crate) builder: SubscriberBuilder<'a, 'b, Mode>,
    pub(crate) callback: Callback,
}

impl<'a, 'b, Mode, Callback> CallbackSubscriberBuilder<'a, 'b, Mode, Callback>
where
    Callback: Fn(Sample) + Send + Sync + 'static,
{
    /// Change the subscription reliability.
    #[inline]
    pub fn reliability(mut self, reliability: Reliability) -> Self {
        self.builder = self.builder.reliability(reliability);
        self
    }

    /// Change the subscription reliability to `Reliable`.
    #[inline]
    pub fn reliable(mut self) -> Self {
        self.builder = self.builder.reliable();
        self
    }

    /// Change the subscription reliability to `BestEffort`.
    #[inline]
    pub fn best_effort(mut self) -> Self {
        self.builder = self.builder.best_effort();
        self
    }
    /// Make the subscription local onlyu.
    #[inline]
    pub fn local(mut self) -> Self {
        self.builder = self.builder.local();
        self
    }
}
impl<'a, 'b, Callback> CallbackSubscriberBuilder<'a, 'b, PushMode, Callback>
where
    Callback: Fn(Sample) + Send + Sync + 'static,
{
    /// Change the subscription mode to Pull.
    #[inline]
    pub fn pull_mode(self) -> CallbackSubscriberBuilder<'a, 'b, PullMode, Callback> {
        let CallbackSubscriberBuilder { builder, callback } = self;
        CallbackSubscriberBuilder {
            builder: builder.pull_mode(),
            callback,
        }
    }
}
impl<'a, 'b, Callback> CallbackSubscriberBuilder<'a, 'b, PullMode, Callback>
where
    Callback: Fn(Sample) + Send + Sync + 'static,
{
    /// Change the subscription mode to Push.
    #[inline]
    pub fn push_mode(self) -> CallbackSubscriberBuilder<'a, 'b, PushMode, Callback> {
        let CallbackSubscriberBuilder { builder, callback } = self;
        CallbackSubscriberBuilder {
            builder: builder.push_mode(),
            callback,
        }
    }
}

impl<'a, Callback> Resolvable for CallbackSubscriberBuilder<'a, '_, PushMode, Callback>
where
    Callback: Fn(Sample) + Send + Sync + 'static,
{
    type Output = ZResult<CallbackSubscriber<'a>>;
}
impl<'a, Callback> Resolvable for CallbackSubscriberBuilder<'a, '_, PullMode, Callback>
where
    Callback: Fn(Sample) + Send + Sync + 'static,
{
    type Output = ZResult<CallbackPullSubscriber<'a>>;
}

impl<F: Fn(Sample) + Send + Sync> SyncResolve for CallbackSubscriberBuilder<'_, '_, PushMode, F> {
    fn res_sync(self) -> Self::Output {
        let key_expr = self.builder.key_expr?;
        let session = self.builder.session;
        if self.builder.local {
            session
                .declare_local_subscriber(&key_expr, Box::new(self.callback))
                .map(|sub_state| CallbackSubscriber {
                    session,
                    state: sub_state,
                    alive: true,
                })
        } else {
            session
                .declare_subscriber_inner(
                    &key_expr,
                    Box::new(self.callback),
                    &SubInfo {
                        reliability: self.builder.reliability,
                        mode: self.builder.mode.into(),
                    },
                )
                .map(|sub_state| CallbackSubscriber {
                    session,
                    state: sub_state,
                    alive: true,
                })
        }
    }
}
impl<F: Fn(Sample) + Send + Sync> AsyncResolve for CallbackSubscriberBuilder<'_, '_, PushMode, F> {
    type Future = futures::future::Ready<Self::Output>;
    fn res_async(self) -> Self::Future {
        futures::future::ready(self.res_sync())
    }
}

impl<F: Fn(Sample) + Send + Sync> SyncResolve for CallbackSubscriberBuilder<'_, '_, PullMode, F> {
    fn res_sync(self) -> Self::Output {
        let key_expr = self.builder.key_expr?;
        let session = self.builder.session;
        if self.builder.local {
            session
                .declare_local_subscriber(&key_expr, Box::new(self.callback))
                .map(|sub_state| CallbackPullSubscriber {
                    inner: CallbackSubscriber {
                        session,
                        state: sub_state,
                        alive: true,
                    },
                })
        } else {
            session
                .declare_subscriber_inner(
                    &key_expr,
                    Box::new(self.callback),
                    &SubInfo {
                        reliability: self.builder.reliability,
                        mode: self.builder.mode.into(),
                    },
                )
                .map(|sub_state| CallbackPullSubscriber {
                    inner: CallbackSubscriber {
                        session,
                        state: sub_state,
                        alive: true,
                    },
                })
        }
    }
}
impl<F: Fn(Sample) + Send + Sync> AsyncResolve for CallbackSubscriberBuilder<'_, '_, PullMode, F> {
    type Future = futures::future::Ready<Self::Output>;
    fn res_async(self) -> Self::Future {
        futures::future::ready(self.res_sync())
    }
}

/// A builder for initializing a [`HandlerSubscriber`].
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
///     .best_effort()
///     .pull_mode()
///     .res()
///     .await
///     .unwrap();
/// # })
/// ```
#[derive(Debug)]
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
pub struct HandlerSubscriberBuilder<'a, 'b, Mode, IntoHandler, Receiver>
where
    IntoHandler: crate::prelude::IntoHandler<Sample, Receiver>,
{
    pub(crate) builder: SubscriberBuilder<'a, 'b, Mode>,
    pub(crate) handler: IntoHandler,
    pub(crate) receiver: PhantomData<Receiver>,
}

impl<'a, 'b, Mode, IntoHandler, Receiver>
    HandlerSubscriberBuilder<'a, 'b, Mode, IntoHandler, Receiver>
where
    IntoHandler: crate::prelude::IntoHandler<Sample, Receiver>,
{
    /// Change the subscription reliability.
    #[inline]
    pub fn reliability(mut self, reliability: Reliability) -> Self {
        self.builder = self.builder.reliability(reliability);
        self
    }

    /// Change the subscription reliability to `Reliable`.
    #[inline]
    pub fn reliable(mut self) -> Self {
        self.builder = self.builder.reliable();
        self
    }

    /// Change the subscription reliability to `BestEffort`.
    #[inline]
    pub fn best_effort(mut self) -> Self {
        self.builder = self.builder.best_effort();
        self
    }

    /// Make the subscription local only.
    #[inline]
    pub fn local(mut self) -> Self {
        self.builder = self.builder.local();
        self
    }
}
impl<'a, 'b, IntoHandler, Receiver>
    HandlerSubscriberBuilder<'a, 'b, PushMode, IntoHandler, Receiver>
where
    IntoHandler: crate::prelude::IntoHandler<Sample, Receiver>,
{
    /// Change the subscription mode to Pull.
    #[inline]
    pub fn pull_mode(self) -> HandlerSubscriberBuilder<'a, 'b, PullMode, IntoHandler, Receiver> {
        let HandlerSubscriberBuilder {
            builder,
            handler,
            receiver,
        } = self;
        HandlerSubscriberBuilder {
            builder: builder.pull_mode(),
            handler,
            receiver,
        }
    }
}
impl<'a, 'b, IntoHandler, Receiver>
    HandlerSubscriberBuilder<'a, 'b, PullMode, IntoHandler, Receiver>
where
    IntoHandler: crate::prelude::IntoHandler<Sample, Receiver>,
{
    /// Change the subscription mode to Pull.
    #[inline]
    pub fn push_mode(self) -> HandlerSubscriberBuilder<'a, 'b, PushMode, IntoHandler, Receiver> {
        let HandlerSubscriberBuilder {
            builder,
            handler,
            receiver,
        } = self;
        HandlerSubscriberBuilder {
            builder: builder.push_mode(),
            handler,
            receiver,
        }
    }
}

/// A subscriber that provides data through a [`Handler`](crate::prelude::Handler).
///
/// HandlerSubscribers can be created from a zenoh [`Session`](crate::Session)
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
pub struct HandlerSubscriber<'a, Receiver> {
    pub subscriber: CallbackSubscriber<'a>,
    pub receiver: Receiver,
}

/// A [`PullMode`] subscriber that provides data through a [`Handler`](crate::prelude::Handler).
///
/// HandlerPullSubscribers only provide data when explicitely pulled by the
/// application with the [`pull`](HandlerPullSubscriber::pull) function.
/// HandlerPullSubscribers can be created from a zenoh [`Session`](crate::Session)
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
pub struct HandlerPullSubscriber<'a, Receiver> {
    pub subscriber: CallbackPullSubscriber<'a>,
    pub receiver: Receiver,
}

impl<'a, Receiver> Deref for HandlerPullSubscriber<'a, Receiver> {
    type Target = Receiver;
    fn deref(&self) -> &Self::Target {
        &self.receiver
    }
}

impl<'a, Receiver> DerefMut for HandlerPullSubscriber<'a, Receiver> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.receiver
    }
}

impl<'a, Receiver> HandlerPullSubscriber<'a, Receiver> {
    /// Pull available data for a [`HandlerPullSubscriber`].
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
    /// Close a [`HandlerPullSubscriber`].
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

impl<'a, Receiver> HandlerSubscriber<'a, Receiver> {
    /// Close a [`HandlerSubscriber`].
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
impl<'a, T> Undeclarable<()> for HandlerSubscriber<'a, T> {
    type Output = <CallbackSubscriber<'a> as Undeclarable<()>>::Output;
    type Undeclaration = <CallbackSubscriber<'a> as Undeclarable<()>>::Undeclaration;
    fn undeclare(self, _: ()) -> Self::Undeclaration {
        Undeclarable::undeclare(self.subscriber, ())
    }
}

impl<Receiver> Deref for HandlerSubscriber<'_, Receiver> {
    type Target = Receiver;

    fn deref(&self) -> &Self::Target {
        &self.receiver
    }
}
impl<Receiver> DerefMut for HandlerSubscriber<'_, Receiver> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.receiver
    }
}

impl<'a, 'b, IntoHandler, Receiver> Resolvable
    for HandlerSubscriberBuilder<'a, 'b, PushMode, IntoHandler, Receiver>
where
    IntoHandler: crate::prelude::IntoHandler<Sample, Receiver>,
{
    type Output = ZResult<HandlerSubscriber<'a, Receiver>>;
}

impl<'a, 'b, IntoHandler, Receiver: Send> SyncResolve
    for HandlerSubscriberBuilder<'a, 'b, PushMode, IntoHandler, Receiver>
where
    IntoHandler: crate::prelude::IntoHandler<Sample, Receiver>,
{
    fn res_sync(self) -> Self::Output {
        let (callback, receiver) = self.handler.into_handler();
        self.builder
            .callback(callback)
            .res_sync()
            .map(|subscriber| HandlerSubscriber {
                subscriber,
                receiver,
            })
    }
}
impl<'a, 'b, IntoHandler, Receiver: Send> AsyncResolve
    for HandlerSubscriberBuilder<'a, 'b, PushMode, IntoHandler, Receiver>
where
    IntoHandler: crate::prelude::IntoHandler<Sample, Receiver>,
{
    type Future = futures::future::Ready<Self::Output>;
    fn res_async(self) -> Self::Future {
        futures::future::ready(self.res_sync())
    }
}

impl<'a, 'b, IntoHandler, Receiver> Resolvable
    for HandlerSubscriberBuilder<'a, 'b, PullMode, IntoHandler, Receiver>
where
    IntoHandler: crate::prelude::IntoHandler<Sample, Receiver>,
{
    type Output = ZResult<HandlerPullSubscriber<'a, Receiver>>;
}

impl<'a, 'b, IntoHandler, Receiver: Send> SyncResolve
    for HandlerSubscriberBuilder<'a, 'b, PullMode, IntoHandler, Receiver>
where
    IntoHandler: crate::prelude::IntoHandler<Sample, Receiver>,
{
    fn res_sync(self) -> Self::Output {
        let (callback, receiver) = self.handler.into_handler();
        self.builder
            .callback(callback)
            .res_sync()
            .map(|subscriber| HandlerPullSubscriber {
                subscriber,
                receiver,
            })
    }
}
impl<'a, 'b, IntoHandler, Receiver: Send> AsyncResolve
    for HandlerSubscriberBuilder<'a, 'b, PullMode, IntoHandler, Receiver>
where
    IntoHandler: crate::prelude::IntoHandler<Sample, Receiver>,
{
    type Future = futures::future::Ready<Self::Output>;
    fn res_async(self) -> Self::Future {
        futures::future::ready(self.res_sync())
    }
}

impl crate::prelude::IntoHandler<Sample, flume::Receiver<Sample>>
    for (flume::Sender<Sample>, flume::Receiver<Sample>)
{
    fn into_handler(self) -> crate::prelude::Handler<Sample, flume::Receiver<Sample>> {
        let (sender, receiver) = self;
        (
            Box::new(move |s| {
                if let Err(e) = sender.send(s) {
                    log::warn!("Error sending sample into flume channel: {}", e)
                }
            }),
            receiver,
        )
    }
}

/// A [`HandlerSubscriber`] that provides data through a `flume` channel.
pub type FlumeSubscriber<'a> = HandlerSubscriber<'a, flume::Receiver<Sample>>;
