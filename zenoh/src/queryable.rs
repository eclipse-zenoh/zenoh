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

use crate::net::transport::Primitives;
use crate::prelude::*;
use crate::sync::ZFuture;
use crate::Session;
use crate::SessionRef;
use crate::API_QUERY_RECEPTION_CHANNEL_SIZE;
use futures::{Future, FutureExt};
use std::fmt;
use std::ops::Deref;
use std::pin::Pin;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::RwLock;
use std::task::{Context, Poll};
use zenoh_protocol_core::QueryableInfo;
use zenoh_sync::{derive_zfuture, Runnable};

/// Structs received by a [`Queryable`](Queryable).
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
            result: ResOrFut::Result(result),
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

enum ResOrFut<'a> {
    Result(Result<Sample, Value>),
    Fut(flume::r#async::SendFut<'a, (ZInt, Sample)>),
    Empty(),
}

#[must_use = "ZFutures do nothing unless you `.wait()`, `.await` or poll them"]
pub struct ReplyBuilder<'a> {
    query: &'a Query,
    result: ResOrFut<'a>,
}

impl Future for ReplyBuilder<'_> {
    type Output = zenoh_core::Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if matches!(self.result, ResOrFut::Result(_)) {
            let result = std::mem::replace(&mut self.result, ResOrFut::Empty());
            if let ResOrFut::Result(result) = result {
                match result {
                    Ok(sample) => {
                        self.result = ResOrFut::Fut(
                            self.query
                                .replies_sender
                                .send_async((self.query.kind, sample)),
                        );
                    }
                    Err(_) => {
                        return Poll::Ready(Err(
                            zerror!("Replying errors is not yet supported!").into()
                        ))
                    }
                }
            }
        }

        if let ResOrFut::Fut(fut) = &mut self.result {
            fut.poll_unpin(cx).map_err(|e| zerror!("{}", e).into())
        } else {
            Poll::Ready(Err(zerror!("Not a future!").into()))
        }
    }
}

impl ZFuture for ReplyBuilder<'_> {
    fn wait(self) -> Self::Output {
        if let ResOrFut::Result(result) = self.result {
            match result {
                Ok(sample) => self
                    .query
                    .replies_sender
                    .send((self.query.kind, sample))
                    .map_err(|e| zerror!("{}", e).into()),
                Err(_) => Err(zerror!("Replying errors is not yet supported!").into()),
            }
        } else {
            Err(zerror!("Not a result!").into())
        }
    }
}

pub(crate) struct QueryableState {
    pub(crate) id: Id,
    pub(crate) key_expr: KeyExpr<'static>,
    pub(crate) kind: ZInt,
    pub(crate) complete: bool,
    pub(crate) callback: Callback<Query>,
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
/// `Queryable` implements the `Stream` trait as well as the
/// [`Receiver`](crate::prelude::Receiver) trait which allows to access the queries:
///  - synchronously as with a [`std::sync::mpsc::Receiver`](std::sync::mpsc::Receiver)
///  - asynchronously as with a [`async_std::channel::Receiver`](async_std::channel::Receiver).
/// `Queryable` also provides a [`recv_async()`](Queryable::recv_async) function which allows
/// to access queries asynchronously without needing a mutable reference to the `Queryable`.
///
/// Queryables are automatically undeclared when dropped.
///
/// # Examples
///
/// ### sync
/// ```no_run
/// # use zenoh::prelude::*;
/// # let session = zenoh::open(config::peer()).wait().unwrap();
///
/// let mut queryable = session.queryable("/key/expression").wait().unwrap();
/// while let Ok(query) = queryable.recv() {
///      println!(">> Handling query '{}'", query.selector());
/// }
/// ```
///
/// ### async
/// ```no_run
/// # async_std::task::block_on(async {
/// # use futures::prelude::*;
/// # use zenoh::prelude::*;
/// # let session = zenoh::open(config::peer()).await.unwrap();
///
/// let queryable = session.queryable("/key/expression").await.unwrap();
/// while let Ok(query) = queryable.recv_async().await {
///      println!(">> Handling query '{}'", query.selector());
/// }
/// # })
/// ```
pub struct CallbackQueryable<'a> {
    pub(crate) session: SessionRef<'a>,
    pub(crate) state: Arc<QueryableState>,
    pub(crate) alive: bool,
}

impl CallbackQueryable<'_> {
    /// Close a [`Queryable`](Queryable) previously created with [`queryable`](Session::queryable).
    ///
    /// Queryables are automatically closed when dropped, but you may want to use this function to handle errors or
    /// close the Queryable asynchronously.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(config::peer()).await.unwrap();
    /// let queryable = session.queryable("/key/expression").await.unwrap();
    /// queryable.close().await.unwrap();
    /// # })
    /// ```
    #[inline]
    #[must_use = "ZFutures do nothing unless you `.wait()`, `.await` or poll them"]
    pub fn close(mut self) -> impl ZFuture<Output = crate::Result<()>> {
        self.alive = false;
        self.session.close_queryable(self.state.id)
    }
}

