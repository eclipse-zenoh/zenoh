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

use std::{collections::HashMap, error::Error, fmt::Display};

#[cfg(feature = "unstable")]
use serde::Deserialize;
#[cfg(feature = "unstable")]
use zenoh_config::wrappers::EntityGlobalId;
use zenoh_keyexpr::OwnedKeyExpr;
#[cfg(feature = "unstable")]
use zenoh_protocol::core::EntityGlobalIdProto;
use zenoh_protocol::core::Parameters;
/// The [`Queryable`](crate::query::Queryable)s to which a query from
/// a [`Session::get`](crate::Session::get) or a [`Querier::get`](crate::query::Querier::get)
/// is delivered.
///
/// * [`QueryTarget::All`] makes the query be delivered to all the matching queryables.
/// * [`QueryTarget::AllComplete`] makes the query be delivered to all the matching queryables
///   which are marked as "complete" with
///   [`QueryableBuilder::complete`](crate::query::QueryableBuilder::complete).
/// * [`QueryTarget::BestMatching`] (default) makes the data to be requested from the
///   queryable(s) selected by zenoh to get the fastest and most complete reply.
///
/// It is set by [`SessionGetBuilder::target`](crate::session::SessionGetBuilder::target)
/// or [`QuerierBuilder::target`](crate::query::QuerierBuilder::target) methods.
///
pub use zenoh_protocol::network::request::ext::QueryTarget;
#[doc(inline)]
pub use zenoh_protocol::zenoh::query::ConsolidationMode;

use crate::api::{
    bytes::ZBytes,
    encoding::Encoding,
    handlers::{Callback, CallbackParameter},
    key_expr::KeyExpr,
    sample::Sample,
    selector::Selector,
};

/// The reply consolidation strategy to apply to replies to a [`get`](crate::Session::get).
///
/// By default, the consolidation strategy is [`QueryConsolidation::AUTO`], which lets the implementation
/// choose the best strategy depending on the query parameters and the number of responders.
/// Other strategies can be selected by using
/// a specific [`ConsolidationMode`] as a parameter of the
/// [`QuerierBuilder::consolidation`](crate::query::QuerierBuilder::consolidation)
/// or [`SessionGetBuilder::consolidation`](crate::session::SessionGetBuilder::consolidation)
/// methods.
///
/// See the documentation of [`ConsolidationMode`] for more details about each strategy.
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

/// An error reply variant returned by [`Querier::get`](crate::query::Querier::get)
/// or [`Session::get`](crate::Session::get) in [`Reply`]
///
/// The `ReplyError` contains the payload with the error information (message or some structured data)
/// and the encoding of this payload.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ReplyError {
    pub(crate) payload: ZBytes,
    pub(crate) encoding: Encoding,
}

impl ReplyError {
    pub(crate) fn new(payload: impl Into<ZBytes>, encoding: Encoding) -> Self {
        Self {
            payload: payload.into(),
            encoding,
        }
    }

    /// Gets the payload of this ReplyError.
    #[inline]
    pub fn payload(&self) -> &ZBytes {
        &self.payload
    }

    /// Gets the mutable payload of this ReplyError.
    #[inline]
    pub fn payload_mut(&mut self) -> &mut ZBytes {
        &mut self.payload
    }

    /// Gets the encoding of this ReplyError.
    #[inline]
    pub fn encoding(&self) -> &Encoding {
        &self.encoding
    }

    /// Constructs an uninitialized empty ReplyError.
    #[zenoh_macros::internal]
    pub fn empty() -> Self {
        ReplyError {
            payload: ZBytes::new(),
            encoding: Encoding::default(),
        }
    }
}

impl Display for ReplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "query returned an error with a {}-byte payload and encoding {}",
            self.payload.len(),
            self.encoding
        )
    }
}

impl Error for ReplyError {}

