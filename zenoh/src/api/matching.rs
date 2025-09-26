//
// Copyright (c) 2024 ZettaScale Technology
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
    collections::HashSet,
    fmt,
    future::{IntoFuture, Ready},
    sync::{Arc, Mutex},
};

use tracing::error;
use zenoh_core::{Resolvable, Wait};
use zenoh_result::ZResult;

use super::{
    handlers::Callback,
    key_expr::KeyExpr,
    sample::Locality,
    session::{UndeclarableSealed, WeakSession},
    Id,
};
use crate::api::handlers::CallbackParameter;

/// A struct that indicates if there exist entities matching the key expression.
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
///
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let publisher = session.declare_publisher("key/expression").await.unwrap();
/// let matching_status = publisher.matching_status().await.unwrap();
/// # }
/// ```
#[derive(Copy, Clone, Debug)]
pub struct MatchingStatus {
    pub(crate) matching: bool,
}

impl CallbackParameter for MatchingStatus {
    type Message<'a> = Self;

    fn from_message(msg: Self::Message<'_>) -> Self {
        msg
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub(crate) enum MatchingStatusType {
    Subscribers,
    Queryables(bool),
}

impl MatchingStatus {
    /// Returns true if there exist entities matching the target (i.e., either Subscribers matching Publisher's key expression or Queryables matching Querier's key expression and target).
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let publisher = session.declare_publisher("key/expression").await.unwrap();
    /// let matching_subscribers: bool = publisher
    ///     .matching_status()
    ///     .await
    ///     .unwrap()
    ///     .matching();
    /// # }
    /// ```
    pub fn matching(&self) -> bool {
        self.matching
    }
}

pub(crate) struct MatchingListenerState {
    pub(crate) id: Id,
    pub(crate) current: Mutex<bool>,
    pub(crate) key_expr: KeyExpr<'static>,
    pub(crate) destination: Locality,
    pub(crate) match_type: MatchingStatusType,
    pub(crate) callback: Callback<MatchingStatus>,
}

impl MatchingListenerState {
    pub(crate) fn is_matching(&self, key_expr: &KeyExpr, match_type: MatchingStatusType) -> bool {
        match match_type {
            MatchingStatusType::Subscribers => {
                self.match_type == MatchingStatusType::Subscribers
                    && self.key_expr.intersects(key_expr)
            }
            MatchingStatusType::Queryables(false) => {
                self.match_type == MatchingStatusType::Queryables(false)
                    && self.key_expr.intersects(key_expr)
            }
            MatchingStatusType::Queryables(true) => {
                (self.match_type == MatchingStatusType::Queryables(false)
                    && self.key_expr.intersects(key_expr))
                    || (self.match_type == MatchingStatusType::Queryables(true)
                        && key_expr.includes(&self.key_expr))
            }
        }
    }
}

impl fmt::Debug for MatchingListenerState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("MatchingListener")
            .field("id", &self.id)
            .field("key_expr", &self.key_expr)
            .field("match_type", &self.match_type)
            .finish()
    }
}

#[derive(Debug)]
pub(crate) struct MatchingListenerInner {
    pub(crate) session: WeakSession,
    pub(crate) matching_listeners: Arc<Mutex<HashSet<Id>>>,
    pub(crate) id: Id,
    pub(crate) undeclare_on_drop: bool,
}

/// A listener that sends notifications when the [`MatchingStatus`] of a
/// corresponding Zenoh entity changes.
///
/// # Examples
/// ```no_run
/// # #[tokio::main]
/// # async fn main() {
///
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let publisher = session.declare_publisher("key/expression").await.unwrap();
/// let matching_listener = publisher.matching_listener().await.unwrap();
/// while let Ok(matching_status) = matching_listener.recv_async().await {
///     if matching_status.matching() {
///         println!("Publisher has matching subscribers.");
///     } else {
///         println!("Publisher has NO MORE matching subscribers.");
///     }
/// }
/// # }
/// ```
#[derive(Debug)]
pub struct MatchingListener<Handler> {
    pub(crate) inner: MatchingListenerInner,
    pub(crate) handler: Handler,
}

impl<Handler> MatchingListener<Handler> {
    /// Undeclare the [`MatchingListener`].
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let publisher = session.declare_publisher("key/expression").await.unwrap();
    /// let matching_listener = publisher.matching_listener().await.unwrap();
    /// matching_listener.undeclare().await.unwrap();
    /// # }
    /// ```
    #[inline]
    pub fn undeclare(self) -> MatchingListenerUndeclaration<Handler>
    where
        Handler: Send,
    {
        self.undeclare_inner(())
    }

    fn undeclare_impl(&mut self) -> ZResult<()> {
        // set the flag first to avoid a double panic if this function panics
        self.inner.undeclare_on_drop = false;
        zlock!(self.inner.matching_listeners).remove(&self.inner.id);
        self.inner
            .session
            .undeclare_matches_listener_inner(self.inner.id)
    }

    /// Returns a reference to this matching listener's handler.
    /// A handler is anything that implements [`IntoHandler`](crate::handlers::IntoHandler).
    /// The default handler is [`DefaultHandler`](crate::handlers::DefaultHandler).
    pub fn handler(&self) -> &Handler {
        &self.handler
    }

    /// Returns a mutable reference to this matching listener's handler.
    /// A handler is anything that implements [`IntoHandler`](crate::handlers::IntoHandler).
    /// The default handler is [`DefaultHandler`](crate::handlers::DefaultHandler).
    pub fn handler_mut(&mut self) -> &mut Handler {
        &mut self.handler
    }

    #[zenoh_macros::internal]
    pub fn set_background(&mut self, background: bool) {
        self.inner.undeclare_on_drop = !background;
    }
}

impl<Handler> Drop for MatchingListener<Handler> {
    fn drop(&mut self) {
        if self.inner.undeclare_on_drop {
            if let Err(error) = self.undeclare_impl() {
                error!(error);
            }
        }
    }
}

impl<Handler: Send> UndeclarableSealed<()> for MatchingListener<Handler> {
    type Undeclaration = MatchingListenerUndeclaration<Handler>;

    fn undeclare_inner(self, _: ()) -> Self::Undeclaration {
        MatchingListenerUndeclaration(self)
    }
}

impl<Handler> std::ops::Deref for MatchingListener<Handler> {
    type Target = Handler;

    fn deref(&self) -> &Self::Target {
        &self.handler
    }
}

impl<Handler> std::ops::DerefMut for MatchingListener<Handler> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.handler
    }
}

/// A [`Resolvable`] returned by [`MatchingListener::undeclare`]
pub struct MatchingListenerUndeclaration<Handler>(MatchingListener<Handler>);

impl<Handler> Resolvable for MatchingListenerUndeclaration<Handler> {
    type To = ZResult<()>;
}

impl<Handler> Wait for MatchingListenerUndeclaration<Handler> {
    fn wait(mut self) -> <Self as Resolvable>::To {
        self.0.undeclare_impl()
    }
}

impl<Handler> IntoFuture for MatchingListenerUndeclaration<Handler> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}
