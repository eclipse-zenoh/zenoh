//
// Copyright (c) 2024 ZettaScale Technology
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
    future::{IntoFuture, Ready},
    sync::Arc,
    time::Duration,
};

use zenoh_core::{Resolvable, Wait};
use zenoh_protocol::{
    core::{CongestionControl, Parameters},
    network::request::ext::QueryTarget,
};
use zenoh_result::ZResult;

use super::sample::QoSBuilderTrait;
#[cfg(feature = "unstable")]
use crate::api::query::ReplyKeyExpr;
#[cfg(feature = "unstable")]
use crate::api::sample::SourceInfo;
#[cfg(feature = "unstable")]
use crate::query::ZenohParameters;
use crate::{
    api::{
        builders::sample::{EncodingBuilderTrait, SampleBuilderTrait},
        bytes::ZBytes,
        encoding::Encoding,
        handlers::{locked, Callback, DefaultHandler, IntoHandler},
        querier::Querier,
        sample::{Locality, QoSBuilder},
    },
    bytes::OptionZBytes,
    key_expr::KeyExpr,
    qos::Priority,
    query::{QueryConsolidation, Reply},
    Session,
};

/// A builder for initializing a `querier`.
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh::{query::{ConsolidationMode, QueryTarget}};
///
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let querier = session.declare_querier("key/expression")
///     .target(QueryTarget::All)
///     .consolidation(ConsolidationMode::None)
///     .await
///     .unwrap();
/// let replies = querier.get()
///     .parameters("value>1")
///     .await
///     .unwrap();
/// while let Ok(reply) = replies.recv_async().await {
///     println!("Received {:?}", reply.result())
/// }
/// # }
/// ```
#[zenoh_macros::unstable]
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
#[derive(Debug)]
pub struct QuerierBuilder<'a, 'b> {
    pub(crate) session: &'a Session,
    pub(crate) key_expr: ZResult<KeyExpr<'b>>,
    pub(crate) target: QueryTarget,
    pub(crate) consolidation: QueryConsolidation,
    pub(crate) qos: QoSBuilder,
    pub(crate) destination: Locality,
    pub(crate) timeout: Duration,
    #[cfg(feature = "unstable")]
    pub(crate) accept_replies: ReplyKeyExpr,
}

#[zenoh_macros::internal_trait]
impl QoSBuilderTrait for QuerierBuilder<'_, '_> {
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

impl QuerierBuilder<'_, '_> {
    /// Change the target of the querier queries.
    #[inline]
    pub fn target(self, target: QueryTarget) -> Self {
        Self { target, ..self }
    }

    /// Change the consolidation mode of the querier queries.
    #[inline]
    pub fn consolidation<QC: Into<QueryConsolidation>>(self, consolidation: QC) -> Self {
        Self {
            consolidation: consolidation.into(),
            ..self
        }
    }

    /// Restrict the matching queryables that will receive the queries
    /// to the ones that have the given [`Locality`](Locality).
    #[zenoh_macros::unstable]
    #[inline]
    pub fn allowed_destination(self, destination: Locality) -> Self {
        Self {
            destination,
            ..self
        }
    }

    /// Set queries timeout.
    #[inline]
    pub fn timeout(self, timeout: Duration) -> Self {
        Self { timeout, ..self }
    }

    /// By default, only replies whose key expressions intersect
    /// with the querier key expression will be received by calls to [`Querier::get`](crate::query::Querier::get) method.
    ///
    /// If allowed to through `accept_replies(ReplyKeyExpr::Any)`, queryables may also reply on key
    /// expressions that don't intersect with the querier's queries.
    #[zenoh_macros::unstable]
    pub fn accept_replies(self, accept: ReplyKeyExpr) -> Self {
        Self {
            accept_replies: accept,
            ..self
        }
    }
}

impl<'b> Resolvable for QuerierBuilder<'_, 'b> {
    type To = ZResult<Querier<'b>>;
}

