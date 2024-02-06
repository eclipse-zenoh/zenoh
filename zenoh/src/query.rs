//
// Copyright (c) 2023 ZettaScale Technology
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
use crate::prelude::*;
#[zenoh_macros::unstable]
use crate::sample::Attachment;
use crate::Session;
use std::collections::HashMap;
use std::future::Ready;
use std::time::Duration;
use zenoh_core::{AsyncResolve, Resolvable, SyncResolve};
use zenoh_result::ZResult;

/// The [`Queryable`](crate::queryable::Queryable)s that should be target of a [`get`](Session::get).
pub use zenoh_protocol::core::QueryTarget;

/// The kind of consolidation.
pub use zenoh_protocol::core::ConsolidationMode;

/// The operation: either manual or automatic.
// ignore_tagging
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode<T> {
    Auto,
    Manual(T),
}

/// The replies consolidation strategy to apply on replies to a [`get`](Session::get).
// tags{session.query_reply.consolidation_mode}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueryConsolidation {
    pub(crate) mode: Mode<ConsolidationMode>,
}

impl QueryConsolidation {
    /// Automatic query consolidation strategy selection.
    // tags{session.query_reply.consolidation_mode.auto}
    pub const AUTO: Self = Self { mode: Mode::Auto };

    pub(crate) const fn from_mode(mode: ConsolidationMode) -> Self {
        Self {
            mode: Mode::Manual(mode),
        }
    }

