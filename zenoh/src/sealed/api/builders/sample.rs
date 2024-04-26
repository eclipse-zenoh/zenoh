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
use crate::sealed::api::bytes::ZBytes;
use crate::sealed::api::encoding::Encoding;
use crate::sealed::api::key_expr::KeyExpr;
use crate::sealed::api::publication::Priority;
use crate::sealed::api::sample::QoS;
use crate::sealed::api::sample::QoSBuilder;
use crate::sealed::api::sample::Sample;
use crate::sealed::api::sample::SampleKind;
use crate::sealed::api::value::Value;
#[cfg(feature = "unstable")]
use crate::{sealed::api::bytes::OptionZBytes, sealed::api::sample::SourceInfo};
use std::marker::PhantomData;
use uhlc::Timestamp;
use zenoh_core::zresult;
use zenoh_protocol::core::CongestionControl;

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
    fn attachment<T: Into<OptionZBytes>>(self, attachment: T) -> Self;
}

pub trait ValueBuilderTrait {
    /// Set the [`Encoding`]
    fn encoding<T: Into<Encoding>>(self, encoding: T) -> Self;
    /// Sets the payload
    fn payload<T: Into<ZBytes>>(self, payload: T) -> Self;
    /// Sets both payload and encoding at once.
    /// This is convenient for passing user type which supports `Into<Value>` when both payload and encoding depends on user type
    fn value<T: Into<Value>>(self, value: T) -> Self;
}

#[derive(Clone, Debug)]
pub struct SampleBuilderPut;
#[derive(Clone, Debug)]
pub struct SampleBuilderDelete;
#[derive(Clone, Debug)]
pub struct SampleBuilderAny;

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
                source_info: SourceInfo::empty(),
                #[cfg(feature = "unstable")]
                attachment: None,
            },
            _t: PhantomData::<SampleBuilderPut>,
        }
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
                payload: ZBytes::empty(),
                kind: SampleKind::Delete,
                encoding: Encoding::default(),
                timestamp: None,
                qos: QoS::default(),
                #[cfg(feature = "unstable")]
                source_info: SourceInfo::empty(),
                #[cfg(feature = "unstable")]
                attachment: None,
            },
            _t: PhantomData::<SampleBuilderDelete>,
        }
    }
}

impl<T> SampleBuilder<T> {
    /// Allows to change keyexpr of [`Sample`]
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

    // Allows to change qos as a whole of [`Sample`]
    pub(crate) fn qos(self, qos: QoS) -> Self {
        Self {
            sample: Sample { qos, ..self.sample },
            _t: PhantomData::<T>,
        }
    }
}

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

#[cfg(feature = "unstable")]
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

    #[zenoh_macros::unstable]
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

impl ValueBuilderTrait for SampleBuilder<SampleBuilderPut> {
    fn encoding<T: Into<Encoding>>(self, encoding: T) -> Self {
        Self {
            sample: Sample {
                encoding: encoding.into(),
                ..self.sample
            },
            _t: PhantomData::<SampleBuilderPut>,
        }
    }
    fn payload<T: Into<ZBytes>>(self, payload: T) -> Self {
        Self {
            sample: Sample {
                payload: payload.into(),
                ..self.sample
            },
            _t: PhantomData::<SampleBuilderPut>,
        }
    }
    fn value<T: Into<Value>>(self, value: T) -> Self {
        let Value { payload, encoding } = value.into();
        Self {
            sample: Sample {
                payload,
                encoding,
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
