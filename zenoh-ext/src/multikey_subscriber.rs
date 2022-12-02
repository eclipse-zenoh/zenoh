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
#[zenoh_core::unstable]
use {
    crate::session_ext::SessionRef,
    std::future::Ready,
    zenoh::handlers::{locked, DefaultHandler},
    zenoh::prelude::r#async::*,
    zenoh::subscriber::{Reliability, Subscriber},
    zenoh::Result as ZResult,
    zenoh_core::{AsyncResolve, Resolvable, ResolveClosure, SyncResolve},
};

#[zenoh_core::unstable]
/// The builder of MultiKeySubscriber, allowing to configure it.
pub struct MultiKeySubscriberBuilder<'a, 'b, Handler> {
    session: SessionRef<'a>,
    key_exprs: Vec<ZResult<KeyExpr<'b>>>,
    reliability: Reliability,
    origin: Locality,
    handler: Handler,
}

#[zenoh_core::unstable]
impl<'a, 'b> MultiKeySubscriberBuilder<'a, 'b, DefaultHandler> {
    pub(crate) fn new(
        session: SessionRef<'a>,
        key_exprs: Vec<ZResult<KeyExpr<'b>>>,
    ) -> MultiKeySubscriberBuilder<'a, 'b, DefaultHandler> {
        MultiKeySubscriberBuilder {
            session,
            key_exprs,
            reliability: Reliability::default(),
            origin: Locality::default(),
            handler: DefaultHandler,
        }
    }

    /// Add callback to MultiKeySubscriber.
    #[inline]
    pub fn callback<Callback>(
        self,
        callback: Callback,
    ) -> MultiKeySubscriberBuilder<'a, 'b, Callback>
    where
        Callback: Fn(Sample) + Send + Sync + 'static,
    {
        let MultiKeySubscriberBuilder {
            session,
            key_exprs,
            reliability,
            origin,
            handler: _,
        } = self;
        MultiKeySubscriberBuilder {
            session,
            key_exprs,
            reliability,
            origin,
            handler: callback,
        }
    }

    /// Add callback to `MultiKeySubscriber`.
    ///
    /// Using this guarantees that your callback will never be called concurrently.
    /// If your callback is also accepted by the [`callback`](MultiKeySubscriberBuilder::callback) method, we suggest you use it instead of `callback_mut`
    #[inline]
    pub fn callback_mut<CallbackMut>(
        self,
        callback: CallbackMut,
    ) -> MultiKeySubscriberBuilder<'a, 'b, impl Fn(Sample) + Send + Sync + 'static>
    where
        CallbackMut: FnMut(Sample) + Send + Sync + 'static,
    {
        self.callback(locked(callback))
    }

    /// Receive the samples for this subscription with a [`Handler`](zenoh::prelude::IntoCallbackReceiverPair).
    #[inline]
    pub fn with<Handler>(self, handler: Handler) -> MultiKeySubscriberBuilder<'a, 'b, Handler>
    where
        Handler: zenoh::prelude::IntoCallbackReceiverPair<'static, Sample>,
    {
        let MultiKeySubscriberBuilder {
            session,
            key_exprs,
            reliability,
            origin,
            handler: _,
        } = self;
        MultiKeySubscriberBuilder {
            session,
            key_exprs,
            reliability,
            origin,
            handler,
        }
    }
}

#[zenoh_core::unstable]
impl<'a, 'b, Handler> MultiKeySubscriberBuilder<'a, 'b, Handler> {
    /// Change the subscription reliability.
    #[inline]
    pub fn reliability(mut self, reliability: Reliability) -> Self {
        self.reliability = reliability;
        self
    }

    /// Change the subscription reliability to Reliable.
    #[inline]
    pub fn reliable(mut self) -> Self {
        self.reliability = Reliability::Reliable;
        self
    }

    /// Change the subscription reliability to BestEffort.
    #[inline]
    pub fn best_effort(mut self) -> Self {
        self.reliability = Reliability::BestEffort;
        self
    }

