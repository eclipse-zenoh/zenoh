//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//

//! Queryable primitives.

use super::net::protocol::core::QueryableInfo;
use super::net::transport::Primitives;
use crate::prelude::*;
use crate::sync::channel::Receiver;
use crate::sync::ZFuture;
use crate::Session;
use crate::SessionRef;
use crate::API_QUERY_RECEPTION_CHANNEL_SIZE;
use async_std::sync::Arc;
use flume::r#async::RecvFut;
use flume::{
    bounded, Iter, RecvError, RecvTimeoutError, Sender, TryIter, TryRecvError, TrySendError,
};
use std::fmt;
use std::pin::Pin;
use std::sync::atomic::Ordering;
use std::task::{Context, Poll};
use zenoh_sync::{derive_zfuture, zreceiver, Runnable};

pub use super::net::protocol::core::queryable::*;

/// Structs received by a [`Queryable`](Queryable).
pub struct Query {
    /// The key_selector of this Query.
    pub(crate) key_selector: KeyExpr<'static>,
    /// The value_selector of this Query.
    pub(crate) value_selector: String,
    /// The sender to use to send replies to this query.
    /// When this sender is dropped, the reply is finalized.
    pub replies_sender: RepliesSender,
}

impl Query {
    /// The full [`Selector`] of this Query.
    #[inline(always)]
    pub fn selector(&self) -> Selector<'_> {
        Selector {
            key_selector: self.key_selector.clone(),
            value_selector: &self.value_selector,
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
    pub fn reply(&'_ self, msg: Sample) {
        self.replies_sender.send(msg);
    }

    /// Tries sending a reply to this Query.
    #[inline(always)]
    pub fn try_reply(&self, msg: Sample) -> core::result::Result<(), TrySendError<Sample>> {
        self.replies_sender.try_send(msg)
    }

    /// Sends a reply to this Query asynchronously.
    #[inline(always)]
    pub async fn reply_async(&'_ self, msg: Sample) {
        self.replies_sender.send_async(msg).await;
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

pub(crate) struct QueryableState {
    pub(crate) id: Id,
    pub(crate) key_expr: KeyExpr<'static>,
    pub(crate) kind: ZInt,
    pub(crate) complete: bool,
    pub(crate) sender: Sender<Query>,
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

zreceiver! {
    /// A [`Receiver`] of [`Query`] returned by
    /// [`Queryable::receiver()`](Queryable::receiver).
    ///
    /// `QueryReceiver` implements the `Stream` trait as well as the
    /// [`Receiver`](crate::prelude::Receiver) trait which allows to access the queries:
    ///  - synchronously as with a [`std::sync::mpsc::Receiver`](std::sync::mpsc::Receiver)
    ///  - asynchronously as with a [`async_std::channel::Receiver`](async_std::channel::Receiver).
    /// `QueryReceiver` also provides a [`recv_async()`](QueryReceiver::recv_async) function which allows
    /// to access queries asynchronously without needing a mutable reference to the `QueryReceiver`.
    ///
    /// `QueryReceiver` implements `Clonable` and it's lifetime is not bound to it's associated
    /// [`Queryable`]. This is useful to move multiple instances to multiple threads/tasks and perform
    /// job stealing. When the associated [`QueryReceiver`] is closed or dropped and all queries
    /// have been received [`Receiver::recv()`](Receiver::recv) will return  `Error` and
    /// `Receiver::next()` will return `None`.
    ///
    /// Examples:
    /// ```
    /// # async_std::task::block_on(async {
    /// use futures::prelude::*;
    /// use zenoh::prelude::*;
    /// let session = zenoh::open(config::peer()).await.unwrap();
    ///
    /// let mut queryable = session.queryable("/key/expression").wait().unwrap();
    /// let task1 = async_std::task::spawn({
    ///     let mut receiver = queryable.receiver().clone();
    ///     async move {
    ///         while let Some(query) = receiver.next().await {
    ///             println!(">> Task1 handling query '{}'", query.selector());
    ///         }
    ///     }
    /// });
    /// let task2 = async_std::task::spawn({
    ///     let mut receiver = queryable.receiver().clone();
    ///     async move {
    ///         while let Some(query) = receiver.next().await {
    ///             println!(">> Task2 handling query '{}'", query.selector());
    ///         }
    ///     }
    /// });
    ///
    /// async_std::task::sleep(std::time::Duration::from_secs(1)).await;
    /// queryable.close().await.unwrap();
    /// futures::join!(task1, task2);
    /// # })
    #[derive(Clone)]
    pub struct QueryReceiver : Receiver<Query> {}
}

zreceiver! {
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
    /// let mut queryable = session.queryable("/key/expression").await.unwrap();
    /// while let Some(query) = queryable.next().await {
    ///      println!(">> Handling query '{}'", query.selector());
    /// }
    /// # })
    /// ```
    pub struct Queryable<'a> : Receiver<Query> {
        pub(crate) session: SessionRef<'a>,
        pub(crate) state: Arc<QueryableState>,
        pub(crate) alive: bool,
        pub(crate) query_receiver: QueryReceiver,
    }
}

impl Queryable<'_> {
    /// Returns a `Clonable` [`QueryReceiver`] which lifetime is not bound to
    /// the associated `Queryable` lifetime.
    pub fn receiver(&mut self) -> &mut QueryReceiver {
        &mut self.query_receiver
    }

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

impl Drop for Queryable<'_> {
    fn drop(&mut self) {
        if self.alive {
            let _ = self.session.close_queryable(self.state.id).wait();
        }
    }
}

impl fmt::Debug for Queryable<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.state.fmt(f)
    }
}

