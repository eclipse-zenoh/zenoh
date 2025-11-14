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
use crate::api::cancellation::CancellationTokenBuilderTrait;
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

/// A builder for initializing a [`Querier`](crate::query::Querier).
/// Returned by the
/// [`Session::declare_querier`](crate::Session::declare_querier) method.
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
    /// Change the target(s) of the querier's queries.
    ///
    /// This method allows to specify whether the request should just return the
    /// data available in the network which matches the key expression
    /// ([QueryTarget::BestMatching], default) or if it should arrive to
    /// all queryables matching the key expression ([QueryTarget::All],
    /// [QueryTarget::AllComplete]).
    ///
    /// See also the [`complete`](crate::query::QueryableBuilder::complete) setting
    /// of the [`Queryable`](crate::query::Queryable)
    #[inline]
    pub fn target(self, target: QueryTarget) -> Self {
        Self { target, ..self }
    }

    /// Change the consolidation mode of the querier's queries.
    ///
    /// The multiple replies to a query may arrive from the network. The
    /// [`ConsolidationMode`](crate::query::ConsolidationMode) enum defines
    /// the strategies of filtering and reordering these replies.
    /// The wrapper struct [`QueryConsolidation`](crate::query::QueryConsolidation)
    /// allows to set an [`ConsolidationMode::AUTO`](crate::query::QueryConsolidation::AUTO)
    /// mode, which lets the implementation choose the best strategy.
    #[inline]
    pub fn consolidation<QC: Into<QueryConsolidation>>(self, consolidation: QC) -> Self {
        Self {
            consolidation: consolidation.into(),
            ..self
        }
    }

    /// Restrict the matching queryables that will receive the queries
    /// to the ones that have the given [`Locality`](Locality).
    #[inline]
    pub fn allowed_destination(self, destination: Locality) -> Self {
        Self {
            destination,
            ..self
        }
    }

    /// Set the query timeout.
    #[inline]
    pub fn timeout(self, timeout: Duration) -> Self {
        Self { timeout, ..self }
    }

    /// See details in the [`ReplyKeyExpr`](crate::query::ReplyKeyExpr) documentation.
    /// Queries may or may not accept replies on key expressions that do not intersect with their own key expression.
    /// This setter allows you to define whether this querier accepts such disjoint replies.
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

/// A builder for configuring a [`get`](crate::query::Querier::get)
/// operation from a [`Querier`](crate::query::Querier).
/// The builder resolves to a [`handler`](crate::handlers) generating a series of
/// [`Reply`](crate::api::query::Reply) for each response received.
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
    #[cfg(feature = "unstable")]
    pub(crate) cancellation_token: Option<crate::api::cancellation::CancellationToken>,
}

#[cfg(feature = "unstable")]
#[zenoh_macros::internal_trait]
impl<Handler> CancellationTokenBuilderTrait for QuerierGetBuilder<'_, '_, Handler> {
    /// Provide a cancellation token that can be used later to interrupt GET operation.
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
    /// let ct = zenoh::cancellation::CancellationToken::default();
    /// let _ = querier
    ///     .get()
    ///     .callback(|reply| {println!("Received {:?}", reply.result());})
    ///     .cancellation_token(ct.clone())
    ///     .await
    ///     .unwrap();
    ///
    /// tokio::task::spawn(async move {
    ///     tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    ///     ct.cancel().await.unwrap();
    /// });
    /// # }
    /// ```
    #[zenoh_macros::unstable_doc]
    fn cancellation_token(
        self,
        cancellation_token: crate::api::cancellation::CancellationToken,
    ) -> Self {
        Self {
            cancellation_token: Some(cancellation_token),
            ..self
        }
    }
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
    #[inline]
    pub fn callback<F>(self, callback: F) -> QuerierGetBuilder<'a, 'b, Callback<Reply>>
    where
        F: Fn(Reply) + Send + Sync + 'static,
    {
        self.with(Callback::from(callback))
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
            #[cfg(feature = "unstable")]
            cancellation_token,
        } = self;
        QuerierGetBuilder {
            querier,
            parameters,
            value,
            attachment,
            #[cfg(feature = "unstable")]
            source_info,
            handler,
            #[cfg(feature = "unstable")]
            cancellation_token,
        }
    }
}
impl<'b, Handler> QuerierGetBuilder<'_, 'b, Handler> {
    /// Set the query payload.
    #[inline]
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
        #[allow(unused_mut)] // mut is needed only for unstable cancellation_token
        let (mut callback, receiver) = self.handler.into_handler();
        #[cfg(feature = "unstable")]
        if self
            .cancellation_token
            .as_ref()
            .map(|ct| ct.is_cancelled())
            .unwrap_or(false)
        {
            return Ok(receiver);
        };
        #[cfg(feature = "unstable")]
        let cancellation_token_and_receiver = self.cancellation_token.map(|ct| {
            let (notifier, receiver) =
                crate::api::cancellation::create_sync_group_receiver_notifier_pair();
            callback.set_on_drop_notifier(notifier);
            (ct, receiver)
        });
        #[allow(unused_mut)]
        // mut is only needed when building with "unstable" feature, which might add extra internal parameters on top of the user-provided ones
        let mut parameters = self.parameters.clone();
        #[cfg(feature = "unstable")]
        if self.querier.accept_replies() == ReplyKeyExpr::Any {
            parameters.set_reply_key_expr_any();
        }
        #[allow(unused_variables)] // qid is only needed for unstable cancellation_token
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
            .map(|qid| {
                #[cfg(feature = "unstable")]
                if let Some((cancellation_token, cancel_receiver)) = cancellation_token_and_receiver
                {
                    let session_clone = self.querier.session.clone();
                    let on_cancel = move || {
                        let _ = session_clone.cancel_query(qid); // fails only if no associated query exists - likely because it was already finalized
                        Ok(())
                    };
                    cancellation_token.add_on_cancel_handler(cancel_receiver, Box::new(on_cancel));
                }
                receiver
            })
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
