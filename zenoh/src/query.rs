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

//! Query primitives.

use crate::net::transport::Primitives;
use crate::prelude::*;
use crate::sync::channel::Receiver;
use crate::Session;
use crate::API_REPLY_RECEPTION_CHANNEL_SIZE;
use flume::{bounded, Iter, RecvError, RecvTimeoutError, Sender, TryIter, TryRecvError};
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::atomic::Ordering;
use std::task::{Context, Poll};
use zenoh_util::sync::Runnable;

/// The [`Queryable`](crate::queryable::Queryable)s that should be target of a [`get`](Session::get).
pub use super::net::protocol::core::Target;

/// The [`Queryable`](crate::queryable::Queryable)s that should be target of a [`get`](Session::get).
pub use super::net::protocol::core::QueryTarget;

/// The kind of consolidation.
pub use super::net::protocol::core::ConsolidationMode;

/// The kind of consolidation that should be applied on replies to a [`get`](Session::get)
/// at different stages of the reply process.
pub use super::net::protocol::core::QueryConsolidation;

/// Structs returned by a [`get`](Session::get).
#[derive(Clone, Debug)]
pub struct Reply {
    /// The [`Sample`] for this Reply.
    pub data: Sample,
    /// The kind of [`Queryable`](crate::queryable::Queryable) that answered this Reply.
    pub replier_kind: ZInt,
    /// The id of the zenoh instance that answered this Reply.
    pub replier_id: PeerId,
}

#[derive(Clone, Debug)]
pub(crate) struct QueryState {
    pub(crate) nb_final: usize,
    pub(crate) reception_mode: ConsolidationMode,
    pub(crate) replies: Option<HashMap<String, Reply>>,
    pub(crate) rep_sender: Sender<Reply>,
}

zreceiver! {
    /// A [`Receiver`] of [`Reply`], result of a [`get`](crate::Session::get) operation.
    ///
    /// The replies of this receiver can be accessed:
    ///  - synchronously as with a [`std::sync::mpsc::Receiver`](std::sync::mpsc::Receiver)
    ///  - asynchronously as with a [`async_std::channel::Receiver`](async_std::channel::Receiver).
    ///
    /// # Examples
    ///
    /// ### sync
    /// ```
    /// # use futures::prelude::*;
    /// # use zenoh::prelude::*;
    /// # let session = zenoh::open(config::peer()).wait().unwrap();
    ///
    /// let mut replies = session.get("/resource/name").wait().unwrap();
    /// while let Ok(reply) = replies.recv() {
    ///     println!(">> Received {:?}", reply.data);
    /// }
    /// ```
    ///
    /// ### async
    /// ```
    /// # async_std::task::block_on(async {
    /// # use futures::prelude::*;
    /// # use zenoh::prelude::*;
    /// # let session = zenoh::open(config::peer()).await.unwrap();
    ///
    /// let mut replies = session.get("/resource/name").await.unwrap();
    /// while let Some(reply) = replies.next().await {
    ///     println!(">> Received {:?}", reply.data);
    /// }
    /// # })
    /// ```
    #[derive(Clone)]
    pub struct ReplyReceiver : Receiver<Reply> {}
}

derive_zfuture! {
    /// A builder for initializing a `query`.
    ///
    /// The result of the query is provided as a [`ReplyReceiver`](ReplyReceiver) and can be
    /// accessed synchronously via [`wait()`](ZFuture::wait()) or asynchronously via `.await`.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use futures::prelude::*;
    /// use zenoh::prelude::*;
    /// use zenoh::query::*;
    /// use zenoh::queryable;
    ///
    /// let session = zenoh::open(config::peer()).await.unwrap();
    /// let mut replies = session
    ///     .get("/resource/name?value>1")
    ///     .target(QueryTarget{ kind: queryable::ALL_KINDS, target: Target::All })
    ///     .consolidation(QueryConsolidation::none())
    ///     .await
    ///     .unwrap();
    /// # })
    /// ```
    #[derive(Debug, Clone)]
    pub struct Getter<'a> {
        pub(crate) session: &'a Session,
        pub(crate) selector: KeyedSelector<'a>,
        pub(crate) target: Option<QueryTarget>,
        pub(crate) consolidation: Option<QueryConsolidation>,
    }
}

impl<'a> Getter<'a> {
    /// Change the target of the query.
    #[inline]
    pub fn target(mut self, target: QueryTarget) -> Self {
        self.target = Some(target);
        self
    }

    /// Change the consolidation mode of the query.
    #[inline]
    pub fn consolidation(mut self, consolidation: QueryConsolidation) -> Self {
        self.consolidation = Some(consolidation);
        self
    }
}

impl Runnable for Getter<'_> {
    type Output = ZResult<ReplyReceiver>;

    fn run(&mut self) -> Self::Output {
        log::trace!(
            "get({}, {:?}, {:?})",
            self.selector,
            self.target,
            self.consolidation
        );
        let mut state = zwrite!(self.session.state);
        let target = self.target.take().unwrap();
        let consolidation = self.consolidation.take().unwrap();
        let qid = state.qid_counter.fetch_add(1, Ordering::SeqCst);
        let (rep_sender, rep_receiver) = bounded(*API_REPLY_RECEPTION_CHANNEL_SIZE);
        let nb_final = if state.local_routing { 2 } else { 1 };
        log::trace!("Register query {} (nb_final = {})", qid, nb_final);
        state.queries.insert(
            qid,
            QueryState {
                nb_final,
                reception_mode: consolidation.reception,
                replies: if consolidation.reception != ConsolidationMode::None {
                    Some(HashMap::new())
                } else {
                    None
                },
                rep_sender,
            },
        );

        let primitives = state.primitives.as_ref().unwrap().clone();
        let local_routing = state.local_routing;
        drop(state);
        primitives.send_query(
            &self.selector.key_selector,
            self.selector.value_selector,
            qid,
            target.clone(),
            consolidation.clone(),
            None,
        );
        if local_routing {
            self.session.handle_query(
                true,
                &self.selector.key_selector,
                self.selector.value_selector,
                qid,
                target,
                consolidation,
            );
        }

        Ok(ReplyReceiver::new(rep_receiver))
    }
}
