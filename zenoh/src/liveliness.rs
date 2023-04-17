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

//! Liveliness primitives.
//!
//! see [`Liveliness`](Liveliness)

use crate::query::Reply;

#[zenoh_macros::unstable]
use {
    crate::{
        handlers::locked,
        handlers::DefaultHandler,
        prelude::*,
        subscriber::{Subscriber, SubscriberInner},
        SessionRef, Undeclarable,
    },
    std::convert::TryInto,
    std::future::Ready,
    std::sync::Arc,
    std::time::Duration,
    zenoh_config::unwrap_or_default,
    zenoh_core::AsyncResolve,
    zenoh_core::Resolvable,
    zenoh_core::Result as ZResult,
    zenoh_core::SyncResolve,
    zenoh_protocol::core::SubInfo,
};

#[zenoh_macros::unstable]
pub(crate) static PREFIX_LIVELINESS: &str = crate::net::routing::PREFIX_LIVELINESS;

#[zenoh_macros::unstable]
lazy_static::lazy_static!(
    pub(crate) static ref KE_PREFIX_LIVELINESS: &'static keyexpr = unsafe { keyexpr::from_str_unchecked(PREFIX_LIVELINESS) };
);

/// A structure with functions to declare a
/// [`LivelinessToken`](LivelinessToken), query
/// existing [`LivelinessTokens`](LivelinessToken)
/// and subscribe to liveliness changes.
///
/// A [`LivelinessToken`](LivelinessToken) is a token which liveliness is tied
/// to the Zenoh [`Session`](Session) and can be monitored by remote applications.
///
/// A [`LivelinessToken`](LivelinessToken) with key `key/expression` can be
/// queried or subscribed to on key `@/liveliness/key/expression`.
///
/// The `Liveliness` structure can be obtained with the
/// [`Session::liveliness()`](Session::liveliness) function
/// of the [`Session`] struct.
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// use zenoh::prelude::r#async::*;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// let liveliness = session
///     .liveliness()
///     .declare_token("key/expression")
///     .res()
///     .await
///     .unwrap();
/// # })
/// ```
#[zenoh_macros::unstable]
pub struct Liveliness<'a> {
    pub(crate) session: SessionRef<'a>,
}

#[zenoh_macros::unstable]
impl<'a> Liveliness<'a> {
    /// Create a [`LivelinessToken`](LivelinessToken) for the given key expression.
    ///
    /// # Arguments
    ///
    /// * `key_expr` - The key expression to create the lieliness token on
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let liveliness = session
    ///     .liveliness()
    ///     .declare_token("key/expression")
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// # })
    /// ```
    #[zenoh_macros::unstable]
    pub fn declare_token<'b, TryIntoKeyExpr>(
        &self,
        key_expr: TryIntoKeyExpr,
    ) -> LivelinessTokenBuilder<'a, 'b>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_core::Error>,
    {
        LivelinessTokenBuilder {
            session: self.session.clone(),
            key_expr: TryIntoKeyExpr::try_into(key_expr).map_err(Into::into),
        }
    }

    /// Create a [`Subscriber`](Subscriber) for liveliness changes matching the given key expression.
    ///
    /// # Arguments
    ///
    /// * `key_expr` - The key expression to subscribe to
    ///
    /// # Examples
    /// ```no_run
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let subscriber = session.liveliness().declare_subscriber("key/expression").res().await.unwrap();
    /// while let Ok(sample) = subscriber.recv_async().await {
    ///     match sample.kind {
    ///         SampleKind::Put => println!("New liveliness: {}", sample.key_expr),
    ///         SampleKind::Delete => println!("Lost liveliness: {}", sample.key_expr),
    ///     }
    /// }
    /// # })
    /// ```
    #[zenoh_macros::unstable]
    pub fn declare_subscriber<'b, TryIntoKeyExpr>(
        &self,
        key_expr: TryIntoKeyExpr,
    ) -> LivelinessSubscriberBuilder<'a, 'b, DefaultHandler>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>,
    {
        LivelinessSubscriberBuilder {
            session: self.session.clone(),
            key_expr: TryIntoKeyExpr::try_into(key_expr).map_err(Into::into),
            handler: DefaultHandler,
        }
    }

    /// Query liveliness tokens with matching key expressions.
    ///
    /// # Arguments
    ///
    /// * `key_expr` - The key expression matching liveliness tokens to query
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let replies = session.liveliness().get("key/expression").res().await.unwrap();
    /// while let Ok(reply) = replies.recv_async().await {
    ///     if let Ok(sample) = reply.sample {
    ///         println!(">> Liveliness token {}", sample.key_expr);
    ///     }
    /// }
    /// # })
    /// ```
    #[zenoh_macros::unstable]
    pub fn get<'b: 'a, TryIntoKeyExpr>(
        &'a self,
        key_expr: TryIntoKeyExpr,
    ) -> LivelinessGetBuilder<'a, 'b, DefaultHandler>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>,
    {
        let key_expr = key_expr.try_into().map_err(Into::into);
        let conf = self.session.runtime.config.lock();
        LivelinessGetBuilder {
            session: &self.session,
            key_expr,
            timeout: Duration::from_millis(unwrap_or_default!(conf.queries_default_timeout())),
            handler: DefaultHandler,
        }
    }
}

