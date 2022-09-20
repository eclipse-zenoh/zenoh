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

use crate::handlers::{locked, Callback, DefaultHandler};
use crate::net::runtime::Runtime;
use crate::prelude::*;
use crate::Session;
use crate::SessionState;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use zenoh_collections::Timed;
use zenoh_core::zresult::ZResult;
use zenoh_core::{AsyncResolve, Resolvable, SyncResolve};

/// The [`Queryable`](crate::queryable::Queryable)s that should be target of a [`get`](Session::get).
pub use zenoh_protocol_core::QueryTarget;

/// The kind of consolidation.
pub use zenoh_protocol_core::ConsolidationMode;

/// The replies consolidation strategy to apply on replies to a [`get`](Session::get).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueryConsolidation {
    pub(crate) mode: Option<ConsolidationMode>,
}

impl QueryConsolidation {
    /// Automatic query consolidation strategy selection.
    pub const AUTO: Self = Self { mode: None };

    pub(crate) const fn from_mode(mode: ConsolidationMode) -> Self {
        Self { mode: Some(mode) }
    }

    /// Returns the requested [`ConsolidationMode`].
    ///
    /// Returns `None` if the mode selection is left to automatic.
    pub fn mode(&self) -> Option<ConsolidationMode> {
        self.mode
    }
}
impl From<ConsolidationMode> for QueryConsolidation {
    fn from(mode: ConsolidationMode) -> Self {
        Self::from_mode(mode)
    }
}

impl Default for QueryConsolidation {
    fn default() -> Self {
        QueryConsolidation::AUTO
    }
}

/// Structs returned by a [`get`](Session::get).
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct Reply {
    /// The result of this Reply.
    pub sample: Result<Sample, Value>,
    /// The id of the zenoh instance that answered this Reply.
    pub replier_id: ZenohId,
}

#[derive(Clone)]
pub(crate) struct QueryTimeout {
    pub(crate) state: Arc<RwLock<SessionState>>,
    pub(crate) runtime: Runtime,
    pub(crate) qid: ZInt,
}

#[async_trait]
impl Timed for QueryTimeout {
    async fn run(&mut self) {
        let mut state = zwrite!(self.state);
        if let Some(query) = state.queries.remove(&self.qid) {
            std::mem::drop(state);
            log::debug!("Timout on query {}! Send error and close.", self.qid);
            if query.reception_mode == ConsolidationMode::Latest {
                for (_, reply) in query.replies.unwrap().into_iter() {
                    let _ = (query.callback)(reply);
                }
            }
            let _ = (query.callback)(Reply {
                sample: Err("Timeout".into()),
                replier_id: self.runtime.zid,
            });
        }
    }
}

pub(crate) struct QueryState {
    pub(crate) nb_final: usize,
    pub(crate) selector: Selector<'static>,
    pub(crate) reception_mode: ConsolidationMode,
    pub(crate) replies: Option<HashMap<OwnedKeyExpr, Reply>>,
    pub(crate) callback: Callback<'static, Reply>,
}

/// A builder for initializing a `query`.
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// use zenoh::prelude::*;
/// use r#async::AsyncResolve;
/// use zenoh::query::*;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// let replies = session
///     .get("key/expression?value>1")
///     .target(QueryTarget::All)
///     .consolidation(ConsolidationMode::None)
///     .res()
///     .await
///     .unwrap();
/// while let Ok(reply) = replies.recv_async().await {
///     println!("Received {:?}", reply.sample)
/// }
/// # })
/// ```
#[derive(Debug)]
pub struct GetBuilder<'a, 'b, Handler> {
    pub(crate) session: &'a Session,
    pub(crate) selector: ZResult<Selector<'b>>,
    pub(crate) target: QueryTarget,
    pub(crate) consolidation: QueryConsolidation,
    pub(crate) timeout: Duration,
    pub(crate) handler: Handler,
}

impl<'a, 'b> GetBuilder<'a, 'b, DefaultHandler> {
    /// Receive the replies for this query with a callback.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    /// use r#async::AsyncResolve;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let queryable = session
    ///     .get("key/expression")
    ///     .callback(|reply| {println!("Received {:?}", reply.sample);})
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// # })
    /// ```
    #[inline]
    pub fn callback<Callback>(self, callback: Callback) -> GetBuilder<'a, 'b, Callback>
    where
        Callback: Fn(Reply) + Send + Sync + 'static,
    {
        let GetBuilder {
            session,
            selector,
            target,
            consolidation,
            timeout,
            handler: _,
        } = self;
        GetBuilder {
            session,
            selector,
            target,
            consolidation,
            timeout,
            handler: callback,
        }
    }

