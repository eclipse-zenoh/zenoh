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
use std::{
    fmt,
    future::{IntoFuture, Ready},
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
    sync::Arc,
};

use uhlc::Timestamp;
use zenoh_core::{Resolvable, Resolve, Wait};
use zenoh_protocol::{
    core::{CongestionControl, EntityId, WireExpr, ZenohId},
    network::{response, Mapping, RequestId, Response, ResponseFinal},
    zenoh::{self, reply::ReplyBody, Del, Put, ResponseBody},
};
use zenoh_result::ZResult;
#[zenoh_macros::unstable]
use {
    super::{
        builders::sample::SampleBuilderTrait, bytes::OptionZBytes, query::ReplyKeyExpr,
        sample::SourceInfo,
    },
    zenoh_protocol::core::EntityGlobalId,
};

use super::{
    builders::sample::{QoSBuilderTrait, SampleBuilder, TimestampBuilderTrait, ValueBuilderTrait},
    bytes::ZBytes,
    encoding::Encoding,
    handlers::{locked, DefaultHandler, IntoHandler},
    key_expr::KeyExpr,
    publisher::Priority,
    sample::{Locality, QoSBuilder, Sample, SampleKind},
    selector::{Parameters, Selector},
    session::{SessionRef, Undeclarable},
    value::Value,
    Id,
};
use crate::net::primitives::Primitives;

pub(crate) struct QueryInner {
    pub(crate) key_expr: KeyExpr<'static>,
    pub(crate) parameters: Parameters<'static>,
    pub(crate) qid: RequestId,
    pub(crate) zid: ZenohId,
    pub(crate) primitives: Arc<dyn Primitives>,
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
    pub(crate) value: Option<Value>,
    pub(crate) attachment: Option<ZBytes>,
}

impl Query {
    /// The full [`Selector`] of this Query.
    #[inline(always)]
    pub fn selector(&self) -> Selector<'_> {
        Selector {
            key_expr: self.inner.key_expr.clone(),
            parameters: self.inner.parameters.clone(),
        }
    }

    /// The key selector part of this Query.
    #[inline(always)]
    pub fn key_expr(&self) -> &KeyExpr<'static> {
        &self.inner.key_expr
    }

    /// This Query's selector parameters.
    #[inline(always)]
    pub fn parameters(&self) -> &Parameters {
        &self.inner.parameters
    }

    /// This Query's value.
    #[inline(always)]
    pub fn value(&self) -> Option<&Value> {
        self.value.as_ref()
    }

    /// This Query's value.
    #[inline(always)]
    pub fn value_mut(&mut self) -> Option<&mut Value> {
        self.value.as_mut()
    }

    /// This Query's payload.
    #[inline(always)]
    pub fn payload(&self) -> Option<&ZBytes> {
        self.value.as_ref().map(|v| &v.payload)
    }

    /// This Query's payload.
    #[inline(always)]
    pub fn payload_mut(&mut self) -> Option<&mut ZBytes> {
        self.value.as_mut().map(|v| &mut v.payload)
    }

    /// This Query's encoding.
    #[inline(always)]
    pub fn encoding(&self) -> Option<&Encoding> {
        self.value.as_ref().map(|v| &v.encoding)
    }

    /// This Query's attachment.
    pub fn attachment(&self) -> Option<&ZBytes> {
        self.attachment.as_ref()
    }

    /// This Query's attachment.
    pub fn attachment_mut(&mut self) -> Option<&mut ZBytes> {
        self.attachment.as_mut()
    }

    /// Sends a reply in the form of [`Sample`] to this Query.
    ///
    /// By default, queries only accept replies whose key expression intersects with the query's.
    /// Unless the query has enabled disjoint replies (you can check this through [`Query::accepts_replies`]),
    /// replying on a disjoint key expression will result in an error when resolving the reply.
    /// This api is for internal use only.
    #[inline(always)]
    #[zenoh_macros::internal]
    pub fn reply_sample(&self, sample: Sample) -> ReplySample<'_> {
        ReplySample {
            query: self,
            sample,
        }
    }

    /// Sends a reply to this Query.
    ///
    /// By default, queries only accept replies whose key expression intersects with the query's.
    /// Unless the query has enabled disjoint replies (you can check this through [`Query::accepts_replies`]),
    /// replying on a disjoint key expression will result in an error when resolving the reply.
    #[inline(always)]
    pub fn reply<'b, TryIntoKeyExpr, IntoZBytes>(
        &self,
        key_expr: TryIntoKeyExpr,
        payload: IntoZBytes,
    ) -> ReplyBuilder<'_, 'b, ReplyBuilderPut>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>,
        IntoZBytes: Into<ZBytes>,
    {
        ReplyBuilder {
            query: self,
            key_expr: key_expr.try_into().map_err(Into::into),
            qos: response::ext::QoSType::RESPONSE.into(),
            kind: ReplyBuilderPut {
                payload: payload.into(),
                encoding: Encoding::default(),
            },
            timestamp: None,
            #[cfg(feature = "unstable")]
            source_info: SourceInfo::empty(),
            attachment: None,
        }
    }

    /// Sends a error reply to this Query.
    ///
    #[inline(always)]
    pub fn reply_err<IntoZBytes>(&self, payload: IntoZBytes) -> ReplyErrBuilder<'_>
    where
        IntoZBytes: Into<ZBytes>,
    {
        ReplyErrBuilder {
            query: self,
            value: Value::new(payload, Encoding::default()),
        }
    }

    /// Sends a delete reply to this Query.
    ///
    /// By default, queries only accept replies whose key expression intersects with the query's.
    /// Unless the query has enabled disjoint replies (you can check this through [`Query::accepts_replies`]),
    /// replying on a disjoint key expression will result in an error when resolving the reply.
    #[inline(always)]
    pub fn reply_del<'b, TryIntoKeyExpr>(
        &self,
        key_expr: TryIntoKeyExpr,
    ) -> ReplyBuilder<'_, 'b, ReplyBuilderDelete>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>,
    {
        ReplyBuilder {
            query: self,
            key_expr: key_expr.try_into().map_err(Into::into),
            qos: response::ext::QoSType::RESPONSE.into(),
            kind: ReplyBuilderDelete,
            timestamp: None,
            #[cfg(feature = "unstable")]
            source_info: SourceInfo::empty(),
            attachment: None,
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
    #[cfg(feature = "unstable")]
    fn _accepts_any_replies(&self) -> ZResult<bool> {
        use crate::api::query::_REPLY_KEY_EXPR_ANY_SEL_PARAM;

        Ok(self
            .parameters()
            .contains_key(_REPLY_KEY_EXPR_ANY_SEL_PARAM))
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

#[zenoh_macros::internal]
pub struct ReplySample<'a> {
    query: &'a Query,
    sample: Sample,
}

#[zenoh_macros::internal]
impl Resolvable for ReplySample<'_> {
    type To = ZResult<()>;
}

#[zenoh_macros::internal]
impl Wait for ReplySample<'_> {
    fn wait(self) -> <Self as Resolvable>::To {
        self.query._reply_sample(self.sample)
    }
}

