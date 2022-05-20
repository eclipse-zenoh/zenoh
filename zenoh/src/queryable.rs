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
use crate::API_QUERY_RECEPTION_CHANNEL_SIZE;
use futures::FutureExt;
use std::fmt;
use std::marker::PhantomData;
use std::ops::Deref;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use zenoh_core::AsyncResolve;
use zenoh_core::Resolvable;
use zenoh_core::Resolve;
use zenoh_core::SyncResolve;

/// Structs received by a [`Queryable`](HandlerQueryable).
pub struct Query {
    /// The key_selector of this Query.
    pub(crate) key_selector: KeyExpr<'static>,
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
            key_selector: self.key_selector.clone(),
            value_selector: (&self.value_selector).into(),
        }
    }

    /// The key selector part of this Query.
    #[inline(always)]
    pub fn key_selector(&self) -> &KeyExpr<'_> {
        &self.key_selector
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
        write!(
            f,
            "Query{{ key_selector: '{}', value_selector: '{}' }}",
            self.key_selector, self.value_selector
        )
    }
}

impl fmt::Display for Query {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Query{{ '{}{}' }}",
            self.key_selector, self.value_selector
        )
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
    pub(crate) key_expr: KeyExpr<'static>,
    pub(crate) kind: ZInt,
    pub(crate) complete: bool,
    pub(crate) callback: Arc<dyn Fn(Query) + Send + Sync>,
}

impl fmt::Debug for QueryableState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Queryable{{ id:{}, key_expr:{} }}",
            self.id, self.key_expr
        )
    }
}

/// An entity able to reply to queries.
///
/// Queryables are automatically undeclared when dropped.
///
/// # Examples
///
/// ### sync
/// ```no_run
/// # use zenoh::prelude::*;
/// # use sync::SyncResolve;
/// # let session = zenoh::open(config::peer()).res().unwrap();
///
/// let mut queryable = session.queryable("/key/expression").res().unwrap();
/// while let Ok(query) = queryable.recv() {
///      println!(">> Handling query '{}'", query.selector());
/// }
/// ```
///
/// ### async
/// ```no_run
/// # async_std::task::block_on(async {
/// # use futures::prelude::*;
/// # use r#async::AsyncResolve;
/// # use zenoh::prelude::*;
/// # let session = zenoh::open(config::peer()).res().await.unwrap();
///
/// let queryable = session.queryable("/key/expression").res().await.unwrap();
/// while let Ok(query) = queryable.recv_async().await {
///      println!(">> Handling query '{}'", query.selector());
/// }
/// # })
/// ```
pub struct CallbackQueryable<'a> {
    pub(crate) session: SessionRef<'a>,
    pub(crate) state: Arc<QueryableState>,
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
    /// let queryable = session.queryable("/key/expression").res().await.unwrap();
    /// queryable.close().res().await.unwrap();
    /// # })
    /// ```
    #[inline]
    pub fn close(self) -> impl Resolve<crate::Result<()>> + 'a {
        QueryableCloser {
            queryable: std::mem::ManuallyDrop::new(self),
            alive: true,
        }
    }
}
pub struct QueryableCloser<'a> {
    queryable: std::mem::ManuallyDrop<CallbackQueryable<'a>>,
    alive: bool,
}
impl Resolvable for QueryableCloser<'_> {
    type Output = crate::Result<()>;
}
impl SyncResolve for QueryableCloser<'_> {
    fn res_sync(mut self) -> Self::Output {
        self.alive = false;
        self.queryable
            .session
            .close_queryable(self.queryable.state.id)
    }
}

#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct QueryableCloserFuture(futures::future::Ready<crate::Result<()>>);
impl std::future::Future for QueryableCloserFuture {
    type Output = crate::Result<()>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.get_mut().0.poll_unpin(cx)
    }
}
impl AsyncResolve for QueryableCloser<'_> {
    type Future = QueryableCloserFuture;
    fn res_async(self) -> Self::Future {
        QueryableCloserFuture(futures::future::ready(self.res_sync()))
    }
}
impl Drop for QueryableCloser<'_> {
    fn drop(&mut self) {
        if self.alive {
            unsafe { std::mem::ManuallyDrop::drop(&mut self.queryable) }
        }
    }
}

impl Drop for CallbackQueryable<'_> {
    fn drop(&mut self) {
        let _ = self.session.close_queryable(self.state.id);
    }
}

impl fmt::Debug for CallbackQueryable<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.state.fmt(f)
    }
}

