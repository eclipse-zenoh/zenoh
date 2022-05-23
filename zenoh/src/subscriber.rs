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
use crate::time::Period;
use crate::API_DATA_RECEPTION_CHANNEL_SIZE;
use crate::{Result as ZResult, SessionRef};
use std::fmt;
use std::marker::PhantomData;
use std::ops::Deref;
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
    pub(crate) key_expr_str: String,
    pub(crate) callback: Callback<Sample>,
}

impl fmt::Debug for SubscriberState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Subscriber")
            .field("id", &self.id)
            .field("key_expr", &self.key_expr_str)
            .finish()
    }
}

/// A subscriber that provides data through a callback.
///
/// Subscribers are automatically undeclared when dropped.
#[derive(Debug)]
pub struct CallbackSubscriber<'a> {
    pub(crate) session: SessionRef<'a>,
    pub(crate) state: Arc<SubscriberState>,
}

impl<'l> CallbackSubscriber<'l> {
    /// Pull available data for a pull-mode [`CallbackSubscriber`](CallbackSubscriber).
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
    ///     .subscribe("/key/expression")
    ///     .callback(|sample| { println!("Received : {} {}", sample.key_expr, sample.value); })
    ///     .mode(SubMode::Pull)
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// subscriber.pull();
    /// # })
    /// ```
    #[inline]
    pub fn pull(&self) -> impl Resolve<ZResult<()>> + '_ {
        self.session.pull(&self.state.key_expr)
    }

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
    ///     .subscribe("/key/expression")
    ///     .callback(data_handler)
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// subscriber.close().res().await.unwrap();
    /// # })
    /// ```
    #[inline]
    pub fn close(self) -> impl Resolve<ZResult<()>> + 'l {
        CallbackSubscriberClose {
            s: std::mem::ManuallyDrop::new(self),
            alive: true,
        }
    }
}
struct CallbackSubscriberClose<'a> {
    s: std::mem::ManuallyDrop<CallbackSubscriber<'a>>,
    alive: bool,
}
impl Resolvable for CallbackSubscriberClose<'_> {
    type Output = ZResult<()>;
}
impl SyncResolve for CallbackSubscriberClose<'_> {
    fn res_sync(mut self) -> Self::Output {
        self.alive = false;
        self.s.session.unsubscribe(self.s.state.id).res_sync()
    }
}
impl AsyncResolve for CallbackSubscriberClose<'_> {
    type Future = futures::future::Ready<ZResult<()>>;
    fn res_async(mut self) -> Self::Future {
        self.alive = false;
        self.s.session.unsubscribe(self.s.state.id).res_async()
    }
}
impl Drop for CallbackSubscriberClose<'_> {
    fn drop(&mut self) {
        if self.alive {
            unsafe { std::mem::ManuallyDrop::drop(&mut self.s) }
        }
    }
}
impl Drop for CallbackSubscriber<'_> {
    fn drop(&mut self) {
        let _ = self.session.unsubscribe(self.state.id).res_sync();
    }
}

#[derive(Debug, Clone)]
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
pub struct SubscriberBuilder<'a, 'b> {
    pub(crate) session: SessionRef<'a>,
    pub(crate) key_expr: KeyExpr<'b>,
    pub(crate) reliability: Reliability,
    pub(crate) mode: SubMode,
    pub(crate) period: Option<Period>,
    pub(crate) local: bool,
}

impl<'a, 'b> SubscriberBuilder<'a, 'b> {
    /// Make the built Subscriber a [`CallbackSubscriber`](CallbackSubscriber).
    #[inline]
    pub fn callback<Callback>(
        self,
        callback: Callback,
    ) -> CallbackSubscriberBuilder<'a, 'b, Callback>
    where
        Callback: Fn(Sample) + Send + Sync + 'static,
    {
        CallbackSubscriberBuilder {
            builder: self,
            callback,
        }
    }

    /// Make the built Subscriber a [`CallbackSubscriber`](CallbackSubscriber).
    #[inline]
    pub fn callback_mut<CallbackMut>(
        self,
        callback: CallbackMut,
    ) -> CallbackSubscriberBuilder<'a, 'b, impl Fn(Sample) + Send + Sync + 'static>
    where
        CallbackMut: FnMut(Sample) + Send + Sync + 'static,
    {
        self.callback(locked(callback))
    }

    /// Make the built Subscriber a [`HandlerSubscriber`](HandlerSubscriber).
    #[inline]
    pub fn with<IntoHandler, Receiver>(
        self,
        handler: IntoHandler,
    ) -> HandlerSubscriberBuilder<'a, 'b, IntoHandler, Receiver>
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

