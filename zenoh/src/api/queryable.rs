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
    ops::{Deref, DerefMut},
    sync::Arc,
};

use tracing::error;
use zenoh_core::{Resolvable, Wait};
use zenoh_protocol::{
    core::{EntityId, Parameters, WireExpr, ZenohIdProto},
    network::{response, Mapping, RequestId, Response, ResponseFinal},
    zenoh::{self, reply::ReplyBody, Del, Put, ResponseBody},
};
use zenoh_result::ZResult;
#[zenoh_macros::unstable]
use {
    crate::api::query::ReplyKeyExpr, zenoh_config::wrappers::EntityGlobalId,
    zenoh_protocol::core::EntityGlobalIdProto,
};

#[cfg(feature = "unstable")]
use crate::api::cancellation::SyncGroup;
#[zenoh_macros::unstable]
use crate::api::sample::SourceInfo;
#[zenoh_macros::unstable]
use crate::api::selector::ZenohParameters;
#[zenoh_macros::internal]
use crate::net::primitives::DummyPrimitives;
use crate::{
    api::{
        builders::reply::{ReplyBuilder, ReplyBuilderDelete, ReplyBuilderPut, ReplyErrBuilder},
        bytes::ZBytes,
        encoding::Encoding,
        handlers::CallbackParameter,
        key_expr::KeyExpr,
        sample::{Locality, Sample, SampleKind},
        selector::Selector,
        session::{UndeclarableSealed, WeakSession},
        Id,
    },
    handlers::Callback,
    net::primitives::Primitives,
};

pub(crate) struct LocalReplyPrimitives {
    session: WeakSession,
}

pub(crate) struct RemoteReplyPrimitives {
    pub(crate) session: Option<WeakSession>,
    pub(crate) primitives: Arc<dyn Primitives>,
}

pub(crate) enum ReplyPrimitives {
    Local(LocalReplyPrimitives),
    Remote(RemoteReplyPrimitives),
}

impl ReplyPrimitives {
    pub(crate) fn new_local(session: WeakSession) -> Self {
        ReplyPrimitives::Local(LocalReplyPrimitives { session })
    }

    pub(crate) fn new_remote(
        session: Option<WeakSession>,
        primitives: Arc<dyn Primitives>,
    ) -> Self {
        ReplyPrimitives::Remote(RemoteReplyPrimitives {
            session,
            primitives,
        })
    }

    pub(crate) fn send_response_final(&self, msg: &mut ResponseFinal) {
        match self {
            ReplyPrimitives::Local(local) => local.session.send_response_final(msg),
            ReplyPrimitives::Remote(remote) => remote.primitives.send_response_final(msg),
        }
    }

    pub(crate) fn send_response(&self, msg: &mut Response) {
        match self {
            ReplyPrimitives::Local(local) => local.session.send_response(msg),
            ReplyPrimitives::Remote(remote) => remote.primitives.send_response(msg),
        }
    }

    pub(crate) fn keyexpr_to_wire(&self, key_expr: &KeyExpr) -> WireExpr<'static> {
        match self {
            ReplyPrimitives::Local(local) => key_expr.to_wire_local(&local.session).to_owned(),
            ReplyPrimitives::Remote(remote) => match &remote.session {
                Some(s) => key_expr.to_wire(s).to_owned(),
                None => WireExpr {
                    scope: 0,
                    suffix: std::borrow::Cow::Owned(key_expr.as_str().into()),
                    mapping: Mapping::Sender,
                },
            },
        }
    }
}

pub(crate) struct QueryInner {
    pub(crate) key_expr: KeyExpr<'static>,
    pub(crate) parameters: Parameters<'static>,
    pub(crate) qid: RequestId,
    pub(crate) zid: ZenohIdProto,
    #[cfg(feature = "unstable")]
    pub(crate) source_info: Option<SourceInfo>,
    pub(crate) primitives: ReplyPrimitives,
}

impl QueryInner {
    #[zenoh_macros::internal]
    fn empty() -> Self {
        QueryInner {
            key_expr: KeyExpr::dummy(),
            parameters: Parameters::empty(),
            qid: 0,
            zid: ZenohIdProto::default(),
            #[cfg(feature = "unstable")]
            source_info: None,
            primitives: ReplyPrimitives::new_remote(None, Arc::new(DummyPrimitives)),
        }
    }
}