/// A builder for initializing a [`Queryable`](HandlerQueryable).
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// use zenoh::prelude::*;
/// use r#async::AsyncResolve;
/// use zenoh::queryable;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// let queryable = session.queryable("/key/expression").res().await.unwrap();
/// # })
/// ```
#[derive(Debug, Clone)]
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
pub struct QueryableBuilder<'a, 'b> {
    pub(crate) session: SessionRef<'a>,
    pub(crate) key_expr: KeyExpr<'b>,
    pub(crate) kind: ZInt,
    pub(crate) complete: bool,
}

impl<'a, 'b> QueryableBuilder<'a, 'b> {
    /// Make the built Queryable a [`CallbackQueryable`](CallbackQueryable).
    #[inline]
    pub fn callback<Callback>(
        self,
        callback: Callback,
    ) -> CallbackQueryableBuilder<'a, 'b, Callback>
    where
        Callback: Fn(Query) + Send + Sync + 'static,
    {
        CallbackQueryableBuilder {
            session: self.session,
            key_expr: self.key_expr,
            kind: self.kind,
            complete: self.complete,
            callback,
        }
    }

    /// Make the built Queryable a [`CallbackQueryable`](CallbackQueryable).
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

    /// Make the built Queryable a [`HandlerQueryable`](HandlerQueryable).
    #[inline]
    pub fn with<IntoHandler, Receiver>(
        self,
        handler: IntoHandler,
    ) -> HandlerQueryableBuilder<'a, 'b, IntoHandler, Receiver>
    where
        IntoHandler: crate::prelude::IntoHandler<Query, Receiver>,
    {
        HandlerQueryableBuilder {
            session: self.session,
            key_expr: self.key_expr,
            kind: self.kind,
            complete: self.complete,
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
/// let queryable = session.queryable("/key/expression").res().await.unwrap();
/// # })
/// ```
#[derive(Clone)]
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
pub struct CallbackQueryableBuilder<'a, 'b, Callback>
where
    Callback: Fn(Query) + Send + Sync + 'static,
{
    pub(crate) session: SessionRef<'a>,
    pub(crate) key_expr: KeyExpr<'b>,
    pub(crate) kind: ZInt,
    pub(crate) complete: bool,
    pub(crate) callback: Callback,
}

impl<'a, 'b, Callback> CallbackQueryableBuilder<'a, 'b, Callback>
where
    Callback: Fn(Query) + Send + Sync + 'static,
{
    /// Change queryable completeness.
    #[inline]
    pub fn complete(mut self, complete: bool) -> Self {
        self.complete = complete;
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
        let session = self.session;
        session
            .declare_queryable(
                &self.key_expr,
                self.kind,
                self.complete,
                Box::new(self.callback),
            )
            .map(move |qable_state| CallbackQueryable {
                session,
                state: qable_state,
            })
    }
}

pub struct HandlerQueryable<'a, Receiver> {
    pub queryable: CallbackQueryable<'a>,
    pub receiver: Receiver,
}

impl<'a, Receiver> HandlerQueryable<'a, Receiver> {
    #[inline]
    pub fn close(self) -> impl Resolve<crate::Result<()>> + 'a {
        self.queryable.close()
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
/// let queryable = session.queryable("/key/expression").res().await.unwrap();
/// # })
/// ```
#[derive(Clone)]
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
pub struct HandlerQueryableBuilder<'a, 'b, IntoHandler, Receiver>
where
    IntoHandler: crate::prelude::IntoHandler<Query, Receiver>,
{
    pub(crate) session: SessionRef<'a>,
    pub(crate) key_expr: KeyExpr<'b>,
    pub(crate) kind: ZInt,
    pub(crate) complete: bool,
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
        self.complete = complete;
        self
    }
}

impl<IntoHandler, Receiver> fmt::Debug for HandlerQueryableBuilder<'_, '_, IntoHandler, Receiver>
where
    IntoHandler: crate::prelude::IntoHandler<Query, Receiver>,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HandlerQueryableBuilder")
            .field("key_expr", &self.key_expr)
            .field("complete", &self.complete)
            .finish()
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
        let session = self.session;
        let (callback, receiver) = self.handler.into_handler();
        session
            .declare_queryable(&self.key_expr, self.kind, self.complete, callback)
            .map(|qable_state| HandlerQueryable {
                queryable: CallbackQueryable {
                    session,
                    state: qable_state,
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