    /// Change the subscription mode.
    #[inline]
    pub fn mode(mut self, mode: SubMode) -> Self {
        self.mode = mode;
        self
    }

    /// Change the subscription mode to Push.
    #[inline]
    pub fn push_mode(mut self) -> Self {
        self.mode = SubMode::Push;
        self.period = None;
        self
    }

    /// Change the subscription mode to Pull.
    #[inline]
    pub fn pull_mode(mut self) -> Self {
        self.mode = SubMode::Pull;
        self
    }

    /// Change the subscription period.
    #[inline]
    pub fn period(mut self, period: Option<Period>) -> Self {
        self.period = period;
        self
    }

    /// Make the subscription local only.
    #[inline]
    pub fn local(mut self) -> Self {
        self.local = true;
        self
    }
}

impl<'a> Resolvable for SubscriberBuilder<'a, '_> {
    type Output = ZResult<HandlerSubscriber<'a, flume::Receiver<Sample>>>;
}

impl<'a> SyncResolve for SubscriberBuilder<'a, '_> {
    fn res_sync(self) -> Self::Output {
        HandlerSubscriberBuilder {
            builder: self,
            handler: flume::bounded(*API_DATA_RECEPTION_CHANNEL_SIZE),
            receiver: PhantomData,
        }
        .res_sync()
    }
}
impl<'a> AsyncResolve for SubscriberBuilder<'a, '_> {
    type Future = futures::future::Ready<Self::Output>;
    fn res_async(self) -> Self::Future {
        futures::future::ready(self.res_sync())
    }
}

/// A builder for initializing a [`CallbackSubscriber`](CallbackSubscriber).
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// use zenoh::prelude::*;
/// use r#async::AsyncResolve;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// let subscriber = session
///     .subscribe("/key/expression")
///     .callback(|sample| { println!("Received : {} {}", sample.key_expr, sample.value); })
///     .best_effort()
///     .pull_mode()
///     .res()
///     .await
///     .unwrap();
/// # })
/// ```
#[derive(Debug, Clone)]
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
pub struct CallbackSubscriberBuilder<'a, 'b, Callback>
where
    Callback: Fn(Sample) + Send + Sync + 'static,
{
    pub(crate) builder: SubscriberBuilder<'a, 'b>,
    pub(crate) callback: Callback,
}

impl<'a, 'b, Callback> CallbackSubscriberBuilder<'a, 'b, Callback>
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

    /// Change the subscription mode.
    #[inline]
    pub fn mode(mut self, mode: SubMode) -> Self {
        self.builder = self.builder.mode(mode);
        self
    }

    /// Change the subscription mode to Push.
    #[inline]
    pub fn push_mode(mut self) -> Self {
        self.builder = self.builder.push_mode();
        self
    }

    /// Change the subscription mode to Pull.
    #[inline]
    pub fn pull_mode(mut self) -> Self {
        self.builder = self.builder.pull_mode();
        self
    }

    /// Change the subscription period.
    #[inline]
    pub fn period(mut self, period: Option<Period>) -> Self {
        self.builder = self.builder.period(period);
        self
    }

    /// Make the subscription local onlyu.
    #[inline]
    pub fn local(mut self) -> Self {
        self.builder = self.builder.local();
        self
    }
}

impl<'a, Callback> Resolvable for CallbackSubscriberBuilder<'a, '_, Callback>
where
    Callback: Fn(Sample) + Send + Sync + 'static,
{
    type Output = ZResult<CallbackSubscriber<'a>>;
}

impl<F: Fn(Sample) + Send + Sync> SyncResolve for CallbackSubscriberBuilder<'_, '_, F> {
    fn res_sync(self) -> Self::Output {
        let session = self.builder.session;
        if self.builder.local {
            session
                .declare_local_subscriber(&self.builder.key_expr, Box::new(self.callback))
                .map(|sub_state| CallbackSubscriber {
                    session,
                    state: sub_state,
                })
        } else {
            session
                .declare_subscriber(
                    &self.builder.key_expr,
                    Box::new(self.callback),
                    &SubInfo {
                        reliability: self.builder.reliability,
                        mode: self.builder.mode,
                        period: self.builder.period,
                    },
                )
                .map(|sub_state| CallbackSubscriber {
                    session,
                    state: sub_state,
                })
        }
    }
}
impl<F: Fn(Sample) + Send + Sync> AsyncResolve for CallbackSubscriberBuilder<'_, '_, F> {
    type Future = futures::future::Ready<Self::Output>;
    fn res_async(self) -> Self::Future {
        futures::future::ready(self.res_sync())
    }
}