/// Struct used by a [`Queryable`](Queryable) to send replies to queries.
#[derive(Clone)]
pub struct RepliesSender {
    pub(crate) kind: ZInt,
    pub(crate) sender: Sender<(ZInt, Sample)>,
}

impl RepliesSender {
    #[inline(always)]
    /// Send a reply.
    pub fn send(&'_ self, msg: Sample) {
        if let Err(e) = self.sender.send((self.kind, msg)) {
            log::error!("Error sending reply: {}", e);
        }
    }

    /// Attempt to send a reply. If the channel is full, an error is returned.
    #[inline(always)]
    pub fn try_send(&self, msg: Sample) -> core::result::Result<(), TrySendError<Sample>> {
        match self.sender.try_send((self.kind, msg)) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(sample)) => Err(TrySendError::Full(sample.1)),
            Err(TrySendError::Disconnected(sample)) => Err(TrySendError::Disconnected(sample.1)),
        }
    }

    #[inline(always)]
    /// Returns the channel capacity of this `RepliesSender`.
    pub fn capacity(&self) -> usize {
        self.sender.capacity().unwrap_or(0)
    }

    #[inline(always)]
    /// Returns true the channel of this `RepliesSender` is empty.
    pub fn is_empty(&self) -> bool {
        self.sender.is_empty()
    }

    #[inline(always)]
    /// Returns true the channel of this `RepliesSender` is full.
    pub fn is_full(&self) -> bool {
        self.sender.is_full()
    }

    #[inline(always)]
    /// Returns the number of replies in the channel of this `RepliesSender`.
    pub fn len(&self) -> usize {
        self.sender.len()
    }

    #[inline(always)]
    /// Asynchronously send a reply. If the channel is full, the returned future
    /// will yield to the async runtime.
    pub async fn send_async(&self, msg: Sample) {
        if let Err(e) = self.sender.send_async((self.kind, msg)).await {
            log::error!("Error sending reply: {}", e);
        }
    }

    // @TODO
    // #[inline(always)]
    // pub fn sink(&self) -> flume::r#async::SendSink<'_, Sample> {
    //     self.sender.sink()
    // }
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
    ///     .kind(queryable::EVAL)
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
    /// Change the queryable kind.
    #[inline]
    pub fn kind(mut self, kind: ZInt) -> Self {
        self.kind = kind;
        self
    }

    /// Change queryable completeness.
    #[inline]
    pub fn complete(mut self, complete: bool) -> Self {
        self.complete = complete;
        self
    }
}

impl<'a> Runnable for QueryableBuilder<'a, '_> {
    type Output = crate::Result<Queryable<'a>>;

    fn run(&mut self) -> Self::Output {
        log::trace!("queryable({:?}, {:?})", self.key_expr, self.kind);
        let mut state = zwrite!(self.session.state);
        let id = state.decl_id_counter.fetch_add(1, Ordering::SeqCst);
        let (sender, receiver) = bounded(*API_QUERY_RECEPTION_CHANNEL_SIZE);
        let qable_state = Arc::new(QueryableState {
            id,
            key_expr: self.key_expr.to_owned(),
            kind: self.kind,
            complete: self.complete,
            sender,
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

        Ok(Queryable::new(
            self.session.clone(),
            qable_state,
            true,
            QueryReceiver::new(receiver.clone()),
            receiver,
        ))
    }
}
