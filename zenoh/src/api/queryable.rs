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
#[zenoh_macros::unstable]
use crate::api::query::ReplyKeyExpr;
#[zenoh_macros::unstable]
use crate::api::sample::SourceInfo;
use crate::api::{
    encoding::Encoding,
    handlers::{locked, DefaultHandler, IntoHandler},
    key_expr::KeyExpr,
    payload::Payload,
    publication::Priority,
    sample::{
        attachment::Attachment,
        builder::{
            DeleteSampleBuilder, PutSampleBuilder, QoSBuilderTrait, SampleBuilder,
            SampleBuilderTrait, TimestampBuilderTrait, ValueBuilderTrait,
        },
        Locality, Sample, SampleKind,
    },
    selector::{Parameters, Selector},
    session::{SessionRef, Undeclarable},
    value::Value,
    Id,
};
use crate::net::primitives::Primitives;
use std::fmt;
use std::future::Ready;
use std::ops::Deref;
use std::sync::Arc;
use uhlc::Timestamp;
use zenoh_config::ZenohId;
use zenoh_core::{AsyncResolve, Resolvable, Resolve, SyncResolve};
use zenoh_protocol::core::CongestionControl;
use zenoh_protocol::core::EntityGlobalId;
use zenoh_protocol::{
    core::{EntityId, WireExpr},
    network::{response, Mapping, RequestId, Response, ResponseFinal},
    zenoh::{self, reply::ReplyBody, Del, Put, ResponseBody},
};
use zenoh_result::ZResult;

use super::query::_REPLY_KEY_EXPR_ANY_SEL_PARAM;

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
            ext_qos: response::ext::QoSType::RESPONSE_FINAL,
            ext_tstamp: None,
        });
    }
}

/// Structs received by a [`Queryable`].
#[derive(Clone)]
pub struct Query {
    pub(crate) inner: Arc<QueryInner>,
    pub(crate) eid: EntityId,
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

    /// Sends a reply in the form of [`Sample`] to this Query.
    ///
    /// By default, queries only accept replies whose key expression intersects with the query's.
    /// Unless the query has enabled disjoint replies (you can check this through [`Query::accepts_replies`]),
    /// replying on a disjoint key expression will result in an error when resolving the reply.
    /// This api is for internal use only.
    #[inline(always)]
    #[cfg(feature = "unstable")]
    #[doc(hidden)]
    pub fn reply_sample(&self, sample: Sample) -> ReplySampleBuilder<'_> {
        ReplySampleBuilder {
            query: self,
            sample_builder: sample.into(),
        }
    }

    /// Sends a reply to this Query.
    ///
    /// By default, queries only accept replies whose key expression intersects with the query's.
    /// Unless the query has enabled disjoint replies (you can check this through [`Query::accepts_replies`]),
    /// replying on a disjoint key expression will result in an error when resolving the reply.
    #[inline(always)]
    pub fn reply<IntoKeyExpr, IntoPayload>(
        &self,
        key_expr: IntoKeyExpr,
        payload: IntoPayload,
    ) -> ReplyBuilder<'_>
    where
        IntoKeyExpr: Into<KeyExpr<'static>>,
        IntoPayload: Into<Payload>,
    {
        let sample_builder =
            SampleBuilder::put(key_expr, payload).qos(response::ext::QoSType::RESPONSE.into());
        ReplyBuilder {
            query: self,
            sample_builder,
        }
    }
    /// Sends a error reply to this Query.
    ///
    #[inline(always)]
    pub fn reply_err<IntoValue>(&self, value: IntoValue) -> ReplyErrBuilder<'_>
    where
        IntoValue: Into<Value>,
    {
        ReplyErrBuilder {
            query: self,
            value: value.into(),
        }
    }

    /// Sends a delete reply to this Query.
    ///
    /// By default, queries only accept replies whose key expression intersects with the query's.
    /// Unless the query has enabled disjoint replies (you can check this through [`Query::accepts_replies`]),
    /// replying on a disjoint key expression will result in an error when resolving the reply.
    #[inline(always)]
    pub fn reply_del<IntoKeyExpr>(&self, key_expr: IntoKeyExpr) -> ReplyDelBuilder<'_>
    where
        IntoKeyExpr: Into<KeyExpr<'static>>,
    {
        let sample_builder =
            DeleteSampleBuilder::new(key_expr).with_qos(response::ext::QoSType::RESPONSE.into());
        ReplyDelBuilder {
            query: self,
            sample_builder,
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
            .get_bools([_REPLY_KEY_EXPR_ANY_SEL_PARAM])
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

pub struct ReplySampleBuilder<'a> {
    query: &'a Query,
    sample_builder: SampleBuilder,
}

impl<'a> ReplySampleBuilder<'a> {
    pub fn put<IntoPayload>(self, payload: IntoPayload) -> ReplyBuilder<'a>
    where
        IntoPayload: Into<Payload>,
    {
        let builder = ReplyBuilder {
            query: self.query,
            sample_builder: self.sample_builder.into(),
        };
        builder.payload(payload)
    }
    pub fn delete(self) -> ReplyDelBuilder<'a> {
        ReplyDelBuilder {
            query: self.query,
            sample_builder: self.sample_builder.into(),
        }
    }
}