impl Drop for CallbackQueryable<'_> {
    fn drop(&mut self) {
        if self.alive {
            let _ = self.session.close_queryable(self.state.id).wait();
        }
    }
}

impl fmt::Debug for CallbackQueryable<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.state.fmt(f)
    }
}

derive_zfuture! {
    /// A builder for initializing a [`Queryable`](Queryable).
    ///
    /// The result of this builder can be accessed synchronously via [`wait()`](ZFuture::wait())
    /// or asynchronously via `.await`.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use futures::prelude::*;
    /// use zenoh::prelude::*;
    /// use zenoh::queryable;
    ///
    /// let session = zenoh::open(config::peer()).await.unwrap();
    /// let mut queryable = session
    ///     .queryable("/key/expression")
    ///     .await
    ///     .unwrap();
    /// # })
    /// ```
    #[derive(Debug, Clone)]
    pub struct QueryableBuilder<'a, 'b> {
        pub(crate) session: SessionRef<'a>,
        pub(crate) key_expr: KeyExpr<'b>,
        pub(crate) kind: ZInt,
        pub(crate) complete: bool,
    }
}

impl<'a, 'b> QueryableBuilder<'a, 'b> {
    /// Make the built Queryable a [`CallbackQueryable`](CallbackQueryable).
    #[inline]
    pub fn callback<Callback>(self, callback: Callback) -> CallbackQueryableBuilder<'a, 'b>
    where
        Callback: FnMut(Query) + Send + Sync + 'static,
    {
        CallbackQueryableBuilder {
            session: self.session,
            key_expr: self.key_expr,
            kind: self.kind,
            complete: self.complete,
            callback: Arc::new(RwLock::new(callback)),
        }
    }

    /// Make the built Queryable a [`HandlerQueryable`](HandlerQueryable).
    #[inline]
    pub fn with<IntoHandler, Receiver>(
        self,
        handler: IntoHandler,
    ) -> HandlerQueryableBuilder<'a, 'b, Receiver>
    where
        IntoHandler: crate::prelude::IntoHandler<Query, Receiver>,
    {
        HandlerQueryableBuilder {
            session: self.session,
            key_expr: self.key_expr,
            kind: self.kind,
            complete: self.complete,
            handler: Some(handler.into_handler()),
        }
    }

    /// Change queryable completeness.
    #[inline]
    pub fn complete(mut self, complete: bool) -> Self {
        self.complete = complete;
        self
    }
}

impl<'a> Runnable for QueryableBuilder<'a, '_> {
    type Output = crate::Result<HandlerQueryable<'a, flume::Receiver<Query>>>;

    fn run(&mut self) -> Self::Output {
        let (handler, receiver) = flume::bounded(*API_QUERY_RECEPTION_CHANNEL_SIZE).into_handler();
        CallbackQueryableBuilder {
            session: self.session.clone(),
            key_expr: self.key_expr.clone(),
            kind: self.kind,
            complete: self.complete,
            callback: handler,
        }
        .run()
        .map(|queryable| HandlerQueryable {
            queryable,
            receiver,
        })
    }
}

derive_zfuture! {
    /// A builder for initializing a [`Queryable`](Queryable).
    ///
    /// The result of this builder can be accessed synchronously via [`wait()`](ZFuture::wait())
    /// or asynchronously via `.await`.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use futures::prelude::*;
    /// use zenoh::prelude::*;
    /// use zenoh::queryable;
    ///
    /// let session = zenoh::open(config::peer()).await.unwrap();
    /// let mut queryable = session
    ///     .queryable("/key/expression")
    ///     .await
    ///     .unwrap();
    /// # })
    /// ```
    #[derive(Clone)]
    pub struct CallbackQueryableBuilder<'a, 'b> {
        pub(crate) session: SessionRef<'a>,
        pub(crate) key_expr: KeyExpr<'b>,
        pub(crate) kind: ZInt,
        pub(crate) complete: bool,
        pub(crate) callback: Callback<Query>,
    }
}

impl<'a, 'b> CallbackQueryableBuilder<'a, 'b> {
    /// Change queryable completeness.
    #[inline]
    pub fn complete(mut self, complete: bool) -> Self {
        self.complete = complete;
        self
    }
}

