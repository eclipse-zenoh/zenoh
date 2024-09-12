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
    convert::TryInto,
    future::{IntoFuture, Ready},
    sync::Arc,
    time::Duration,
};

use tracing::error;
use zenoh_config::unwrap_or_default;
use zenoh_core::{Resolvable, Resolve, Result as ZResult, Wait};

use super::{
    handlers::{locked, DefaultHandler, IntoHandler},
    key_expr::KeyExpr,
    query::Reply,
    sample::{Locality, Sample},
    session::{Session, UndeclarableSealed},
    subscriber::{Subscriber, SubscriberInner},
    Id,
};
use crate::api::session::WeakSession;

/// A structure with functions to declare a
/// [`LivelinessToken`](LivelinessToken), query
/// existing [`LivelinessTokens`](LivelinessToken)
/// and subscribe to liveliness changes.
///
/// A [`LivelinessToken`](LivelinessToken) is a token which liveliness is tied
/// to the Zenoh [`Session`](Session) and can be monitored by remote applications.
///
/// The `Liveliness` structure can be obtained with the
/// [`Session::liveliness()`](Session::liveliness) function
/// of the [`Session`] struct.
///
/// # Examples
/// ### Declaring a token
/// ```
/// # #[tokio::main]
/// # async fn main() {
///
/// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
/// let liveliness = session
///     .liveliness()
///     .declare_token("key/expression")
///     .await
///     .unwrap();
/// # }
/// ```
///
/// ### Querying tokens
/// ```
/// # #[tokio::main]
/// # async fn main() {
///
/// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
/// let replies = session.liveliness().get("key/**").await.unwrap();
/// while let Ok(reply) = replies.recv_async().await {
///     if let Ok(sample) = reply.result() {
///         println!(">> Liveliness token {}", sample.key_expr());
///     }
/// }
/// # }
/// ```
///
/// ### Subscribing to liveliness changes
/// ```no_run
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh::sample::SampleKind;
///
/// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
/// let subscriber = session.liveliness().declare_subscriber("key/**").await.unwrap();
/// while let Ok(sample) = subscriber.recv_async().await {
///     match sample.kind() {
///         SampleKind::Put => println!("New liveliness: {}", sample.key_expr()),
///         SampleKind::Delete => println!("Lost liveliness: {}", sample.key_expr()),
///     }
/// }
/// # }
/// ```
#[zenoh_macros::unstable]
pub struct Liveliness<'a> {
    pub(crate) session: &'a Session,
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
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// let liveliness = session
    ///     .liveliness()
    ///     .declare_token("key/expression")
    ///     .await
    ///     .unwrap();
    /// # }
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
            session: self.session,
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
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::sample::SampleKind;
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// let subscriber = session.liveliness().declare_subscriber("key/expression").await.unwrap();
    /// while let Ok(sample) = subscriber.recv_async().await {
    ///     match sample.kind() {
    ///         SampleKind::Put => println!("New liveliness: {}", sample.key_expr()),
    ///         SampleKind::Delete => println!("Lost liveliness: {}", sample.key_expr()),
    ///     }
    /// }
    /// # }
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
            session: self.session,
            key_expr: TryIntoKeyExpr::try_into(key_expr).map_err(Into::into),
            handler: DefaultHandler::default(),
            history: false,
            undeclare_on_drop: true,
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
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// let replies = session.liveliness().get("key/expression").await.unwrap();
    /// while let Ok(reply) = replies.recv_async().await {
    ///     if let Ok(sample) = reply.result() {
    ///         println!(">> Liveliness token {}", sample.key_expr());
    ///     }
    /// }
    /// # }
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
        let timeout = {
            let conf = self.session.0.runtime.config().lock();
            Duration::from_millis(unwrap_or_default!(conf.queries_default_timeout()))
        };
        LivelinessGetBuilder {
            session: self.session,
            key_expr,
            timeout,
            handler: DefaultHandler::default(),
        }
    }
}