/// A builder for initializing a [`LivelinessToken`](LivelinessToken).
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// use zenoh::prelude::r#async::*;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// let liveliness = session
///     .liveliness()
///     .declare_token("key/expression")
///     .res()
///     .await
///     .unwrap();
/// # })
/// ```
#[zenoh_macros::unstable]
#[derive(Debug)]
pub struct LivelinessTokenBuilder<'a, 'b> {
    pub(crate) session: SessionRef<'a>,
    pub(crate) key_expr: ZResult<KeyExpr<'b>>,
}

#[zenoh_macros::unstable]
impl<'a> Resolvable for LivelinessTokenBuilder<'a, '_> {
    type To = ZResult<LivelinessToken<'a>>;
}

#[zenoh_macros::unstable]
impl SyncResolve for LivelinessTokenBuilder<'_, '_> {
    #[inline]
    fn res_sync(self) -> <Self as Resolvable>::To {
        let session = self.session;
        let key_expr = self.key_expr?.into_owned();
        session
            .declare_liveliness_inner(&key_expr)
            .map(|tok_state| LivelinessToken {
                session,
                state: tok_state,
                alive: true,
            })
    }
}

#[zenoh_macros::unstable]
impl AsyncResolve for LivelinessTokenBuilder<'_, '_> {
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}

#[zenoh_macros::unstable]
#[derive(Debug)]
pub(crate) struct LivelinessTokenState {
    pub(crate) id: Id,
    pub(crate) key_expr: KeyExpr<'static>,
}

/// A token whose liveliness is tied to the Zenoh [`Session`](Session)
/// and can be monitored by remote applications.
///
/// A `LivelinessToken` with key `key/expression` can be queried or subscribed
/// to on key `@/liveliness/key/expression`.
///
/// A declared liveliness token will be seen as alive by any other Zenoh
/// application in the system that monitors it while the liveliness token
/// is not undeclared or dropped, while the Zenoh application that declared
/// it is alive (didn't stop or crashed) and while the Zenoh application
/// that declared the token has Zenoh connectivity with the Zenoh application
/// that monitors it.
///
/// `LivelinessTokens` are automatically undeclared when dropped.
///
/// # Examples
/// ```no_run
/// # async_std::task::block_on(async {
/// use zenoh::prelude::r#async::*;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// let liveliness = session
///     .liveliness()
///     .declare_token("key/expression")
///     .res()
///     .await
///     .unwrap();
/// # })
/// ```
#[zenoh_macros::unstable]
#[derive(Debug)]
pub struct LivelinessToken<'a> {
    pub(crate) session: SessionRef<'a>,
    pub(crate) state: Arc<LivelinessTokenState>,
    pub(crate) alive: bool,
}

/// A [`Resolvable`] returned when undeclaring a [`LivelinessToken`](LivelinessToken).
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// use zenoh::prelude::r#async::*;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// let liveliness = session
///     .liveliness()
///     .declare_token("key/expression")
///     .res()
///     .await
///     .unwrap();
///
/// liveliness.undeclare().res().await.unwrap();
/// # })
/// ```
#[zenoh_macros::unstable]
pub struct LivelinessTokenUndeclaration<'a> {
    token: LivelinessToken<'a>,
}

#[zenoh_macros::unstable]
impl Resolvable for LivelinessTokenUndeclaration<'_> {
    type To = ZResult<()>;
}

#[zenoh_macros::unstable]
impl SyncResolve for LivelinessTokenUndeclaration<'_> {
    fn res_sync(mut self) -> <Self as Resolvable>::To {
        self.token.alive = false;
        self.token.session.undeclare_liveliness(self.token.state.id)
    }
}

#[zenoh_macros::unstable]
impl<'a> AsyncResolve for LivelinessTokenUndeclaration<'a> {
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}

