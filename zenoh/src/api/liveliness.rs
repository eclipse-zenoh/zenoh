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
#[cfg(feature = "unstable")]
use zenoh_config::wrappers::EntityGlobalId;
use zenoh_core::{Resolvable, Resolve, Result as ZResult, Wait};

use crate::{
    api::{
        handlers::{locked, DefaultHandler, IntoHandler},
        key_expr::KeyExpr,
        query::Reply,
        sample::{Locality, Sample},
        session::{Session, UndeclarableSealed, WeakSession},
        subscriber::{Subscriber, SubscriberInner},
        Id,
    },
    handlers::Callback,
};

/// A structure with functions to declare a [`LivelinessToken`](LivelinessToken),
/// query existing [`LivelinessTokens`](LivelinessToken)
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
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
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
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
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
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
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
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
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
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
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
        }
    }

    /// Create a [`Querier`](LivelinessQuerier) for to perform queries on liveliness tokens matching the given key expression.
    ///
    /// # Arguments
    ///
    /// * `key_expr` - The key expression to perform queries on
    ///
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let querier = session.liveliness().declare_querier("key/expression").await.unwrap();
    /// let tokens = querier.get().await.unwrap();
    /// while let Ok(reply) = tokens.recv_async().await {
    ///     if let Ok(sample) = reply.result() {
    ///         println!(">> Liveliness token {}", sample.key_expr());
    ///     }
    /// }
    /// # }
    /// ```
    #[zenoh_macros::unstable]
    pub fn declare_querier<'b, TryIntoKeyExpr>(
        &self,
        key_expr: TryIntoKeyExpr,
    ) -> LivelinessQuerierBuilder<'a, 'b>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>,
    {
        let timeout = {
            let conf = &self.session.0.runtime.config().lock().0;
            Duration::from_millis(unwrap_or_default!(conf.queries_default_timeout()))
        };
        LivelinessQuerierBuilder {
            session: self.session,
            key_expr: TryIntoKeyExpr::try_into(key_expr).map_err(Into::into),
            timeout,
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
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let replies = session.liveliness().get("key/expression").await.unwrap();
    /// while let Ok(reply) = replies.recv_async().await {
    ///     if let Ok(sample) = reply.result() {
    ///         println!(">> Liveliness token {}", sample.key_expr());
    ///     }
    /// }
    /// # }
    /// ```
    #[zenoh_macros::unstable]
    pub fn get<'b, TryIntoKeyExpr>(
        &self,
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
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let liveliness = session
///     .liveliness()
///     .declare_token("key/expression")
///     .await
///     .unwrap();
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
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

/// A token whose liveliness is tied to the Zenoh [`Session`](Session).
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
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let liveliness = session
///     .liveliness()
///     .declare_token("key/expression")
///     .await
///     .unwrap();
/// # }
/// ```
#[zenoh_macros::unstable]
#[must_use = "Liveliness tokens will be immediately dropped and undeclared if not bound to a variable"]
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
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let liveliness = session
///     .liveliness()
///     .declare_token("key/expression")
///     .await
///     .unwrap();
///
/// liveliness.undeclare().await.unwrap();
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
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
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
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

/// A builder for initializing a liveliness.
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
///
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let subscriber = session
///     .liveliness()
///     .declare_subscriber("key/expression")
///     .await
///     .unwrap();
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
#[zenoh_macros::unstable]
#[derive(Debug)]
pub struct LivelinessSubscriberBuilder<'a, 'b, Handler, const BACKGROUND: bool = false> {
    pub session: &'a Session,
    pub key_expr: ZResult<KeyExpr<'b>>,
    pub handler: Handler,
    pub history: bool,
}

#[zenoh_macros::unstable]
impl<'a, 'b> LivelinessSubscriberBuilder<'a, 'b, DefaultHandler> {
    /// Receive the samples for this liveliness subscription with a callback.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
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
    pub fn callback<F>(self, callback: F) -> LivelinessSubscriberBuilder<'a, 'b, Callback<Sample>>
    where
        F: Fn(Sample) + Send + Sync + 'static,
    {
        self.with(Callback::new(Arc::new(callback)))
    }

    /// Receive the samples for this liveliness subscription with a mutable callback.
    ///
    /// Using this guarantees that your callback will never be called concurrently.
    /// If your callback is also accepted by the [`callback`](LivelinessSubscriberBuilder::callback) method, we suggest you use it instead of `callback_mut`.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
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
    pub fn callback_mut<F>(
        self,
        callback: F,
    ) -> LivelinessSubscriberBuilder<'a, 'b, Callback<Sample>>
    where
        F: FnMut(Sample) + Send + Sync + 'static,
    {
        self.callback(locked(callback))
    }

    /// Receive the samples for this liveliness subscription with a [`Handler`](IntoHandler).
    ///
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
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
        Handler: IntoHandler<Sample>,
    {
        let LivelinessSubscriberBuilder {
            session,
            key_expr,
            handler: _,
            history,
        } = self;
        LivelinessSubscriberBuilder {
            session,
            key_expr,
            handler,
            history,
        }
    }
}

impl<'a, 'b> LivelinessSubscriberBuilder<'a, 'b, Callback<Sample>> {
    /// Register the subscriber callback to be run in background until the session is closed.
    ///
    /// Background builder doesn't return a `Subscriber` object anymore.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// // no need to assign and keep a variable with a background subscriber
    /// session
    ///     .liveliness()
    ///     .declare_subscriber("key/expression")
    ///     .callback(|sample| { println!("Received: {} {:?}", sample.key_expr(), sample.payload()); })
    ///     .background()
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
    pub fn background(self) -> LivelinessSubscriberBuilder<'a, 'b, Callback<Sample>, true> {
        LivelinessSubscriberBuilder {
            session: self.session,
            key_expr: self.key_expr,
            handler: self.handler,
            history: self.history,
        }
    }
}

impl<Handler, const BACKGROUND: bool> LivelinessSubscriberBuilder<'_, '_, Handler, BACKGROUND> {
    #[inline]
    #[zenoh_macros::unstable]
    pub fn history(mut self, history: bool) -> Self {
        self.history = history;
        self
    }
}

#[zenoh_macros::unstable]
impl<Handler> Resolvable for LivelinessSubscriberBuilder<'_, '_, Handler>
where
    Handler: IntoHandler<Sample> + Send,
    Handler::Handler: Send,
{
    type To = ZResult<Subscriber<Handler::Handler>>;
}

#[zenoh_macros::unstable]
impl<Handler> Wait for LivelinessSubscriberBuilder<'_, '_, Handler>
where
    Handler: IntoHandler<Sample> + Send,
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
                    session: self.session.downgrade(),
                    id: sub_state.id,
                    key_expr: sub_state.key_expr.clone(),
                    kind: SubscriberKind::LivelinessSubscriber,
                    undeclare_on_drop: true,
                },
                handler,
            })
    }
}

