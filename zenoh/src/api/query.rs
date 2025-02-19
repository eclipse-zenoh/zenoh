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
use zenoh_config::ZenohId;
use zenoh_keyexpr::OwnedKeyExpr;
use zenoh_protocol::core::Parameters;
#[cfg(feature = "unstable")]
use zenoh_protocol::core::ZenohIdProto;
/// The [`Queryable`](crate::query::Queryable)s that should be target of a [`get`](crate::Session::get).
pub use zenoh_protocol::network::request::ext::QueryTarget;
#[doc(inline)]
pub use zenoh_protocol::zenoh::query::ConsolidationMode;

use crate::api::{
    bytes::ZBytes, encoding::Encoding, handlers::Callback, key_expr::KeyExpr, sample::Sample,
    selector::Selector,
};

/// The replies consolidation strategy to apply on replies to a [`get`](crate::Session::get).
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

/// Error returned by a [`get`](crate::Session::get).
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
            "query returned an error with a {} bytes payload and encoding {}",
            self.payload.len(),
            self.encoding
        )
    }
}

impl Error for ReplyError {}

/// Struct returned by a [`get`](crate::Session::get).
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
    // @TODO: maybe return an `Option<EntityGlobalId>`?
    //
    /// Gets the id of the zenoh instance that answered this Reply.
    pub fn replier_id(&self) -> Option<ZenohId> {
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
    pub(crate) fn selector(&self) -> Selector {
        Selector::borrowed(&self.key_expr, &self.parameters)
    }
}
/// The kind of accepted query replies.
#[zenoh_macros::unstable]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Deserialize)]
pub enum ReplyKeyExpr {
    /// Accept replies whose key expressions may not match the query key expression.
    Any,
    #[default]
    /// Accept replies whose key expressions match the query key expression.
    MatchingQuery,
}
