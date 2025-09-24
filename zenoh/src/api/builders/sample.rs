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
use std::marker::PhantomData;

use uhlc::Timestamp;
use zenoh_core::zresult;
use zenoh_protocol::core::CongestionControl;
#[cfg(feature = "unstable")]
use zenoh_protocol::core::Reliability;

use crate::api::{
    bytes::{OptionZBytes, ZBytes},
    encoding::Encoding,
    key_expr::KeyExpr,
    publisher::Priority,
    sample::{QoS, QoSBuilder, Sample, SampleKind},
};
#[zenoh_macros::internal]
use crate::pubsub::{
    PublicationBuilder, PublicationBuilderDelete, PublicationBuilderPut, Publisher,
};
#[cfg(feature = "unstable")]
use crate::sample::SourceInfo;
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
    /// Sets or clears the timestamp
    fn timestamp<T: Into<Option<Timestamp>>>(self, timestamp: T) -> Self;
}

pub trait SampleBuilderTrait {
    /// Attach source information
    #[zenoh_macros::unstable]
    fn source_info(self, source_info: SourceInfo) -> Self;
    /// Attach user-provided data in key-value format
    fn attachment<T: Into<OptionZBytes>>(self, attachment: T) -> Self;
}

pub trait EncodingBuilderTrait {
    /// Set the [`Encoding`]
    fn encoding<T: Into<Encoding>>(self, encoding: T) -> Self;
}

/// The type modifier for a [`SampleBuilder`] to create a [`Put`](crate::sample::SampleKind::Put) sample.
#[derive(Clone, Debug)]
pub struct SampleBuilderPut;
/// The type modifier for a [`SampleBuilder`] to create a [`Delete`](crate::sample::SampleKind::Delete) sample.
#[derive(Clone, Debug)]
pub struct SampleBuilderDelete;
/// The type modifier for a [`SampleBuilder`] for the building stage
/// when the sample [`kind`](crate::sample::Sample::kind) is not yet specified.
///
/// With this modifier the `SampleBuilder` can't be resolved;
/// the selection of the kind must be done with [`SampleBuilder::put`] or
/// [`SampleBuilder::delete`].
#[derive(Clone, Debug)]
pub struct SampleBuilderAny;

/// A builder for [`Sample`](crate::sample::Sample)
///
/// As the `Sample` struct is not mutable, the `SampleBuilder` can be used to
/// create or modify an existing `Sample` instance or to create a new one from scratch
/// for storing it for later use.
#[derive(Clone, Debug)]
pub struct SampleBuilder<T> {
    sample: Sample,
    _t: PhantomData<T>,
}

impl SampleBuilder<SampleBuilderPut> {
    pub fn put<IntoKeyExpr, IntoZBytes>(
        key_expr: IntoKeyExpr,
        payload: IntoZBytes,
    ) -> SampleBuilder<SampleBuilderPut>
    where
        IntoKeyExpr: Into<KeyExpr<'static>>,
        IntoZBytes: Into<ZBytes>,
    {
        Self {
            sample: Sample {
                key_expr: key_expr.into(),
                payload: payload.into(),
                kind: SampleKind::Put,
                encoding: Encoding::default(),
                timestamp: None,
                qos: QoS::default(),
                #[cfg(feature = "unstable")]
                reliability: Reliability::DEFAULT,
                #[cfg(feature = "unstable")]
                source_info: SourceInfo::empty(),
                attachment: None,
            },
            _t: PhantomData::<SampleBuilderPut>,
        }
    }

    pub fn payload<IntoZBytes>(mut self, payload: IntoZBytes) -> Self
    where
        IntoZBytes: Into<ZBytes>,
    {
        self.sample.payload = payload.into();
        self
    }
}

impl SampleBuilder<SampleBuilderDelete> {
    pub fn delete<IntoKeyExpr>(key_expr: IntoKeyExpr) -> SampleBuilder<SampleBuilderDelete>
    where
        IntoKeyExpr: Into<KeyExpr<'static>>,
    {
        Self {
            sample: Sample {
                key_expr: key_expr.into(),
                payload: ZBytes::new(),
                kind: SampleKind::Delete,
                encoding: Encoding::default(),
                timestamp: None,
                qos: QoS::default(),
                #[cfg(feature = "unstable")]
                reliability: Reliability::DEFAULT,
                #[cfg(feature = "unstable")]
                source_info: SourceInfo::empty(),
                attachment: None,
            },
            _t: PhantomData::<SampleBuilderDelete>,
        }
    }
}

impl<T> SampleBuilder<T> {
    /// Allows changing the key expression of a [`Sample`]
    pub fn keyexpr<IntoKeyExpr>(self, key_expr: IntoKeyExpr) -> Self
    where
        IntoKeyExpr: Into<KeyExpr<'static>>,
    {
        Self {
            sample: Sample {
                key_expr: key_expr.into(),
                ..self.sample
            },
            _t: PhantomData::<T>,
        }
    }

    // Allows changing the QoS of a [`Sample`] as a whole
    pub(crate) fn qos(self, qos: QoS) -> Self {
        Self {
            sample: Sample { qos, ..self.sample },
            _t: PhantomData::<T>,
        }
    }

    #[zenoh_macros::unstable]
    pub fn reliability(self, reliability: Reliability) -> Self {
        Self {
            sample: Sample {
                reliability,
                ..self.sample
            },
            _t: PhantomData::<T>,
        }
    }
}