impl TimestampBuilderTrait for ReplySampleBuilder<'_> {
    fn timestamp<T: Into<Option<Timestamp>>>(self, timestamp: T) -> Self {
        Self {
            sample_builder: self.sample_builder.timestamp(timestamp),
            ..self
        }
    }
}

impl SampleBuilderTrait for ReplySampleBuilder<'_> {
    #[cfg(feature = "unstable")]
    fn source_info(self, source_info: SourceInfo) -> Self {
        Self {
            sample_builder: self.sample_builder.source_info(source_info),
            ..self
        }
    }

    #[cfg(feature = "unstable")]
    fn attachment<T: Into<Option<Attachment>>>(self, attachment: T) -> Self {
        Self {
            sample_builder: self.sample_builder.attachment(attachment),
            ..self
        }
    }
}

impl QoSBuilderTrait for ReplySampleBuilder<'_> {
    fn congestion_control(self, congestion_control: CongestionControl) -> Self {
        Self {
            sample_builder: self.sample_builder.congestion_control(congestion_control),
            ..self
        }
    }

    fn priority(self, priority: Priority) -> Self {
        Self {
            sample_builder: self.sample_builder.priority(priority),
            ..self
        }
    }

    fn express(self, is_express: bool) -> Self {
        Self {
            sample_builder: self.sample_builder.express(is_express),
            ..self
        }
    }
}

impl Resolvable for ReplySampleBuilder<'_> {
    type To = ZResult<()>;
}

impl SyncResolve for ReplySampleBuilder<'_> {
    fn res_sync(self) -> <Self as Resolvable>::To {
        self.query._reply_sample(self.sample_builder.into())
    }
}

impl AsyncResolve for ReplySampleBuilder<'_> {
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}

/// A builder returned by [`Query::reply()`](Query::reply)
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
#[derive(Debug)]
pub struct ReplyBuilder<'a> {
    query: &'a Query,
    sample_builder: PutSampleBuilder,
}

impl TimestampBuilderTrait for ReplyBuilder<'_> {
    fn timestamp<T: Into<Option<Timestamp>>>(self, timestamp: T) -> Self {
        Self {
            sample_builder: self.sample_builder.timestamp(timestamp),
            ..self
        }
    }
}

impl SampleBuilderTrait for ReplyBuilder<'_> {
    #[cfg(feature = "unstable")]
    fn source_info(self, source_info: SourceInfo) -> Self {
        Self {
            sample_builder: self.sample_builder.source_info(source_info),
            ..self
        }
    }

    #[cfg(feature = "unstable")]
    fn attachment<T: Into<Option<Attachment>>>(self, attachment: T) -> Self {
        Self {
            sample_builder: self.sample_builder.attachment(attachment),
            ..self
        }
    }
}

