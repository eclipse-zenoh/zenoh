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

//! Queryable primitives.

use crate::handlers::{locked, DefaultHandler};
use crate::net::primitives::Primitives;
use crate::prelude::*;
#[zenoh_macros::unstable]
use crate::query::ReplyKeyExpr;
#[zenoh_macros::unstable]
use crate::sample::Attachment;
use crate::sample::DataInfo;
use crate::SessionRef;
use crate::Undeclarable;

use std::fmt;
use std::future::Ready;
use std::ops::Deref;
use std::sync::Arc;
use zenoh_core::{AsyncResolve, Resolvable, SyncResolve};
use zenoh_protocol::core::WireExpr;
use zenoh_protocol::network::{response, Mapping, RequestId, Response, ResponseFinal};
use zenoh_protocol::zenoh::ext::ValueType;
use zenoh_protocol::zenoh::reply::ext::ConsolidationType;
use zenoh_protocol::zenoh::{self, ResponseBody};
use zenoh_result::ZResult;

pub(crate) struct QueryInner {
    /// The key expression of this Query.
    pub(crate) key_expr: KeyExpr<'static>,
    /// This Query's selector parameters.
    pub(crate) parameters: String,
    /// This Query's body.
    pub(crate) value: Option<Value>,

    pub(crate) qid: RequestId,
    pub(crate) zid: ZenohId,
    pub(crate) primitives: Arc<dyn Primitives>,
    #[cfg(feature = "unstable")]
    pub(crate) attachment: Option<Attachment>,
}

impl Drop for QueryInner {
    fn drop(&mut self) {
        self.primitives.send_response_final(ResponseFinal {
            rid: self.qid,
            ext_qos: response::ext::QoSType::response_final_default(),
            ext_tstamp: None,
        });
    }
}

/// Structs received by a [`Queryable`].
#[derive(Clone)]
pub struct Query {
    pub(crate) inner: Arc<QueryInner>,
}

impl Query {
    /// The full [`Selector`] of this Query.
    #[inline(always)]
    pub fn selector(&self) -> Selector<'_> {
        Selector {
            key_expr: self.inner.key_expr.clone(),
            parameters: (&self.inner.parameters).into(),
        }
    }

    /// The key selector part of this Query.
    #[inline(always)]
    pub fn key_expr(&self) -> &KeyExpr<'static> {
        &self.inner.key_expr
    }

    /// This Query's selector parameters.
    #[inline(always)]
    pub fn parameters(&self) -> &str {
        &self.inner.parameters
    }

    /// This Query's value.
    #[inline(always)]
    pub fn value(&self) -> Option<&Value> {
        self.inner.value.as_ref()
    }

    #[zenoh_macros::unstable]
    pub fn attachment(&self) -> Option<&Attachment> {
        self.inner.attachment.as_ref()
    }

    /// Sends a reply to this Query.
    ///
    /// By default, queries only accept replies whose key expression intersects with the query's.
    /// Unless the query has enabled disjoint replies (you can check this through [`Query::accepts_replies`]),
    /// replying on a disjoint key expression will result in an error when resolving the reply.
    #[inline(always)]
    pub fn reply(&self, result: Result<Sample, Value>) -> ReplyBuilder<'_> {
        ReplyBuilder {
            query: self,
            result,
        }
    }

    /// Queries may or may not accept replies on key expressions that do not intersect with their own key expression.
    /// This getter allows you to check whether or not a specific query does.
    #[zenoh_macros::unstable]
    pub fn accepts_replies(&self) -> ZResult<ReplyKeyExpr> {
        self._accepts_any_replies().map(|any| {
            if any {
                ReplyKeyExpr::Any
            } else {
                ReplyKeyExpr::MatchingQuery
            }
        })
    }
    fn _accepts_any_replies(&self) -> ZResult<bool> {
        self.parameters()
            .get_bools([crate::query::_REPLY_KEY_EXPR_ANY_SEL_PARAM])
            .map(|a| a[0])
    }
}

impl fmt::Debug for Query {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Query")
            .field("key_selector", &self.inner.key_expr)
            .field("parameters", &self.inner.parameters)
            .finish()
    }
}

