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
use std::future::{IntoFuture, Ready};

use uhlc::Timestamp;
use zenoh_core::{Resolvable, Wait};
use zenoh_protocol::{
    core::{CongestionControl, WireExpr},
    network::{response, Mapping, Response},
    zenoh::{self, ResponseBody},
};
use zenoh_result::ZResult;

#[zenoh_macros::unstable]
use crate::api::sample::SourceInfo;
use crate::api::{
    builders::sample::{
        EncodingBuilderTrait, QoSBuilderTrait, SampleBuilder, SampleBuilderTrait,
        TimestampBuilderTrait,
    },
    bytes::{OptionZBytes, ZBytes},
    encoding::Encoding,
    key_expr::KeyExpr,
    publisher::Priority,
    queryable::Query,
    sample::QoSBuilder,
};

/// The type modifier for a [`ReplyBuilder`] to create a reply with a [`Put`](crate::sample::SampleKind::Put) sample.
#[derive(Debug)]
pub struct ReplyBuilderPut {
    payload: ZBytes,
    encoding: Encoding,
}

/// The type modifier for a [`ReplyBuilder`] to create a reply with a [`Delete`](crate::sample::SampleKind::Delete) sample.
#[derive(Debug)]
pub struct ReplyBuilderDelete;

/// A builder for a [`Reply`](crate::query::Reply)
/// returned by [`Query::reply()`](Query::reply) and [`Query::reply_del()`](Query::reply_del)
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
#[derive(Debug)]
pub struct ReplyBuilder<'a, 'b, T> {
    query: &'a Query,
    key_expr: ZResult<KeyExpr<'b>>,
    kind: T,
    timestamp: Option<Timestamp>,
    qos: QoSBuilder,
    #[cfg(feature = "unstable")]
    source_info: SourceInfo,
    attachment: Option<ZBytes>,
}

impl<'a, 'b> ReplyBuilder<'a, 'b, ReplyBuilderPut> {
    pub(crate) fn new<TryIntoKeyExpr, IntoZBytes>(
        query: &'a Query,
        key_expr: TryIntoKeyExpr,
        payload: IntoZBytes,
    ) -> Self
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>,
        IntoZBytes: Into<ZBytes>,
    {
        Self {
            query,
            key_expr: key_expr.try_into().map_err(Into::into),
            qos: response::ext::QoSType::RESPONSE.into(),
            kind: ReplyBuilderPut {
                payload: payload.into(),
                encoding: Encoding::default(),
            },
            timestamp: None,
            #[cfg(feature = "unstable")]
            source_info: SourceInfo::empty(),
            attachment: None,
        }
    }
}

impl<'a, 'b> ReplyBuilder<'a, 'b, ReplyBuilderDelete> {
    pub(crate) fn new<TryIntoKeyExpr>(query: &'a Query, key_expr: TryIntoKeyExpr) -> Self
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>,
    {
        Self {
            query,
            key_expr: key_expr.try_into().map_err(Into::into),
            qos: response::ext::QoSType::RESPONSE.into(),
            kind: ReplyBuilderDelete,
            timestamp: None,
            #[cfg(feature = "unstable")]
            source_info: SourceInfo::empty(),
            attachment: None,
        }
    }
}

#[zenoh_macros::internal_trait]
impl<T> TimestampBuilderTrait for ReplyBuilder<'_, '_, T> {
    fn timestamp<U: Into<Option<Timestamp>>>(self, timestamp: U) -> Self {
        Self {
            timestamp: timestamp.into(),
            ..self
        }
    }
}

#[zenoh_macros::internal_trait]
impl<T> SampleBuilderTrait for ReplyBuilder<'_, '_, T> {
    fn attachment<U: Into<OptionZBytes>>(self, attachment: U) -> Self {
        let attachment: OptionZBytes = attachment.into();
        Self {
            attachment: attachment.into(),
            ..self
        }
    }

    #[cfg(feature = "unstable")]
    fn source_info(self, source_info: SourceInfo) -> Self {
        Self {
            source_info,
            ..self
        }
    }
}

#[zenoh_macros::internal_trait]
impl<T> QoSBuilderTrait for ReplyBuilder<'_, '_, T> {
    fn congestion_control(self, congestion_control: CongestionControl) -> Self {
        let qos = self.qos.congestion_control(congestion_control);
        Self { qos, ..self }
    }