#[derive(Debug, Clone)]
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
pub struct HandlerSubscriberBuilder<'a, 'b, IntoHandler, Receiver>
where
    IntoHandler: crate::prelude::IntoHandler<Sample, Receiver>,
{
    pub(crate) builder: SubscriberBuilder<'a, 'b>,
    pub(crate) handler: IntoHandler,
    pub(crate) receiver: PhantomData<Receiver>,
}

impl<'a, 'b, IntoHandler, Receiver> HandlerSubscriberBuilder<'a, 'b, IntoHandler, Receiver>
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

    /// Change the subscription mode.
    #[inline]
    pub fn mode(mut self, mode: SubMode) -> Self {
        self.builder = self.builder.mode(mode);
        self
    }

    /// Change the subscription mode to Push.
    #[inline]
    pub fn push_mode(mut self) -> Self {
        self.builder = self.builder.push_mode();
        self
    }

    /// Change the subscription mode to Pull.
    #[inline]
    pub fn pull_mode(mut self) -> Self {
        self.builder = self.builder.pull_mode();
        self
    }

    /// Change the subscription period.
    #[inline]
    pub fn period(mut self, period: Option<Period>) -> Self {
        self.builder = self.builder.period(period);
        self
    }

    /// Make the subscription local onlyu.
    #[inline]
    pub fn local(mut self) -> Self {
        self.builder = self.builder.local();
        self
    }
}

#[derive(Debug)]
pub struct HandlerSubscriber<'a, Receiver> {
    pub subscriber: CallbackSubscriber<'a>,
    pub receiver: Receiver,
}

impl<'a, Receiver> HandlerSubscriber<'a, Receiver> {
    #[inline]
    pub fn pull(&self) -> impl Resolve<ZResult<()>> + '_ {
        self.subscriber.pull()
    }
    /// Close a [`HandlerSubscriber`](HandlerSubscriber) previously created with [`subscribe`](crate::Session::subscribe).
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
    /// let subscriber = session.subscribe("/key/expression").res().await.unwrap();
    /// subscriber.close().res().await.unwrap();
    /// # })
    /// ```
    #[inline]
    pub fn close(self) -> impl Resolve<ZResult<()>> + 'a {
        self.subscriber.close()
    }
}

impl<Receiver> Deref for HandlerSubscriber<'_, Receiver> {
    type Target = Receiver;

    fn deref(&self) -> &Self::Target {
        &self.receiver
    }
}

impl HandlerSubscriber<'_, flume::Receiver<Sample>> {
    pub fn forward<'selflifetime, E: 'selflifetime, S>(
        &'selflifetime mut self,
        sink: S,
    ) -> futures::stream::Forward<
        impl futures::TryStream<Ok = Sample, Error = E, Item = Result<Sample, E>> + 'selflifetime,
        S,
    >
    where
        S: futures::sink::Sink<Sample, Error = E>,
    {
        futures::StreamExt::forward(futures::StreamExt::map(self.receiver.stream(), Ok), sink)
    }
}

impl<'a, 'b, IntoHandler, Receiver> Resolvable
    for HandlerSubscriberBuilder<'a, 'b, IntoHandler, Receiver>
where
    IntoHandler: crate::prelude::IntoHandler<Sample, Receiver>,
{
    type Output = ZResult<HandlerSubscriber<'a, Receiver>>;
}

impl<'a, 'b, IntoHandler, Receiver: Send> SyncResolve
    for HandlerSubscriberBuilder<'a, 'b, IntoHandler, Receiver>
where
    IntoHandler: crate::prelude::IntoHandler<Sample, Receiver>,
{
    fn res_sync(self) -> Self::Output {
        let session = self.builder.session;
        let (callback, receiver) = self.handler.into_handler();
        let subscriber = if self.builder.local {
            session
                .declare_local_subscriber(&self.builder.key_expr, callback)
                .map(|sub_state| CallbackSubscriber {
                    session,
                    state: sub_state,
                })
        } else {
            session
                .declare_subscriber(
                    &self.builder.key_expr,
                    callback,
                    &SubInfo {
                        reliability: self.builder.reliability,
                        mode: self.builder.mode,
                        period: self.builder.period,
                    },
                )
                .map(|sub_state| CallbackSubscriber {
                    session,
                    state: sub_state,
                })
        };

        subscriber.map(|subscriber| HandlerSubscriber {
            subscriber,
            receiver,
        })
    }
}
impl<'a, 'b, IntoHandler, Receiver: Send> AsyncResolve
    for HandlerSubscriberBuilder<'a, 'b, IntoHandler, Receiver>
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

pub type FlumeSubscriber<'a> = HandlerSubscriber<'a, flume::Receiver<Sample>>;