#[zenoh_macros::internal]
impl IntoFuture for ReplySample<'_> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

#[derive(Debug)]
pub struct ReplyBuilderPut {
    payload: ZBytes,
    encoding: Encoding,
}
#[derive(Debug)]
pub struct ReplyBuilderDelete;

/// A builder returned by [`Query::reply()`](Query::reply) and [`Query::reply_del()`](Query::reply_del)
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
#[derive(Debug)]
pub struct ReplyBuilder<'a, 'b, T> {
    query: &'a Query,
    key_expr: ZResult<KeyExpr<'b>>,
    kind: T,
    timestamp: Option<Timestamp>,
    qos: QoSBuilder,
    #[cfg(feature = "unstable")]
    source_info: SourceInfo,
    attachment: Option<ZBytes>,
}

impl<T> TimestampBuilderTrait for ReplyBuilder<'_, '_, T> {
    fn timestamp<U: Into<Option<Timestamp>>>(self, timestamp: U) -> Self {
        Self {
            timestamp: timestamp.into(),
            ..self
        }
    }
}

#[cfg(feature = "unstable")]
impl<T> SampleBuilderTrait for ReplyBuilder<'_, '_, T> {
    fn attachment<U: Into<OptionZBytes>>(self, attachment: U) -> Self {
        let attachment: OptionZBytes = attachment.into();
        Self {
            attachment: attachment.into(),
            ..self
        }
    }

    #[cfg(feature = "unstable")]
    fn source_info(self, source_info: SourceInfo) -> Self {
        Self {
            source_info,
            ..self
        }
    }
}

impl<T> QoSBuilderTrait for ReplyBuilder<'_, '_, T> {
    fn congestion_control(self, congestion_control: CongestionControl) -> Self {
        let qos = self.qos.congestion_control(congestion_control);
        Self { qos, ..self }
    }

    fn priority(self, priority: Priority) -> Self {
        let qos = self.qos.priority(priority);
        Self { qos, ..self }
    }

    fn express(self, is_express: bool) -> Self {
        let qos = self.qos.express(is_express);
        Self { qos, ..self }
    }
}

impl ValueBuilderTrait for ReplyBuilder<'_, '_, ReplyBuilderPut> {
    fn encoding<T: Into<Encoding>>(self, encoding: T) -> Self {
        Self {
            kind: ReplyBuilderPut {
                encoding: encoding.into(),
                ..self.kind
            },
            ..self
        }
    }