/// A builder for initializing a [`LivelinessToken`](LivelinessToken).
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
///
/// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
/// let liveliness = session
///     .liveliness()
///     .declare_token("key/expression")
///     .await
///     .unwrap();
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
#[zenoh_macros::unstable]
#[derive(Debug)]
pub struct LivelinessTokenBuilder<'a, 'b> {
    pub(crate) session: &'a Session,
    pub(crate) key_expr: ZResult<KeyExpr<'b>>,
}

#[zenoh_macros::unstable]
impl Resolvable for LivelinessTokenBuilder<'_, '_> {
    type To = ZResult<LivelinessToken>;
}

#[zenoh_macros::unstable]
impl Wait for LivelinessTokenBuilder<'_, '_> {
    #[inline]
    fn wait(self) -> <Self as Resolvable>::To {
        let session = self.session;
        let key_expr = self.key_expr?.into_owned();
        session
            .0
            .declare_liveliness_inner(&key_expr)
            .map(|tok_state| LivelinessToken {
                session: self.session.downgrade(),
                state: tok_state,
                undeclare_on_drop: true,
            })
    }
}

#[zenoh_macros::unstable]
impl IntoFuture for LivelinessTokenBuilder<'_, '_> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
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
/// A declared liveliness token will be seen as alive by any other Zenoh
/// application in the system that monitors it while the liveliness token
/// is not undeclared or dropped, while the Zenoh application that declared
/// it is alive (didn't stop or crashed) and while the Zenoh application
/// that declared the token has Zenoh connectivity with the Zenoh application
/// that monitors it.
///
/// Liveliness tokens are automatically undeclared when dropped.
///
/// # Examples
/// ```no_run
/// # #[tokio::main]
/// # async fn main() {
///
/// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
/// let liveliness = session
///     .liveliness()
///     .declare_token("key/expression")
///     .await
///     .unwrap();
/// # }
/// ```
#[zenoh_macros::unstable]
#[derive(Debug)]
pub struct LivelinessToken {
    session: WeakSession,
    state: Arc<LivelinessTokenState>,
    undeclare_on_drop: bool,
}

/// A [`Resolvable`] returned when undeclaring a [`LivelinessToken`].
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
///
/// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
/// let liveliness = session
///     .liveliness()
///     .declare_token("key/expression")
///     .await
///     .unwrap();
///
/// liveliness.undeclare().await.unwrap();
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
#[zenoh_macros::unstable]
pub struct LivelinessTokenUndeclaration(LivelinessToken);

#[zenoh_macros::unstable]
impl Resolvable for LivelinessTokenUndeclaration {
    type To = ZResult<()>;
}

#[zenoh_macros::unstable]
impl Wait for LivelinessTokenUndeclaration {
    fn wait(mut self) -> <Self as Resolvable>::To {
        self.0.undeclare_impl()
    }
}

#[zenoh_macros::unstable]
impl IntoFuture for LivelinessTokenUndeclaration {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

#[zenoh_macros::unstable]
impl LivelinessToken {
    /// Undeclare the [`LivelinessToken`].
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// let liveliness = session
    ///     .liveliness()
    ///     .declare_token("key/expression")
    ///     .await
    ///     .unwrap();
    ///
    /// liveliness.undeclare().await.unwrap();
    /// # }
    /// ```
    #[inline]
    pub fn undeclare(self) -> impl Resolve<ZResult<()>> {
        UndeclarableSealed::undeclare_inner(self, ())
    }

    fn undeclare_impl(&mut self) -> ZResult<()> {
        // set the flag first to avoid double panic if this function panic
        self.undeclare_on_drop = false;
        self.session.undeclare_liveliness(self.state.id)
    }
}

#[zenoh_macros::unstable]
impl UndeclarableSealed<()> for LivelinessToken {
    type Undeclaration = LivelinessTokenUndeclaration;

