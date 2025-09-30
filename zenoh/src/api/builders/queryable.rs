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
use std::future::{IntoFuture, Ready};

use zenoh_core::{Resolvable, Wait};
use zenoh_result::ZResult;

use crate::{
    api::{
        handlers::{locked, DefaultHandler, IntoHandler},
        key_expr::KeyExpr,
        queryable::{Query, Queryable, QueryableInner},
        sample::Locality,
    },
    handlers::Callback,
    Session,
};

/// A builder for initializing a [`Queryable`].
/// Returned by the
/// [`Session::declare_queryable`](crate::Session::declare_queryable) method.
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let queryable = session.declare_queryable("key/expression").await.unwrap();
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
#[derive(Debug)]
pub struct QueryableBuilder<'a, 'b, Handler, const BACKGROUND: bool = false> {
    pub(crate) session: &'a Session,
    pub(crate) key_expr: ZResult<KeyExpr<'b>>,
    pub(crate) complete: bool,
    pub(crate) origin: Locality,
    pub(crate) handler: Handler,
}

impl<'a, 'b> QueryableBuilder<'a, 'b, DefaultHandler> {
    /// Receive the queries for this queryable with a callback.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let queryable = session
    ///     .declare_queryable("key/expression")
    ///     .callback(|query| {println!(">> Handling query '{}'", query.selector());})
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
    #[inline]
    pub fn callback<F>(self, callback: F) -> QueryableBuilder<'a, 'b, Callback<Query>>
    where
        F: Fn(Query) + Send + Sync + 'static,
    {
        self.with(Callback::from(callback))
    }

    /// Receive the queries for this Queryable with a mutable callback.
    ///
    /// Using this guarantees that your callback will never be called concurrently.
    /// If your callback is also accepted by the [`callback`](QueryableBuilder::callback) method, we suggest you use it instead of `callback_mut`.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let mut n = 0;
    /// let queryable = session
    ///     .declare_queryable("key/expression")
    ///     .callback_mut(move |query| {n += 1;})
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
    #[inline]
    pub fn callback_mut<F>(self, callback: F) -> QueryableBuilder<'a, 'b, Callback<Query>>
    where
        F: FnMut(Query) + Send + Sync + 'static,
    {
        self.callback(locked(callback))
    }

    /// Receive the queries for this Queryable with a [`Handler`](crate::handlers::IntoHandler).
    ///
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let queryable = session
    ///     .declare_queryable("key/expression")
    ///     .with(flume::bounded(32))
    ///     .await
    ///     .unwrap();
    /// while let Ok(query) = queryable.recv_async().await {
    ///     println!(">> Handling query '{}'", query.selector());
    /// }
    /// # }
    /// ```
    #[inline]
    pub fn with<Handler>(self, handler: Handler) -> QueryableBuilder<'a, 'b, Handler>
    where
        Handler: IntoHandler<Query>,
    {
        let QueryableBuilder {
            session,
            key_expr,
            complete,
            origin,
            handler: _,
        } = self;
        QueryableBuilder {
            session,
            key_expr,
            complete,
            origin,
            handler,
        }
    }
}

impl<'a, 'b> QueryableBuilder<'a, 'b, Callback<Query>> {
    /// Make the queryable run in the background until the session is closed.
    ///
    /// The background builder doesn't return a `Queryable` object anymore.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// // no need to assign and keep a variable with a background queryable
    /// session
    ///     .declare_queryable("key/expression")
    ///     .callback(|query| {println!(">> Handling query '{}'", query.selector());})
    ///     .background()
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
    pub fn background(self) -> QueryableBuilder<'a, 'b, Callback<Query>, true> {
        QueryableBuilder {
            session: self.session,
            key_expr: self.key_expr,
            complete: self.complete,
            origin: self.origin,
            handler: self.handler,
        }
    }
}

impl<Handler, const BACKGROUND: bool> QueryableBuilder<'_, '_, Handler, BACKGROUND> {
    /// Change queryable completeness.
    /// When queryable is declared as "complete", it promises to have all the data
    /// associated with its key expression, so it's not necessary to query other nodes
    /// for data matching its key expression.
    ///
    /// E.g. a queryable serving key expression `foo/*` is "complete". The queryer
    /// requests data matching `foo/bar` and gets data from this queryable
    /// only even if there are other queryables matching `foo/bar`.
    ///
    /// But for "complete" queryable serving key expression `foo/bar` the request
    /// for `foo/*` will be sent to other queryables as well as the data from this
    /// queryable doesn't cover the whole key expression `foo/*`.
    ///
    /// This is default behavior which corresponds to
    ///  [`QueryTarget::BestMatching`](crate::query::QueryTarget::BestMatching) parameter
    /// of querier's [`target`](crate::query::QuerierBuilder::target).
    ///
    /// It's also possible to forcibly request all available queryables with
    /// [`QueryTarget::All`](crate::query::QueryTarget::All) parameter,
    /// or to request only "complete" ones with
    /// [`QueryTarget::AllComplete`](crate::query::QueryTarget::AllComplete)
    /// parameter.
    #[inline]
    pub fn complete(mut self, complete: bool) -> Self {
        self.complete = complete;
        self
    }

    /// Restrict the matching queries that will be received by this [`Queryable`]
    /// to the ones that have the given [`Locality`](Locality).
    #[inline]
    pub fn allowed_origin(mut self, origin: Locality) -> Self {
        self.origin = origin;
        self
    }
}

impl<Handler> Resolvable for QueryableBuilder<'_, '_, Handler>
where
    Handler: IntoHandler<Query> + Send,
    Handler::Handler: Send,
{
    type To = ZResult<Queryable<Handler::Handler>>;
}

impl<Handler> Wait for QueryableBuilder<'_, '_, Handler>
where
    Handler: IntoHandler<Query> + Send,
    Handler::Handler: Send,
{
    fn wait(self) -> <Self as Resolvable>::To {
        let session = self.session;
        let (callback, receiver) = self.handler.into_handler();
        let ke = self.key_expr?;
        session
            .0
            .declare_queryable_inner(&ke, self.complete, self.origin, callback)
            .map(|qable_state| Queryable {
                inner: QueryableInner {
                    session: self.session.downgrade(),
                    id: qable_state.id,
                    undeclare_on_drop: true,
                    key_expr: ke.into_owned(),
                },
                handler: receiver,
            })
    }
}

impl<Handler> IntoFuture for QueryableBuilder<'_, '_, Handler>
where
    Handler: IntoHandler<Query> + Send,
    Handler::Handler: Send,
{
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

impl Resolvable for QueryableBuilder<'_, '_, Callback<Query>, true> {
    type To = ZResult<()>;
}

impl Wait for QueryableBuilder<'_, '_, Callback<Query>, true> {
    fn wait(self) -> <Self as Resolvable>::To {
        self.session.0.declare_queryable_inner(
            &self.key_expr?,
            self.complete,
            self.origin,
            self.handler,
        )?;
        Ok(())
    }
}

impl IntoFuture for QueryableBuilder<'_, '_, Callback<Query>, true> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}
