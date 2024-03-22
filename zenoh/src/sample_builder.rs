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
    fn with_timestamp(self, timestamp: Timestamp) -> Self;
    #[zenoh_macros::unstable]
    fn with_source_info(self, source_info: SourceInfo) -> Self;
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

pub struct SampleBuilder(Sample);

impl SampleBuilderTrait for SampleBuilder {
    fn with_keyexpr<IntoKeyExpr>(self, key_expr: IntoKeyExpr) -> Self
    where
        IntoKeyExpr: Into<KeyExpr<'static>>,
    {
        let mut this = self;
        this.0.key_expr = key_expr.into();
        this
    }

    fn with_timestamp(self, timestamp: Timestamp) -> Self {
        let mut this = self;
        this.0.timestamp = Some(timestamp);
        this
    }
    #[zenoh_macros::unstable]
    fn with_source_info(self, source_info: SourceInfo) -> Self {
        let mut this = self;
        this.0.source_info = source_info;
        this
    }
    #[zenoh_macros::unstable]
    fn with_attachment(self, attachment: Attachment) -> Self {
        let mut this = self;
        this.0.attachment = Some(attachment);
        this
    }
    fn congestion_control(self, congestion_control: CongestionControl) -> Self {
        let mut this = self;
        this.0.qos = this.0.qos.with_congestion_control(congestion_control);
        this
    }
    fn priority(self, priority: Priority) -> Self {
        let mut this = self;
        this.0.qos = this.0.qos.with_priority(priority);
        this
    }
    fn express(self, is_express: bool) -> Self {
        let mut this = self;
        this.0.qos = this.0.qos.with_express(is_express);
        this
    }
}

pub struct PutSampleBuilder(SampleBuilder);

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
    pub fn without_timestamp(self) -> Self {
        let mut this = self;
        this.0 .0.timestamp = None;
        this
    }
    pub fn without_attachment(self) -> Self {
        let mut this = self;
        this.0 .0.attachment = None;
        this
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
    #[zenoh_macros::unstable]
    fn with_source_info(self, source_info: SourceInfo) -> Self {
        Self(self.0.with_source_info(source_info))
    }
    #[zenoh_macros::unstable]
    fn with_attachment(self, attachment: Attachment) -> Self {
        Self(self.0.with_attachment(attachment))
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
        let mut this = self;
        this.0 .0.encoding = encoding;
        this
    }
    fn with_payload<IntoPayload>(self, payload: IntoPayload) -> Self
    where
        IntoPayload: Into<Payload>,
    {
        let mut this = self;
        this.0 .0.payload = payload.into();
        this
    }
}

pub struct DeleteSampleBuilder(SampleBuilder);

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
    #[zenoh_macros::unstable]
    fn with_source_info(self, source_info: SourceInfo) -> Self {
        Self(self.0.with_source_info(source_info))
    }
    #[zenoh_macros::unstable]
    fn with_attachment(self, attachment: Attachment) -> Self {
        Self(self.0.with_attachment(attachment))
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