impl Drop for QueryInner {
    fn drop(&mut self) {
        self.primitives.send_response_final(&mut ResponseFinal {
            rid: self.qid,
            ext_qos: response::ext::QoSType::RESPONSE_FINAL,
            ext_tstamp: None,
        });
    }
}

/// The request received by a [`Queryable`].
///
/// The `Query` provides all data sent by [`Querier::get`](crate::query::Querier::get)
/// or [`Session::get`](crate::Session::get): the key expression, the
/// parameters, the payload, and the attachment, if any.
///
/// The reply to the query should be made with one of its methods:
/// - [`Query::reply`](crate::query::Query::reply) to reply with a data [`Sample`](crate::sample::Sample) of kind [`Put`](crate::sample::SampleKind::Put),
/// - [`Query::reply_del`](crate::query::Query::reply_del) to reply with a data [`Sample`](crate::sample::Sample) of kind [`Delete`](crate::sample::SampleKind::Delete),
/// - [`Query::reply_err`](crate::query::Query::reply_err) to send an error reply.
///
/// The important detail: the [`Query::key_expr`] is **not** the key expression
/// which should be used as the parameter of [`reply`](Query::reply), because it may contain globs.
/// The [`Queryable`]'s key expression is the one that should be used.
/// For example, the `Query` may contain the key expression `foo/*` and the reply
/// should be sent with `foo/bar` or `foo/baz`, depending on the concrete querier.
#[derive(Clone)]
pub struct Query {
    pub(crate) inner: Arc<QueryInner>,
    pub(crate) eid: EntityId,
    pub(crate) value: Option<(ZBytes, Encoding)>,
    pub(crate) attachment: Option<ZBytes>,
}

