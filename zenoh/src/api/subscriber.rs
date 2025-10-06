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
    fmt,
    future::{IntoFuture, Ready},
    ops::{Deref, DerefMut},
};

use tracing::error;
use zenoh_core::{Resolvable, Wait};
use zenoh_result::ZResult;
#[cfg(feature = "unstable")]
use {zenoh_config::wrappers::EntityGlobalId, zenoh_protocol::core::EntityGlobalIdProto};

use crate::api::{
    handlers::Callback,
    key_expr::KeyExpr,
    sample::{Locality, Sample},
    session::{UndeclarableSealed, WeakSession},
    Id,
};

pub(crate) struct SubscriberState {
    pub(crate) id: Id,
    pub(crate) remote_id: Id,
    pub(crate) key_expr: KeyExpr<'static>,
    pub(crate) origin: Locality,
    pub(crate) callback: Callback<Sample>,
    pub(crate) history: bool,
}

impl fmt::Debug for SubscriberState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Subscriber")
            .field("id", &self.id)
            .field("key_expr", &self.key_expr)
            .finish()
    }
}

#[derive(Debug)]
pub(crate) struct SubscriberInner {
    pub(crate) session: WeakSession,
    pub(crate) id: Id,
    pub(crate) key_expr: KeyExpr<'static>,
    pub(crate) kind: SubscriberKind,
    pub(crate) undeclare_on_drop: bool,
}

/// A [`Resolvable`] returned by [`Subscriber::undeclare`]
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let subscriber = session
///     .declare_subscriber("key/expression")
///     .await
///     .unwrap();
/// subscriber.undeclare().await.unwrap();
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
pub struct SubscriberUndeclaration<Handler>(Subscriber<Handler>);

impl<Handler> Resolvable for SubscriberUndeclaration<Handler> {
    type To = ZResult<()>;
}

impl<Handler> Wait for SubscriberUndeclaration<Handler> {
    fn wait(mut self) -> <Self as Resolvable>::To {
        self.0.undeclare_impl()
    }
}

impl<Handler> IntoFuture for SubscriberUndeclaration<Handler> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

/// A subscriber that provides data through a [`Handler`](crate::handlers::IntoHandler).
///
/// Subscribers can be created from a zenoh [`Session`](crate::Session)
/// with the [`declare_subscriber`](crate::Session::declare_subscriber) function.
///
/// # Examples
///
/// Run subscriber with callback in the background until the session is closed:
/// ```no_run
/// # #[tokio::main]
/// # async fn main() {
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// session
///     .declare_subscriber("key/expression")
///     .callback(|sample| { println!("Received: {} {:?}", sample.key_expr(), sample.payload()) })
///     .background()
///     .await
///     .unwrap();
/// // subscriber runs in the background until the session is closed
/// # }
/// ```
///
/// Run subscriber with channel handler:
/// ```no_run
/// # #[tokio::main]
/// # async fn main() {
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let subscriber = session
///     .declare_subscriber("key/expression")
///     .with(flume::bounded(32))
///     .await
///     .unwrap();
/// while let Ok(sample) = subscriber.recv_async().await {
///     println!("Received: {} {:?}", sample.key_expr(), sample.payload());
/// }
/// // subscriber is undeclared at the end of the scope
/// # }
/// ```
#[non_exhaustive]
#[derive(Debug)]
pub struct Subscriber<Handler> {
    pub(crate) inner: SubscriberInner,
    pub(crate) handler: Handler,
}

impl<Handler> Subscriber<Handler> {
    /// Returns the [`EntityGlobalId`] of this Subscriber.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let subscriber = session.declare_subscriber("key/expression")
    ///     .await
    ///     .unwrap();
    /// let subscriber_id = subscriber.id();
    /// # }
    /// ```
    #[zenoh_macros::unstable]
    pub fn id(&self) -> EntityGlobalId {
        EntityGlobalIdProto {
            zid: self.inner.session.zid().into(),
            eid: self.inner.id,
        }
        .into()
    }

    /// Returns the [`KeyExpr`] this subscriber subscribes to.
    pub fn key_expr(&self) -> &KeyExpr<'static> {
        &self.inner.key_expr
    }

    /// Returns a reference to this subscriber's handler.
    /// A handler is anything that implements [`IntoHandler`](crate::handlers::IntoHandler).
    /// The default handler is [`DefaultHandler`](crate::handlers::DefaultHandler).
    pub fn handler(&self) -> &Handler {
        &self.handler
    }

    /// Returns a mutable reference to this subscriber's handler.
    /// A handler is anything that implements [`IntoHandler`](crate::handlers::IntoHandler).
    /// The default handler is [`DefaultHandler`](crate::handlers::DefaultHandler).
    pub fn handler_mut(&mut self) -> &mut Handler {
        &mut self.handler
    }

    /// Undeclare the [`Subscriber`].
    ///
    /// This subscriber's [`Callback`](crate::handlers::Callback) will be dropped as part of the
    /// undeclaration.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let subscriber = session.declare_subscriber("key/expression")
    ///     .await
    ///     .unwrap();
    /// subscriber.undeclare().await.unwrap();
    /// # }
    /// ```
    #[inline]
    pub fn undeclare(self) -> SubscriberUndeclaration<Handler>
    where
        Handler: Send,
    {
        self.undeclare_inner(())
    }

    fn undeclare_impl(&mut self) -> ZResult<()> {
        // set the flag first to avoid double panic if this function panics
        self.inner.undeclare_on_drop = false;
        self.inner
            .session
            .undeclare_subscriber_inner(self.inner.id, self.inner.kind)
    }

    #[zenoh_macros::internal]
    pub fn set_background(&mut self, background: bool) {
        self.inner.undeclare_on_drop = !background;
    }

    #[zenoh_macros::internal]
    pub fn session(&self) -> &crate::Session {
        self.inner.session.session()
    }
}

impl<Handler> Drop for Subscriber<Handler> {
    fn drop(&mut self) {
        if self.inner.undeclare_on_drop {
            if let Err(error) = self.undeclare_impl() {
                error!(error);
            }
        }
    }
}

impl<Handler: Send> UndeclarableSealed<()> for Subscriber<Handler> {
    type Undeclaration = SubscriberUndeclaration<Handler>;

    fn undeclare_inner(self, _: ()) -> Self::Undeclaration {
        SubscriberUndeclaration(self)
    }
}

impl<Handler> Deref for Subscriber<Handler> {
    type Target = Handler;

    fn deref(&self) -> &Self::Target {
        self.handler()
    }
}
impl<Handler> DerefMut for Subscriber<Handler> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.handler_mut()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SubscriberKind {
    Subscriber,
    LivelinessSubscriber,
}