#[zenoh_macros::unstable]
impl<'a> LivelinessToken<'a> {
    /// Undeclare a [`LivelinessToken`](LivelinessToken).
    ///
    /// LivelinessTokens are automatically closed when dropped,
    /// but you may want to use this function to handle errors or
    /// undeclare the LivelinessToken asynchronously.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let liveliness = session
    ///     .liveliness()
    ///     .declare_token("key/expression")
    ///     .res()
    ///     .await
    ///     .unwrap();
    ///
    /// liveliness.undeclare().res().await.unwrap();
    /// # })
    /// ```
    #[inline]
    pub fn undeclare(self) -> impl Resolve<ZResult<()>> + 'a {
        Undeclarable::undeclare_inner(self, ())
    }
}

#[zenoh_macros::unstable]
impl<'a> Undeclarable<(), LivelinessTokenUndeclaration<'a>> for LivelinessToken<'a> {
    fn undeclare_inner(self, _: ()) -> LivelinessTokenUndeclaration<'a> {
        LivelinessTokenUndeclaration { token: self }
    }
}

#[zenoh_macros::unstable]
impl Drop for LivelinessToken<'_> {
    fn drop(&mut self) {
        if self.alive {
            let _ = self.session.undeclare_liveliness(self.state.id);
        }
    }
}

/// A builder for initializing a [`FlumeSubscriber`](FlumeSubscriber).
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
///     .pull_mode()
///     .res()
///     .await
///     .unwrap();
/// # })
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
#[zenoh_macros::unstable]
#[derive(Debug)]
pub struct LivelinessSubscriberBuilder<'a, 'b, Handler> {
    pub session: SessionRef<'a>,
    pub key_expr: ZResult<KeyExpr<'b>>,
    pub handler: Handler,
}