impl fmt::Display for Query {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Query")
            .field(
                "selector",
                &format!("{}{}", &self.inner.key_expr, &self.inner.parameters),
            )
            .finish()
    }
}

/// A builder returned by [`Query::reply()`](Query::reply).
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
#[derive(Debug)]
pub struct ReplyBuilder<'a> {
    query: &'a Query,
    result: Result<Sample, Value>,
}

impl<'a> ReplyBuilder<'a> {
    #[allow(clippy::result_large_err)]
    #[zenoh_macros::unstable]
    pub fn with_attachment(mut self, attachment: Attachment) -> Result<Self, (Self, Attachment)> {
        match &mut self.result {
            Ok(sample) => {
                sample.attachment = Some(attachment);
                Ok(self)
            }
            Err(_) => Err((self, attachment)),
        }
    }
}

impl<'a> Resolvable for ReplyBuilder<'a> {
    type To = ZResult<()>;
}

impl SyncResolve for ReplyBuilder<'_> {
    fn res_sync(self) -> <Self as Resolvable>::To {
        match self.result {
            Ok(sample) => {
                if !self.query._accepts_any_replies().unwrap_or(false)
                    && !self.query.key_expr().intersects(&sample.key_expr)
                {
                    bail!("Attempted to reply on `{}`, which does not intersect with query `{}`, despite query only allowing replies on matching key expressions", sample.key_expr, self.query.key_expr())
                }
                let Sample {
                    key_expr,
                    value: Value { payload, encoding },
                    kind,
                    timestamp,
                    qos,
                    #[cfg(feature = "unstable")]
                    source_info,
                    #[cfg(feature = "unstable")]
                    attachment,
                } = sample;
                #[allow(unused_mut)]
                let mut data_info = DataInfo {
                    kind,
                    encoding: Some(encoding),
                    timestamp,
                    qos,
                    source_id: None,
                    source_sn: None,
                };
                #[allow(unused_mut)]
                let mut ext_attachment = None;
                #[cfg(feature = "unstable")]
                {
                    data_info.source_id = source_info.source_id;
                    data_info.source_sn = source_info.source_sn;
                    if let Some(attachment) = attachment {
                        ext_attachment = Some(attachment.into());
                    }
                }
                self.query.inner.primitives.send_response(Response {
                    rid: self.query.inner.qid,
                    wire_expr: WireExpr {
                        scope: 0,
                        suffix: std::borrow::Cow::Owned(key_expr.into()),
                        mapping: Mapping::Sender,
                    },
                    payload: ResponseBody::Reply(zenoh::Reply {
                        timestamp: data_info.timestamp,
                        encoding: data_info.encoding.unwrap_or_default(),
                        ext_sinfo: if data_info.source_id.is_some() || data_info.source_sn.is_some()
                        {
                            Some(zenoh::reply::ext::SourceInfoType {
                                zid: data_info.source_id.unwrap_or_default(),
                                eid: 0, // @TODO use proper EntityId (#703)
                                sn: data_info.source_sn.unwrap_or_default() as u32,
                            })
                        } else {
                            None
                        },
                        ext_consolidation: ConsolidationType::default(),
                        #[cfg(feature = "shared-memory")]
                        ext_shm: None,
                        ext_attachment,
                        ext_unknown: vec![],
                        payload,
                    }),
                    ext_qos: response::ext::QoSType::response_default(),
                    ext_tstamp: None,
                    ext_respid: Some(response::ext::ResponderIdType {
                        zid: self.query.inner.zid,
                        eid: 0, // @TODO use proper EntityId (#703)
                    }),
                });
                Ok(())
            }
            Err(payload) => {
                self.query.inner.primitives.send_response(Response {
                    rid: self.query.inner.qid,
                    wire_expr: WireExpr {
                        scope: 0,
                        suffix: std::borrow::Cow::Owned(self.query.key_expr().as_str().to_owned()),
                        mapping: Mapping::Sender,
                    },
                    payload: ResponseBody::Err(zenoh::Err {
                        timestamp: None,
                        is_infrastructure: false,
                        ext_sinfo: None,
                        ext_unknown: vec![],
                        ext_body: Some(ValueType {
                            #[cfg(feature = "shared-memory")]
                            ext_shm: None,
                            payload: payload.payload,
                            encoding: payload.encoding,
                        }),
                        code: 0, // TODO
                    }),
                    ext_qos: response::ext::QoSType::response_default(),
                    ext_tstamp: None,
                    ext_respid: Some(response::ext::ResponderIdType {
                        zid: self.query.inner.zid,
                        eid: 0, // @TODO use proper EntityId (#703)
                    }),
                });
                Ok(())
            }
        }
    }
}