    fn undeclare_inner(self, _: ()) -> Self::Undeclaration {
        LivelinessTokenUndeclaration(self)
    }
}

#[zenoh_macros::unstable]
impl Drop for LivelinessToken {
    fn drop(&mut self) {
        if self.undeclare_on_drop {
            if let Err(error) = self.undeclare_impl() {
                error!(error);
            }
        }
    }
}

/// A builder for initializing a liveliness [`FlumeSubscriber`](FlumeSubscriber).
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
///
/// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
/// let subscriber = session
///     .liveliness()
///     .declare_subscriber("key/expression")
///     .await
///     .unwrap();
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
#[zenoh_macros::unstable]
#[derive(Debug)]
pub struct LivelinessSubscriberBuilder<'a, 'b, Handler> {
    pub session: &'a Session,
    pub key_expr: ZResult<KeyExpr<'b>>,
    pub handler: Handler,
    pub history: bool,
    pub undeclare_on_drop: bool,
}

#[zenoh_macros::unstable]
impl<'a, 'b> LivelinessSubscriberBuilder<'a, 'b, DefaultHandler> {
    /// Receive the samples for this liveliness subscription with a callback.
    ///
    /// Liveliness subscriber will not be undeclared when dropped,
    /// with the callback running in background until the session is closed.
    ///
    /// It is in fact just a convenient shortcut for
    /// `.with(my_callback).undeclare_on_drop(false)`.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// let subscriber = session
    ///     .liveliness()
    ///     .declare_subscriber("key/expression")
    ///     .callback(|sample| { println!("Received: {} {:?}", sample.key_expr(), sample.payload()); })
    ///     .await
    ///     .unwrap();
    /// # }
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
        self.with(callback).undeclare_on_drop(false)
    }

    /// Receive the samples for this liveliness subscription with a mutable callback.
    ///
    /// Using this guarantees that your callback will never be called concurrently.
    /// If your callback is also accepted by the [`callback`](LivelinessSubscriberBuilder::callback) method, we suggest you use it instead of `callback_mut`.
    ///
    /// Liveliness subscriber will not be undeclared when dropped,
    /// with the callback running in background until the session is closed.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// let mut n = 0;
    /// let subscriber = session
    ///     .liveliness()
    ///     .declare_subscriber("key/expression")
    ///     .callback_mut(move |_sample| { n += 1; })
    ///     .await
    ///     .unwrap();
    /// # }
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

    /// Receive the samples for this liveliness subscription with a [`Handler`](crate::prelude::IntoHandler).
    ///
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// let subscriber = session
    ///     .liveliness()
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
    #[zenoh_macros::unstable]
    pub fn with<Handler>(self, handler: Handler) -> LivelinessSubscriberBuilder<'a, 'b, Handler>
    where
        Handler: crate::handlers::IntoHandler<'static, Sample>,
    {
        let LivelinessSubscriberBuilder {
            session,
            key_expr,
            handler: _,
            history,
            undeclare_on_drop,
        } = self;
        LivelinessSubscriberBuilder {
            session,
            key_expr,
            handler,
            history,
            undeclare_on_drop,
        }
    }
}

impl<Handler> LivelinessSubscriberBuilder<'_, '_, Handler> {
    /// Set whether the liveliness subscriber will be undeclared when dropped.
    ///
    /// The method is usually used in combination with a callback like in
    /// [`callback`](Self::callback) method, or a channel sender.
    /// Be careful when using it, as liveliness subscribers not undeclared will consume
    /// resources until the session is closed.
    #[inline]
    pub fn undeclare_on_drop(mut self, undeclare_on_drop: bool) -> Self {
        self.undeclare_on_drop = undeclare_on_drop;
        self
    }

    #[inline]
    #[zenoh_macros::unstable]
    pub fn history(mut self, history: bool) -> Self {
        self.history = history;
        self
    }
}

#[zenoh_macros::unstable]
impl<'a, Handler> Resolvable for LivelinessSubscriberBuilder<'a, '_, Handler>
where
    Handler: IntoHandler<'static, Sample> + Send,
    Handler::Handler: Send,
{
    type To = ZResult<Subscriber<Handler::Handler>>;
}