    fn payload<T: Into<ZBytes>>(self, payload: T) -> Self {
        Self {
            kind: ReplyBuilderPut {
                payload: payload.into(),
                ..self.kind
            },
            ..self
        }
    }
    fn value<T: Into<Value>>(self, value: T) -> Self {
        let Value { payload, encoding } = value.into();
        Self {
            kind: ReplyBuilderPut { payload, encoding },
            ..self
        }
    }
}

impl<T> Resolvable for ReplyBuilder<'_, '_, T> {
    type To = ZResult<()>;
}

impl Wait for ReplyBuilder<'_, '_, ReplyBuilderPut> {
    fn wait(self) -> <Self as Resolvable>::To {
        let key_expr = self.key_expr?.into_owned();
        let sample = SampleBuilder::put(key_expr, self.kind.payload)
            .encoding(self.kind.encoding)
            .timestamp(self.timestamp)
            .qos(self.qos.into());
        #[cfg(feature = "unstable")]
        let sample = sample.source_info(self.source_info);
        let sample = sample.attachment(self.attachment);
        self.query._reply_sample(sample.into())
    }
}

impl Wait for ReplyBuilder<'_, '_, ReplyBuilderDelete> {
    fn wait(self) -> <Self as Resolvable>::To {
        let key_expr = self.key_expr?.into_owned();
        let sample = SampleBuilder::delete(key_expr)
            .timestamp(self.timestamp)
            .qos(self.qos.into());
        #[cfg(feature = "unstable")]
        let sample = sample.source_info(self.source_info);
        let sample = sample.attachment(self.attachment);
        self.query._reply_sample(sample.into())
    }
}

impl Query {
    fn _reply_sample(&self, sample: Sample) -> ZResult<()> {
        let c = zcondfeat!(
            "unstable",
            !self._accepts_any_replies().unwrap_or(false),
            true
        );
        if c && !self.key_expr().intersects(&sample.key_expr) {
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
                        ext_attachment: sample.attachment.map(|a| a.into()),
                        ext_unknown: vec![],
                        payload: sample.payload.into(),
                    }),
                    SampleKind::Delete => ReplyBody::Del(Del {
                        timestamp: sample.timestamp,
                        ext_sinfo,
                        ext_attachment: sample.attachment.map(|a| a.into()),
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

impl IntoFuture for ReplyBuilder<'_, '_, ReplyBuilderPut> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

impl IntoFuture for ReplyBuilder<'_, '_, ReplyBuilderDelete> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

/// A builder returned by [`Query::reply_err()`](Query::reply_err).
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
#[derive(Debug)]
pub struct ReplyErrBuilder<'a> {
    query: &'a Query,
    value: Value,
}

impl ValueBuilderTrait for ReplyErrBuilder<'_> {
    fn encoding<T: Into<Encoding>>(self, encoding: T) -> Self {
        let mut value = self.value.clone();
        value.encoding = encoding.into();
        Self { value, ..self }
    }

    fn payload<T: Into<ZBytes>>(self, payload: T) -> Self {
        let mut value = self.value.clone();
        value.payload = payload.into();
        Self { value, ..self }
    }

    fn value<T: Into<Value>>(self, value: T) -> Self {
        Self {
            value: value.into(),
            ..self
        }
    }
}

impl<'a> Resolvable for ReplyErrBuilder<'a> {
    type To = ZResult<()>;
}

impl Wait for ReplyErrBuilder<'_> {
    fn wait(self) -> <Self as Resolvable>::To {
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
                #[cfg(feature = "shared-memory")]
                ext_shm: None,
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

impl<'a> IntoFuture for ReplyErrBuilder<'a> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
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
/// use zenoh::prelude::*;
///
/// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
/// let queryable = session.declare_queryable("key/expression").await.unwrap();
/// while let Ok(query) = queryable.recv_async().await {
///     println!(">> Handling query '{}'", query.selector());
///     query.reply("key/expression", "value")
///         .await
///         .unwrap();
/// }
/// # }
/// ```
#[derive(Debug)]
pub(crate) struct CallbackQueryable<'a> {
    pub(crate) session: SessionRef<'a>,
    pub(crate) state: Arc<QueryableState>,
    background: bool,
}

impl<'a> Undeclarable<(), QueryableUndeclaration<'a>> for CallbackQueryable<'a> {
    fn undeclare_inner(self, _: ()) -> QueryableUndeclaration<'a> {
        QueryableUndeclaration {
            queryable: ManuallyDrop::new(self),
        }
    }
}

