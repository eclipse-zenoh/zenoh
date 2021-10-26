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
use crate::API_QUERY_RECEPTION_CHANNEL_SIZE;
use async_std::sync::Arc;
use flume::{
    bounded, Iter, RecvError, RecvTimeoutError, Sender, TryIter, TryRecvError, TrySendError,
};
use std::fmt;
use std::pin::Pin;
use std::sync::atomic::Ordering;
use std::task::{Context, Poll};
use zenoh_util::sync::Runnable;

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
    pub fn try_reply(&self, msg: Sample) -> Result<(), TrySendError<Sample>> {
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
    /// A [`Receiver`] of [`Query`].
    ///
    /// Returned by [`Queryable`](crate::Queryable).[`receiver`](crate::Queryable::receiver)(), it must be used
    /// to wait for queries mathing the [`Queryable`](crate::Queryable).
    ///
    /// The queries of this receiver can be accessed:
    ///  - synchronously as with a [`std::sync::mpsc::Receiver`](std::sync::mpsc::Receiver)
    ///  - asynchronously as with a [`async_std::channel::Receiver`](async_std::channel::Receiver).
    ///
    /// # Examples
    ///
    /// ### sync
    /// ```no_run
    /// # use zenoh::prelude::*;
    /// # let session = zenoh::open(config::peer()).wait().unwrap();
    ///
    /// let mut queryable = session.register_queryable("/resource/name").wait().unwrap();
    /// while let Ok(query) = queryable.receiver().recv() {
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
    /// let mut queryable = session.register_queryable("/resource/name").await.unwrap();
    /// while let Some(query) = queryable.receiver().next().await {
    ///      println!(">> Handling query '{}'", query.selector());
    /// }
    /// # })
    /// ```
    #[derive(Clone)]
    pub struct QueryReceiver : Receiver<Query> {}
}

/// An entity able to reply to queries.
///
/// Queryables are automatically unregistered when dropped.
pub struct Queryable<'a> {
    pub(crate) session: &'a Session,
    pub(crate) state: Arc<QueryableState>,
    pub(crate) alive: bool,
    pub(crate) receiver: QueryReceiver,
}

impl Queryable<'_> {
    /// Gets the [`QueryReceiver`](crate::queryable::QueryReceiver) of this `Queryable`.
    ///
    /// This receiver must be used to listen for incomming queries.
    ///
    /// # Examples
    /// ```no_run
    /// # async_std::task::block_on(async {
    /// use futures::prelude::*;
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(config::peer()).await.unwrap();
    /// let mut queryable = session.register_queryable("/resource/name").await.unwrap();
    /// while let Some(query) = queryable.receiver().next().await {
    ///      println!(">> Handling query '{}'", query.selector());
    ///      query.reply(Sample::new("/resource/name".to_string(), "some value"));
    /// }
    /// # })
    /// ```
    pub fn receiver(&mut self) -> &mut QueryReceiver {
        &mut self.receiver
    }

    /// Undeclare a [`Queryable`](Queryable) previously declared with [`register_queryable`](Session::register_queryable).
    ///
    /// Queryables are automatically unregistered when dropped, but you may want to use this function to handle errors or
    /// unregister the Queryable asynchronously.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(config::peer()).await.unwrap();
    /// let queryable = session.register_queryable("/resource/name").await.unwrap();
    /// queryable.unregister().await.unwrap();
    /// # })
    /// ```
    #[inline]
    #[must_use = "ZFutures do nothing unless you `.wait()`, `.await` or poll them"]
    pub fn unregister(mut self) -> impl ZFuture<Output = ZResult<()>> {
        self.alive = false;
        self.session.unregister_queryable(self.state.id)
    }
}

impl Drop for Queryable<'_> {
    fn drop(&mut self) {
        if self.alive {
            let _ = self.session.unregister_queryable(self.state.id).wait();
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
    pub fn try_send(&self, msg: Sample) -> Result<(), TrySendError<Sample>> {
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
    ///     .register_queryable("/resource/name")
    ///     .kind(queryable::EVAL)
    ///     .await
    ///     .unwrap();
    /// # })
    /// ```
    #[derive(Debug, Clone)]
    pub struct QueryableBuilder<'a, 'b> {
        pub(crate) session: &'a Session,
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
    type Output = ZResult<Queryable<'a>>;

    fn run(&mut self) -> Self::Output {
        log::trace!("register_queryable({:?}, {:?})", self.key_expr, self.kind);
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

        Ok(Queryable {
            session: self.session,
            state: qable_state,
            alive: true,
            receiver: QueryReceiver::new(receiver),
        })
    }
}
