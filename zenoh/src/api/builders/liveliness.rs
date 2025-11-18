use std::{
    future::{IntoFuture, Ready},
    time::Duration,
};

use zenoh_core::{Resolvable, Result as ZResult, Wait};

#[cfg(feature = "unstable")]
use crate::api::cancellation::CancellationTokenBuilderTrait;
use crate::api::{
    handlers::{locked, Callback, DefaultHandler, IntoHandler},
    key_expr::KeyExpr,
    liveliness::LivelinessToken,
    query::Reply,
    sample::{Locality, Sample},
    session::Session,
    subscriber::{Subscriber, SubscriberInner, SubscriberKind},
};

/// A builder for initializing a [`LivelinessToken`](LivelinessToken)
/// returned by the [`Liveliness::declare_token`](crate::liveliness::Liveliness::declare_token) method.
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
#[derive(Debug)]
pub struct LivelinessTokenBuilder<'a, 'b> {
    pub(crate) session: &'a Session,
    pub(crate) key_expr: ZResult<KeyExpr<'b>>,
}

impl Resolvable for LivelinessTokenBuilder<'_, '_> {
    type To = ZResult<LivelinessToken>;
}

impl Wait for LivelinessTokenBuilder<'_, '_> {
    #[inline]
    fn wait(self) -> <Self as Resolvable>::To {
        let session = self.session;
        let key_expr = self.key_expr?.into_owned();
        session
            .0
            .declare_liveliness_inner(&key_expr)
            .map(|id| LivelinessToken {
                session: self.session.downgrade(),
                id,
                undeclare_on_drop: true,
            })
    }
}

impl IntoFuture for LivelinessTokenBuilder<'_, '_> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

/// A builder for initializing a liveliness subscription.
///
/// The builder is returned by the [`Liveliness::declare_subscriber`](crate::liveliness::Liveliness::declare_subscriber) method and
/// resolves to the [`Subscriber`] receiving the liveliness samples.
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
#[derive(Debug)]
pub struct LivelinessSubscriberBuilder<'a, 'b, Handler, const BACKGROUND: bool = false> {
    pub session: &'a Session,
    pub key_expr: ZResult<KeyExpr<'b>>,
    pub handler: Handler,
    pub history: bool,
}

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
    pub fn callback<F>(self, callback: F) -> LivelinessSubscriberBuilder<'a, 'b, Callback<Sample>>
    where
        F: Fn(Sample) + Send + Sync + 'static,
    {
        self.with(Callback::from(callback))
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
    /// Make subscriber run in background until the session is closed.
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
    pub fn history(mut self, history: bool) -> Self {
        self.history = history;
        self
    }
}

impl<Handler> Resolvable for LivelinessSubscriberBuilder<'_, '_, Handler>
where
    Handler: IntoHandler<Sample> + Send,
    Handler::Handler: Send,
{
    type To = ZResult<Subscriber<Handler::Handler>>;
}

impl<Handler> Wait for LivelinessSubscriberBuilder<'_, '_, Handler>
where
    Handler: IntoHandler<Sample> + Send,
    Handler::Handler: Send,
{
    fn wait(self) -> <Self as Resolvable>::To {
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

impl<Handler> IntoFuture for LivelinessSubscriberBuilder<'_, '_, Handler>
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

impl Resolvable for LivelinessSubscriberBuilder<'_, '_, Callback<Sample>, true> {
    type To = ZResult<()>;
}

impl Wait for LivelinessSubscriberBuilder<'_, '_, Callback<Sample>, true> {
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

impl IntoFuture for LivelinessSubscriberBuilder<'_, '_, Callback<Sample>, true> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

/// Builder for initializing a liveliness query.
///
/// The builder is returned by the [`Liveliness::get`](crate::liveliness::Liveliness::get) method and
/// resolves to a handler which returns the [`Reply`]s from the liveliness tokens.
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
    #[cfg(feature = "unstable")]
    pub(crate) cancellation_token: Option<crate::api::cancellation::CancellationToken>,
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
        self.with(Callback::from(callback))
    }

    /// Receive the replies for this liveliness query with a mutable callback.
    ///
    /// Using this guarantees that your callback will never be called concurrently.
    /// If your callback is also accepted by the [`callback`](LivelinessGetBuilder::callback)
    /// method, we suggest you use it instead of `callback_mut`.
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
            #[cfg(feature = "unstable")]
            cancellation_token,
        } = self;
        LivelinessGetBuilder {
            session,
            key_expr,
            timeout,
            handler,
            #[cfg(feature = "unstable")]
            cancellation_token,
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

#[cfg(feature = "unstable")]
#[zenoh_macros::internal_trait]
impl<Handler> CancellationTokenBuilderTrait for LivelinessGetBuilder<'_, '_, Handler> {
    /// Provide a cancellation token that can be used later to interrupt operation.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let ct = zenoh::cancellation::CancellationToken::default();
    /// let replies = session
    ///     .liveliness()
    ///     .get("key/expression")
    ///     .with(flume::bounded(32))
    ///     .cancellation_token(ct.clone())
    ///     .await
    ///     .unwrap();
    /// tokio::task::spawn(async move {
    ///     tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    ///     ct.cancel().await.unwrap();
    /// });
    /// while let Ok(reply) = replies.recv_async().await {
    ///     println!("Received {:?}", reply.result());
    /// }
    /// # }
    /// ```
    #[zenoh_macros::unstable_doc]
    fn cancellation_token(
        self,
        cancellation_token: crate::api::cancellation::CancellationToken,
    ) -> Self {
        Self {
            cancellation_token: Some(cancellation_token),
            ..self
        }
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
        #[allow(unused_mut)] // mut is needed only for unstable cancellation_token
        let (mut callback, receiver) = self.handler.into_handler();
        #[cfg(feature = "unstable")]
        if self
            .cancellation_token
            .as_ref()
            .map(|ct| ct.is_cancelled())
            .unwrap_or(false)
        {
            return Ok(receiver);
        };
        #[cfg(feature = "unstable")]
        let cancellation_token_and_receiver = self.cancellation_token.map(|ct| {
            let (notifier, receiver) =
                crate::api::cancellation::create_sync_group_receiver_notifier_pair();
            callback.set_on_drop_notifier(notifier);
            (ct, receiver)
        });
        #[allow(unused_variables)] // qid is only needed for unstable cancellation_token
        self.session
            .0
            .liveliness_query(&self.key_expr?, self.timeout, callback)
            .map(|qid| {
                #[cfg(feature = "unstable")]
                if let Some((cancellation_token, cancel_receiver)) = cancellation_token_and_receiver
                {
                    let session_clone = self.session.clone();
                    let on_cancel = move || {
                        let _ = session_clone.0.cancel_liveliness_query(qid); // fails only if no associated query exists - likely because it was already finalized
                        Ok(())
                    };
                    cancellation_token.add_on_cancel_handler(cancel_receiver, Box::new(on_cancel));
                }
                receiver
            })
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