impl<'a> AsyncResolve for ReplyBuilder<'a> {
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}

pub(crate) struct QueryableState {
    pub(crate) id: Id,
    pub(crate) key_expr: WireExpr<'static>,
    pub(crate) complete: bool,
    pub(crate) origin: Locality,
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
/// with the [`declare_queryable`](crate::Session::declare_queryable) function
/// and the [`callback`](QueryableBuilder::callback) function
/// of the resulting builder.
///
/// Queryables are automatically undeclared when dropped.
///
/// # Examples
/// ```no_run
/// # #[tokio::main]
/// # async fn main() {
/// use futures::prelude::*;
/// use zenoh::prelude::r#async::*;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// let queryable = session.declare_queryable("key/expression").res().await.unwrap();
/// while let Ok(query) = queryable.recv_async().await {
///     println!(">> Handling query '{}'", query.selector());
///     query.reply(Ok(Sample::try_from("key/expression", "value").unwrap()))
///         .res()
///         .await
///         .unwrap();
/// }
/// # }
/// ```
#[derive(Debug)]
pub(crate) struct CallbackQueryable<'a> {
    pub(crate) session: SessionRef<'a>,
    pub(crate) state: Arc<QueryableState>,
    pub(crate) alive: bool,
}

impl<'a> Undeclarable<(), QueryableUndeclaration<'a>> for CallbackQueryable<'a> {
    fn undeclare_inner(self, _: ()) -> QueryableUndeclaration<'a> {
        QueryableUndeclaration { queryable: self }
    }
}

/// A [`Resolvable`] returned when undeclaring a queryable.
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh::prelude::r#async::*;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// let queryable = session.declare_queryable("key/expression").res().await.unwrap();
/// queryable.undeclare().res().await.unwrap();
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
pub struct QueryableUndeclaration<'a> {
    queryable: CallbackQueryable<'a>,
}

impl Resolvable for QueryableUndeclaration<'_> {
    type To = ZResult<()>;
}

impl SyncResolve for QueryableUndeclaration<'_> {
    fn res_sync(mut self) -> <Self as Resolvable>::To {
        self.queryable.alive = false;
        self.queryable
            .session
            .close_queryable(self.queryable.state.id)
    }
}

impl<'a> AsyncResolve for QueryableUndeclaration<'a> {
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}

impl Drop for CallbackQueryable<'_> {
    fn drop(&mut self) {
        if self.alive {
            let _ = self.session.close_queryable(self.state.id);
        }
    }
}

/// A builder for initializing a [`Queryable`].
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh::prelude::r#async::*;
/// use zenoh::queryable;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// let queryable = session.declare_queryable("key/expression").res().await.unwrap();
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
#[derive(Debug)]
pub struct QueryableBuilder<'a, 'b, Handler> {
    pub(crate) session: SessionRef<'a>,
    pub(crate) key_expr: ZResult<KeyExpr<'b>>,
    pub(crate) complete: bool,
    pub(crate) origin: Locality,
    pub(crate) handler: Handler,
}

