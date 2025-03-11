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
use std::{convert::TryFrom, fmt};

use serde::{Deserialize, Serialize};
use zenoh_config::{qos::PublisherLocalityConf, wrappers::EntityGlobalId};
#[cfg(feature = "unstable")]
use zenoh_protocol::core::Reliability;
use zenoh_protocol::{
    core::{CongestionControl, Timestamp},
    network::declare::ext::QoSType,
};

use crate::api::{
    builders::sample::QoSBuilderTrait, bytes::ZBytes, encoding::Encoding, key_expr::KeyExpr,
    publisher::Priority,
};

/// The sequence number of the [`Sample`] from the source.
pub type SourceSn = u32;

/// The locality of samples to be received by subscribers or targeted by publishers.
#[zenoh_macros::unstable]
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
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

impl From<PublisherLocalityConf> for Locality {
    fn from(value: PublisherLocalityConf) -> Self {
        match value {
            PublisherLocalityConf::SessionLocal => Self::SessionLocal,
            PublisherLocalityConf::Remote => Self::Remote,
            PublisherLocalityConf::Any => Self::Any,
        }
    }
}

impl From<Locality> for PublisherLocalityConf {
    fn from(value: Locality) -> Self {
        match value {
            Locality::SessionLocal => Self::SessionLocal,
            Locality::Remote => Self::Remote,
            Locality::Any => Self::Any,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct DataInfo {
    pub kind: SampleKind,
    pub encoding: Option<Encoding>,
    pub timestamp: Option<Timestamp>,
    pub source_id: Option<EntityGlobalId>,
    pub source_sn: Option<SourceSn>,
    pub qos: QoS,
}

pub(crate) trait DataInfoIntoSample {
    fn into_sample<IntoKeyExpr, IntoZBytes>(
        self,
        key_expr: IntoKeyExpr,
        payload: IntoZBytes,
        #[cfg(feature = "unstable")] reliability: Reliability,
        attachment: Option<ZBytes>,
    ) -> Sample
    where
        IntoKeyExpr: Into<KeyExpr<'static>>,
        IntoZBytes: Into<ZBytes>;
}

impl DataInfoIntoSample for DataInfo {
    // This function is for internal use only.
    // Technically it may create invalid sample (e.g. a delete sample with a payload and encoding)
    // The test for it is intentionally not added to avoid inserting extra "if" into hot path.
    // The correctness of the data should be ensured by the caller.
    #[inline]
    fn into_sample<IntoKeyExpr, IntoZBytes>(
        self,
        key_expr: IntoKeyExpr,
        payload: IntoZBytes,
        #[cfg(feature = "unstable")] reliability: Reliability,
        attachment: Option<ZBytes>,
    ) -> Sample
    where
        IntoKeyExpr: Into<KeyExpr<'static>>,
        IntoZBytes: Into<ZBytes>,
    {
        Sample {
            key_expr: key_expr.into(),
            payload: payload.into(),
            kind: self.kind,
            encoding: self.encoding.unwrap_or_default(),
            timestamp: self.timestamp,
            qos: self.qos,
            #[cfg(feature = "unstable")]
            reliability,
            #[cfg(feature = "unstable")]
            source_info: SourceInfo {
                source_id: self.source_id,
                source_sn: self.source_sn,
            },
            attachment,
        }
    }
}

impl DataInfoIntoSample for Option<DataInfo> {
    #[inline]
    fn into_sample<IntoKeyExpr, IntoZBytes>(
        self,
        key_expr: IntoKeyExpr,
        payload: IntoZBytes,
        #[cfg(feature = "unstable")] reliability: Reliability,
        attachment: Option<ZBytes>,
    ) -> Sample
    where
        IntoKeyExpr: Into<KeyExpr<'static>>,
        IntoZBytes: Into<ZBytes>,
    {
        if let Some(data_info) = self {
            data_info.into_sample(
                key_expr,
                payload,
                #[cfg(feature = "unstable")]
                reliability,
                attachment,
            )
        } else {
            Sample {
                key_expr: key_expr.into(),
                payload: payload.into(),
                kind: SampleKind::Put,
                encoding: Encoding::default(),
                timestamp: None,
                qos: QoS::default(),
                #[cfg(feature = "unstable")]
                reliability,
                #[cfg(feature = "unstable")]
                source_info: SourceInfo::empty(),
                attachment,
            }
        }
    }
}

/// Information on the source of a zenoh [`Sample`].
#[zenoh_macros::unstable]
#[derive(Debug, Clone)]
pub struct SourceInfo {
    pub(crate) source_id: Option<EntityGlobalId>,
    pub(crate) source_sn: Option<SourceSn>,
}

#[zenoh_macros::unstable]
impl SourceInfo {
    #[zenoh_macros::unstable]
    /// Build a new [`SourceInfo`].
    pub fn new(source_id: Option<EntityGlobalId>, source_sn: Option<SourceSn>) -> Self {
        Self {
            source_id,
            source_sn,
        }
    }

    #[zenoh_macros::unstable]
    /// The [`EntityGlobalId`] of the zenoh entity that published the concerned [`Sample`].
    pub fn source_id(&self) -> Option<&EntityGlobalId> {
        self.source_id.as_ref()
    }

    #[zenoh_macros::unstable]
    /// The sequence number of the [`Sample`] from the source.
    pub fn source_sn(&self) -> Option<SourceSn> {
        self.source_sn
    }
}

#[test]
#[cfg(feature = "unstable")]
fn source_info_stack_size() {
    use zenoh_protocol::core::ZenohIdProto;

    use crate::api::sample::{SourceInfo, SourceSn};

    assert_eq!(std::mem::size_of::<ZenohIdProto>(), 16);
    assert_eq!(std::mem::size_of::<Option<ZenohIdProto>>(), 17);
    assert_eq!(std::mem::size_of::<Option<SourceSn>>(), 8);
    assert_eq!(std::mem::size_of::<Option<EntityGlobalId>>(), 24);
    assert_eq!(std::mem::size_of::<SourceInfo>(), 24 + 8);
}

#[zenoh_macros::unstable]
impl SourceInfo {
    pub(crate) fn empty() -> Self {
        SourceInfo {
            source_id: None,
            source_sn: None,
        }
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.source_id.is_none() && self.source_sn.is_none()
    }
}

#[zenoh_macros::unstable]
impl From<SourceInfo> for Option<zenoh_protocol::zenoh::put::ext::SourceInfoType> {
    fn from(source_info: SourceInfo) -> Option<zenoh_protocol::zenoh::put::ext::SourceInfoType> {
        if source_info.is_empty() {
            None
        } else {
            Some(zenoh_protocol::zenoh::put::ext::SourceInfoType {
                id: source_info.source_id.unwrap_or_default().into(),
                sn: source_info.source_sn.unwrap_or_default(),
            })
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

#[zenoh_macros::unstable]
impl Default for SourceInfo {
    fn default() -> Self {
        Self::empty()
    }
}

/// The kind of a `Sample`.
#[repr(u8)]
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub enum SampleKind {
    /// if the `Sample` was issued by a `put` operation.
    #[default]
    Put = 0,
    /// if the `Sample` was issued by a `delete` operation.
    Delete = 1,
}

impl fmt::Display for SampleKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SampleKind::Put => write!(f, "PUT"),
            SampleKind::Delete => write!(f, "DELETE"),
        }
    }
}

impl TryFrom<u64> for SampleKind {
    type Error = u64;
    fn try_from(kind: u64) -> Result<Self, u64> {
        match kind {
            0 => Ok(SampleKind::Put),
            1 => Ok(SampleKind::Delete),
            _ => Err(kind),
        }
    }
}

/// Structure with public fields for sample. It's convenient if it's necessary to decompose a sample into its fields.
pub struct SampleFields {
    pub key_expr: KeyExpr<'static>,
    pub payload: ZBytes,
    pub kind: SampleKind,
    pub encoding: Encoding,
    pub timestamp: Option<Timestamp>,
    pub express: bool,
    pub priority: Priority,
    pub congestion_control: CongestionControl,
    #[cfg(feature = "unstable")]
    pub reliability: Reliability,
    #[cfg(feature = "unstable")]
    pub source_info: SourceInfo,
    pub attachment: Option<ZBytes>,
}

impl From<Sample> for SampleFields {
    fn from(sample: Sample) -> Self {
        SampleFields {
            key_expr: sample.key_expr,
            payload: sample.payload,
            kind: sample.kind,
            encoding: sample.encoding,
            timestamp: sample.timestamp,
            express: sample.qos.express(),
            priority: sample.qos.priority(),
            congestion_control: sample.qos.congestion_control(),
            #[cfg(feature = "unstable")]
            reliability: sample.reliability,
            #[cfg(feature = "unstable")]
            source_info: sample.source_info,
            attachment: sample.attachment,
        }
    }
}

/// A zenoh sample.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct Sample {
    pub(crate) key_expr: KeyExpr<'static>,
    pub(crate) payload: ZBytes,
    pub(crate) kind: SampleKind,
    pub(crate) encoding: Encoding,
    pub(crate) timestamp: Option<Timestamp>,
    pub(crate) qos: QoS,
    #[cfg(feature = "unstable")]
    pub(crate) reliability: Reliability,
    #[cfg(feature = "unstable")]
    pub(crate) source_info: SourceInfo,
    pub(crate) attachment: Option<ZBytes>,
}

impl Sample {
    /// Gets the key expression on which this Sample was published.
    #[inline]
    pub fn key_expr(&self) -> &KeyExpr<'static> {
        &self.key_expr
    }

    /// Gets the payload of this Sample.
    #[inline]
    pub fn payload(&self) -> &ZBytes {
        &self.payload
    }

    /// Gets the payload of this Sample.
    #[inline]
    pub fn payload_mut(&mut self) -> &mut ZBytes {
        &mut self.payload
    }

    /// Gets the kind of this Sample.
    #[inline]
    pub fn kind(&self) -> SampleKind {
        self.kind
    }

    /// Gets the encoding of this sample
    #[inline]
    pub fn encoding(&self) -> &Encoding {
        &self.encoding
    }

    /// Gets the timestamp of this Sample
    #[inline]
    pub fn timestamp(&self) -> Option<&Timestamp> {
        self.timestamp.as_ref()
    }

    /// Gets the congetion control of this Sample
    pub fn congestion_control(&self) -> CongestionControl {
        self.qos.congestion_control()
    }

    /// Gets the priority of this Sample
    pub fn priority(&self) -> Priority {
        self.qos.priority()
    }

    /// Gets the reliability of this Sample
    #[zenoh_macros::unstable]
    pub fn reliability(&self) -> Reliability {
        self.reliability
    }

    /// Gets the express flag value. If `true`, the message is not batched during transmission, in order to reduce latency.
    pub fn express(&self) -> bool {
        self.qos.express()
    }

    /// Gets infos on the source of this Sample.
    #[zenoh_macros::unstable]
    #[inline]
    pub fn source_info(&self) -> &SourceInfo {
        &self.source_info
    }

    /// Gets the sample attachment: a map of key-value pairs, where each key and value are byte-slices.
    #[inline]
    pub fn attachment(&self) -> Option<&ZBytes> {
        self.attachment.as_ref()
    }

    /// Gets the sample attachment: a map of key-value pairs, where each key and value are byte-slices.
    #[inline]
    pub fn attachment_mut(&mut self) -> Option<&mut ZBytes> {
        self.attachment.as_mut()
    }

    /// Constructs an uninitialized empty Sample.
    #[zenoh_macros::internal]
    pub fn empty() -> Self {
        Sample {
            key_expr: KeyExpr::dummy(),
            payload: ZBytes::new(),
            kind: SampleKind::Put,
            encoding: Encoding::default(),
            timestamp: None,
            qos: QoS::default(),
            #[cfg(feature = "unstable")]
            reliability: Reliability::default(),
            #[cfg(feature = "unstable")]
            source_info: SourceInfo::empty(),
            attachment: None,
        }
    }
}

/// Structure containing quality of service data
#[derive(Debug, Default, Copy, Clone, Eq, PartialEq)]
pub(crate) struct QoS {
    inner: QoSType,
}

#[derive(Debug)]
pub(crate) struct QoSBuilder(QoS);

impl From<QoS> for QoSBuilder {
    fn from(qos: QoS) -> Self {
        QoSBuilder(qos)
    }
}

impl From<QoSType> for QoSBuilder {
    fn from(qos: QoSType) -> Self {
        QoSBuilder(QoS { inner: qos })
    }
}

impl From<QoSBuilder> for QoS {
    fn from(builder: QoSBuilder) -> Self {
        builder.0
    }
}

#[zenoh_macros::internal_trait]
impl QoSBuilderTrait for QoSBuilder {
    fn congestion_control(self, congestion_control: CongestionControl) -> Self {
        let mut inner = self.0.inner;
        inner.set_congestion_control(congestion_control);
        Self(QoS { inner })
    }

    fn priority(self, priority: Priority) -> Self {
        let mut inner = self.0.inner;
        inner.set_priority(priority.into());
        Self(QoS { inner })
    }

    fn express(self, is_express: bool) -> Self {
        let mut inner = self.0.inner;
        inner.set_is_express(is_express);
        Self(QoS { inner })
    }
}

impl QoS {
    /// Gets priority of the message.
    pub fn priority(&self) -> Priority {
        match Priority::try_from(self.inner.get_priority()) {
            Ok(p) => p,
            Err(e) => {
                tracing::trace!(
                    "Failed to convert priority: {}; replacing with default value",
                    e.to_string()
                );
                Priority::default()
            }
        }
    }

    /// Gets congestion control of the message.
    pub fn congestion_control(&self) -> CongestionControl {
        self.inner.get_congestion_control()
    }

    /// Gets express flag value. If `true`, the message is not batched during transmission, in order to reduce latency.
    pub fn express(&self) -> bool {
        self.inner.is_express()
    }
}

impl From<QoSType> for QoS {
    fn from(qos: QoSType) -> Self {
        QoS { inner: qos }
    }
}

impl From<QoS> for QoSType {
    fn from(qos: QoS) -> Self {
        qos.inner
    }
}