/// An answer received from a [`Queryable`](crate::query::Queryable).
///
/// The `Reply` contains the result of the request to a [`Queryable`](crate::query::Queryable) by
/// [`Session::get`](crate::Session::get) or [`Querier::get`](crate::query::Querier::get).
///
/// It may be either a successful result with a [`Sample`](crate::sample::Sample) or an error with a [`ReplyError`].
/// The method [`Reply::result`] is provided to access the result.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct Reply {
    pub(crate) result: Result<Sample, ReplyError>,
    #[cfg(feature = "unstable")]
    pub(crate) replier_id: Option<EntityGlobalIdProto>,
}

impl Reply {
    /// Gets a borrowed result of this `Reply`. Use [`Reply::into_result`] to take ownership of the result.
    pub fn result(&self) -> Result<&Sample, &ReplyError> {
        self.result.as_ref()
    }

    /// Gets a mutable borrowed result of this `Reply`. Use [`Reply::into_result`] to take ownership of the result.
    pub fn result_mut(&mut self) -> Result<&mut Sample, &mut ReplyError> {
        self.result.as_mut()
    }

    /// Converts this `Reply` into its result. Use [`Reply::result`] if you don't want to take ownership.
    pub fn into_result(self) -> Result<Sample, ReplyError> {
        self.result
    }

    #[zenoh_macros::unstable]
    /// Gets the ID of the zenoh instance that answered this reply.
    pub fn replier_id(&self) -> Option<EntityGlobalId> {
        self.replier_id.map(Into::into)
    }

    /// Constructs an uninitialized empty Reply.
    #[zenoh_macros::internal]
    pub fn empty() -> Self {
        Reply {
            result: Ok(Sample::empty()),
            #[cfg(feature = "unstable")]
            replier_id: None,
        }
    }
}

impl CallbackParameter for Reply {
    type Message<'a> = Self;

    fn from_message(msg: Self::Message<'_>) -> Self {
        msg
    }
}

impl From<Reply> for Result<Sample, ReplyError> {
    fn from(value: Reply) -> Self {
        value.into_result()
    }
}

pub(crate) struct LivelinessQueryState {
    pub(crate) callback: Callback<Reply>,
}

pub(crate) struct QueryState {
    pub(crate) nb_final: usize,
    pub(crate) key_expr: KeyExpr<'static>,
    pub(crate) parameters: Parameters<'static>,
    pub(crate) reception_mode: ConsolidationMode,
    pub(crate) replies: Option<HashMap<OwnedKeyExpr, Reply>>,
    pub(crate) callback: Callback<Reply>,
}

impl QueryState {
    pub(crate) fn selector(&self) -> Selector<'_> {
        Selector::borrowed(&self.key_expr, &self.parameters)
    }
}
/// The kinds of accepted query replies.
///
/// The [`Queryable`](crate::query::Queryable) may serve glob-like key expressions.
/// E.g., the queryable may be declared with the key expression `foo/*`.
/// At the same time, it may send replies with more specific key expressions, e.g., `foo/bar` or `foo/baz`.
/// This may cause a situation when the queryable receives a query with the key expression `foo/bar`
/// and replies to it with the key expression `foo/baz`.
///
/// By default, this behavior is not allowed. Calling [`Query::reply`](crate::query::Query::reply) on
/// a query for `foo/bar` with key expression `foo/baz` will result in an error on the sending side.
///
/// But if the query is sent with the [`ReplyKeyExpr::Any`] parameter in [`accept_replies`](crate::session::SessionGetBuilder::accept_replies) (for
/// [`Session::get`](crate::session::Session::get) or
/// [`accept_replies`](crate::query::QuerierBuilder::accept_replies) for
/// [`Querier::get`](crate::query::Querier::get))
/// then the reply with a disjoint key expression will be accepted for this query.
///
/// The [`Queryable`](crate::query::Queryable) may check this parameter with
/// [`Query::accepts_replies`](crate::query::Query::accepts_replies).
#[zenoh_macros::unstable]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Deserialize)]
pub enum ReplyKeyExpr {
    /// Accept replies whose key expressions may not match the query key expression.
    Any,
    #[default]
    /// Accept replies whose key expressions match the query key expression.
    MatchingQuery,
}
