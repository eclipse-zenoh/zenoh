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
use zenoh_protocol::{core::CongestionControl, network::request::ext::QueryTarget};
use zenoh_result::ZResult;

#[cfg(feature = "unstable")]
use crate::api::cancellation::CancellationTokenBuilderTrait;
#[cfg(feature = "unstable")]
use crate::api::query::ReplyKeyExpr;
#[cfg(feature = "unstable")]
use crate::api::{sample::SourceInfo, selector::ZenohParameters};
use crate::{
    api::{
        builders::sample::{EncodingBuilderTrait, QoSBuilderTrait, SampleBuilderTrait},
        bytes::ZBytes,
        encoding::Encoding,
        handlers::{locked, Callback, DefaultHandler, IntoHandler},
        publisher::Priority,
        sample::{Locality, QoSBuilder},
        selector::Selector,
        session::Session,
    },
    bytes::OptionZBytes,
    query::{QueryConsolidation, Reply},
};

/// A builder for configuring a [`get`](crate::Session::get)
/// operation from a [`Session`](crate::Session).
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
/// let replies = session
///     .get("key/expression?value>1")
///     .target(QueryTarget::All)
///     .consolidation(ConsolidationMode::None)
///     .await
///     .unwrap();
/// while let Ok(reply) = replies.recv_async().await {
///     println!("Received {:?}", reply.result())
/// }
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
#[derive(Debug)]
pub struct SessionGetBuilder<'a, 'b, Handler> {
    pub(crate) session: &'a Session,
    pub(crate) selector: ZResult<Selector<'b>>,
    pub(crate) target: QueryTarget,
    pub(crate) consolidation: QueryConsolidation,
    pub(crate) qos: QoSBuilder,
    pub(crate) destination: Locality,
    pub(crate) timeout: Duration,
    pub(crate) handler: Handler,
    pub(crate) value: Option<(ZBytes, Encoding)>,
    pub(crate) attachment: Option<ZBytes>,
    #[cfg(feature = "unstable")]
    pub(crate) source_info: SourceInfo,
    #[cfg(feature = "unstable")]
    pub(crate) cancellation_token: Option<crate::api::cancellation::CancellationToken>,
}

#[zenoh_macros::internal_trait]
impl<Handler> SampleBuilderTrait for SessionGetBuilder<'_, '_, Handler> {
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
impl<Handler> QoSBuilderTrait for SessionGetBuilder<'_, '_, Handler> {
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

#[zenoh_macros::internal_trait]
impl<Handler> EncodingBuilderTrait for SessionGetBuilder<'_, '_, Handler> {
    fn encoding<T: Into<Encoding>>(self, encoding: T) -> Self {
        let mut value = self.value.unwrap_or_default();
        value.1 = encoding.into();
        Self {
            value: Some(value),
            ..self
        }
    }
}

#[cfg(feature = "unstable")]
#[zenoh_macros::internal_trait]
impl<Handler> CancellationTokenBuilderTrait for SessionGetBuilder<'_, '_, Handler> {
    /// Provide a cancellation token that can be used later to interrupt GET operation.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let ct = zenoh::cancellation::CancellationToken::default();
    /// let query = session
    ///     .get("key/expression")
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

impl<'a, 'b> SessionGetBuilder<'a, 'b, DefaultHandler> {
    /// Receive the replies for this query with a callback.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let queryable = session
    ///     .get("key/expression")
    ///     .callback(|reply| {println!("Received {:?}", reply.result());})
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
    #[inline]
    pub fn callback<F>(self, callback: F) -> SessionGetBuilder<'a, 'b, Callback<Reply>>
    where
        F: Fn(Reply) + Send + Sync + 'static,
    {
        self.with(Callback::from(callback))
    }

    /// Receive the replies for this query with a mutable callback.
    ///
    /// Using this guarantees that your callback will never be called concurrently.
    /// If your callback is also accepted by the [`callback`](crate::session::SessionGetBuilder::callback) method, we suggest you use it instead of `callback_mut`.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let mut n = 0;
    /// let queryable = session
    ///     .get("key/expression")
    ///     .callback_mut(move |reply| {n += 1;})
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
    #[inline]
    pub fn callback_mut<F>(self, callback: F) -> SessionGetBuilder<'a, 'b, Callback<Reply>>
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
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let replies = session
    ///     .get("key/expression")
    ///     .with(flume::bounded(32))
    ///     .await
    ///     .unwrap();
    /// while let Ok(reply) = replies.recv_async().await {
    ///     println!("Received {:?}", reply.result());
    /// }
    /// # }
    /// ```
    #[inline]
    pub fn with<Handler>(self, handler: Handler) -> SessionGetBuilder<'a, 'b, Handler>
    where
        Handler: IntoHandler<Reply>,
    {
        let SessionGetBuilder {
            session,
            selector,
            target,
            consolidation,
            qos,
            destination,
            timeout,
            value,
            attachment,
            #[cfg(feature = "unstable")]
            source_info,
            handler: _,
            #[cfg(feature = "unstable")]
            cancellation_token,
        } = self;
        SessionGetBuilder {
            session,
            selector,
            target,
            consolidation,
            qos,
            destination,
            timeout,
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
impl<Handler> SessionGetBuilder<'_, '_, Handler> {
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

    /// Change the target(s) of the query.
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

    /// Change the consolidation mode of the query.
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

    /// Restrict the matching queryables that will receive the query
    /// to the ones that have the given [`Locality`](Locality).
    #[zenoh_macros::unstable]
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

    /// See details in [`ReplyKeyExpr`](crate::query::ReplyKeyExpr) documentation.
    /// Queries may or may not accept replies on key expressions that do not intersect with their own key expression.
    /// This setter allows you to define whether this get operation accepts such disjoint replies.
    #[zenoh_macros::unstable]
    pub fn accept_replies(self, accept: ReplyKeyExpr) -> Self {
        if accept == ReplyKeyExpr::Any {
            if let Ok(Selector {
                key_expr,
                mut parameters,
            }) = self.selector
            {
                parameters.to_mut().set_reply_key_expr_any();
                let selector = Ok(Selector {
                    key_expr,
                    parameters,
                });
                return Self { selector, ..self };
            }
        }
        self
    }
}

impl<Handler> Resolvable for SessionGetBuilder<'_, '_, Handler>
where
    Handler: IntoHandler<Reply> + Send,
    Handler::Handler: Send,
{
    type To = ZResult<Handler::Handler>;
}

impl<Handler> Wait for SessionGetBuilder<'_, '_, Handler>
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
        let Selector {
            key_expr,
            parameters,
        } = self.selector?;
        #[allow(unused_variables)] // qid is only needed for unstable cancellation_token
        self.session
            .0
            .query(
                &key_expr,
                &parameters,
                self.target,
                self.consolidation,
                self.qos.into(),
                self.destination,
                self.timeout,
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
                    let session_clone = self.session.clone();
                    let on_cancel = move || {
                        let _ = session_clone.0.cancel_query(qid); // fails only if no associated query exists - likely because it was already finalized
                        Ok(())
                    };
                    cancellation_token.add_on_cancel_handler(cancel_receiver, Box::new(on_cancel));
                }
                receiver
            })
    }
}

impl<Handler> IntoFuture for SessionGetBuilder<'_, '_, Handler>
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
