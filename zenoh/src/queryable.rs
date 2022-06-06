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

//! Queryable primitives.

use crate::prelude::*;
use crate::SessionRef;
use crate::Undeclarable;
use crate::API_QUERY_RECEPTION_CHANNEL_SIZE;
use futures::FutureExt;
use std::fmt;
use std::marker::PhantomData;
use std::ops::Deref;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use zenoh_core::{AsyncResolve, Resolvable, Resolve, Result as ZResult, SyncResolve};

/// Structs received by a [`Queryable`](HandlerQueryable).
pub struct Query {
    /// The key_selector of this Query.
    pub(crate) key_expr: KeyExpr<'static>,
    /// The value_selector of this Query.
    pub(crate) value_selector: String,

    pub(crate) kind: ZInt,
    /// The sender to use to send replies to this query.
    /// When this sender is dropped, the reply is finalized.
    pub(crate) replies_sender: flume::Sender<(ZInt, Sample)>,
}

impl Query {
    /// The full [`Selector`] of this Query.
    #[inline(always)]
    pub fn selector(&self) -> Selector<'_> {
        Selector {
            key_expr: self.key_expr.clone(),
            value_selector: (&self.value_selector).into(),
        }
    }

    /// The key selector part of this Query.
    #[inline(always)]
    pub fn key_expr(&self) -> &KeyExpr<'static> {
        &self.key_expr
    }

    /// The value selector part of this Query.
    #[inline(always)]
    pub fn value_selector(&self) -> &str {
        &self.value_selector
    }

    /// Sends a reply to this Query.
    #[inline(always)]
    pub fn reply(&self, result: Result<Sample, Value>) -> ReplyBuilder<'_> {
        ReplyBuilder {
            query: self,
            result,
        }
    }
}

impl fmt::Debug for Query {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Query")
            .field("key_selector", &self.key_expr)
            .field("value_selector", &self.value_selector)
            .finish()
    }
}

impl fmt::Display for Query {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Query")
            .field(
                "selector",
                &format!("{}{}", &self.key_expr, &self.value_selector),
            )
            .finish()
    }
}

#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
pub struct ReplyBuilder<'a> {
    query: &'a Query,
    result: Result<Sample, Value>,
}

impl Resolvable for ReplyBuilder<'_> {
    type Output = zenoh_core::Result<()>;
}
impl SyncResolve for ReplyBuilder<'_> {
    fn res_sync(self) -> Self::Output {
        match self.result {
            Ok(sample) => self
                .query
                .replies_sender
                .send((self.query.kind, sample))
                .map_err(|e| zerror!("{}", e).into()),
            Err(_) => Err(zerror!("Replying errors is not yet supported!").into()),
        }
    }
}

#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct ReplyFuture<'a>(
    Result<flume::r#async::SendFut<'a, (ZInt, Sample)>, Option<zenoh_core::Error>>,
);
impl std::future::Future for ReplyFuture<'_> {
    type Output = zenoh_core::Result<()>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match &mut self.get_mut().0 {
            Ok(sender) => sender.poll_unpin(cx).map_err(|e| zerror!(e).into()),
            Err(e) => Poll::Ready(Err(e
                .take()
                .unwrap_or_else(|| zerror!("Overpolling of ReplyFuture detected").into()))),
        }
    }
}
impl<'a> AsyncResolve for ReplyBuilder<'a> {
    type Future = ReplyFuture<'a>;
    fn res_async(self) -> Self::Future {
        ReplyFuture(match self.result {
            Ok(sample) => Ok(self
                .query
                .replies_sender
                .send_async((self.query.kind, sample))),
            Err(_) => Err(Some(
                zerror!("Replying errors is not yet supported!").into(),
            )),
        })
    }
}

pub(crate) struct QueryableState {
    pub(crate) id: Id,
    pub(crate) key_expr: WireExpr<'static>,
    pub(crate) kind: ZInt,
    pub(crate) complete: bool,
    pub(crate) callback: Arc<dyn Fn(Query) + Send + Sync>,
}

impl fmt::Debug for QueryableState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Queryable")
            .field("id", &self.id)
            .field("key_expr", &self.key_expr)
            .field("complete", &self.complete)
            .finish()
    }
}

/// An entity able to reply to queries through a callback.
///
/// CallbackQueryables can be created from a zenoh [`Session`](crate::Session)
/// with the [`queryable`](crate::Session::subscribe) function
/// and the [`callback`](QueryableBuilder::callback) function
/// of the resulting builder.
///

