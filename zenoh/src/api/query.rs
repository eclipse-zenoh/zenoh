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
    collections::HashMap,
    future::{IntoFuture, Ready},
    time::Duration,
};

#[cfg(feature = "unstable")]
use zenoh_config::ZenohId;
use zenoh_core::{Resolvable, Wait};
use zenoh_keyexpr::OwnedKeyExpr;
#[cfg(feature = "unstable")]
use zenoh_protocol::core::ZenohIdProto;
use zenoh_protocol::core::{CongestionControl, Parameters};
use zenoh_result::ZResult;

#[cfg(feature = "unstable")]
use super::{
    builders::sample::SampleBuilderTrait, bytes::OptionZBytes, sample::SourceInfo,
    selector::ZenohParameters,
};
use super::{
    builders::sample::{EncodingBuilderTrait, QoSBuilderTrait},
    bytes::ZBytes,
    encoding::Encoding,
    handlers::{locked, Callback, DefaultHandler, IntoHandler},
    key_expr::KeyExpr,
    publisher::Priority,
    sample::{Locality, QoSBuilder, Sample},
    selector::Selector,
    session::Session,
    value::Value,
};

/// The [`Queryable`](crate::query::Queryable)s that should be target of a [`get`](Session::get).
pub type QueryTarget = zenoh_protocol::network::request::ext::TargetType;

/// The kind of consolidation.
pub type ConsolidationMode = zenoh_protocol::zenoh::query::Consolidation;

/// The replies consolidation strategy to apply on replies to a [`get`](Session::get).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueryConsolidation {
    pub(crate) mode: ConsolidationMode,
}

impl QueryConsolidation {
    pub const DEFAULT: Self = Self::AUTO;
    /// Automatic query consolidation strategy selection.
    pub const AUTO: Self = Self {
        mode: ConsolidationMode::Auto,
    };

    pub(crate) const fn from_mode(mode: ConsolidationMode) -> Self {
        Self { mode }
    }

    /// Returns the requested [`ConsolidationMode`].
    pub fn mode(&self) -> ConsolidationMode {
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
        Self::DEFAULT
    }
}

/// Error returned by a [`get`](Session::get).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ReplyError {
    pub(crate) payload: ZBytes,
    pub(crate) encoding: Encoding,
}

impl ReplyError {
    /// Gets the payload of this ReplyError.
    #[inline]
    pub fn payload(&self) -> &ZBytes {
        &self.payload
    }

    /// Gets the encoding of this ReplyError.
    #[inline]
    pub fn encoding(&self) -> &Encoding {
        &self.encoding
    }
}

impl From<Value> for ReplyError {
    fn from(value: Value) -> Self {
        Self {
            payload: value.payload,
            encoding: value.encoding,
        }
    }
}

/// Struct returned by a [`get`](Session::get).
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct Reply {
    pub(crate) result: Result<Sample, ReplyError>,
    #[cfg(feature = "unstable")]
    pub(crate) replier_id: Option<ZenohIdProto>,
}

impl Reply {
    /// Gets the a borrowed result of this `Reply`. Use [`Reply::into_result`] to take ownership of the result.
    pub fn result(&self) -> Result<&Sample, &ReplyError> {
        self.result.as_ref()
    }

    /// Gets the a mutable borrowed result of this `Reply`. Use [`Reply::into_result`] to take ownership of the result.
    pub fn result_mut(&mut self) -> Result<&mut Sample, &mut ReplyError> {
        self.result.as_mut()
    }

    /// Converts this `Reply` into the its result. Use [`Reply::result`] it you don't want to take ownership.
    pub fn into_result(self) -> Result<Sample, ReplyError> {
        self.result
    }

    #[zenoh_macros::unstable]
    /// Gets the id of the zenoh instance that answered this Reply.
    pub fn replier_id(&self) -> Option<ZenohId> {
        self.replier_id.map(Into::into)
    }
}

impl From<Reply> for Result<Sample, ReplyError> {
    fn from(value: Reply) -> Self {
        value.into_result()
    }
}