impl Query {
    /// The full [`Selector`] of this Query.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let queryable = session
    ///     .declare_queryable("key/expression")
    ///     .callback(move |query| { println!("{}", query.selector()); })
    ///     .await
    ///     .unwrap();
    /// # session.get("key/expression").await.unwrap();
    /// # }
    #[inline(always)]
    pub fn selector(&self) -> Selector<'_> {
        Selector::borrowed(&self.inner.key_expr, &self.inner.parameters)
    }

    /// The key selector part of this Query.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let queryable = session
    ///     .declare_queryable("key/expression")
    ///     .callback(move |query| { println!("{}", query.key_expr()); })
    ///     .await
    ///     .unwrap();
    /// # session.get("key/expression").await.unwrap();
    /// # }
    #[inline(always)]
    pub fn key_expr(&self) -> &KeyExpr<'static> {
        &self.inner.key_expr
    }

    /// This Query's selector parameters.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let queryable = session
    ///     .declare_queryable("key/expression")
    ///     .callback(move |query| { println!("{}", query.parameters()); })
    ///     .await
    ///     .unwrap();
    /// # session.get("key/expression").await.unwrap();
    /// # }
    #[inline(always)]
    pub fn parameters(&self) -> &Parameters<'static> {
        &self.inner.parameters
    }

    /// This Query's payload.
    ///
    /// # Examples
    /// ```
    /// # use zenoh::bytes::ZBytes;
    /// # #[tokio::main]
    /// # async fn main() {
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let queryable = session
    ///     .declare_queryable("key/expression")
    ///     .callback(move |query| {
    ///         let payload: Option<&ZBytes> = query.payload();
    ///     })
    ///     .await
    ///     .unwrap();
    /// # session.get("key/expression").await.unwrap();
    /// # }
    #[inline(always)]
    pub fn payload(&self) -> Option<&ZBytes> {
        self.value.as_ref().map(|v| &v.0)
    }

    /// This Query's payload (mutable).
    ///
    /// # Examples
    /// ```
    /// # use zenoh::bytes::ZBytes;
    /// # #[tokio::main]
    /// # async fn main() {
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let queryable = session
    ///     .declare_queryable("key/expression")
    ///     .callback(move |mut query| {
    ///         let payload: Option<&mut ZBytes> = query.payload_mut();
    ///     })
    ///     .await
    ///     .unwrap();
    /// # session.get("key/expression").await.unwrap();
    /// # }
    #[inline(always)]
    pub fn payload_mut(&mut self) -> Option<&mut ZBytes> {
        self.value.as_mut().map(|v| &mut v.0)
    }

    /// This Query's encoding.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let queryable = session
    ///     .declare_queryable("key/expression")
    ///     .callback(move |query| { println!("{:?}", query.encoding()); })
    ///     .await
    ///     .unwrap();
    /// # session.get("key/expression").await.unwrap();
    /// # }
    #[inline(always)]
    pub fn encoding(&self) -> Option<&Encoding> {
        self.value.as_ref().map(|v| &v.1)
    }

    /// This Query's attachment.
    ///
    /// # Examples
    /// ```
    /// # use zenoh::bytes::ZBytes;
    /// # #[tokio::main]
    /// # async fn main() {
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let queryable = session
    ///     .declare_queryable("key/expression")
    ///     .callback(move |query| {
    ///         let attachment: Option<&ZBytes> = query.attachment();
    ///     })
    ///     .await
    ///     .unwrap();
    /// # session.get("key/expression").await.unwrap();
    /// # }
    pub fn attachment(&self) -> Option<&ZBytes> {
        self.attachment.as_ref()
    }

    /// This Query's attachment (mutable).
    ///
    /// # Examples
    /// ```
    /// # use zenoh::bytes::ZBytes;
    /// # #[tokio::main]
    /// # async fn main() {
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let queryable = session
    ///     .declare_queryable("key/expression")
    ///     .callback(move |mut query| {
    ///         let attachment: Option<&mut ZBytes> = query.attachment_mut();
    ///     })
    ///     .await
    ///     .unwrap();
    /// # session.get("key/expression").await.unwrap();
    /// # }
    pub fn attachment_mut(&mut self) -> Option<&mut ZBytes> {
        self.attachment.as_mut()
    }

    /// Gets info on the source of this Query.
    #[zenoh_macros::unstable]
    #[inline]
    pub fn source_info(&self) -> Option<&SourceInfo> {
        self.inner.source_info.as_ref()
    }

    /// Sends a reply in the form of [`Sample`] to this Query.
    ///
    /// This api is for internal use only.
    ///
    /// # Examples
    /// ```
    /// # use zenoh::sample::Sample;
    /// # #[tokio::main]
    /// # async fn main() {
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let queryable = session
    ///     .declare_queryable("key/expression")
    ///     .callback(move |query| { query.reply_sample(Sample::empty()); })
    ///     .await
    ///     .unwrap();
    /// # session.get("key/expression").await.unwrap();
    /// # }
    #[inline(always)]
    #[zenoh_macros::internal]
    pub fn reply_sample(&self, sample: Sample) -> ReplySample<'_> {
        ReplySample {
            query: self,
            sample,
        }
    }

    /// Sends a [`Sample`](crate::sample::Sample) of kind [`Put`](crate::sample::SampleKind::Put)
    /// as a reply to this Query.
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
        ReplyBuilder::<'_, 'b, ReplyBuilderPut>::new(self, key_expr, payload)
    }

    /// Sends a [`ReplyError`](crate::query::ReplyError) as a reply to this Query.
    #[inline(always)]
    pub fn reply_err<IntoZBytes>(&self, payload: IntoZBytes) -> ReplyErrBuilder<'_>
    where
        IntoZBytes: Into<ZBytes>,
    {
        ReplyErrBuilder::new(self, payload)
    }

    /// Sends a [`Sample`](crate::sample::Sample) of kind [`Delete`](crate::sample::SampleKind::Delete)
    /// as a reply to this Query.
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
        ReplyBuilder::<'_, 'b, ReplyBuilderDelete>::new(self, key_expr)
    }

    /// See details in [`ReplyKeyExpr`](crate::query::ReplyKeyExpr) documentation.
    ///
    /// Queries may or may not accept replies on key expressions that do not intersect with their own key expression.
    /// This getter allows you to check whether or not a specific query does so.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let queryable = session
    ///     .declare_queryable("key/expression")
    ///     .callback(move |query| { query.accepts_replies(); })
    ///     .await
    ///     .unwrap();
    /// # session.get("key/expression").await.unwrap();
    /// # }
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
        Ok(self.parameters().reply_key_expr_any())
    }

    /// Constructs an empty Query without payload or attachment, referencing the same inner query.
    ///
    /// # Examples
    /// ```
    /// # fn main() {
    /// let query = unsafe { zenoh::query::Query::empty() };
    /// # }
    #[zenoh_macros::internal]
    pub unsafe fn empty() -> Self {
        Query {
            inner: Arc::new(QueryInner::empty()),
            eid: 0,
            value: None,
            attachment: None,
        }
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

impl CallbackParameter for Query {
    type Message<'a> = Self;

    fn from_message(msg: Self::Message<'_>) -> Self {
        msg
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

impl Query {
    pub(crate) fn _reply_sample(&self, sample: Sample) -> ZResult<()> {
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
        let ext_sinfo = sample.source_info.map(Into::into);
        self.inner.primitives.send_response(&mut Response {
            rid: self.inner.qid,
            wire_expr: self.inner.primitives.keyexpr_to_wire(&sample.key_expr),
            payload: ResponseBody::Reply(zenoh::Reply {
                consolidation: zenoh::ConsolidationMode::DEFAULT,
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
pub(crate) struct QueryableState {
    pub(crate) id: Id,
    pub(crate) key_expr: KeyExpr<'static>,
    pub(crate) complete: bool,
    pub(crate) origin: Locality,
    pub(crate) callback: Callback<Query>,
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

#[derive(Debug)]
pub(crate) struct QueryableInner {
    pub(crate) session: WeakSession,
    pub(crate) id: Id,
    pub(crate) undeclare_on_drop: bool,
    pub(crate) key_expr: KeyExpr<'static>,
}

/// A [`Resolvable`] returned when undeclaring a [`Queryable`].
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
///
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let queryable = session.declare_queryable("key/expression").await.unwrap();
/// queryable.undeclare().await.unwrap();
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
pub struct QueryableUndeclaration<Handler> {
    queryable: Queryable<Handler>,
    #[cfg(feature = "unstable")]
    wait_callbacks: bool,
}

impl<Handler> QueryableUndeclaration<Handler> {
    /// Block in undeclare operation until all currently running instances of query callbacks (if any) return.
    #[zenoh_macros::unstable]
    pub fn wait_callbacks(mut self) -> Self {
        self.wait_callbacks = true;
        self
    }
}

impl<Handler> Resolvable for QueryableUndeclaration<Handler> {
    type To = ZResult<()>;
}

impl<Handler> Wait for QueryableUndeclaration<Handler> {
    fn wait(mut self) -> <Self as Resolvable>::To {
        self.queryable.undeclare_impl()?;
        #[cfg(feature = "unstable")]
        if self.wait_callbacks {
            self.queryable.callback_sync_group.wait();
        }
        Ok(())
    }
}

impl<Handler> IntoFuture for QueryableUndeclaration<Handler> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}
/// A `Queryable` is an entity that implements the query/reply pattern.
///
/// A `Queryable` is declared by the
/// [`Session::declare_queryable`](crate::Session::declare_queryable) method
/// and serves [`Query`](crate::query::Query) using callback
/// or channel (see [handlers](crate::handlers) module documentation for details).
///
/// The `Queryable` receives [`Query`](crate::query::Query) requests from
/// [`Querier::get`](crate::query::Querier::get) or from [`Session::get`](crate::Session::get)
/// and sends back replies with the methods of the [`Query`](crate::query::Query): [`reply`](crate::query::Query::reply),
/// [`reply_err`](crate::query::Query::reply_err) or [`reply_del`](crate::query::Query::reply_del).
///
/// # Examples
///
/// Using callback:
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// use futures::prelude::*;
///
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let queryable = session
///     .declare_queryable("key/expression")
///     .callback(move |query| {
///         use crate::zenoh::Wait;
///         println!(">> Handling query '{}'", query.selector());
///         query.reply("key/expression", "value").wait().unwrap();
/// #       format!("{query}");
/// #       format!("{query:?}");
///     })
///     .await
///     .unwrap();
/// # format!("{queryable:?}");
/// # session.get("key/expression").await.unwrap();
/// # }
/// ```
///
/// Using channel handler:
/// ```no_run
/// # #[tokio::main]
/// # async fn main() {
///
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let queryable = session
///     .declare_queryable("key/expression")
///     .await
///     .unwrap();
/// while let Ok(query) = queryable.recv_async().await {
///     println!(">> Handling query '{}'", query.selector());
///     query.reply("key/expression", "value").await.unwrap();
/// }
/// // queryable is undeclared at the end of the scope
/// # }
/// ```
#[non_exhaustive]
#[derive(Debug)]
pub struct Queryable<Handler> {
    pub(crate) inner: QueryableInner,
    pub(crate) handler: Handler,
    #[cfg(feature = "unstable")]
    pub(crate) callback_sync_group: SyncGroup,
}

impl<Handler> Queryable<Handler> {
    /// Returns the [`EntityGlobalId`] of this Queryable.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let queryable = session.declare_queryable("key/expression").await.unwrap();
    /// let queryable_id = queryable.id();
    /// # }
    /// ```
    #[zenoh_macros::unstable]
    pub fn id(&self) -> EntityGlobalId {
        EntityGlobalIdProto {
            zid: self.inner.session.zid().into(),
            eid: self.inner.id,
        }
        .into()
    }

    /// Returns a reference to this queryable's handler.
    /// A handler is anything that implements [`IntoHandler`](crate::handlers::IntoHandler).
    /// The default handler is [`DefaultHandler`](crate::handlers::DefaultHandler).
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let queryable = session.declare_queryable("key/expression").await.unwrap();
    /// let handler = queryable.handler();
    /// # }
    /// ```
    pub fn handler(&self) -> &Handler {
        &self.handler
    }

    /// Returns a mutable reference to this queryable's handler.
    /// A handler is anything that implements [`IntoHandler`](crate::handlers::IntoHandler).
    /// The default handler is [`DefaultHandler`](crate::handlers::DefaultHandler).
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let mut queryable = session.declare_queryable("key/expression").await.unwrap();
    /// let handler = queryable.handler_mut();
    /// # }
    /// ```
    pub fn handler_mut(&mut self) -> &mut Handler {
        &mut self.handler
    }

    /// Undeclare the [`Queryable`].
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let queryable = session.declare_queryable("key/expression").await.unwrap();
    /// queryable.undeclare().await.unwrap();
    /// # }
    /// ```
    #[inline]
    pub fn undeclare(self) -> QueryableUndeclaration<Handler>
    where
        Handler: Send,
    {
        UndeclarableSealed::undeclare_inner(self, ())
    }

    fn undeclare_impl(&mut self) -> ZResult<()> {
        // set the flag first to avoid double panic if this function panic
        self.inner.undeclare_on_drop = false;
        self.inner.session.close_queryable(self.inner.id)
    }

    /// Make queryable run in background until the session is closed.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let mut queryable = session.declare_queryable("key/expression").await.unwrap();
    /// queryable.set_background(true);
    /// # }
    /// ```
    #[zenoh_macros::internal]
    pub fn set_background(&mut self, background: bool) {
        self.inner.undeclare_on_drop = !background;
    }

    /// Returns the [`KeyExpr`] this queryable responds to.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let queryable = session.declare_queryable("key/expression")
    ///     .await
    ///     .unwrap();
    /// let key_expr = queryable.key_expr();
    /// # }
    /// ```
    #[inline]
    pub fn key_expr(&self) -> &KeyExpr<'static> {
        &self.inner.key_expr
    }
}

impl<Handler> Drop for Queryable<Handler> {
    fn drop(&mut self) {
        if self.inner.undeclare_on_drop {
            if let Err(error) = self.undeclare_impl() {
                error!(error);
            }
        }
    }
}

impl<Handler: Send> UndeclarableSealed<()> for Queryable<Handler> {
    type Undeclaration = QueryableUndeclaration<Handler>;

    fn undeclare_inner(self, _: ()) -> Self::Undeclaration {
        QueryableUndeclaration {
            queryable: self,
            #[cfg(feature = "unstable")]
            wait_callbacks: false,
        }
    }
}

impl<Handler> Deref for Queryable<Handler> {
    type Target = Handler;

    fn deref(&self) -> &Self::Target {
        self.handler()
    }
}

impl<Handler> DerefMut for Queryable<Handler> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.handler_mut()
    }
}
