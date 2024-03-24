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

use crate::sample::Attachment;
use crate::sample::QoS;
use crate::sample::SourceInfo;
use crate::Encoding;
use crate::KeyExpr;
use crate::Payload;
use crate::Priority;
use crate::Sample;
use crate::SampleKind;
use uhlc::Timestamp;
use zenoh_core::zresult;
use zenoh_core::AsyncResolve;
use zenoh_core::Resolvable;
use zenoh_core::SyncResolve;
use zenoh_protocol::core::CongestionControl;

pub trait SampleBuilderTrait {
    fn with_keyexpr<IntoKeyExpr>(self, key_expr: IntoKeyExpr) -> Self
    where
        IntoKeyExpr: Into<KeyExpr<'static>>;
    fn with_timestamp_opt(self, timestamp: Option<Timestamp>) -> Self;
    fn with_timestamp(self, timestamp: Timestamp) -> Self;
    #[zenoh_macros::unstable]
    fn with_source_info(self, source_info: SourceInfo) -> Self;
    #[zenoh_macros::unstable]
    fn with_attachment_opt(self, attachment: Option<Attachment>) -> Self;
    #[zenoh_macros::unstable]
    fn with_attachment(self, attachment: Attachment) -> Self;
    fn congestion_control(self, congestion_control: CongestionControl) -> Self;
    fn priority(self, priority: Priority) -> Self;
    fn express(self, is_express: bool) -> Self;
}

pub trait PutSampleBuilderTrait: SampleBuilderTrait {
    fn with_encoding(self, encoding: Encoding) -> Self;
    fn with_payload<IntoPayload>(self, payload: IntoPayload) -> Self
    where
        IntoPayload: Into<Payload>;
}

pub trait DeleteSampleBuilderTrait: SampleBuilderTrait {}

#[derive(Debug)]
pub struct SampleBuilder(Sample);

impl SampleBuilder {
    pub fn new<IntoKeyExpr>(key_expr: IntoKeyExpr) -> Self
    where
        IntoKeyExpr: Into<KeyExpr<'static>>,
    {
        Self(Sample {
            key_expr: key_expr.into(),
            payload: Payload::empty(),
            kind: SampleKind::default(),
            encoding: Encoding::default(),
            timestamp: None,
            qos: QoS::default(),
            #[cfg(feature = "unstable")]
            source_info: SourceInfo::empty(),
            #[cfg(feature = "unstable")]
            attachment: None,
        })
    }
}

impl SampleBuilderTrait for SampleBuilder {
    fn with_keyexpr<IntoKeyExpr>(self, key_expr: IntoKeyExpr) -> Self
    where
        IntoKeyExpr: Into<KeyExpr<'static>>,
    {
        Self(Sample {
            key_expr: key_expr.into(),
            ..self.0
        })
    }

    fn with_timestamp_opt(self, timestamp: Option<Timestamp>) -> Self {
        Self(Sample {
            timestamp,
            ..self.0
        })
    }

    fn with_timestamp(self, timestamp: Timestamp) -> Self {
        self.with_timestamp_opt(Some(timestamp))
    }

    #[zenoh_macros::unstable]
    fn with_source_info(self, source_info: SourceInfo) -> Self {
        Self(Sample {
            source_info,
            ..self.0
        })
    }

    #[zenoh_macros::unstable]
    fn with_attachment_opt(self, attachment: Option<Attachment>) -> Self {
        Self(Sample {
            attachment,
            ..self.0
        })
    }