    /// Receive the replies for this query with a mutable callback.
    ///
    /// Using this guarantees that your callback will never be called concurrently.
    /// If your callback is also accepted by the [`callback`](GetBuilder::callback) method, we suggest you use it instead of `callback_mut`
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
    ///     .get("key/expression")
    ///     .callback_mut(move |reply| {n += 1;})
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// # })
    /// ```
    #[inline]
    pub fn callback_mut<CallbackMut>(
        self,
        callback: CallbackMut,
    ) -> GetBuilder<'a, 'b, impl Fn(Reply) + Send + Sync + 'static>
    where
        CallbackMut: FnMut(Reply) + Send + Sync + 'static,
    {
        self.callback(locked(callback))
    }

    /// Receive the replies for this query with a [`Handler`](crate::prelude::IntoCallbackReceiverPair).
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    /// use r#async::AsyncResolve;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let replies = session
    ///     .get("key/expression")
    ///     .with(flume::bounded(32))
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// while let Ok(reply) = replies.recv_async().await {
    ///     println!("Received {:?}", reply.sample);
    /// }
    /// # })
    /// ```
    #[inline]
    pub fn with<Handler>(self, handler: Handler) -> GetBuilder<'a, 'b, Handler>
    where
        Handler: crate::prelude::IntoCallbackReceiverPair<'static, Reply>,
    {
        let GetBuilder {
            session,
            selector,
            target,
            consolidation,
            timeout,
            handler: _,
        } = self;
        GetBuilder {
            session,
            selector,
            target,
            consolidation,
            timeout,
            handler,
        }
    }
}
impl<'a, 'b, Handler> GetBuilder<'a, 'b, Handler> {
    /// Change the target of the query.
    #[inline]
    pub fn target(mut self, target: QueryTarget) -> Self {
        self.target = target;
        self
    }

    /// Change the consolidation mode of the query.
    #[inline]
    pub fn consolidation<QC: Into<QueryConsolidation>>(mut self, consolidation: QC) -> Self {
        self.consolidation = consolidation.into();
        self
    }

    /// Set query timeout.
    #[inline]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// By default, `get` will filter out
    #[zenoh_core::unstable]
    pub fn accept_replies(self, value: ReplyKeyExpr) -> Self {
        let Self {
            session,
            selector,
            target,
            consolidation,
            timeout,
            handler,
        } = self;
        Self {
            session,
            selector: selector.and_then(|mut s| {
                let any_selparam = s.parameter_index(_REPLY_KEY_EXPR_ANY_SEL_PARAM)?;
                match (value, any_selparam) {
                    (ReplyKeyExpr::Any, None) => {
                        let s = s.parameters_mut();
                        if !s.is_empty() {
                            s.push('&')
                        }
                        s.push_str(_REPLY_KEY_EXPR_ANY_SEL_PARAM);
                    }
                    (ReplyKeyExpr::MatchingQuery, Some(index)) => {
                        let s = s.parameters_mut();
                        let mut start = index as usize;
                        let pend = start + _REPLY_KEY_EXPR_ANY_SEL_PARAM.len();
                        if start != 0 {
                            start -= 1
                        }
                        match s[pend..].find('&') {
                            Some(end) => std::mem::drop(s.drain(start..end + pend)),
                            None => s.truncate(pend),
                        }
                    }
                    _ => {}
                }
                Ok(s)
            }),
            target,
            consolidation,
            timeout,
            handler,
        }
    }
}
pub(crate) const _REPLY_KEY_EXPR_ANY_SEL_PARAM: &str = "_anyke";
#[zenoh_core::unstable]
pub const REPLY_KEY_EXPR_ANY_SEL_PARAM: &str = _REPLY_KEY_EXPR_ANY_SEL_PARAM;

#[zenoh_core::unstable]
pub enum ReplyKeyExpr {
    Any,
    MatchingQuery,
}

impl<Handler> Resolvable for GetBuilder<'_, '_, Handler>
where
    Handler: crate::prelude::IntoCallbackReceiverPair<'static, Reply>,
{
    type Output = ZResult<Handler::Receiver>;
}

impl<Handler> SyncResolve for GetBuilder<'_, '_, Handler>
where
    Handler: crate::prelude::IntoCallbackReceiverPair<'static, Reply>,
    Handler::Receiver: Send,
{
    fn res_sync(self) -> Self::Output {
        let (callback, receiver) = self.handler.into_cb_receiver_pair();

        self.session
            .query(
                self.selector?,
                self.target,
                self.consolidation,
                self.timeout,
                callback,
            )
            .map(|_| receiver)
    }
}

impl<Handler> AsyncResolve for GetBuilder<'_, '_, Handler>
where
    Handler: crate::prelude::IntoCallbackReceiverPair<'static, Reply>,
    Handler::Receiver: Send,
{
    type Future = futures::future::Ready<Self::Output>;

    fn res_async(self) -> Self::Future {
        futures::future::ready(self.res_sync())
    }
}