impl<'a> Runnable for CallbackQueryableBuilder<'a, '_> {
    type Output = crate::Result<CallbackQueryable<'a>>;

    fn run(&mut self) -> Self::Output {
        log::trace!("queryable({:?})", self.key_expr);
        let mut state = zwrite!(self.session.state);
        let id = state.decl_id_counter.fetch_add(1, Ordering::SeqCst);
        let qable_state = Arc::new(QueryableState {
            id,
            key_expr: self.key_expr.to_owned(),
            kind: self.kind,
            complete: self.complete,
            callback: self.callback.clone(),
        });
        #[cfg(feature = "complete_n")]
        {
            state.queryables.insert(id, qable_state.clone());

            if self.complete {
                let primitives = state.primitives.as_ref().unwrap().clone();
                let complete = Session::complete_twin_qabls(&state, &self.key_expr, self.kind);
                drop(state);
                let qabl_info = QueryableInfo {
                    complete,
                    distance: 0,
                };
                primitives.decl_queryable(&self.key_expr, self.kind, &qabl_info, None);
            }
        }
        #[cfg(not(feature = "complete_n"))]
        {
            let twin_qabl = Session::twin_qabl(&state, &self.key_expr, self.kind);
            let complete_twin_qabl =
                twin_qabl && Session::complete_twin_qabl(&state, &self.key_expr, self.kind);

            state.queryables.insert(id, qable_state.clone());

            if !twin_qabl || (!complete_twin_qabl && self.complete) {
                let primitives = state.primitives.as_ref().unwrap().clone();
                let complete = if !complete_twin_qabl && self.complete {
                    1
                } else {
                    0
                };
                drop(state);
                let qabl_info = QueryableInfo {
                    complete,
                    distance: 0,
                };
                primitives.decl_queryable(&self.key_expr, self.kind, &qabl_info, None);
            }
        }

        Ok(CallbackQueryable {
            session: self.session.clone(),
            state: qable_state,
            alive: true,
        })
    }
}

pub struct HandlerQueryable<'a, Receiver> {
    pub queryable: CallbackQueryable<'a>,
    pub receiver: Receiver,
}

impl<Receiver> HandlerQueryable<'_, Receiver> {
    #[inline]
    #[must_use = "ZFutures do nothing unless you `.wait()`, `.await` or poll them"]
    pub fn close(self) -> impl ZFuture<Output = crate::Result<()>> {
        self.queryable.close()
    }
}

impl<Receiver> Deref for HandlerQueryable<'_, Receiver> {
    type Target = Receiver;

    fn deref(&self) -> &Self::Target {
        &self.receiver
    }
}

/// A builder for initializing a [`Queryable`](Queryable).
///
/// The result of this builder can be accessed synchronously via [`wait()`](ZFuture::wait())
/// or asynchronously via `.await`.
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// use futures::prelude::*;
/// use zenoh::prelude::*;
/// use zenoh::queryable;
///
/// let session = zenoh::open(config::peer()).await.unwrap();
/// let mut queryable = session
///     .queryable("/key/expression")
///     .await
///     .unwrap();
/// # })
/// ```
pub struct HandlerQueryableBuilder<'a, 'b, Receiver> {
    pub(crate) session: SessionRef<'a>,
    pub(crate) key_expr: KeyExpr<'b>,
    pub(crate) kind: ZInt,
    pub(crate) complete: bool,
    pub(crate) handler: Option<crate::prelude::Handler<Query, Receiver>>,
}

impl<'a, 'b, Receiver> HandlerQueryableBuilder<'a, 'b, Receiver> {
    /// Change queryable completeness.
    #[inline]
    pub fn complete(mut self, complete: bool) -> Self {
        self.complete = complete;
        self
    }
}

impl<'a, 'b, U> std::future::Future for HandlerQueryableBuilder<'a, 'b, U>
where
    U: Unpin,
{
    type Output = <Self as Runnable>::Output;

    #[inline]
    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut async_std::task::Context<'_>,
    ) -> std::task::Poll<<Self as ::std::future::Future>::Output> {
        std::task::Poll::Ready(self.run())
    }
}

impl<'a, 'b, U> zenoh_sync::ZFuture for HandlerQueryableBuilder<'a, 'b, U>
where
    U: Send + Sync + Unpin,
{
    #[inline]
    fn wait(mut self) -> Self::Output {
        self.run()
    }
}

impl<U> fmt::Debug for HandlerQueryableBuilder<'_, '_, U> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HandlerQueryableBuilder")
            .field("key_expr", &self.key_expr)
            .field("complete", &self.complete)
            .finish()
    }
}

impl<'a, Receiver> Runnable for HandlerQueryableBuilder<'a, '_, Receiver> {
    type Output = crate::Result<HandlerQueryable<'a, Receiver>>;

    fn run(&mut self) -> Self::Output {
        let (handler, receiver) = self.handler.take().unwrap();
        CallbackQueryableBuilder {
            session: self.session.clone(),
            key_expr: self.key_expr.clone(),
            kind: self.kind,
            complete: self.complete,
            callback: handler,
        }
        .run()
        .map(|queryable| HandlerQueryable {
            queryable,
            receiver,
        })
    }
}

impl crate::prelude::IntoHandler<Query, flume::Receiver<Query>>
    for (flume::Sender<Query>, flume::Receiver<Query>)
{
    fn into_handler(self) -> crate::prelude::Handler<Query, flume::Receiver<Query>> {
        let (sender, receiver) = self;
        (
            std::sync::Arc::new(std::sync::RwLock::new(move |s| {
                if let Err(e) = sender.send(s) {
                    log::warn!("Error sending query into flume channel: {}", e)
                }
            })),
            receiver,
        )
    }
}

pub type FlumeQueryable<'a> = HandlerQueryable<'a, flume::Receiver<Query>>;