#[cfg(feature = "unstable")]
pub(crate) struct LivelinessQueryState {
    pub(crate) callback: Callback<'static, Reply>,
}

pub(crate) struct QueryState {
    pub(crate) nb_final: usize,
    pub(crate) key_expr: KeyExpr<'static>,
    pub(crate) parameters: Parameters<'static>,
    pub(crate) reception_mode: ConsolidationMode,
    pub(crate) replies: Option<HashMap<OwnedKeyExpr, Reply>>,
    pub(crate) callback: Callback<'static, Reply>,
}

impl QueryState {
    pub(crate) fn selector(&self) -> Selector {
        Selector::borrowed(&self.key_expr, &self.parameters)
    }
}

/// A builder for initializing a `query`.
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh::{prelude::*, query::{ConsolidationMode, QueryTarget}};
///
/// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
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
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
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
    pub(crate) value: Option<Value>,
    pub(crate) attachment: Option<ZBytes>,
    #[cfg(feature = "unstable")]
    pub(crate) source_info: SourceInfo,
}

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

impl<Handler> EncodingBuilderTrait for SessionGetBuilder<'_, '_, Handler> {
    fn encoding<T: Into<Encoding>>(self, encoding: T) -> Self {
        let mut value = self.value.unwrap_or_default();
        value.encoding = encoding.into();
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
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// let queryable = session
    ///     .get("key/expression")
    ///     .callback(|reply| {println!("Received {:?}", reply.result());})
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
    #[inline]
    pub fn callback<Callback>(self, callback: Callback) -> SessionGetBuilder<'a, 'b, Callback>
    where
        Callback: Fn(Reply) + Send + Sync + 'static,
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
            handler: callback,
        }
    }

    /// Receive the replies for this query with a mutable callback.
    ///
    /// Using this guarantees that your callback will never be called concurrently.
    /// If your callback is also accepted by the [`callback`](crate::session::SessionGetBuilder::callback) method, we suggest you use it instead of `callback_mut`
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
    ///     .get("key/expression")
    ///     .callback_mut(move |reply| {n += 1;})
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
    #[inline]
    pub fn callback_mut<CallbackMut>(
        self,
        callback: CallbackMut,
    ) -> SessionGetBuilder<'a, 'b, impl Fn(Reply) + Send + Sync + 'static>
    where
        CallbackMut: FnMut(Reply) + Send + Sync + 'static,
    {
        self.callback(locked(callback))
    }

    /// Receive the replies for this query with a [`Handler`](crate::handlers::IntoHandler).
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
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
        Handler: IntoHandler<'static, Reply>,
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
impl<'a, 'b, Handler> SessionGetBuilder<'a, 'b, Handler> {
    #[inline]
    pub fn payload<IntoZBytes>(mut self, payload: IntoZBytes) -> Self
    where
        IntoZBytes: Into<ZBytes>,
    {
        let mut value = self.value.unwrap_or_default();
        value.payload = payload.into();
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

    /// Restrict the matching queryables that will receive the query
    /// to the ones that have the given [`Locality`](crate::prelude::Locality).
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

#[zenoh_macros::unstable]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ReplyKeyExpr {
    Any,
    #[default]
    MatchingQuery,
}

impl<Handler> Resolvable for SessionGetBuilder<'_, '_, Handler>
where
    Handler: IntoHandler<'static, Reply> + Send,
    Handler::Handler: Send,
{
    type To = ZResult<Handler::Handler>;
}

impl<Handler> Wait for SessionGetBuilder<'_, '_, Handler>
where
    Handler: IntoHandler<'static, Reply> + Send,
    Handler::Handler: Send,
{
    fn wait(self) -> <Self as Resolvable>::To {
        let (callback, receiver) = self.handler.into_handler();
        let Selector {
            key_expr,
            parameters,
        } = self.selector?;
        self.session
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
    Handler: IntoHandler<'static, Reply> + Send,
    Handler::Handler: Send,
{
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}