#[zenoh_macros::unstable]
impl<Handler> Wait for LivelinessSubscriberBuilder<'_, '_, Handler>
where
    Handler: IntoHandler<'static, Sample> + Send,
    Handler::Handler: Send,
{
    #[zenoh_macros::unstable]
    fn wait(self) -> <Self as Resolvable>::To {
        use super::subscriber::SubscriberKind;

        let key_expr = self.key_expr?;
        let session = self.session;
        let (callback, handler) = self.handler.into_handler();
        session
            .0
            .declare_liveliness_subscriber_inner(
                &key_expr,
                Locality::default(),
                self.history,
                callback,
            )
            .map(|sub_state| Subscriber {
                inner: SubscriberInner {
                    #[cfg(feature = "unstable")]
                    session_id: session.zid(),
                    session: self.session.downgrade(),
                    state: sub_state,
                    kind: SubscriberKind::LivelinessSubscriber,
                    undeclare_on_drop: self.undeclare_on_drop,
                },
                handler,
            })
    }
}

#[zenoh_macros::unstable]
impl<Handler> IntoFuture for LivelinessSubscriberBuilder<'_, '_, Handler>
where
    Handler: IntoHandler<'static, Sample> + Send,
    Handler::Handler: Send,
{
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    #[zenoh_macros::unstable]
    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

/// A builder for initializing a liveliness `query`.
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// # use std::convert::TryFrom;
///
/// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
/// let tokens = session
///     .liveliness()
///     .get("key/expression")
///     .await
///     .unwrap();
/// while let Ok(token) = tokens.recv_async().await {
///     match token.result() {
///         Ok(sample) => println!("Alive token ('{}')", sample.key_expr().as_str()),
///         Err(err) => println!("Received (ERROR: '{:?}')", err.payload()),
///     }
/// }
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
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
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// let queryable = session
    ///     .liveliness()
    ///     .get("key/expression")
    ///     .callback(|reply| { println!("Received {:?}", reply.result()); })
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
    #[inline]
    pub fn callback<Callback>(self, callback: Callback) -> LivelinessGetBuilder<'a, 'b, Callback>
    where
        Callback: Fn(Reply) + Send + Sync + 'static,
    {
        self.with(callback)
    }

    /// Receive the replies for this liveliness query with a mutable callback.
    ///
    /// Using this guarantees that your callback will never be called concurrently.
    /// If your callback is also accepted by the [`callback`](LivelinessGetBuilder::callback) method, we suggest you use it instead of `callback_mut`.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// let mut n = 0;
    /// let queryable = session
    ///     .liveliness()
    ///     .get("key/expression")
    ///     .callback_mut(move |reply| {n += 1;})
    ///     .await
    ///     .unwrap();
    /// # }
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

    /// Receive the replies for this liveliness query with a [`Handler`](crate::handlers::IntoHandler).
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// let replies = session
    ///     .liveliness()
    ///     .get("key/expression")
    ///     .with(flume::bounded(32))
    ///     .await
    ///     .unwrap();
    /// while let Ok(reply) = replies.recv_async().await {
    ///     println!("Received {:?}", reply.result());
    /// }
    /// # }
    /// ```
    #[inline]
    pub fn with<Handler>(self, handler: Handler) -> LivelinessGetBuilder<'a, 'b, Handler>
    where
        Handler: IntoHandler<'static, Reply>,
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

impl<Handler> LivelinessGetBuilder<'_, '_, Handler> {
    /// Set query timeout.
    #[inline]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

impl<Handler> Resolvable for LivelinessGetBuilder<'_, '_, Handler>
where
    Handler: IntoHandler<'static, Reply> + Send,
    Handler::Handler: Send,
{
    type To = ZResult<Handler::Handler>;
}

impl<Handler> Wait for LivelinessGetBuilder<'_, '_, Handler>
where
    Handler: IntoHandler<'static, Reply> + Send,
    Handler::Handler: Send,
{
    fn wait(self) -> <Self as Resolvable>::To {
        let (callback, receiver) = self.handler.into_handler();
        self.session
            .0
            .liveliness_query(&self.key_expr?, self.timeout, callback)
            .map(|_| receiver)
    }
}

impl<Handler> IntoFuture for LivelinessGetBuilder<'_, '_, Handler>
where
    Handler: IntoHandler<'static, Reply> + Send,
    Handler::Handler: Send,
{
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}