///
/// Queryables are automatically undeclared when dropped.
///
/// # Examples
/// ```no_run
/// # async_std::task::block_on(async {
/// use futures::prelude::*;
/// use r#async::AsyncResolve;
/// use zenoh::prelude::*;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// let queryable = session.declare_queryable("key/expression").res().await.unwrap();
/// while let Ok(query) = queryable.recv_async().await {
///     println!(">> Handling query '{}'", query.selector());
///     query.reply(Ok(Sample::try_from("key/expression", "value").unwrap())).res().await.unwrap();
/// }
/// # })
/// ```
#[derive(Debug)]
pub struct CallbackQueryable<'a> {
    pub(crate) session: SessionRef<'a>,
    pub(crate) state: Arc<QueryableState>,
    pub(crate) alive: bool,
}

impl<'a> CallbackQueryable<'a> {
    /// Close a [`CallbackQueryable`](CallbackQueryable) previously created with [`queryable`](crate::Session::queryable).
    ///
    /// Queryables are automatically closed when dropped, but you may want to use this function to handle errors or
    /// close the Queryable asynchronously.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    /// use r#async::AsyncResolve;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let queryable = session.declare_queryable("key/expression").res().await.unwrap();
    /// queryable.undeclare().res().await.unwrap();
    /// # })
    /// ```
    #[inline]
    pub fn undeclare(self) -> impl Resolve<crate::Result<()>> + 'a {
        Undeclarable::undeclare(self, ())
    }
}
impl<'a> Undeclarable<()> for CallbackQueryable<'a> {
    type Output = ZResult<()>;
    type Undeclaration = QueryableUndeclare<'a>;
    fn undeclare(self, _: ()) -> Self::Undeclaration {
        QueryableUndeclare { queryable: self }
    }
}
pub struct QueryableUndeclare<'a> {
    queryable: CallbackQueryable<'a>,
}
impl<'a> Resolvable for QueryableUndeclare<'a> {
    type Output = ZResult<()>;
}
impl SyncResolve for QueryableUndeclare<'_> {
    fn res_sync(mut self) -> Self::Output {
        self.queryable.alive = false;
        self.queryable
            .session
            .close_queryable(self.queryable.state.id)
    }
}
impl AsyncResolve for QueryableUndeclare<'_> {
    type Future = futures::future::Ready<Self::Output>;

    fn res_async(self) -> Self::Future {
        futures::future::ready(self.res_sync())
    }
}

impl Drop for CallbackQueryable<'_> {
    fn drop(&mut self) {
        if self.alive {
            let _ = self.session.close_queryable(self.state.id);
        }
    }
}

/// A builder for initializing a [`FlumeQueryable`].
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// use zenoh::prelude::*;
/// use r#async::AsyncResolve;
/// use zenoh::queryable;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// let queryable = session.declare_queryable("key/expression").res().await.unwrap();
/// # })
/// ```
#[derive(Debug)]
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
pub struct QueryableBuilder<'a, 'b> {
    pub(crate) session: SessionRef<'a>,
    pub(crate) key_expr: ZResult<KeyExpr<'b>>,
    pub(crate) kind: ZInt,
    pub(crate) complete: bool,
}
impl<'a, 'b> Clone for QueryableBuilder<'a, 'b> {
    fn clone(&self) -> Self {
        Self {
            session: self.session.clone(),
            key_expr: match &self.key_expr {
                Ok(k) => Ok(k.clone()),
                Err(e) => Err(zerror!("Cloned KE error: {}", e).into()),
            },
            kind: self.kind,
            complete: self.complete,
        }
    }
}

impl<'a, 'b> QueryableBuilder<'a, 'b> {
    /// Receive the queries for this Queryable with a callback.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    /// use r#async::AsyncResolve;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let queryable = session
    ///     .declare_queryable("key/expression")
    ///     .callback(|query| {println!(">> Handling query '{}'", query.selector());})
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// # })
    /// ```
    #[inline]
    pub fn callback<Callback>(
        self,
        callback: Callback,
    ) -> CallbackQueryableBuilder<'a, 'b, Callback>
    where
        Callback: Fn(Query) + Send + Sync + 'static,
    {
        CallbackQueryableBuilder {
            builder: self,
            callback,
        }
    }

    /// Receive the queries for this Queryable with a mutable callback.
    ///
    /// Using this guarantees that your callback will never be called concurrently.
    /// If your callback is also accepted by the [`callback`](QueryableBuilder::callback) method, we suggest you use it instead of `callback_mut`
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    /// use r#async::AsyncResolve;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let mut n = 0;
    /// let queryable = session
    ///     .declare_queryable("key/expression")
    ///     .callback_mut(move |query| {n += 1;})
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// # })
    /// ```
    #[inline]
    pub fn callback_mut<CallbackMut>(
        self,
        callback: CallbackMut,
    ) -> CallbackQueryableBuilder<'a, 'b, impl Fn(Query) + Send + Sync + 'static>
    where
        CallbackMut: FnMut(Query) + Send + Sync + 'static,
    {
        self.callback(locked(callback))
    }

