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
use crate::sample::QoSBuilder;
use crate::Session;
#[cfg(feature = "unstable")]
use crate::{payload::OptionPayload, sample::Attachment};
use std::collections::HashMap;
use std::future::Ready;
use std::time::Duration;
use zenoh_core::{AsyncResolve, Resolvable, SyncResolve};
use zenoh_result::ZResult;

/// The [`Queryable`](crate::queryable::Queryable)s that should be target of a [`get`](Session::get).
pub type QueryTarget = zenoh_protocol::network::request::ext::TargetType;

/// The kind of consolidation.
pub type ConsolidationMode = zenoh_protocol::zenoh::query::Consolidation;

/// The operation: either manual or automatic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode<T> {
    Auto,
    Manual(T),
}

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

/// Structs returned by a [`get`](Session::get).
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct Reply {
    pub(crate) result: Result<Sample, Value>,
    pub(crate) replier_id: ZenohId,
}

impl Reply {
    /// Gets the a borrowed result of this `Reply`. Use [`Reply::into_result`] to take ownership of the result.
    pub fn result(&self) -> Result<&Sample, &Value> {
        self.result.as_ref()
    }

    /// Converts this `Reply` into the its result. Use [`Reply::result`] it you don't want to take ownership.
    pub fn into_result(self) -> Result<Sample, Value> {
        self.result
    }

    /// Gets the id of the zenoh instance that answered this Reply.
    pub fn replier_id(&self) -> ZenohId {
        self.replier_id
    }
}

impl From<Reply> for Result<Sample, Value> {
    fn from(value: Reply) -> Self {
        value.into_result()
    }
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
/// # #[tokio::main]
/// # async fn main() {
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
///     println!("Received {:?}", reply.sample())
/// }
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
#[derive(Debug)]
pub struct GetBuilder<'a, 'b, Handler> {
    pub(crate) session: &'a Session,
    pub(crate) selector: ZResult<Selector<'b>>,
    pub(crate) scope: ZResult<Option<KeyExpr<'b>>>,
    pub(crate) target: QueryTarget,
    pub(crate) consolidation: QueryConsolidation,
    pub(crate) qos: QoSBuilder,
    pub(crate) destination: Locality,
    pub(crate) timeout: Duration,
    pub(crate) handler: Handler,
    pub(crate) value: Option<Value>,
    #[cfg(feature = "unstable")]
    pub(crate) attachment: Option<Attachment>,
    #[cfg(feature = "unstable")]
    pub(crate) source_info: SourceInfo,
}

#[zenoh_macros::unstable]
impl<Handler> SampleBuilderTrait for GetBuilder<'_, '_, Handler> {
    #[cfg(feature = "unstable")]
    fn source_info(self, source_info: SourceInfo) -> Self {
        Self {
            source_info,
            ..self
        }
    }

    #[cfg(feature = "unstable")]
    fn attachment<T: Into<OptionPayload>>(self, attachment: T) -> Self {
        let attachment: OptionPayload = attachment.into();
        Self {
            attachment: attachment.into(),
            ..self
        }
    }
}

impl QoSBuilderTrait for GetBuilder<'_, '_, DefaultHandler> {
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

impl<Handler> ValueBuilderTrait for GetBuilder<'_, '_, Handler> {
    fn encoding<T: Into<Encoding>>(self, encoding: T) -> Self {
        let mut value = self.value.unwrap_or_default();
        value.encoding = encoding.into();
        Self {
            value: Some(value),
            ..self
        }
    }

    fn payload<T: Into<Payload>>(self, payload: T) -> Self {
        let mut value = self.value.unwrap_or_default();
        value.payload = payload.into();
        Self {
            value: Some(value),
            ..self
        }
    }
    fn value<T: Into<Value>>(self, value: T) -> Self {
        let value: Value = value.into();
        Self {
            value: if value.is_empty() { None } else { Some(value) },
            ..self
        }
    }
}

impl<'a, 'b> GetBuilder<'a, 'b, DefaultHandler> {
    /// Receive the replies for this query with a callback.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let queryable = session
    ///     .get("key/expression")
    ///     .callback(|reply| {println!("Received {:?}", reply.sample());})
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
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
            qos,
            destination,
            timeout,
            value,
            #[cfg(feature = "unstable")]
            attachment,
            #[cfg(feature = "unstable")]
            source_info,
            handler: _,
        } = self;
        GetBuilder {
            session,
            selector,
            scope,
            target,
            consolidation,
            qos,
            destination,
            timeout,
            value,
            #[cfg(feature = "unstable")]
            attachment,
            #[cfg(feature = "unstable")]
            source_info,
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
    /// # #[tokio::main]
    /// # async fn main() {
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
    /// # }
    /// ```
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

    /// Receive the replies for this query with a [`Handler`](crate::prelude::IntoHandler).
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
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
    ///     println!("Received {:?}", reply.sample());
    /// }
    /// # }
    /// ```
    #[inline]
    pub fn with<Handler>(self, handler: Handler) -> GetBuilder<'a, 'b, Handler>
    where
        Handler: IntoHandler<'static, Reply>,
    {
        let GetBuilder {
            session,
            selector,
            scope,
            target,
            consolidation,
            qos,
            destination,
            timeout,
            value,
            #[cfg(feature = "unstable")]
            attachment,
            #[cfg(feature = "unstable")]
            source_info,
            handler: _,
        } = self;
        GetBuilder {
            session,
            selector,
            scope,
            target,
            consolidation,
            qos,
            destination,
            timeout,
            value,
            #[cfg(feature = "unstable")]
            attachment,
            #[cfg(feature = "unstable")]
            source_info,
            handler,
        }
    }
}
impl<'a, 'b, Handler> GetBuilder<'a, 'b, Handler> {
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
        Self {
            selector: self
                .selector
                .and_then(|s| s.accept_any_keyexpr(accept == ReplyKeyExpr::Any)),
            ..self
        }
    }
}

pub(crate) const _REPLY_KEY_EXPR_ANY_SEL_PARAM: &str = "_anyke";
#[zenoh_macros::unstable]
pub const REPLY_KEY_EXPR_ANY_SEL_PARAM: &str = _REPLY_KEY_EXPR_ANY_SEL_PARAM;

#[zenoh_macros::unstable]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    Handler: IntoHandler<'static, Reply> + Send,
    Handler::Handler: Send,
{
    type To = ZResult<Handler::Handler>;
}

impl<Handler> SyncResolve for GetBuilder<'_, '_, Handler>
where
    Handler: IntoHandler<'static, Reply> + Send,
    Handler::Handler: Send,
{
    fn res_sync(self) -> <Self as Resolvable>::To {
        let (callback, receiver) = self.handler.into_handler();

        self.session
            .query(
                &self.selector?,
                &self.scope?,
                self.target,
                self.consolidation,
                self.qos.into(),
                self.destination,
                self.timeout,
                self.value,
                #[cfg(feature = "unstable")]
                self.attachment,
                #[cfg(feature = "unstable")]
                self.source_info,
                callback,
            )
            .map(|_| receiver)
    }
}

impl<Handler> AsyncResolve for GetBuilder<'_, '_, Handler>
where
    Handler: IntoHandler<'static, Reply> + Send,
    Handler::Handler: Send,
{
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}
