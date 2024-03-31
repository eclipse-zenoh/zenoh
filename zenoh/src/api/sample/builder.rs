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
#[zenoh_macros::unstable]
use crate::api::sample::Attachment;
#[zenoh_macros::unstable]
use crate::api::sample::SourceInfo;
use crate::api::{
    encoding::Encoding,
    key_expr::KeyExpr,
    payload::Payload,
    publication::Priority,
    sample::{QoS, QoSBuilder, Sample},
    value::Value,
};
use uhlc::Timestamp;
use zenoh_core::zresult;
use zenoh_protocol::core::CongestionControl;

use super::SampleKind;

pub trait QoSBuilderTrait {
    /// Change the `congestion_control` to apply when routing the data.
    fn congestion_control(self, congestion_control: CongestionControl) -> Self;
    /// Change the priority of the written data.
    fn priority(self, priority: Priority) -> Self;
    /// Change the `express` policy to apply when routing the data.
    /// When express is set to `true`, then the message will not be batched.
    /// This usually has a positive impact on latency but negative impact on throughput.
    fn express(self, is_express: bool) -> Self;
}

pub trait TimestampBuilderTrait {
    /// Sets of clears timestamp
    fn timestamp<T: Into<Option<Timestamp>>>(self, timestamp: T) -> Self;
}

pub trait SampleBuilderTrait {
    /// Attach source information
    #[zenoh_macros::unstable]
    fn source_info(self, source_info: SourceInfo) -> Self;
    /// Attach user-provided data in key-value format
    #[zenoh_macros::unstable]
    fn attachment<T: Into<Option<Attachment>>>(self, attachment: T) -> Self;
}

pub trait ValueBuilderTrait {
    /// Set the [`Encoding`]
    fn encoding<T: Into<Encoding>>(self, encoding: T) -> Self;
    /// Sets the payload
    fn payload<T: Into<Payload>>(self, payload: T) -> Self;
    /// Sets both payload and encoding at once.
    /// This is convenient for passing user type which supports `Into<Value>` when both payload and encoding depends on user type
    fn value<T: Into<Value>>(self, value: T) -> Self;
}

#[derive(Debug)]
pub struct SampleBuilder(Sample);

impl SampleBuilder {
    pub fn put<IntoKeyExpr, IntoPayload>(
        key_expr: IntoKeyExpr,
        payload: IntoPayload,
    ) -> PutSampleBuilder
    where
        IntoKeyExpr: Into<KeyExpr<'static>>,
        IntoPayload: Into<Payload>,
    {
        PutSampleBuilder::new(key_expr, payload)
    }
    pub fn delete<IntoKeyExpr>(key_expr: IntoKeyExpr) -> DeleteSampleBuilder
    where
        IntoKeyExpr: Into<KeyExpr<'static>>,
    {
        DeleteSampleBuilder::new(key_expr)
    }
    /// Allows to change keyexpr of [`Sample`]
    pub fn keyexpr<IntoKeyExpr>(self, key_expr: IntoKeyExpr) -> Self
    where
        IntoKeyExpr: Into<KeyExpr<'static>>,
    {
        Self(Sample {
            key_expr: key_expr.into(),
            ..self.0
        })
    }
}

impl TimestampBuilderTrait for SampleBuilder {
    fn timestamp<T: Into<Option<Timestamp>>>(self, timestamp: T) -> Self {
        Self(Sample {
            timestamp: timestamp.into(),
            ..self.0
        })
    }
}

impl SampleBuilderTrait for SampleBuilder {
    #[zenoh_macros::unstable]
    fn source_info(self, source_info: SourceInfo) -> Self {
        Self(Sample {
            source_info,
            ..self.0
        })
    }

    #[zenoh_macros::unstable]
    fn attachment<T: Into<Option<Attachment>>>(self, attachment: T) -> Self {
        Self(Sample {
            attachment: attachment.into(),
            ..self.0
        })
    }
}