    #[zenoh_macros::unstable]
    fn with_attachment(self, attachment: Attachment) -> Self {
        self.with_attachment_opt(Some(attachment))
    }
    fn congestion_control(self, congestion_control: CongestionControl) -> Self {
        Self(Sample {
            qos: self.0.qos.with_congestion_control(congestion_control),
            ..self.0
        })
    }
    fn priority(self, priority: Priority) -> Self {
        Self(Sample {
            qos: self.0.qos.with_priority(priority),
            ..self.0
        })
    }
    fn express(self, is_express: bool) -> Self {
        Self(Sample {
            qos: self.0.qos.with_express(is_express),
            ..self.0
        })
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
    pub fn new<IntoKeyExpr, IntoPayload>(key_expr: IntoKeyExpr, payload: IntoPayload) -> Self
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
    // It's convenient to set QoS as a whole for internal usage. For user API there are `congestion_control`, `priority` and `express` methods.
    pub(crate) fn with_qos(self, qos: QoS) -> Self {
        Self(SampleBuilder(Sample { qos, ..self.0 .0 }))
    }
}

impl SampleBuilderTrait for PutSampleBuilder {
    fn with_keyexpr<IntoKeyExpr>(self, key_expr: IntoKeyExpr) -> Self
    where
        IntoKeyExpr: Into<KeyExpr<'static>>,
    {
        Self(self.0.with_keyexpr(key_expr))
    }
    fn with_timestamp(self, timestamp: Timestamp) -> Self {
        Self(self.0.with_timestamp(timestamp))
    }
    fn with_timestamp_opt(self, timestamp: Option<Timestamp>) -> Self {
        Self(self.0.with_timestamp_opt(timestamp))
    }
    #[zenoh_macros::unstable]
    fn with_source_info(self, source_info: SourceInfo) -> Self {
        Self(self.0.with_source_info(source_info))
    }
    #[zenoh_macros::unstable]
    fn with_attachment(self, attachment: Attachment) -> Self {
        Self(self.0.with_attachment(attachment))
    }
    #[zenoh_macros::unstable]
    fn with_attachment_opt(self, attachment: Option<Attachment>) -> Self {
        Self(self.0.with_attachment_opt(attachment))
    }
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

impl PutSampleBuilderTrait for PutSampleBuilder {
    fn with_encoding(self, encoding: Encoding) -> Self {
        Self(SampleBuilder(Sample {
            encoding,
            ..self.0 .0
        }))
    }
    fn with_payload<IntoPayload>(self, payload: IntoPayload) -> Self
    where
        IntoPayload: Into<Payload>,
    {
        Self(SampleBuilder(Sample {
            payload: payload.into(),
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
    // It's convenient to set QoS as a whole for internal usage. For user API there are `congestion_control`, `priority` and `express` methods.
    pub(crate) fn with_qos(self, qos: QoS) -> Self {
        Self(SampleBuilder(Sample { qos, ..self.0 .0 }))
    }
}

impl SampleBuilderTrait for DeleteSampleBuilder {
    fn with_keyexpr<IntoKeyExpr>(self, key_expr: IntoKeyExpr) -> Self
    where
        IntoKeyExpr: Into<KeyExpr<'static>>,
    {
        Self(self.0.with_keyexpr(key_expr))
    }
    fn with_timestamp(self, timestamp: Timestamp) -> Self {
        Self(self.0.with_timestamp(timestamp))
    }
    fn with_timestamp_opt(self, timestamp: Option<Timestamp>) -> Self {
        Self(self.0.with_timestamp_opt(timestamp))
    }
    #[zenoh_macros::unstable]
    fn with_source_info(self, source_info: SourceInfo) -> Self {
        Self(self.0.with_source_info(source_info))
    }
    #[zenoh_macros::unstable]
    fn with_attachment(self, attachment: Attachment) -> Self {
        Self(self.0.with_attachment(attachment))
    }
    #[zenoh_macros::unstable]
    fn with_attachment_opt(self, attachment: Option<Attachment>) -> Self {
        Self(self.0.with_attachment_opt(attachment))
    }
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

impl DeleteSampleBuilderTrait for DeleteSampleBuilder {}

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

impl Resolvable for SampleBuilder {
    type To = Sample;
}

impl Resolvable for PutSampleBuilder {
    type To = Sample;
}

impl Resolvable for DeleteSampleBuilder {
    type To = Sample;
}

impl SyncResolve for SampleBuilder {
    fn res_sync(self) -> Self::To {
        self.0
    }
}

impl SyncResolve for PutSampleBuilder {
    fn res_sync(self) -> Self::To {
        self.0.res_sync()
    }
}

impl SyncResolve for DeleteSampleBuilder {
    fn res_sync(self) -> Self::To {
        self.0.res_sync()
    }
}

impl AsyncResolve for SampleBuilder {
    type Future = futures::future::Ready<Self::To>;
    fn res_async(self) -> Self::Future {
        futures::future::ready(self.0)
    }
}

impl AsyncResolve for PutSampleBuilder {
    type Future = futures::future::Ready<Self::To>;
    fn res_async(self) -> Self::Future {
        self.0.res_async()
    }
}

impl AsyncResolve for DeleteSampleBuilder {
    type Future = futures::future::Ready<Self::To>;
    fn res_async(self) -> Self::Future {
        self.0.res_async()
    }
}