#[zenoh_macros::unstable]
impl<Handler> IntoFuture for LivelinessSubscriberBuilder<'_, '_, Handler>
where
    Handler: IntoHandler<Sample> + Send,
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
impl Resolvable for LivelinessSubscriberBuilder<'_, '_, Callback<Sample>, true> {
    type To = ZResult<()>;
}

#[zenoh_macros::unstable]
impl Wait for LivelinessSubscriberBuilder<'_, '_, Callback<Sample>, true> {
    #[zenoh_macros::unstable]
    fn wait(self) -> <Self as Resolvable>::To {
        self.session.0.declare_liveliness_subscriber_inner(
            &self.key_expr?,
            Locality::default(),
            self.history,
            self.handler,
        )?;
        Ok(())
    }
}

#[zenoh_macros::unstable]
impl IntoFuture for LivelinessSubscriberBuilder<'_, '_, Callback<Sample>, true> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    #[zenoh_macros::unstable]
    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

/// A querier that allows to perform livleiness queries.
///
/// Queriers are automatically undeclared when dropped.
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
///
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let querier = session.liveliness().declare_querier("key/expression").await.unwrap();
/// let tokens = querier.get().await.unwrap();
/// # }
/// ```
#[derive(Debug)]
pub struct LivelinessQuerier<'a> {
    pub(crate) session: WeakSession,
    pub(crate) id: Id,
    pub(crate) key_expr: KeyExpr<'a>,
    pub(crate) timeout: Duration,
    pub(crate) undeclare_on_drop: bool,
}

/// A builder for initializing a liveliness querier.
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
///
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let querier = session
///     .liveliness()
///     .declare_querier("key/expression")
///     .await
///     .unwrap();
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
#[zenoh_macros::unstable]
#[derive(Debug)]
pub struct LivelinessQuerierBuilder<'a, 'b> {
    session: &'a Session,
    key_expr: ZResult<KeyExpr<'b>>,
    timeout: Duration,
}