impl QoSBuilderTrait for SampleBuilder {
    fn congestion_control(self, congestion_control: CongestionControl) -> Self {
        let qos: QoSBuilder = self.0.qos.into();
        let qos = qos.congestion_control(congestion_control).into();
        Self(Sample { qos, ..self.0 })
    }
    fn priority(self, priority: Priority) -> Self {
        let qos: QoSBuilder = self.0.qos.into();
        let qos = qos.priority(priority).into();
        Self(Sample { qos, ..self.0 })
    }
    fn express(self, is_express: bool) -> Self {
        let qos: QoSBuilder = self.0.qos.into();
        let qos = qos.express(is_express).into();
        Self(Sample { qos, ..self.0 })
    }
}

#[derive(Debug)]
pub struct PutSampleBuilder(SampleBuilder);

impl From<SampleBuilder> for PutSampleBuilder {
    fn from(sample_builder: SampleBuilder) -> Self {
        Self(SampleBuilder(Sample {
            kind: SampleKind::Put,
            ..sample_builder.0
        }))
    }
}

impl PutSampleBuilder {
    fn new<IntoKeyExpr, IntoPayload>(key_expr: IntoKeyExpr, payload: IntoPayload) -> Self
    where
        IntoKeyExpr: Into<KeyExpr<'static>>,
        IntoPayload: Into<Payload>,
    {
        Self(SampleBuilder::from(Sample {
            key_expr: key_expr.into(),
            payload: payload.into(),
            kind: SampleKind::Put,
            encoding: Encoding::default(),
            timestamp: None,
            qos: QoS::default(),
            #[cfg(feature = "unstable")]
            source_info: SourceInfo::empty(),
            #[cfg(feature = "unstable")]
            attachment: None,
        }))
    }
    /// Allows to change keyexpr of [`Sample`]
    pub fn keyexpr<IntoKeyExpr>(self, key_expr: IntoKeyExpr) -> Self
    where
        IntoKeyExpr: Into<KeyExpr<'static>>,
    {
        Self(self.0.keyexpr(key_expr))
    }
    // It's convenient to set QoS as a whole for internal usage. For user API there are `congestion_control`, `priority` and `express` methods.
    pub(crate) fn qos(self, qos: QoS) -> Self {
        Self(SampleBuilder(Sample { qos, ..self.0 .0 }))
    }
}

impl TimestampBuilderTrait for PutSampleBuilder {
    fn timestamp<T: Into<Option<Timestamp>>>(self, timestamp: T) -> Self {
        Self(self.0.timestamp(timestamp))
    }
}

impl SampleBuilderTrait for PutSampleBuilder {
    #[zenoh_macros::unstable]
    fn source_info(self, source_info: SourceInfo) -> Self {
        Self(self.0.source_info(source_info))
    }
    #[zenoh_macros::unstable]
    fn attachment<T: Into<Option<Attachment>>>(self, attachment: T) -> Self {
        Self(self.0.attachment(attachment))
    }
}

impl QoSBuilderTrait for PutSampleBuilder {
    fn congestion_control(self, congestion_control: CongestionControl) -> Self {
        Self(self.0.congestion_control(congestion_control))
    }
    fn priority(self, priority: Priority) -> Self {
        Self(self.0.priority(priority))
    }
    fn express(self, is_express: bool) -> Self {
        Self(self.0.express(is_express))
    }
}

impl ValueBuilderTrait for PutSampleBuilder {
    fn encoding<T: Into<Encoding>>(self, encoding: T) -> Self {
        Self(SampleBuilder(Sample {
            encoding: encoding.into(),
            ..self.0 .0
        }))
    }
    fn payload<T: Into<Payload>>(self, payload: T) -> Self {
        Self(SampleBuilder(Sample {
            payload: payload.into(),
            ..self.0 .0
        }))
    }
    fn value<T: Into<Value>>(self, value: T) -> Self {
        let Value { payload, encoding } = value.into();
        Self(SampleBuilder(Sample {
            payload,
            encoding,
            ..self.0 .0
        }))
    }
}