    /// Receive the queries for this Queryable with a [`Handler`](crate::prelude::Handler).
    ///
    /// # Examples
    /// ```no_run
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    /// use r#async::AsyncResolve;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let queryable = session
    ///     .declare_queryable("key/expression")
    ///     .with(flume::bounded(32))
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// while let Ok(query) = queryable.recv_async().await {
    ///     println!(">> Handling query '{}'", query.selector());
    /// }
    /// # })
    /// ```
    #[inline]
    pub fn with<IntoHandler, Receiver>(
        self,
        handler: IntoHandler,
    ) -> HandlerQueryableBuilder<'a, 'b, IntoHandler, Receiver>
    where
        IntoHandler: crate::prelude::IntoHandler<Query, Receiver>,
    {
        HandlerQueryableBuilder {
            builder: self,
            handler,
            receiver: PhantomData,
        }
    }

    /// Change queryable completeness.
    #[inline]
    pub fn complete(mut self, complete: bool) -> Self {
        self.complete = complete;
        self
    }
}

impl<'a> Resolvable for QueryableBuilder<'a, '_> {
    type Output = crate::Result<HandlerQueryable<'a, flume::Receiver<Query>>>;
}
impl SyncResolve for QueryableBuilder<'_, '_> {
    fn res_sync(self) -> Self::Output {
        self.with(flume::bounded(*API_QUERY_RECEPTION_CHANNEL_SIZE))
            .res_sync()
    }
}
impl AsyncResolve for QueryableBuilder<'_, '_> {
    type Future = futures::future::Ready<Self::Output>;
    fn res_async(self) -> Self::Future {
        self.with(flume::bounded(*API_QUERY_RECEPTION_CHANNEL_SIZE))
            .res_async()
    }
}

/// A builder for initializing a [`Queryable`](CallbackQueryable).
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// use zenoh::prelude::*;
/// use r#async::AsyncResolve;
/// use zenoh::queryable;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// let queryable = session.declare_queryable("key/expression").res().await.unwrap();
/// # })
/// ```
#[derive(Debug, Clone)]
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
pub struct CallbackQueryableBuilder<'a, 'b, Callback>
where
    Callback: Fn(Query) + Send + Sync + 'static,
{
    pub(crate) builder: QueryableBuilder<'a, 'b>,
    pub(crate) callback: Callback,
}

impl<'a, 'b, Callback> CallbackQueryableBuilder<'a, 'b, Callback>
where
    Callback: Fn(Query) + Send + Sync + 'static,
{
    /// Change queryable completeness.
    #[inline]
    pub fn complete(mut self, complete: bool) -> Self {
        self.builder = self.builder.complete(complete);
        self
    }
}

impl<'a, Callback> Resolvable for CallbackQueryableBuilder<'a, '_, Callback>
where
    Callback: Fn(Query) + Send + Sync + 'static,
{
    type Output = crate::Result<CallbackQueryable<'a>>;
}

impl<Callback> AsyncResolve for CallbackQueryableBuilder<'_, '_, Callback>
where
    Callback: Fn(Query) + Send + Sync + 'static,
{
    type Future = futures::future::Ready<Self::Output>;

    fn res_async(self) -> Self::Future {
        futures::future::ready(self.res_sync())
    }
}

impl<Callback> SyncResolve for CallbackQueryableBuilder<'_, '_, Callback>
where
    Callback: Fn(Query) + Send + Sync + 'static,
{
    fn res_sync(self) -> Self::Output {
        let session = self.builder.session;
        session
            .declare_queryable_inner(
                &(&self.builder.key_expr?).into(),
                self.builder.kind,
                self.builder.complete,
                Box::new(self.callback),
            )
            .map(move |qable_state| CallbackQueryable {
                session,
                state: qable_state,
                alive: true,
            })
    }
}

