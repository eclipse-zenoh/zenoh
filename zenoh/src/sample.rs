//
// Copyright (c) 2022 ZettaScale Technology
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
use std::convert::TryInto;

use zenoh_protocol::proto::DataInfo;

use crate::buffers::ZBuf;
use crate::prelude::{KeyExpr, SampleKind, Value};
use crate::time::{new_reception_timestamp, Timestamp};

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
        }
    }
    /// Creates a new Sample.
    #[inline]
    pub fn try_from<TryIntoKeyExpr, IntoValue>(
        key_expr: TryIntoKeyExpr,
        value: IntoValue,
    ) -> Result<Self, zenoh_core::Error>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'static>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'static>>>::Error: Into<zenoh_core::Error>,
        IntoValue: Into<Value>,
    {
        Ok(Sample {
            key_expr: key_expr.try_into().map_err(Into::into)?,
            value: value.into(),
            kind: SampleKind::default(),
            timestamp: None,
        })
    }

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
            }
        } else {
            Sample {
                key_expr,
                value,
                kind: SampleKind::default(),
                timestamp: None,
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