#[derive(Debug)]
pub struct DeleteSampleBuilder(SampleBuilder);

impl From<SampleBuilder> for DeleteSampleBuilder {
    fn from(sample_builder: SampleBuilder) -> Self {
        Self(SampleBuilder(Sample {
            kind: SampleKind::Delete,
            ..sample_builder.0
        }))
    }
}

impl DeleteSampleBuilder {
    pub fn new<IntoKeyExpr>(key_expr: IntoKeyExpr) -> Self
    where
        IntoKeyExpr: Into<KeyExpr<'static>>,
    {
        Self(SampleBuilder::from(Sample {
            key_expr: key_expr.into(),
            payload: Payload::empty(),
            kind: SampleKind::Delete,
            encoding: Encoding::default(),
            timestamp: None,
            qos: QoS::default(),
            #[cfg(feature = "unstable")]
            source_info: SourceInfo::empty(),
            #[cfg(feature = "unstable")]
            attachment: None,
        }))
    }
    /// Allows to change keyexpr of [`Sample`]
    pub fn with_keyexpr<IntoKeyExpr>(self, key_expr: IntoKeyExpr) -> Self
    where
        IntoKeyExpr: Into<KeyExpr<'static>>,
    {
        Self(self.0.keyexpr(key_expr))
    }
    // It's convenient to set QoS as a whole for internal usage. For user API there are `congestion_control`, `priority` and `express` methods.
    pub(crate) fn with_qos(self, qos: QoS) -> Self {
        Self(SampleBuilder(Sample { qos, ..self.0 .0 }))
    }
}

impl TimestampBuilderTrait for DeleteSampleBuilder {
    fn timestamp<T: Into<Option<Timestamp>>>(self, timestamp: T) -> Self {
        Self(self.0.timestamp(timestamp))
    }
}

impl SampleBuilderTrait for DeleteSampleBuilder {
    #[zenoh_macros::unstable]
    fn source_info(self, source_info: SourceInfo) -> Self {
        Self(self.0.source_info(source_info))
    }
    #[zenoh_macros::unstable]
    fn attachment<T: Into<Option<Attachment>>>(self, attachment: T) -> Self {
        Self(self.0.attachment(attachment))
    }
}

impl QoSBuilderTrait for DeleteSampleBuilder {
    fn congestion_control(self, congestion_control: CongestionControl) -> Self {
        Self(self.0.congestion_control(congestion_control))
    }
    fn priority(self, priority: Priority) -> Self {
        Self(self.0.priority(priority))
    }
    fn express(self, is_express: bool) -> Self {
        Self(self.0.express(is_express))
    }
}

impl From<Sample> for SampleBuilder {
    fn from(sample: Sample) -> Self {
        SampleBuilder(sample)
    }
}

impl TryFrom<Sample> for PutSampleBuilder {
    type Error = zresult::Error;
    fn try_from(sample: Sample) -> Result<Self, Self::Error> {
        if sample.kind != SampleKind::Put {
            bail!("Sample is not a put sample")
        }
        Ok(Self(SampleBuilder(sample)))
    }
}

impl TryFrom<Sample> for DeleteSampleBuilder {
    type Error = zresult::Error;
    fn try_from(sample: Sample) -> Result<Self, Self::Error> {
        if sample.kind != SampleKind::Delete {
            bail!("Sample is not a delete sample")
        }
        Ok(Self(SampleBuilder(sample)))
    }
}

impl From<SampleBuilder> for Sample {
    fn from(sample_builder: SampleBuilder) -> Self {
        sample_builder.0
    }
}

impl From<PutSampleBuilder> for Sample {
    fn from(put_sample_builder: PutSampleBuilder) -> Self {
        put_sample_builder.0 .0
    }
}

impl From<DeleteSampleBuilder> for Sample {
    fn from(delete_sample_builder: DeleteSampleBuilder) -> Self {
        delete_sample_builder.0 .0
    }
}