    fn priority(self, priority: Priority) -> Self {
        let qos = self.qos.priority(priority);
        Self { qos, ..self }
    }

    fn express(self, is_express: bool) -> Self {
        let qos = self.qos.express(is_express);
        Self { qos, ..self }
    }
}

#[zenoh_macros::internal_trait]
impl EncodingBuilderTrait for ReplyBuilder<'_, '_, ReplyBuilderPut> {
    fn encoding<T: Into<Encoding>>(self, encoding: T) -> Self {
        Self {
            kind: ReplyBuilderPut {
                encoding: encoding.into(),
                ..self.kind
            },
            ..self
        }
    }
}

impl<T> Resolvable for ReplyBuilder<'_, '_, T> {
    type To = ZResult<()>;
}

impl Wait for ReplyBuilder<'_, '_, ReplyBuilderPut> {
    fn wait(self) -> <Self as Resolvable>::To {
        let key_expr = self.key_expr?.into_owned();
        let sample = SampleBuilder::put(key_expr, self.kind.payload)
            .encoding(self.kind.encoding)
            .timestamp(self.timestamp)
            .qos(self.qos.into());
        #[cfg(feature = "unstable")]
        let sample = sample.source_info(self.source_info);
        let sample = sample.attachment(self.attachment);
        self.query._reply_sample(sample.into())
    }
}

impl Wait for ReplyBuilder<'_, '_, ReplyBuilderDelete> {
    fn wait(self) -> <Self as Resolvable>::To {
        let key_expr = self.key_expr?.into_owned();
        let sample = SampleBuilder::delete(key_expr)
            .timestamp(self.timestamp)
            .qos(self.qos.into());
        #[cfg(feature = "unstable")]
        let sample = sample.source_info(self.source_info);
        let sample = sample.attachment(self.attachment);
        self.query._reply_sample(sample.into())
    }
}

impl IntoFuture for ReplyBuilder<'_, '_, ReplyBuilderPut> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

impl IntoFuture for ReplyBuilder<'_, '_, ReplyBuilderDelete> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

/// A builder for a [`ReplyError`](crate::query::ReplyError)
/// returned by [`Query::reply_err()`](Query::reply_err).
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
#[derive(Debug)]
pub struct ReplyErrBuilder<'a> {
    query: &'a Query,
    payload: ZBytes,
    encoding: Encoding,
}

impl<'a> ReplyErrBuilder<'a> {
    pub(crate) fn new<IntoZBytes>(query: &'a Query, payload: IntoZBytes) -> ReplyErrBuilder<'a>
    where
        IntoZBytes: Into<ZBytes>,
    {
        Self {
            query,
            payload: payload.into(),
            encoding: Encoding::default(),
        }
    }
}

#[zenoh_macros::internal_trait]
impl EncodingBuilderTrait for ReplyErrBuilder<'_> {
    fn encoding<T: Into<Encoding>>(self, encoding: T) -> Self {
        Self {
            encoding: encoding.into(),
            ..self
        }
    }
}

impl Resolvable for ReplyErrBuilder<'_> {
    type To = ZResult<()>;
}

impl Wait for ReplyErrBuilder<'_> {
    fn wait(self) -> <Self as Resolvable>::To {
        self.query.inner.primitives.send_response(&mut Response {
            rid: self.query.inner.qid,
            wire_expr: WireExpr {
                scope: 0,
                suffix: std::borrow::Cow::Owned(self.query.key_expr().as_str().to_owned()),
                mapping: Mapping::Sender,
            },
            payload: ResponseBody::Err(zenoh::Err {
                encoding: self.encoding.into(),
                ext_sinfo: None,
                #[cfg(feature = "shared-memory")]
                ext_shm: None,
                ext_unknown: vec![],
                payload: self.payload.into(),
            }),
            ext_qos: response::ext::QoSType::RESPONSE,
            ext_tstamp: None,
            ext_respid: Some(response::ext::ResponderIdType {
                zid: self.query.inner.zid,
                eid: self.query.eid,
            }),
        });
        Ok(())
    }
}

impl IntoFuture for ReplyErrBuilder<'_> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}