#[zenoh_macros::internal_trait]
impl<T> TimestampBuilderTrait for SampleBuilder<T> {
    fn timestamp<U: Into<Option<Timestamp>>>(self, timestamp: U) -> Self {
        Self {
            sample: Sample {
                timestamp: timestamp.into(),
                ..self.sample
            },
            _t: PhantomData::<T>,
        }
    }
}

#[zenoh_macros::internal_trait]
impl<T> SampleBuilderTrait for SampleBuilder<T> {
    #[zenoh_macros::unstable]
    fn source_info(self, source_info: SourceInfo) -> Self {
        Self {
            sample: Sample {
                source_info,
                ..self.sample
            },
            _t: PhantomData::<T>,
        }
    }

    fn attachment<U: Into<OptionZBytes>>(self, attachment: U) -> Self {
        let attachment: OptionZBytes = attachment.into();
        Self {
            sample: Sample {
                attachment: attachment.into(),
                ..self.sample
            },
            _t: PhantomData::<T>,
        }
    }
}

#[zenoh_macros::internal_trait]
impl<T> QoSBuilderTrait for SampleBuilder<T> {
    fn congestion_control(self, congestion_control: CongestionControl) -> Self {
        let qos: QoSBuilder = self.sample.qos.into();
        let qos = qos.congestion_control(congestion_control).into();
        Self {
            sample: Sample { qos, ..self.sample },
            _t: PhantomData::<T>,
        }
    }
    fn priority(self, priority: Priority) -> Self {
        let qos: QoSBuilder = self.sample.qos.into();
        let qos = qos.priority(priority).into();
        Self {
            sample: Sample { qos, ..self.sample },
            _t: PhantomData::<T>,
        }
    }
    fn express(self, is_express: bool) -> Self {
        let qos: QoSBuilder = self.sample.qos.into();
        let qos = qos.express(is_express).into();
        Self {
            sample: Sample { qos, ..self.sample },
            _t: PhantomData::<T>,
        }
    }
}

#[zenoh_macros::internal_trait]
impl EncodingBuilderTrait for SampleBuilder<SampleBuilderPut> {
    fn encoding<T: Into<Encoding>>(self, encoding: T) -> Self {
        Self {
            sample: Sample {
                encoding: encoding.into(),
                ..self.sample
            },
            _t: PhantomData::<SampleBuilderPut>,
        }
    }
}

impl From<Sample> for SampleBuilder<SampleBuilderAny> {
    fn from(sample: Sample) -> Self {
        SampleBuilder {
            sample,
            _t: PhantomData::<SampleBuilderAny>,
        }
    }
}

impl TryFrom<Sample> for SampleBuilder<SampleBuilderPut> {
    type Error = zresult::Error;
    fn try_from(sample: Sample) -> Result<Self, Self::Error> {
        if sample.kind != SampleKind::Put {
            bail!("Sample is not a put sample")
        }
        Ok(SampleBuilder {
            sample,
            _t: PhantomData::<SampleBuilderPut>,
        })
    }
}

impl TryFrom<Sample> for SampleBuilder<SampleBuilderDelete> {
    type Error = zresult::Error;
    fn try_from(sample: Sample) -> Result<Self, Self::Error> {
        if sample.kind != SampleKind::Delete {
            bail!("Sample is not a delete sample")
        }
        Ok(SampleBuilder {
            sample,
            _t: PhantomData::<SampleBuilderDelete>,
        })
    }
}

impl<T> From<SampleBuilder<T>> for Sample {
    fn from(sample_builder: SampleBuilder<T>) -> Self {
        sample_builder.sample
    }
}

#[zenoh_macros::internal]
impl From<&PublicationBuilder<&Publisher<'_>, PublicationBuilderPut>> for Sample {
    fn from(builder: &PublicationBuilder<&Publisher<'_>, PublicationBuilderPut>) -> Self {
        Sample {
            key_expr: builder.publisher.key_expr.clone().into_owned(),
            payload: builder.kind.payload.clone(),
            kind: SampleKind::Put,
            encoding: builder.kind.encoding.clone(),
            timestamp: builder.timestamp,
            qos: QoSBuilder::from(QoS::default())
                .congestion_control(builder.publisher.congestion_control)
                .priority(builder.publisher.priority)
                .express(builder.publisher.is_express)
                .into(),
            #[cfg(feature = "unstable")]
            reliability: builder.publisher.reliability,
            #[cfg(feature = "unstable")]
            source_info: builder.source_info.clone(),
            attachment: builder.attachment.clone(),
        }
    }
}

#[zenoh_macros::internal]
impl From<&PublicationBuilder<&Publisher<'_>, PublicationBuilderDelete>> for Sample {
    fn from(builder: &PublicationBuilder<&Publisher<'_>, PublicationBuilderDelete>) -> Self {
        Sample {
            key_expr: builder.publisher.key_expr.clone().into_owned(),
            payload: ZBytes::new(),
            kind: SampleKind::Put,
            encoding: Encoding::ZENOH_BYTES,
            timestamp: builder.timestamp,
            qos: QoSBuilder::from(QoS::default())
                .congestion_control(builder.publisher.congestion_control)
                .priority(builder.publisher.priority)
                .express(builder.publisher.is_express)
                .into(),
            #[cfg(feature = "unstable")]
            reliability: builder.publisher.reliability,
            #[cfg(feature = "unstable")]
            source_info: builder.source_info.clone(),
            attachment: builder.attachment.clone(),
        }
    }
}