impl Wait for QuerierBuilder<'_, '_> {
    fn wait(self) -> <Self as Resolvable>::To {
        let mut key_expr = self.key_expr?;
        if !key_expr.is_fully_optimized(&self.session.0) {
            key_expr = self.session.declare_keyexpr(key_expr).wait()?;
        }
        let id = self
            .session
            .0
            .declare_querier_inner(key_expr.clone(), self.destination)?;
        Ok(Querier {
            session: self.session.downgrade(),
            id,
            key_expr,
            qos: self.qos.into(),
            destination: self.destination,
            undeclare_on_drop: true,
            target: self.target,
            consolidation: self.consolidation,
            timeout: self.timeout,
            #[cfg(feature = "unstable")]
            accept_replies: self.accept_replies,
            #[cfg(feature = "unstable")]
            matching_listeners: Default::default(),
        })
    }
}

impl IntoFuture for QuerierBuilder<'_, '_> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

/// A builder for initializing a `query` to be sent from the querier.
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh::{query::{ConsolidationMode, QueryTarget}};
///
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let querier = session.declare_querier("key/expression")
///     .target(QueryTarget::All)
///     .consolidation(ConsolidationMode::None)
///     .await
///     .unwrap();
/// let replies = querier
///     .get()
///     .parameters("value>1")
///     .await
///     .unwrap();
/// while let Ok(reply) = replies.recv_async().await {
///     println!("Received {:?}", reply.result())
/// }
/// # }
/// ```
#[zenoh_macros::unstable]
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
#[derive(Debug)]
pub struct QuerierGetBuilder<'a, 'b, Handler> {
    pub(crate) querier: &'a Querier<'a>,
    pub(crate) parameters: Parameters<'b>,
    pub(crate) handler: Handler,
    pub(crate) value: Option<(ZBytes, Encoding)>,
    pub(crate) attachment: Option<ZBytes>,
    #[cfg(feature = "unstable")]
    pub(crate) source_info: SourceInfo,
}

#[zenoh_macros::internal_trait]
impl<Handler> SampleBuilderTrait for QuerierGetBuilder<'_, '_, Handler> {
    #[zenoh_macros::unstable]
    fn source_info(self, source_info: SourceInfo) -> Self {
        Self {
            source_info,
            ..self
        }
    }

    fn attachment<T: Into<OptionZBytes>>(self, attachment: T) -> Self {
        let attachment: OptionZBytes = attachment.into();
        Self {
            attachment: attachment.into(),
            ..self
        }
    }
}

#[zenoh_macros::internal_trait]
impl<Handler> EncodingBuilderTrait for QuerierGetBuilder<'_, '_, Handler> {
    fn encoding<T: Into<Encoding>>(self, encoding: T) -> Self {
        let mut value = self.value.unwrap_or_default();
        value.1 = encoding.into();
        Self {
            value: Some(value),
            ..self
        }
    }
}

impl<'a, 'b> QuerierGetBuilder<'a, 'b, DefaultHandler> {
    /// Receive the replies for this query with a callback.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::{query::{ConsolidationMode, QueryTarget}};
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let querier = session.declare_querier("key/expression")
    ///     .target(QueryTarget::All)
    ///     .consolidation(ConsolidationMode::None)
    ///     .await
    ///     .unwrap();
    /// let _ = querier
    ///     .get()
    ///     .callback(|reply| {println!("Received {:?}", reply.result());})
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
    #[zenoh_macros::unstable]
    #[inline]
    pub fn callback<F>(self, callback: F) -> QuerierGetBuilder<'a, 'b, Callback<Reply>>
    where
        F: Fn(Reply) + Send + Sync + 'static,
    {
        self.with(Callback::new(Arc::new(callback)))
    }