impl QoSBuilderTrait for ReplyBuilder<'_> {
    fn congestion_control(self, congestion_control: CongestionControl) -> Self {
        Self {
            sample_builder: self.sample_builder.congestion_control(congestion_control),
            ..self
        }
    }

    fn priority(self, priority: Priority) -> Self {
        Self {
            sample_builder: self.sample_builder.priority(priority),
            ..self
        }
    }

    fn express(self, is_express: bool) -> Self {
        Self {
            sample_builder: self.sample_builder.express(is_express),
            ..self
        }
    }
}

impl ValueBuilderTrait for ReplyBuilder<'_> {
    fn encoding<T: Into<Encoding>>(self, encoding: T) -> Self {
        Self {
            sample_builder: self.sample_builder.encoding(encoding),
            ..self
        }
    }

    fn payload<T: Into<Payload>>(self, payload: T) -> Self {
        Self {
            sample_builder: self.sample_builder.payload(payload),
            ..self
        }
    }
    fn value<T: Into<Value>>(self, value: T) -> Self {
        let Value { payload, encoding } = value.into();
        Self {
            sample_builder: self.sample_builder.payload(payload).encoding(encoding),
            ..self
        }
    }
}

/// A builder returned by [`Query::reply_del()`](Query::reply)
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
#[derive(Debug)]
pub struct ReplyDelBuilder<'a> {
    query: &'a Query,
    sample_builder: DeleteSampleBuilder,
}

impl TimestampBuilderTrait for ReplyDelBuilder<'_> {
    fn timestamp<T: Into<Option<Timestamp>>>(self, timestamp: T) -> Self {
        Self {
            sample_builder: self.sample_builder.timestamp(timestamp),
            ..self
        }
    }
}

impl SampleBuilderTrait for ReplyDelBuilder<'_> {
    #[cfg(feature = "unstable")]
    fn source_info(self, source_info: SourceInfo) -> Self {
        Self {
            sample_builder: self.sample_builder.source_info(source_info),
            ..self
        }
    }

    #[cfg(feature = "unstable")]
    fn attachment<T: Into<Option<Attachment>>>(self, attachment: T) -> Self {
        Self {
            sample_builder: self.sample_builder.attachment(attachment),
            ..self
        }
    }
}

impl QoSBuilderTrait for ReplyDelBuilder<'_> {
    fn congestion_control(self, congestion_control: CongestionControl) -> Self {
        Self {
            sample_builder: self.sample_builder.congestion_control(congestion_control),
            ..self
        }
    }

    fn priority(self, priority: Priority) -> Self {
        Self {
            sample_builder: self.sample_builder.priority(priority),
            ..self
        }
    }

    fn express(self, is_express: bool) -> Self {
        Self {
            sample_builder: self.sample_builder.express(is_express),
            ..self
        }
    }
}

/// A builder returned by [`Query::reply_err()`](Query::reply_err).
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
#[derive(Debug)]
pub struct ReplyErrBuilder<'a> {
    query: &'a Query,
    value: Value,
}

impl<'a> Resolvable for ReplyBuilder<'a> {
    type To = ZResult<()>;
}

impl SyncResolve for ReplyBuilder<'_> {
    fn res_sync(self) -> <Self as Resolvable>::To {
        self.query._reply_sample(self.sample_builder.into())
    }
}

impl<'a> Resolvable for ReplyDelBuilder<'a> {
    type To = ZResult<()>;
}

impl SyncResolve for ReplyDelBuilder<'_> {
    fn res_sync(self) -> <Self as Resolvable>::To {
        self.query._reply_sample(self.sample_builder.into())
    }
}

impl<'a> AsyncResolve for ReplyBuilder<'a> {
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}

impl<'a> AsyncResolve for ReplyDelBuilder<'a> {
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}

