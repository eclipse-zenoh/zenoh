use std::future::{IntoFuture, Ready};

use zenoh_core::{Resolvable, Result as ZResult, Wait};

use crate::{
    api::subscriber::{SubscriberInner, SubscriberKind},
    handlers::{locked, Callback, DefaultHandler, IntoHandler},
    key_expr::KeyExpr,
    liveliness::LivelinessToken,
    pubsub::Subscriber,
    sample::{Locality, Sample},
    Session,
};

/// A builder for initializing a [`LivelinessToken`](LivelinessToken)
/// returned by the [`Liveliness::declare_token`] method.
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
/// The builder is returned by the [`Liveliness::declare_subscriber`] method and
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