impl<'a, 'b> LivelinessQuerierBuilder<'a, 'b> {
    /// Set querier's queries timeout.
    #[inline]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

#[zenoh_macros::unstable]
impl<'a, 'b> Resolvable for LivelinessQuerierBuilder<'a, 'b> {
    type To = ZResult<LivelinessQuerier<'b>>;
}

#[zenoh_macros::unstable]
impl<'a> Wait for LivelinessQuerierBuilder<'a, '_> {
    #[zenoh_macros::unstable]
    fn wait(self) -> <Self as Resolvable>::To {
        let mut key_expr = self.key_expr?;
        if !key_expr.is_fully_optimized(&self.session.0) {
            key_expr = self.session.declare_keyexpr(key_expr).wait()?;
        }
        let id = self.session.0.declare_liveliness_querier_inner(&key_expr)?;
        Ok(LivelinessQuerier {
            session: self.session.downgrade(),
            id,
            key_expr,
            timeout: self.timeout,
            undeclare_on_drop: true,
        })
    }
}

#[zenoh_macros::unstable]
impl IntoFuture for LivelinessQuerierBuilder<'_, '_> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    #[zenoh_macros::unstable]
    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

impl<'a> LivelinessQuerier<'a> {
    /// Returns the [`EntityGlobalId`] of this Querier.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let querier = session.declare_querier("key/expression")
    ///     .await
    ///     .unwrap();
    /// let querier_id = querier.id();
    /// # }
    /// ```
    #[zenoh_macros::unstable]
    pub fn id(&self) -> EntityGlobalId {
        use zenoh_protocol::core::EntityGlobalIdProto;
        EntityGlobalIdProto {
            zid: self.session.zid().into(),
            eid: self.id,
        }
        .into()
    }

    #[inline]
    pub fn key_expr(&self) -> &KeyExpr<'a> {
        &self.key_expr
    }

    /// Perform a liveliness a query.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let querier = session.liveliness().declare_querier("key/expression").await.unwrap();
    /// let tokens = querier.get();
    /// # }
    /// ```
    #[inline]
    pub fn get(&self) -> LivelinessQuerierGetBuilder<'_, DefaultHandler> {
        LivelinessQuerierGetBuilder {
            querier: self,
            handler: DefaultHandler::default(),
        }
    }

    /// Undeclare the [`LivelinessQuerier`], informing the network that it needn't optimize queries for its key expression anymore.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let querier = session.liveliness().declare_querier("key/expression").await.unwrap();
    /// querier.undeclare().await.unwrap();
    /// # }
    /// ```
    pub fn undeclare(self) -> impl Resolve<ZResult<()>> + 'a {
        UndeclarableSealed::undeclare_inner(self, ())
    }

    fn undeclare_impl(&mut self) -> ZResult<()> {
        // set the flag first to avoid double panic if this function panic
        self.undeclare_on_drop = false;
        self.session.undeclare_liveliness_querier_inner(self.id)
    }
}

impl<'a> UndeclarableSealed<()> for LivelinessQuerier<'a> {
    type Undeclaration = LivelinessQuerierUndeclaration<'a>;

    fn undeclare_inner(self, _: ()) -> Self::Undeclaration {
        LivelinessQuerierUndeclaration(self)
    }
}

/// A [`Resolvable`] returned when undeclaring a publisher.
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
///
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let querier = session.declare_querier("key/expression").await.unwrap();
/// querier.undeclare().await.unwrap();
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
pub struct LivelinessQuerierUndeclaration<'a>(LivelinessQuerier<'a>);

impl Resolvable for LivelinessQuerierUndeclaration<'_> {
    type To = ZResult<()>;
}

impl Wait for LivelinessQuerierUndeclaration<'_> {
    fn wait(mut self) -> <Self as Resolvable>::To {
        self.0.undeclare_impl()
    }
}

impl IntoFuture for LivelinessQuerierUndeclaration<'_> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

impl Drop for LivelinessQuerier<'_> {
    fn drop(&mut self) {
        if self.undeclare_on_drop {
            if let Err(error) = self.undeclare_impl() {
                error!(error);
            }
        }
    }
}

/// A builder for initializing a liveliness querier's `query`.
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// # use std::convert::TryFrom;
///
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let querier = session.liveliness().declare_querier("key/expression").await.unwrap();
/// let tokens = querier
///     .get()
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
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
#[derive(Debug)]
pub struct LivelinessQuerierGetBuilder<'a, Handler> {
    pub(crate) querier: &'a LivelinessQuerier<'a>,
    pub(crate) handler: Handler,
}

