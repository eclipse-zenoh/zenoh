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

//! Sample primitives
use crate::buffers::ZBuf;
#[zenoh_macros::unstable]
use crate::prelude::ZenohId;
use crate::prelude::{KeyExpr, SampleKind, Value};
use crate::query::Reply;
use crate::time::{new_reception_timestamp, Timestamp};
#[zenoh_macros::unstable]
use serde::Serialize;
use std::convert::{TryFrom, TryInto};
#[zenoh_macros::unstable]
use zenoh_protocol::core::ZInt;
use zenoh_protocol::zenoh::DataInfo;

/// The locality of samples to be received by subscribers or targeted by publishers.
#[zenoh_macros::unstable]
#[derive(Clone, Copy, Debug, Default, Serialize, PartialEq, Eq)]
pub enum Locality {
    SessionLocal,
    Remote,
    #[default]
    Any,
}
#[cfg(not(feature = "unstable"))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Locality {
    SessionLocal,
    Remote,
    #[default]
    Any,
}

/// Informations on the source of a zenoh [`Sample`].
#[zenoh_macros::unstable]
#[derive(Debug, Clone)]
pub struct SourceInfo {
    /// The [`ZenohId`] of the zenoh instance that published the concerned [`Sample`].
    pub source_id: Option<ZenohId>,
    /// The sequence number of the [`Sample`] from the source.
    pub source_sn: Option<ZInt>,
}

#[test]
#[cfg(feature = "unstable")]
fn source_info_stack_size() {
    assert_eq!(std::mem::size_of::<SourceInfo>(), 16 * 2);
}

#[zenoh_macros::unstable]
impl SourceInfo {
    pub(crate) fn empty() -> Self {
        SourceInfo {
            source_id: None,
            source_sn: None,
        }
    }
}

#[zenoh_macros::unstable]
impl From<DataInfo> for SourceInfo {
    fn from(data_info: DataInfo) -> Self {
        SourceInfo {
            source_id: data_info.source_id,
            source_sn: data_info.source_sn,
        }
    }
}

#[zenoh_macros::unstable]
impl From<Option<DataInfo>> for SourceInfo {
    fn from(data_info: Option<DataInfo>) -> Self {
        match data_info {
            Some(data_info) => data_info.into(),
            None => SourceInfo::empty(),
        }
    }
}

/// A zenoh sample.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct Sample {
    /// The key expression on which this Sample was published.
    pub key_expr: KeyExpr<'static>,
    /// The value of this Sample.
    pub value: Value,
    /// The kind of this Sample.
    pub kind: SampleKind,
    /// The [`Timestamp`] of this Sample.
    pub timestamp: Option<Timestamp>,

    #[cfg(feature = "unstable")]
    /// <div class="stab unstable">
    ///   <span class="emoji">🔬</span>
    ///   This API has been marked as unstable: it works as advertised, but we may change it in a future release.
    ///   To use it, you must enable zenoh's <code>unstable</code> feature flag.
    /// </div>
    ///
    /// Infos on the source of this Sample.
    pub source_info: SourceInfo,
}

impl Sample {
    /// Creates a new Sample.
    #[inline]
    pub fn new<IntoKeyExpr, IntoValue>(key_expr: IntoKeyExpr, value: IntoValue) -> Self
    where
        IntoKeyExpr: Into<KeyExpr<'static>>,
        IntoValue: Into<Value>,
    {
        Sample {
            key_expr: key_expr.into(),
            value: value.into(),
            kind: SampleKind::default(),
            timestamp: None,
            #[cfg(feature = "unstable")]
            source_info: SourceInfo::empty(),
        }
    }
    /// Creates a new Sample.
    #[inline]
    pub fn try_from<TryIntoKeyExpr, IntoValue>(
        key_expr: TryIntoKeyExpr,
        value: IntoValue,
    ) -> Result<Self, zenoh_result::Error>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'static>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'static>>>::Error: Into<zenoh_result::Error>,
        IntoValue: Into<Value>,
    {
        Ok(Sample {
            key_expr: key_expr.try_into().map_err(Into::into)?,
            value: value.into(),
            kind: SampleKind::default(),
            timestamp: None,
            #[cfg(feature = "unstable")]
            source_info: SourceInfo::empty(),
        })
    }

    /// Creates a new Sample with optional data info.
    #[inline]
    pub(crate) fn with_info(
        key_expr: KeyExpr<'static>,
        payload: ZBuf,
        data_info: Option<DataInfo>,
    ) -> Self {
        let mut value: Value = payload.into();
        if let Some(data_info) = data_info {
            if let Some(encoding) = &data_info.encoding {
                value.encoding = encoding.clone();
            }
            Sample {
                key_expr,
                value,
                kind: data_info.kind,
                timestamp: data_info.timestamp,
                #[cfg(feature = "unstable")]
                source_info: data_info.into(),
            }
        } else {
            Sample {
                key_expr,
                value,
                kind: SampleKind::default(),
                timestamp: None,
                #[cfg(feature = "unstable")]
                source_info: SourceInfo::empty(),
            }
        }
    }

    #[inline]
    pub(crate) fn split(self) -> (KeyExpr<'static>, ZBuf, DataInfo) {
        let info = DataInfo {
            kind: self.kind,
            encoding: Some(self.value.encoding),
            timestamp: self.timestamp,
            #[cfg(feature = "shared-memory")]
            sliced: false,
            #[cfg(feature = "unstable")]
            source_id: self.source_info.source_id,
            #[cfg(not(feature = "unstable"))]
            source_id: None,
            #[cfg(feature = "unstable")]
            source_sn: self.source_info.source_sn,
            #[cfg(not(feature = "unstable"))]
            source_sn: None,
        };
        (self.key_expr, self.value.payload, info)
    }

    /// Gets the timestamp of this Sample.
    #[inline]
    pub fn get_timestamp(&self) -> Option<&Timestamp> {
        self.timestamp.as_ref()
    }

    /// Sets the timestamp of this Sample.
    #[inline]
    pub fn with_timestamp(mut self, timestamp: Timestamp) -> Self {
        self.timestamp = Some(timestamp);
        self
    }

    /// Sets the source info of this Sample.
    #[zenoh_macros::unstable]
    #[inline]
    pub fn with_source_info(mut self, source_info: SourceInfo) -> Self {
        self.source_info = source_info;
        self
    }

    #[inline]
    /// Ensure that an associated Timestamp is present in this Sample.
    /// If not, a new one is created with the current system time and 0x00 as id.
    /// Get the timestamp of this sample (either existing one or newly created)
    pub fn ensure_timestamp(&mut self) -> &Timestamp {
        if let Some(ref timestamp) = self.timestamp {
            timestamp
        } else {
            let timestamp = new_reception_timestamp();
            self.timestamp = Some(timestamp);
            self.timestamp.as_ref().unwrap()
        }
    }
}

impl std::ops::Deref for Sample {
    type Target = Value;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl std::ops::DerefMut for Sample {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

impl std::fmt::Display for Sample {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.kind {
            SampleKind::Delete => write!(f, "{}({})", self.kind, self.key_expr),
            _ => write!(f, "{}({}: {})", self.kind, self.key_expr, self.value),
        }
    }
}

impl TryFrom<Reply> for Sample {
    type Error = Value;

    fn try_from(value: Reply) -> Result<Self, Self::Error> {
        value.sample
    }
}