impl<'a, 'b> QueryableBuilder<'a, 'b, DefaultHandler> {
    /// Receive the queries for this Queryable with a callback.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let queryable = session
    ///     .declare_queryable("key/expression")
    ///     .callback(|query| {println!(">> Handling query '{}'", query.selector());})
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
    #[inline]
    pub fn callback<Callback>(self, callback: Callback) -> QueryableBuilder<'a, 'b, Callback>
    where
        Callback: Fn(Query) + Send + Sync + 'static,
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
            handler: callback,
        }
    }

    /// Receive the queries for this Queryable with a mutable callback.
    ///
    /// Using this guarantees that your callback will never be called concurrently.
    /// If your callback is also accepted by the [`callback`](QueryableBuilder::callback) method, we suggest you use it instead of `callback_mut`
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let mut n = 0;
    /// let queryable = session
    ///     .declare_queryable("key/expression")
    ///     .callback_mut(move |query| {n += 1;})
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
    #[inline]
    pub fn callback_mut<CallbackMut>(
        self,
        callback: CallbackMut,
    ) -> QueryableBuilder<'a, 'b, impl Fn(Query) + Send + Sync + 'static>
    where
        CallbackMut: FnMut(Query) + Send + Sync + 'static,
    {
        self.callback(locked(callback))
    }

    /// Receive the queries for this Queryable with a [`Handler`](crate::prelude::IntoCallbackReceiverPair).
    ///
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::r#async::*;
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
    /// # }
    /// ```
    #[inline]
    pub fn with<Handler>(self, handler: Handler) -> QueryableBuilder<'a, 'b, Handler>
    where
        Handler: crate::prelude::IntoCallbackReceiverPair<'static, Query>,
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

    /// Restrict the matching queries that will be receive by this [`Queryable`]
    /// to the ones that have the given [`Locality`](crate::prelude::Locality).
    #[inline]
    #[zenoh_macros::unstable]
    pub fn allowed_origin(mut self, origin: Locality) -> Self {
        self.origin = origin;
        self
    }
}
impl<'a, 'b, Handler> QueryableBuilder<'a, 'b, Handler> {
    /// Change queryable completeness.
    #[inline]
    pub fn complete(mut self, complete: bool) -> Self {
        self.complete = complete;
        self
    }
}

/// A queryable that provides data through a [`Handler`](crate::prelude::IntoCallbackReceiverPair).
///
/// Queryables can be created from a zenoh [`Session`]
/// with the [`declare_queryable`](crate::Session::declare_queryable) function
/// and the [`with`](QueryableBuilder::with) function
/// of the resulting builder.
///
/// Queryables are automatically undeclared when dropped.
///
/// # Examples
/// ```no_run
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh::prelude::r#async::*;
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
///     query.reply(Ok(Sample::try_from("key/expression", "value").unwrap()))
///         .res()
///         .await
///         .unwrap();
/// }
/// # }
/// ```
#[non_exhaustive]
#[derive(Debug)]
pub struct Queryable<'a, Receiver> {
    pub(crate) queryable: CallbackQueryable<'a>,
    pub receiver: Receiver,
}

impl<'a, Receiver> Queryable<'a, Receiver> {
    #[inline]
    pub fn undeclare(self) -> impl Resolve<ZResult<()>> + 'a {
        Undeclarable::undeclare_inner(self, ())
    }
}

impl<'a, T> Undeclarable<(), QueryableUndeclaration<'a>> for Queryable<'a, T> {
    fn undeclare_inner(self, _: ()) -> QueryableUndeclaration<'a> {
        Undeclarable::undeclare_inner(self.queryable, ())
    }
}

impl<Receiver> Deref for Queryable<'_, Receiver> {
    type Target = Receiver;

    fn deref(&self) -> &Self::Target {
        &self.receiver
    }
}

impl<'a, Handler> Resolvable for QueryableBuilder<'a, '_, Handler>
where
    Handler: IntoCallbackReceiverPair<'static, Query> + Send,
    Handler::Receiver: Send,
{
    type To = ZResult<Queryable<'a, Handler::Receiver>>;
}

impl<'a, Handler> SyncResolve for QueryableBuilder<'a, '_, Handler>
where
    Handler: IntoCallbackReceiverPair<'static, Query> + Send,
    Handler::Receiver: Send,
{
    fn res_sync(self) -> <Self as Resolvable>::To {
        let session = self.session;
        let (callback, receiver) = self.handler.into_cb_receiver_pair();
        session
            .declare_queryable_inner(
                &self.key_expr?.to_wire(&session),
                self.complete,
                self.origin,
                callback,
            )
            .map(|qable_state| Queryable {
                queryable: CallbackQueryable {
                    session,
                    state: qable_state,
                    alive: true,
                },
                receiver,
            })
    }
}

impl<'a, Handler> AsyncResolve for QueryableBuilder<'a, '_, Handler>
where
    Handler: IntoCallbackReceiverPair<'static, Query> + Send,
    Handler::Receiver: Send,
{
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}