impl<'a> LivelinessQuerierGetBuilder<'a, DefaultHandler> {
    /// Receive the replies for this livliness querier's query with a callback.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let querier = session.liveliness().declare_querier("key/expression").await.unwrap();
    /// let _ = querier
    ///     .get()
    ///     .callback(|reply| { println!("Received {:?}", reply.result()); })
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
    #[inline]
    pub fn callback<F>(self, callback: F) -> LivelinessQuerierGetBuilder<'a, Callback<Reply>>
    where
        F: Fn(Reply) + Send + Sync + 'static,
    {
        self.with(Callback::new(Arc::new(callback)))
    }

    /// Receive the replies for this liveliness querier's query with a mutable callback.
    ///
    /// Using this guarantees that your callback will never be called concurrently.
    /// If your callback is also accepted by the [`callback`](LivelinessQuerierGetBuilder::callback) method, we suggest you use it instead of `callback_mut`.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let mut n = 0;
    /// let querier = session.liveliness().declare_querier("key/expression").await.unwrap();
    /// let _ = querier
    ///     .get()
    ///     .callback_mut(move |reply| {n += 1;})
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
    #[inline]
    pub fn callback_mut<F>(self, callback: F) -> LivelinessQuerierGetBuilder<'a, Callback<Reply>>
    where
        F: FnMut(Reply) + Send + Sync + 'static,
    {
        self.callback(locked(callback))
    }

    /// Receive the replies for this liveliness querier's query with a [`Handler`](crate::handlers::IntoHandler).
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let querier = session.liveliness().declare_querier("key/expression").await.unwrap();
    /// let replies = querier
    ///     .get()
    ///     .with(flume::bounded(32))
    ///     .await
    ///     .unwrap();
    /// while let Ok(reply) = replies.recv_async().await {
    ///     println!("Received {:?}", reply.result());
    /// }
    /// # }
    /// ```
    #[inline]
    pub fn with<Handler>(self, handler: Handler) -> LivelinessQuerierGetBuilder<'a, Handler>
    where
        Handler: IntoHandler<Reply>,
    {
        let LivelinessQuerierGetBuilder {
            querier,
            handler: _,
        } = self;
        LivelinessQuerierGetBuilder { querier, handler }
    }
}

impl<Handler> Resolvable for LivelinessQuerierGetBuilder<'_, Handler>
where
    Handler: IntoHandler<Reply> + Send,
    Handler::Handler: Send,
{
    type To = ZResult<Handler::Handler>;
}

impl<Handler> Wait for LivelinessQuerierGetBuilder<'_, Handler>
where
    Handler: IntoHandler<Reply> + Send,
    Handler::Handler: Send,
{
    fn wait(self) -> <Self as Resolvable>::To {
        let (callback, receiver) = self.handler.into_handler();
        self.querier
            .session
            .liveliness_query(&self.querier.key_expr, self.querier.timeout, callback)
            .map(|_| receiver)
    }
}

impl<Handler> IntoFuture for LivelinessQuerierGetBuilder<'_, Handler>
where
    Handler: IntoHandler<Reply> + Send,
    Handler::Handler: Send,
{
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

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
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
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
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
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
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let queryable = session
    ///     .liveliness()
    ///     .get("key/expression")
    ///     .callback(|reply| { println!("Received {:?}", reply.result()); })
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
    #[inline]
    pub fn callback<F>(self, callback: F) -> LivelinessGetBuilder<'a, 'b, Callback<Reply>>
    where
        F: Fn(Reply) + Send + Sync + 'static,
    {
        self.with(Callback::new(Arc::new(callback)))
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
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
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
    pub fn callback_mut<F>(self, callback: F) -> LivelinessGetBuilder<'a, 'b, Callback<Reply>>
    where
        F: FnMut(Reply) + Send + Sync + 'static,
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
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
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
        Handler: IntoHandler<Reply>,
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
    Handler: IntoHandler<Reply> + Send,
    Handler::Handler: Send,
{
    type To = ZResult<Handler::Handler>;
}

impl<Handler> Wait for LivelinessGetBuilder<'_, '_, Handler>
where
    Handler: IntoHandler<Reply> + Send,
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
    Handler: IntoHandler<Reply> + Send,
    Handler::Handler: Send,
{
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}
