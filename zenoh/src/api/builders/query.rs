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
use zenoh_protocol::{core::CongestionControl, network::request::ext::QueryTarget};
use zenoh_result::ZResult;

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

/// A builder for initializing a `query`.
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
impl QoSBuilderTrait for SessionGetBuilder<'_, '_, DefaultHandler> {
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
        self.with(Callback::new(Arc::new(callback)))
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

    /// Change the target of the query.
    #[inline]
    pub fn target(self, target: QueryTarget) -> Self {
        Self { target, ..self }
    }

    /// Change the consolidation mode of the query.
    #[inline]
    pub fn consolidation<QC: Into<QueryConsolidation>>(self, consolidation: QC) -> Self {
        Self {
            consolidation: consolidation.into(),
            ..self
        }
    }

    ///
    ///
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

    /// Set query timeout.
    #[inline]
    pub fn timeout(self, timeout: Duration) -> Self {
        Self { timeout, ..self }
    }

    ///
    ///
    /// By default, `get` guarantees that it will only receive replies whose key expressions intersect
    /// with the queried key expression.
    ///
    /// If allowed to through `accept_replies(ReplyKeyExpr::Any)`, queryables may also reply on key
    /// expressions that don't intersect with the query's.
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
        let (callback, receiver) = self.handler.into_handler();
        let Selector {
            key_expr,
            parameters,
        } = self.selector?;
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
            .map(|_| receiver)
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