/// A [`Resolvable`] returned when undeclaring a queryable.
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh::prelude::*;
///
/// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
/// let queryable = session.declare_queryable("key/expression").await.unwrap();
/// queryable.undeclare().await.unwrap();
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
pub struct QueryableUndeclaration<'a> {
    // ManuallyDrop wrapper prevents the drop code to be executed,
    // which would lead to a double undeclaration
    queryable: ManuallyDrop<CallbackQueryable<'a>>,
}

impl Resolvable for QueryableUndeclaration<'_> {
    type To = ZResult<()>;
}

impl Wait for QueryableUndeclaration<'_> {
    fn wait(self) -> <Self as Resolvable>::To {
        self.queryable
            .session
            .close_queryable(self.queryable.state.id)
    }
}

impl<'a> IntoFuture for QueryableUndeclaration<'a> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

impl Drop for CallbackQueryable<'_> {
    fn drop(&mut self) {
        if !self.background {
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
/// use zenoh::prelude::*;
///
/// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
/// let queryable = session.declare_queryable("key/expression").await.unwrap();
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
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// let queryable = session
    ///     .declare_queryable("key/expression")
    ///     .callback(|query| {println!(">> Handling query '{}'", query.selector());})
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
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// let mut n = 0;
    /// let queryable = session
    ///     .declare_queryable("key/expression")
    ///     .callback_mut(move |query| {n += 1;})
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
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// let queryable = session
    ///     .declare_queryable("key/expression")
    ///     .with(flume::bounded(32))
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
/// use zenoh::prelude::*;
///
/// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
/// let queryable = session
///     .declare_queryable("key/expression")
///     .with(flume::bounded(32))
///     .await
///     .unwrap();
/// while let Ok(query) = queryable.recv_async().await {
///     println!(">> Handling query '{}'", query.selector());
///     query.reply("key/expression", "value")
///         .await
///         .unwrap();
/// }
/// # }
/// ```
#[non_exhaustive]
#[derive(Debug)]
pub struct Queryable<'a, Handler> {
    pub(crate) queryable: CallbackQueryable<'a>,
    pub(crate) handler: Handler,
}

impl<'a, Handler> Queryable<'a, Handler> {
    /// Returns the [`EntityGlobalId`] of this Queryable.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// let queryable = session.declare_queryable("key/expression")
    ///     .await
    ///     .unwrap();
    /// let queryable_id = queryable.id();
    /// # }
    /// ```
    #[zenoh_macros::unstable]
    pub fn id(&self) -> EntityGlobalId {
        EntityGlobalId {
            zid: self.queryable.session.zid().into(),
            eid: self.queryable.state.id,
        }
    }

    /// Returns a reference to this queryable's handler.
    /// An handler is anything that implements [`IntoHandler`].
    /// The default handler is [`DefaultHandler`].
    pub fn handler(&self) -> &Handler {
        &self.handler
    }

    /// Returns a mutable reference to this queryable's handler.
    /// An handler is anything that implements [`IntoHandler`].
    /// The default handler is [`DefaultHandler`].
    pub fn handler_mut(&mut self) -> &mut Handler {
        &mut self.handler
    }

    #[inline]
    pub fn undeclare(self) -> impl Resolve<ZResult<()>> + 'a {
        Undeclarable::undeclare_inner(self, ())
    }

    /// Make the queryable run in background, until the session is closed.
    #[inline]
    #[zenoh_macros::unstable]
    pub fn background(mut self) {
        self.queryable.background = true;
    }
}

impl<'a, T> Undeclarable<(), QueryableUndeclaration<'a>> for Queryable<'a, T> {
    fn undeclare_inner(self, _: ()) -> QueryableUndeclaration<'a> {
        Undeclarable::undeclare_inner(self.queryable, ())
    }
}

impl<Handler> Deref for Queryable<'_, Handler> {
    type Target = Handler;

    fn deref(&self) -> &Self::Target {
        self.handler()
    }
}

impl<Handler> DerefMut for Queryable<'_, Handler> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.handler_mut()
    }
}

impl<'a, Handler> Resolvable for QueryableBuilder<'a, '_, Handler>
where
    Handler: IntoHandler<'static, Query> + Send,
    Handler::Handler: Send,
{
    type To = ZResult<Queryable<'a, Handler::Handler>>;
}

impl<'a, Handler> Wait for QueryableBuilder<'a, '_, Handler>
where
    Handler: IntoHandler<'static, Query> + Send,
    Handler::Handler: Send,
{
    fn wait(self) -> <Self as Resolvable>::To {
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
                    background: false,
                },
                handler: receiver,
            })
    }
}

impl<'a, Handler> IntoFuture for QueryableBuilder<'a, '_, Handler>
where
    Handler: IntoHandler<'static, Query> + Send,
    Handler::Handler: Send,
{
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}