#[zenoh_macros::unstable]
impl<'a, 'b> LivelinessSubscriberBuilder<'a, 'b, DefaultHandler> {
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
    ///     .callback(|sample| { println!("Received: {} {}", sample.key_expr, sample.value); })
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// # })
    /// ```
    #[inline]
    #[zenoh_macros::unstable]
    pub fn callback<Callback>(
        self,
        callback: Callback,
    ) -> LivelinessSubscriberBuilder<'a, 'b, Callback>
    where
        Callback: Fn(Sample) + Send + Sync + 'static,
    {
        let LivelinessSubscriberBuilder {
            session,
            key_expr,
            handler: _,
        } = self;
        LivelinessSubscriberBuilder {
            session,
            key_expr,
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
    #[zenoh_macros::unstable]
    pub fn callback_mut<CallbackMut>(
        self,
        callback: CallbackMut,
    ) -> LivelinessSubscriberBuilder<'a, 'b, impl Fn(Sample) + Send + Sync + 'static>
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
    ///     println!("Received: {} {}", sample.key_expr, sample.value);
    /// }
    /// # })
    /// ```
    #[inline]
    #[zenoh_macros::unstable]
    pub fn with<Handler>(self, handler: Handler) -> LivelinessSubscriberBuilder<'a, 'b, Handler>
    where
        Handler: crate::prelude::IntoCallbackReceiverPair<'static, Sample>,
    {
        let LivelinessSubscriberBuilder {
            session,
            key_expr,
            handler: _,
        } = self;
        LivelinessSubscriberBuilder {
            session,
            key_expr,
            handler,
        }
    }
}

#[zenoh_macros::unstable]
impl<'a, Handler> Resolvable for LivelinessSubscriberBuilder<'a, '_, Handler>
where
    Handler: IntoCallbackReceiverPair<'static, Sample> + Send,
    Handler::Receiver: Send,
{
    type To = ZResult<Subscriber<'a, Handler::Receiver>>;
}

#[zenoh_macros::unstable]
impl<'a, Handler> SyncResolve for LivelinessSubscriberBuilder<'a, '_, Handler>
where
    Handler: IntoCallbackReceiverPair<'static, Sample> + Send,
    Handler::Receiver: Send,
{
    #[zenoh_macros::unstable]
    fn res_sync(self) -> <Self as Resolvable>::To {
        let key_expr = self.key_expr?;
        let session = self.session;
        let (callback, receiver) = self.handler.into_cb_receiver_pair();
        session
            .declare_subscriber_inner(
                &key_expr,
                &Some(KeyExpr::from(*KE_PREFIX_LIVELINESS)),
                Locality::default(),
                callback,
                &SubInfo::default(),
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

#[zenoh_macros::unstable]
impl<'a, Handler> AsyncResolve for LivelinessSubscriberBuilder<'a, '_, Handler>
where
    Handler: IntoCallbackReceiverPair<'static, Sample> + Send,
    Handler::Receiver: Send,
{
    type Future = Ready<Self::To>;

    #[zenoh_macros::unstable]
    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}

/// A builder for initializing a liveliness `query`.
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// # use std::convert::TryFrom;
/// use zenoh::prelude::r#async::*;
/// use zenoh::query::*;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// let tokens = session
///     .liveliness()
///     .get("key/expression")
///     .res()
///     .await
///     .unwrap();
/// while let Ok(token) = tokens.recv_async().await {
///     match token.sample {
///         Ok(sample) => println!("Alive token ('{}')", sample.key_expr.as_str(),),
///         Err(err) => println!("Received (ERROR: '{}')", String::try_from(&err).unwrap()),
///     }
/// }
/// # })
/// ```
#[derive(Debug)]
pub struct LivelinessGetBuilder<'a, 'b, Handler> {
    pub(crate) session: &'a Session,
    pub(crate) key_expr: ZResult<KeyExpr<'b>>,
    pub(crate) timeout: Duration,
    pub(crate) handler: Handler,
}

impl<'a, 'b> LivelinessGetBuilder<'a, 'b, DefaultHandler> {
    /// Receive the replies for this query with a callback.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let queryable = session
    ///     .liveliness()
    ///     .get("key/expression")
    ///     .callback(|reply| {println!("Received {:?}", reply.sample);})
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// # })
    /// ```
    #[inline]
    pub fn callback<Callback>(self, callback: Callback) -> LivelinessGetBuilder<'a, 'b, Callback>
    where
        Callback: Fn(Reply) + Send + Sync + 'static,
    {
        let LivelinessGetBuilder {
            session,
            key_expr,
            timeout,
            handler: _,
        } = self;
        LivelinessGetBuilder {
            session,
            key_expr,
            timeout,
            handler: callback,
        }
    }

    /// Receive the replies for this query with a mutable callback.
    ///
    /// Using this guarantees that your callback will never be called concurrently.
    /// If your callback is also accepted by the [`callback`](LivelinessGetBuilder::callback) method, we suggest you use it instead of `callback_mut`
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let mut n = 0;
    /// let queryable = session
    ///     .liveliness()
    ///     .get("key/expression")
    ///     .callback_mut(move |reply| {n += 1;})
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// # })
    /// ```
    #[inline]
    pub fn callback_mut<CallbackMut>(
        self,
        callback: CallbackMut,
    ) -> LivelinessGetBuilder<'a, 'b, impl Fn(Reply) + Send + Sync + 'static>
    where
        CallbackMut: FnMut(Reply) + Send + Sync + 'static,
    {
        self.callback(locked(callback))
    }

    /// Receive the replies for this query with a [`Handler`](crate::prelude::IntoCallbackReceiverPair).
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let replies = session
    ///     .liveliness()
    ///     .get("key/expression")
    ///     .with(flume::bounded(32))
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// while let Ok(reply) = replies.recv_async().await {
    ///     println!("Received {:?}", reply.sample);
    /// }
    /// # })
    /// ```
    #[inline]
    pub fn with<Handler>(self, handler: Handler) -> LivelinessGetBuilder<'a, 'b, Handler>
    where
        Handler: IntoCallbackReceiverPair<'static, Reply>,
    {
        let LivelinessGetBuilder {
            session,
            key_expr,
            timeout,
            handler: _,
        } = self;
        LivelinessGetBuilder {
            session,
            key_expr,
            timeout,
            handler,
        }
    }
}

impl<'a, 'b, Handler> LivelinessGetBuilder<'a, 'b, Handler> {
    /// Set query timeout.
    #[inline]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

impl<Handler> Resolvable for LivelinessGetBuilder<'_, '_, Handler>
where
    Handler: IntoCallbackReceiverPair<'static, Reply> + Send,
    Handler::Receiver: Send,
{
    type To = ZResult<Handler::Receiver>;
}

impl<Handler> SyncResolve for LivelinessGetBuilder<'_, '_, Handler>
where
    Handler: IntoCallbackReceiverPair<'static, Reply> + Send,
    Handler::Receiver: Send,
{
    fn res_sync(self) -> <Self as Resolvable>::To {
        let (callback, receiver) = self.handler.into_cb_receiver_pair();

        self.session
            .query(
                &self.key_expr?.into(),
                &Some(KeyExpr::from(*KE_PREFIX_LIVELINESS)),
                QueryTarget::default(),
                QueryConsolidation::default(),
                Locality::default(),
                self.timeout,
                None,
                callback,
            )
            .map(|_| receiver)
    }
}

impl<Handler> AsyncResolve for LivelinessGetBuilder<'_, '_, Handler>
where
    Handler: IntoCallbackReceiverPair<'static, Reply> + Send,
    Handler::Receiver: Send,
{
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}