impl Query {
    fn _reply_sample(&self, sample: Sample) -> ZResult<()> {
        if !self._accepts_any_replies().unwrap_or(false)
            && !self.key_expr().intersects(&sample.key_expr)
        {
            bail!("Attempted to reply on `{}`, which does not intersect with query `{}`, despite query only allowing replies on matching key expressions", sample.key_expr, self.key_expr())
        }
        #[cfg(not(feature = "unstable"))]
        let ext_sinfo = None;
        #[cfg(feature = "unstable")]
        let ext_sinfo = sample.source_info.into();
        self.inner.primitives.send_response(Response {
            rid: self.inner.qid,
            wire_expr: WireExpr {
                scope: 0,
                suffix: std::borrow::Cow::Owned(sample.key_expr.into()),
                mapping: Mapping::Sender,
            },
            payload: ResponseBody::Reply(zenoh::Reply {
                consolidation: zenoh::Consolidation::DEFAULT,
                ext_unknown: vec![],
                payload: match sample.kind {
                    SampleKind::Put => ReplyBody::Put(Put {
                        timestamp: sample.timestamp,
                        encoding: sample.encoding.into(),
                        ext_sinfo,
                        #[cfg(feature = "shared-memory")]
                        ext_shm: None,
                        #[cfg(feature = "unstable")]
                        ext_attachment: sample.attachment.map(|a| a.into()),
                        #[cfg(not(feature = "unstable"))]
                        ext_attachment: None,
                        ext_unknown: vec![],
                        payload: sample.payload.into(),
                    }),
                    SampleKind::Delete => ReplyBody::Del(Del {
                        timestamp: sample.timestamp,
                        ext_sinfo,
                        #[cfg(feature = "unstable")]
                        ext_attachment: sample.attachment.map(|a| a.into()),
                        #[cfg(not(feature = "unstable"))]
                        ext_attachment: None,
                        ext_unknown: vec![],
                    }),
                },
            }),
            ext_qos: sample.qos.into(),
            ext_tstamp: None,
            ext_respid: Some(response::ext::ResponderIdType {
                zid: self.inner.zid,
                eid: self.eid,
            }),
        });
        Ok(())
    }
}

impl<'a> Resolvable for ReplyErrBuilder<'a> {
    type To = ZResult<()>;
}

impl SyncResolve for ReplyErrBuilder<'_> {
    fn res_sync(self) -> <Self as Resolvable>::To {
        self.query.inner.primitives.send_response(Response {
            rid: self.query.inner.qid,
            wire_expr: WireExpr {
                scope: 0,
                suffix: std::borrow::Cow::Owned(self.query.key_expr().as_str().to_owned()),
                mapping: Mapping::Sender,
            },
            payload: ResponseBody::Err(zenoh::Err {
                encoding: self.value.encoding.into(),
                ext_sinfo: None,
                ext_unknown: vec![],
                payload: self.value.payload.into(),
            }),
            ext_qos: response::ext::QoSType::RESPONSE,
            ext_tstamp: None,
            ext_respid: Some(response::ext::ResponderIdType {
                zid: self.query.inner.zid,
                eid: self.query.eid,
            }),
        });
        Ok(())
    }
}
impl<'a> AsyncResolve for ReplyErrBuilder<'a> {
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
///     query.reply(KeyExpr::try_from("key/expression").unwrap(), "value")
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

    /// Receive the queries for this Queryable with a [`Handler`](crate::prelude::IntoHandler).
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
        Handler: IntoHandler<'static, Query>,
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

/// A queryable that provides data through a [`Handler`](crate::prelude::IntoHandler).
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
///     query.reply(KeyExpr::try_from("key/expression").unwrap(), "value")
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
    /// Returns the [`EntityGlobalId`] of this Queryable.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let queryable = session.declare_queryable("key/expression")
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// let queryable_id = queryable.id();
    /// # }
    /// ```
    #[zenoh_macros::unstable]
    pub fn id(&self) -> EntityGlobalId {
        EntityGlobalId {
            zid: self.queryable.session.zid(),
            eid: self.queryable.state.id,
        }
    }

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
    Handler: IntoHandler<'static, Query> + Send,
    Handler::Handler: Send,
{
    type To = ZResult<Queryable<'a, Handler::Handler>>;
}

impl<'a, Handler> SyncResolve for QueryableBuilder<'a, '_, Handler>
where
    Handler: IntoHandler<'static, Query> + Send,
    Handler::Handler: Send,
{
    fn res_sync(self) -> <Self as Resolvable>::To {
        let session = self.session;
        let (callback, receiver) = self.handler.into_handler();
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
    Handler: IntoHandler<'static, Query> + Send,
    Handler::Handler: Send,
{
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}