    /// Restrict the matching publications that will be receive by this [`MultiKeySubscriber`]
    /// to the ones that have the given [`Locality`](zenoh::prelude::Locality).
    #[zenoh_core::unstable]
    #[inline]
    pub fn allowed_origin(mut self, origin: Locality) -> Self {
        self.origin = origin;
        self
    }

    fn with_static_keys(mut self) -> MultiKeySubscriberBuilder<'a, 'static, Handler> {
        MultiKeySubscriberBuilder {
            session: self.session,
            key_exprs: self
                .key_exprs
                .drain(..)
                .map(|r| r.map(|s| s.into_owned()))
                .collect(),
            reliability: self.reliability,
            origin: self.origin,
            handler: self.handler,
        }
    }
}

#[zenoh_core::unstable]
impl<'a, Handler> Resolvable for MultiKeySubscriberBuilder<'a, '_, Handler>
where
    Handler: IntoCallbackReceiverPair<'static, Sample>,
    Handler::Receiver: Send,
{
    type To = ZResult<MultiKeySubscriber<'a, Handler::Receiver>>;
}

#[zenoh_core::unstable]
impl<Handler> SyncResolve for MultiKeySubscriberBuilder<'_, '_, Handler>
where
    Handler: IntoCallbackReceiverPair<'static, Sample> + Send,
    Handler::Receiver: Send,
{
    fn res_sync(self) -> <Self as Resolvable>::To {
        MultiKeySubscriber::new(self.with_static_keys())
    }
}

#[zenoh_core::unstable]
impl<'a, Handler> AsyncResolve for MultiKeySubscriberBuilder<'a, '_, Handler>
where
    Handler: IntoCallbackReceiverPair<'static, Sample> + Send,
    Handler::Receiver: Send,
{
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}

#[zenoh_core::unstable]
pub struct MultiKeySubscriber<'a, Receiver> {
    subscribers: Vec<Subscriber<'a, ()>>,
    receiver: Receiver,
}

#[zenoh_core::unstable]
impl<Receiver> std::ops::Deref for MultiKeySubscriber<'_, Receiver> {
    type Target = Receiver;
    fn deref(&self) -> &Self::Target {
        &self.receiver
    }
}

#[zenoh_core::unstable]
impl<Receiver> std::ops::DerefMut for MultiKeySubscriber<'_, Receiver> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.receiver
    }
}

#[zenoh_core::unstable]
impl<'a, Receiver> MultiKeySubscriber<'a, Receiver> {
    fn new<Handler>(conf: MultiKeySubscriberBuilder<'a, 'a, Handler>) -> ZResult<Self>
    where
        Handler: IntoCallbackReceiverPair<'static, Sample, Receiver = Receiver> + Send,
    {
        let session = conf.session;
        let reliability = conf.reliability;
        let origin = conf.origin;
        let (callback, receiver) = conf.handler.into_cb_receiver_pair();
        let mut subscribers = vec![];
        for key_expr in conf.key_exprs.into_iter().flatten() {
            let callback = callback.clone();
            let subscriber = match session.clone() {
                SessionRef::Borrow(session) => session
                    .declare_subscriber(&key_expr)
                    .callback(move |s| callback(s))
                    .reliability(reliability)
                    .allowed_origin(origin)
                    .res_sync()?,
                SessionRef::Shared(session) => session
                    .declare_subscriber(&key_expr)
                    .callback(move |s| callback(s))
                    .reliability(reliability)
                    .allowed_origin(origin)
                    .res_sync()?,
            };
            subscribers.push(subscriber);
        }
        Ok(Self {
            subscribers,
            receiver,
        })
    }

    pub fn key_exprs(&self) -> impl Iterator<Item = &KeyExpr<'static>> {
        self.subscribers.iter().map(|s| s.key_expr())
    }
}

#[zenoh_core::unstable]
impl<'a, Receiver> MultiKeySubscriber<'a, Receiver>
where
    Receiver: Send + 'a,
{
    #[inline]
    pub fn undeclare(mut self) -> impl Resolve<ZResult<()>> + 'a {
        ResolveClosure::new(move || {
            for sub in self.subscribers.drain(..) {
                sub.undeclare().res_sync()?;
            }
            Ok(())
        })
    }
}
