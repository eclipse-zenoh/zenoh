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
    time::Duration,
};

use tracing::error;
use zenoh_core::{Resolvable, Resolve, Result as ZResult, Wait};

use crate::api::{
    builders::liveliness::{
        LivelinessGetBuilder, LivelinessSubscriberBuilder, LivelinessTokenBuilder,
    },
    handlers::DefaultHandler,
    key_expr::KeyExpr,
    session::{Session, UndeclarableSealed, WeakSession},
    Id,
};

/// A structure with functions to declare a [`LivelinessToken`](LivelinessToken),
/// query existing [`LivelinessTokens`](LivelinessToken)
/// and subscribe to liveliness changes.
///
/// A [`LivelinessToken`](LivelinessToken) is a token whose liveliness is tied
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
pub struct Liveliness<'a> {
    pub(crate) session: &'a Session,
}

impl<'a> Liveliness<'a> {
    /// Create a [`LivelinessToken`](LivelinessToken) for the given key expression.
    ///
    /// # Arguments
    ///
    /// * `key_expr` - The key expression to create the liveliness token on
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

    /// Create a [`Subscriber`](crate::pubsub::Subscriber) for liveliness changes matching the given key expression.
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
            Duration::from_millis(
                self.session
                    .0
                    .runtime
                    .get_config()
                    .queries_default_timeout_ms(),
            )
        };
        LivelinessGetBuilder {
            session: self.session,
            key_expr,
            timeout,
            handler: DefaultHandler::default(),
            #[cfg(feature = "unstable")]
            cancellation_token: None,
        }
    }
}

/// A token whose liveliness is tied to the Zenoh [`Session`](Session).
///
/// A declared liveliness token will be seen as alive by any other Zenoh
/// application in the system that monitors it while the liveliness token
/// is not undeclared or dropped, while the Zenoh application that declared
/// it is alive (hasn't stopped or crashed) and while the Zenoh application
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
#[must_use = "Liveliness tokens will be immediately dropped and undeclared if not bound to a variable"]
#[derive(Debug)]
pub struct LivelinessToken {
    pub(crate) session: WeakSession,
    pub(crate) id: Id,
    pub(crate) undeclare_on_drop: bool,
}

/// A [`Resolvable`] returned by [`LivelinessToken::undeclare`]
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
pub struct LivelinessTokenUndeclaration(LivelinessToken);

impl Resolvable for LivelinessTokenUndeclaration {
    type To = ZResult<()>;
}

impl Wait for LivelinessTokenUndeclaration {
    fn wait(mut self) -> <Self as Resolvable>::To {
        self.0.undeclare_impl()
    }
}

impl IntoFuture for LivelinessTokenUndeclaration {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

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
        // set the flag first to avoid double panic if this function panics
        self.undeclare_on_drop = false;
        self.session.undeclare_liveliness(self.id)
    }
}

impl UndeclarableSealed<()> for LivelinessToken {
    type Undeclaration = LivelinessTokenUndeclaration;

    fn undeclare_inner(self, _: ()) -> Self::Undeclaration {
        LivelinessTokenUndeclaration(self)
    }
}

impl Drop for LivelinessToken {
    fn drop(&mut self) {
        if self.undeclare_on_drop {
            if let Err(error) = self.undeclare_impl() {
                error!(error);
            }
        }
    }
}