    /// Receive the replies for this query with a mutable callback.
    ///
    /// Using this guarantees that your callback will never be called concurrently.
    /// If your callback is also accepted by the [`callback`](crate::query::QuerierGetBuilder::callback) method, we suggest you use it instead of `callback_mut`.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::{query::{ConsolidationMode, QueryTarget}};
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let querier = session.declare_querier("key/expression")
    ///     .target(QueryTarget::All)
    ///     .consolidation(ConsolidationMode::None)
    ///     .await
    ///     .unwrap();
    /// let mut n = 0;
    /// let _ = querier
    ///     .get()
    ///     .callback_mut(move |reply| {n += 1;})
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
    #[zenoh_macros::unstable]
    #[inline]
    pub fn callback_mut<F>(self, callback: F) -> QuerierGetBuilder<'a, 'b, Callback<Reply>>
    where
        F: FnMut(Reply) + Send + Sync + 'static,
    {
        self.callback(locked(callback))
    }

    /// Receive the replies for this query with a [`Handler`](crate::handlers::IntoHandler).
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::{query::{ConsolidationMode, QueryTarget}};
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let querier = session.declare_querier("key/expression")
    ///     .target(QueryTarget::All)
    ///     .consolidation(ConsolidationMode::None)
    ///     .await
    ///     .unwrap();
    /// let replies = querier
    ///     .get()
    ///     .with(flume::bounded(32))
    ///     .await
    ///     .unwrap();
    /// while let Ok(reply) = replies.recv_async().await {
    ///     println!("Received {:?}", reply.result());
    /// }
    /// # }
    /// ```
    #[zenoh_macros::unstable]
    #[inline]
    pub fn with<Handler>(self, handler: Handler) -> QuerierGetBuilder<'a, 'b, Handler>
    where
        Handler: IntoHandler<Reply>,
    {
        let QuerierGetBuilder {
            querier,
            parameters,
            value,
            attachment,
            #[cfg(feature = "unstable")]
            source_info,
            handler: _,
        } = self;
        QuerierGetBuilder {
            querier,
            parameters,
            value,
            attachment,
            #[cfg(feature = "unstable")]
            source_info,
            handler,
        }
    }
}
impl<'b, Handler> QuerierGetBuilder<'_, 'b, Handler> {
    /// Set the query payload.
    #[inline]
    #[zenoh_macros::unstable]
    pub fn payload<IntoZBytes>(mut self, payload: IntoZBytes) -> Self
    where
        IntoZBytes: Into<ZBytes>,
    {
        let mut value = self.value.unwrap_or_default();
        value.0 = payload.into();
        self.value = Some(value);
        self
    }

    /// Set the query selector parameters.
    #[inline]
    #[zenoh_macros::unstable]
    pub fn parameters<P>(mut self, parameters: P) -> Self
    where
        P: Into<Parameters<'b>>,
    {
        self.parameters = parameters.into();
        self
    }
}

impl<Handler> Resolvable for QuerierGetBuilder<'_, '_, Handler>
where
    Handler: IntoHandler<Reply> + Send,
    Handler::Handler: Send,
{
    type To = ZResult<Handler::Handler>;
}

impl<Handler> Wait for QuerierGetBuilder<'_, '_, Handler>
where
    Handler: IntoHandler<Reply> + Send,
    Handler::Handler: Send,
{
    fn wait(self) -> <Self as Resolvable>::To {
        let (callback, receiver) = self.handler.into_handler();

        #[allow(unused_mut)]
        // mut is only needed when building with "unstable" feature, which might add extra internal parameters on top of the user-provided ones
        let mut parameters = self.parameters.clone();
        #[cfg(feature = "unstable")]
        if self.querier.accept_replies() == ReplyKeyExpr::Any {
            parameters.set_reply_key_expr_any();
        }
        self.querier
            .session
            .query(
                &self.querier.key_expr,
                &parameters,
                self.querier.target,
                self.querier.consolidation,
                self.querier.qos,
                self.querier.destination,
                self.querier.timeout,
                self.value,
                self.attachment,
                #[cfg(feature = "unstable")]
                self.source_info,
                callback,
            )
            .map(|_| receiver)
    }
}

impl<Handler> IntoFuture for QuerierGetBuilder<'_, '_, Handler>
where
    Handler: IntoHandler<Reply> + Send,
    Handler::Handler: Send,
{
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}