/// A queryable that provides data through a [`Handler`](crate::prelude::Handler).
///
/// HandlerQueryables can be created from a zenoh [`Session`](crate::Session)
/// with the [`queryable`](crate::Session::queryable) function
/// and the [`with`](QueryableBuilder::with) function
/// of the resulting builder.
///
/// Queryables are automatically undeclared when dropped.
///
/// # Examples
/// ```no_run
/// # async_std::task::block_on(async {
/// use zenoh::prelude::*;
/// use r#async::AsyncResolve;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// let queryable = session
///     .declare_queryable("key/expression")
///     .with(flume::bounded(32))
///     .res()
///     .await
///     .unwrap();
/// while let Ok(query) = queryable.recv_async().await {
///     println!(">> Handling query '{}'", query.selector());
///     query.reply(Ok(Sample::try_from("key/expression", "value").unwrap())).res().await.unwrap();
/// }
/// # })
/// ```
#[derive(Debug)]
pub struct HandlerQueryable<'a, Receiver> {
    pub queryable: CallbackQueryable<'a>,
    pub receiver: Receiver,
}

impl<'a, Receiver> HandlerQueryable<'a, Receiver> {
    #[inline]
    pub fn undeclare(self) -> impl Resolve<crate::Result<()>> + 'a {
        Undeclarable::undeclare(self, ())
    }
}
impl<'a, T> Undeclarable<()> for HandlerQueryable<'a, T> {
    type Output = <CallbackQueryable<'a> as Undeclarable<()>>::Output;
    type Undeclaration = <CallbackQueryable<'a> as Undeclarable<()>>::Undeclaration;
    fn undeclare(self, _: ()) -> Self::Undeclaration {
        Undeclarable::undeclare(self.queryable, ())
    }
}

impl<Receiver> Deref for HandlerQueryable<'_, Receiver> {
    type Target = Receiver;

    fn deref(&self) -> &Self::Target {
        &self.receiver
    }
}

/// A builder for initializing a [`HandlerQueryable`].
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// use zenoh::prelude::*;
/// use r#async::AsyncResolve;
/// use zenoh::queryable;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// let queryable = session
///     .declare_queryable("key/expression")
///     .with(flume::bounded(32))
///     .res()
///     .await
///     .unwrap();
/// # })
/// ```
#[derive(Debug, Clone)]
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
pub struct HandlerQueryableBuilder<'a, 'b, IntoHandler, Receiver>
where
    IntoHandler: crate::prelude::IntoHandler<Query, Receiver>,
{
    pub(crate) builder: QueryableBuilder<'a, 'b>,
    pub(crate) handler: IntoHandler,
    pub(crate) receiver: PhantomData<Receiver>,
}

impl<'a, 'b, IntoHandler, Receiver> HandlerQueryableBuilder<'a, 'b, IntoHandler, Receiver>
where
    IntoHandler: crate::prelude::IntoHandler<Query, Receiver>,
{
    /// Change queryable completeness.
    #[inline]
    pub fn complete(mut self, complete: bool) -> Self {
        self.builder = self.builder.complete(complete);
        self
    }
}

impl<'a, IntoHandler, Receiver> Resolvable
    for HandlerQueryableBuilder<'a, '_, IntoHandler, Receiver>
where
    IntoHandler: crate::prelude::IntoHandler<Query, Receiver>,
{
    type Output = crate::Result<HandlerQueryable<'a, Receiver>>;
}

impl<'a, IntoHandler, Receiver: Send> SyncResolve
    for HandlerQueryableBuilder<'a, '_, IntoHandler, Receiver>
where
    IntoHandler: crate::prelude::IntoHandler<Query, Receiver>,
{
    fn res_sync(self) -> Self::Output {
        let session = self.builder.session;
        let (callback, receiver) = self.handler.into_handler();
        session
            .declare_queryable_inner(
                &(&self.builder.key_expr?).into(),
                self.builder.kind,
                self.builder.complete,
                callback,
            )
            .map(|qable_state| HandlerQueryable {
                queryable: CallbackQueryable {
                    session,
                    state: qable_state,
                    alive: true,
                },
                receiver,
            })
    }
}
impl<IntoHandler, Receiver: Send> AsyncResolve
    for HandlerQueryableBuilder<'_, '_, IntoHandler, Receiver>
where
    IntoHandler: crate::prelude::IntoHandler<Query, Receiver>,
{
    type Future = futures::future::Ready<Self::Output>;
    fn res_async(self) -> Self::Future {
        futures::future::ready(self.res_sync())
    }
}

impl crate::prelude::IntoHandler<Query, flume::Receiver<Query>>
    for (flume::Sender<Query>, flume::Receiver<Query>)
{
    fn into_handler(self) -> crate::prelude::Handler<Query, flume::Receiver<Query>> {
        let (sender, receiver) = self;
        (
            Box::new(move |s| {
                if let Err(e) = sender.send(s) {
                    log::warn!("Error sending query into flume channel: {}", e)
                }
            }),
            receiver,
        )
    }
}

pub type FlumeQueryable<'a> = HandlerQueryable<'a, flume::Receiver<Query>>;