    /// Returns the requested [`ConsolidationMode`].
    // ignore_tagging
    pub fn mode(&self) -> Mode<ConsolidationMode> {
        self.mode
    }
}
impl From<Mode<ConsolidationMode>> for QueryConsolidation {
    fn from(mode: Mode<ConsolidationMode>) -> Self {
        Self { mode }
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
// tags{reply}
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct Reply {
    /// The result of this Reply.
    // tags{reply.sample}
    pub sample: Result<Sample, Value>,
    /// The id of the zenoh instance that answered this Reply.
    // tags{reply.replier_id}
    pub replier_id: ZenohId,
}

pub(crate) struct QueryState {
    pub(crate) nb_final: usize,
    pub(crate) selector: Selector<'static>,
    pub(crate) scope: Option<KeyExpr<'static>>,
    pub(crate) reception_mode: ConsolidationMode,
    pub(crate) replies: Option<HashMap<OwnedKeyExpr, Reply>>,
    pub(crate) callback: Callback<'static, Reply>,
}

/// A builder for initializing a `query`.
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// use zenoh::prelude::r#async::*;
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
// ignore_tagging
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
#[derive(Debug)]
pub struct GetBuilder<'a, 'b, Handler> {
    pub(crate) session: &'a Session,
    pub(crate) selector: ZResult<Selector<'b>>,
    pub(crate) scope: ZResult<Option<KeyExpr<'b>>>,
    pub(crate) target: QueryTarget,
    pub(crate) consolidation: QueryConsolidation,
    pub(crate) destination: Locality,
    pub(crate) timeout: Duration,
    pub(crate) handler: Handler,
    pub(crate) value: Option<Value>,
    #[cfg(feature = "unstable")]
    pub(crate) attachment: Option<Attachment>,
}

impl<'a, 'b> GetBuilder<'a, 'b, DefaultHandler> {
    /// Receive the replies for this query with a callback.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
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
    // tags{session.query_reply.callback}
    #[inline]
    pub fn callback<Callback>(self, callback: Callback) -> GetBuilder<'a, 'b, Callback>
    where
        Callback: Fn(Reply) + Send + Sync + 'static,
    {
        let GetBuilder {
            session,
            selector,
            scope,
            target,
            consolidation,
            destination,
            timeout,
            value,
            #[cfg(feature = "unstable")]
            attachment,
            handler: _,
        } = self;
        GetBuilder {
            session,
            selector,
            scope,
            target,
            consolidation,
            destination,
            timeout,
            value,
            #[cfg(feature = "unstable")]
            attachment,
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
    /// use zenoh::prelude::r#async::*;
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
    // tags{session.query_reply.callback}
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
    /// use zenoh::prelude::r#async::*;
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
    // tags{session.query_reply.pipe}
    #[inline]
    pub fn with<Handler>(self, handler: Handler) -> GetBuilder<'a, 'b, Handler>
    where
        Handler: IntoCallbackReceiverPair<'static, Reply>,
    {
        let GetBuilder {
            session,
            selector,
            scope,
            target,
            consolidation,
            destination,
            timeout,
            value,
            #[cfg(feature = "unstable")]
            attachment,
            handler: _,
        } = self;
        GetBuilder {
            session,
            selector,
            scope,
            target,
            consolidation,
            destination,
            timeout,
            value,
            #[cfg(feature = "unstable")]
            attachment,
            handler,
        }
    }
}
impl<'a, 'b, Handler> GetBuilder<'a, 'b, Handler> {
    /// Change the target of the query.
    // tags{session.query_reply.target.set}
    #[inline]
    pub fn target(mut self, target: QueryTarget) -> Self {
        self.target = target;
        self
    }

    /// Change the consolidation mode of the query.
    // tags{session.query_reply.consolidation_mode.set}
    #[inline]
    pub fn consolidation<QC: Into<QueryConsolidation>>(mut self, consolidation: QC) -> Self {
        self.consolidation = consolidation.into();
        self
    }

    /// Restrict the matching queryables that will receive the query
    /// to the ones that have the given [`Locality`](crate::prelude::Locality).
    // tags{session.query_reply.allowed_destination.set}
    #[zenoh_macros::unstable]
    #[inline]
    pub fn allowed_destination(mut self, destination: Locality) -> Self {
        self.destination = destination;
        self
    }

    /// Set query timeout.
    // tags{session.query_reply.timeout.set}
    #[inline]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set query value.
    // tags{session.query_reply.value.set}
    #[inline]
    pub fn with_value<IntoValue>(mut self, value: IntoValue) -> Self
    where
        IntoValue: Into<Value>,
    {
        self.value = Some(value.into());
        self
    }

    #[zenoh_macros::unstable]
    // tags{session.query_reply.attachment.set}
    pub fn with_attachment(mut self, attachment: Attachment) -> Self {
        self.attachment = Some(attachment);
        self
    }

    /// By default, `get` guarantees that it will only receive replies whose key expressions intersect
    /// with the queried key expression.
    ///
    /// If allowed to through `accept_replies(ReplyKeyExpr::Any)`, queryables may also reply on key
    /// expressions that don't intersect with the query's.
    // tags{session.query_reply.accept_replies.set}
    #[zenoh_macros::unstable]
    pub fn accept_replies(self, accept: ReplyKeyExpr) -> Self {
        let Self {
            session,
            selector,
            scope,
            target,
            consolidation,
            destination,
            timeout,
            value,
            attachment,
            handler,
        } = self;
        Self {
            session,
            selector: selector.and_then(|s| s.accept_any_keyexpr(accept == ReplyKeyExpr::Any)),
            scope,
            target,
            consolidation,
            destination,
            timeout,
            value,
            attachment,
            handler,
        }
    }
}

pub(crate) const _REPLY_KEY_EXPR_ANY_SEL_PARAM: &str = "_anyke";
#[zenoh_macros::unstable]
// tags{session.query_reply.reply_key_expr_any_sel_param}
pub const REPLY_KEY_EXPR_ANY_SEL_PARAM: &str = _REPLY_KEY_EXPR_ANY_SEL_PARAM;

#[zenoh_macros::unstable]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
// tags{session.query_reply.accept_replies}
pub enum ReplyKeyExpr {
    Any,
    MatchingQuery,
}

#[zenoh_macros::unstable]
impl Default for ReplyKeyExpr {
    fn default() -> Self {
        ReplyKeyExpr::MatchingQuery
    }
}

impl<Handler> Resolvable for GetBuilder<'_, '_, Handler>
where
    Handler: IntoCallbackReceiverPair<'static, Reply> + Send,
    Handler::Receiver: Send,
{
    type To = ZResult<Handler::Receiver>;
}

impl<Handler> SyncResolve for GetBuilder<'_, '_, Handler>
where
    Handler: IntoCallbackReceiverPair<'static, Reply> + Send,
    Handler::Receiver: Send,
{
    fn res_sync(self) -> <Self as Resolvable>::To {
        let (callback, receiver) = self.handler.into_cb_receiver_pair();

        self.session
            .query(
                &self.selector?,
                &self.scope?,
                self.target,
                self.consolidation,
                self.destination,
                self.timeout,
                self.value,
                #[cfg(feature = "unstable")]
                self.attachment,
                callback,
            )
            .map(|_| receiver)
    }
}

impl<Handler> AsyncResolve for GetBuilder<'_, '_, Handler>
where
    Handler: IntoCallbackReceiverPair<'static, Reply> + Send,
    Handler::Receiver: Send,
{
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}
