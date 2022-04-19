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

//! Query primitives.

use crate::net::transport::Primitives;
use crate::prelude::*;
use crate::sync::channel::Receiver;
use crate::Session;
use crate::API_REPLY_RECEPTION_CHANNEL_SIZE;
use flume::r#async::RecvFut;
use flume::{bounded, Iter, RecvError, RecvTimeoutError, Sender, TryIter, TryRecvError};
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::atomic::Ordering;
use std::task::{Context, Poll};
use zenoh_sync::{derive_zfuture, zreceiver, Runnable};

/// The [`Queryable`](crate::queryable::Queryable)s that should be target of a [`get`](Session::get).
pub use zenoh_protocol_core::Target;

/// The [`Queryable`](crate::queryable::Queryable)s that should be target of a [`get`](Session::get).
pub use zenoh_protocol_core::QueryTarget;

/// The kind of consolidation.
pub use zenoh_protocol_core::ConsolidationMode;

/// The kind of consolidation that should be applied on replies to a [`get`](Session::get)
/// at different stages of the reply process.
pub use zenoh_protocol_core::ConsolidationStrategy;

/// The replies consolidation strategy to apply on replies to a [`get`](Session::get).
#[derive(Clone, Debug)]
pub enum QueryConsolidation {
    Auto,
    Manual(ConsolidationStrategy),
}

impl QueryConsolidation {
    /// Automatic query consolidation strategy selection.
    ///
    /// A query consolidation strategy will automatically be selected depending
    /// the query selector. If the selector contains time range properties,
    /// no consolidation is performed. Otherwise the [`reception`](QueryConsolidation::reception) strategy is used.
    #[inline]
    pub fn auto() -> Self {
        QueryConsolidation::Auto
    }

    /// No consolidation performed.
    ///
    /// This is usefull when querying timeseries data bases or
    /// when using quorums.
    #[inline]
    pub fn none() -> Self {
        QueryConsolidation::Manual(ConsolidationStrategy::none())
    }

    /// Lazy consolidation performed at all stages.
    ///
    /// This strategy offers the best latency. Replies are directly
    /// transmitted to the application when received without needing
    /// to wait for all replies.
    ///
    /// This mode does not garantie that there will be no duplicates.
    #[inline]
    pub fn lazy() -> Self {
        QueryConsolidation::Manual(ConsolidationStrategy::lazy())
    }

    /// Full consolidation performed at reception.
    ///
    /// This is the default strategy. It offers the best latency while
    /// garantying that there will be no duplicates.
    #[inline]
    pub fn reception() -> Self {
        QueryConsolidation::Manual(ConsolidationStrategy::reception())
    }

    /// Full consolidation performed on last router and at reception.
    ///
    /// This mode offers a good latency while optimizing bandwidth on
    /// the last transport link between the router and the application.
    #[inline]
    pub fn last_router() -> Self {
        QueryConsolidation::Manual(ConsolidationStrategy::last_router())
    }

    /// Full consolidation performed everywhere.
    ///
    /// This mode optimizes bandwidth on all links in the system
    /// but will provide a very poor latency.
    #[inline]
    pub fn full() -> Self {
        QueryConsolidation::Manual(ConsolidationStrategy::full())
    }
}

impl Default for QueryConsolidation {
    fn default() -> Self {
        QueryConsolidation::Auto
    }
}

/// Structs returned by a [`get`](Session::get).
#[derive(Clone, Debug)]
pub struct Reply {
    /// The [`Sample`] for this Reply.
    pub sample: Sample,
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
    /// `ReplyReceiver` implements the `Stream` trait as well as the
    /// [`Receiver`](crate::prelude::Receiver) trait which allows to access the queries:
    ///  - synchronously as with a [`std::sync::mpsc::Receiver`](std::sync::mpsc::Receiver)
    ///  - asynchronously as with a [`async_std::channel::Receiver`](async_std::channel::Receiver).
    /// `ReplyReceiver` also provides a [`recv_async()`](ReplyReceiver::recv_async) function which allows
    /// to access replies asynchronously without needing a mutable reference to the `ReplyReceiver`.
    ///
    ///
    /// # Examples
    ///
    /// ### sync
    /// ```
    /// # use futures::prelude::*;
    /// # use zenoh::prelude::*;
    /// # let session = zenoh::open(config::peer()).wait().unwrap();
    ///
    /// let mut replies = session.get("/key/expression").wait().unwrap();
    /// while let Ok(reply) = replies.recv() {
    ///     println!(">> Received {:?}", reply.sample);
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
    /// let mut replies = session.get("/key/expression").await.unwrap();
    /// while let Some(reply) = replies.next().await {
    ///     println!(">> Received {:?}", reply.sample);
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
    ///     .get("/key/expression?value>1")
    ///     .target(QueryTarget{ kind: queryable::ALL_KINDS, target: Target::All })
    ///     .consolidation(QueryConsolidation::none())
    ///     .await
    ///     .unwrap();
    /// # })
    /// ```
    #[derive(Debug, Clone)]
    pub struct GetBuilder<'a, 'b> {
        pub(crate) session: &'a Session,
        pub(crate) selector: Selector<'b>,
        pub(crate) target: Option<QueryTarget>,
        pub(crate) consolidation: Option<QueryConsolidation>,
        pub(crate) local_routing: Option<bool>,
    }
}

impl<'a, 'b> GetBuilder<'a, 'b> {
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

    /// Enable or disable local routing.
    #[inline]
    pub fn local_routing(mut self, local_routing: bool) -> Self {
        self.local_routing = Some(local_routing);
        self
    }
}

impl Runnable for GetBuilder<'_, '_> {
    type Output = zenoh_core::Result<ReplyReceiver>;

    fn run(&mut self) -> Self::Output {
        log::trace!(
            "get({}, {:?}, {:?})",
            self.selector,
            self.target,
            self.consolidation
        );
        let mut state = zwrite!(self.session.state);
        let target = self.target.take().unwrap();
        let consolidation = match self.consolidation.take().unwrap() {
            QueryConsolidation::Auto => {
                if self.selector.has_time_range() {
                    ConsolidationStrategy::none()
                } else {
                    ConsolidationStrategy::default()
                }
            }
            QueryConsolidation::Manual(strategy) => strategy,
        };
        let local_routing = self.local_routing.unwrap_or(state.local_routing);
        let qid = state.qid_counter.fetch_add(1, Ordering::SeqCst);
        let (rep_sender, rep_receiver) = bounded(*API_REPLY_RECEPTION_CHANNEL_SIZE);
        let nb_final = if local_routing { 2 } else { 1 };
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

        drop(state);
        primitives.send_query(
            &self.selector.key_selector,
            self.selector.value_selector.as_ref(),
            qid,
            target.clone(),
            consolidation.clone(),
            None,
        );
        if local_routing {
            self.session.handle_query(
                true,
                &self.selector.key_selector,
                self.selector.value_selector.as_ref(),
                qid,
                target,
                consolidation,
            );
        }

        Ok(ReplyReceiver::new(rep_receiver))
    }
}
