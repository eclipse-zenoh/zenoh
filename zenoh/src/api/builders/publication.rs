use std::future::Ready;

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
use crate::api::builders::sample::SampleBuilderTrait;
use crate::api::builders::sample::{QoSBuilderTrait, TimestampBuilderTrait, ValueBuilderTrait};
use crate::api::key_expr::KeyExpr;
use crate::api::publication::Priority;
use crate::api::sample::Locality;
use crate::api::sample::SampleKind;
#[cfg(feature = "unstable")]
use crate::api::sample::SourceInfo;
use crate::api::session::SessionRef;
use crate::api::value::Value;
use crate::api::{
    encoding::Encoding, payload::Payload, publication::Publisher, sample::Attachment,
};
use zenoh_core::{AsyncResolve, Resolvable, Result as ZResult, SyncResolve};
use zenoh_protocol::core::CongestionControl;
use zenoh_protocol::network::Mapping;

pub type SessionPutBuilder<'a, 'b> =
    PublicationBuilder<PublisherBuilder<'a, 'b>, PublicationBuilderPut>;

pub type SessionDeleteBuilder<'a, 'b> =
    PublicationBuilder<PublisherBuilder<'a, 'b>, PublicationBuilderDelete>;

pub type PublisherPutBuilder<'a> = PublicationBuilder<&'a Publisher<'a>, PublicationBuilderPut>;

pub type PublisherDeleteBuilder<'a> =
    PublicationBuilder<&'a Publisher<'a>, PublicationBuilderDelete>;

#[derive(Debug, Clone)]
pub struct PublicationBuilderPut {
    pub(crate) payload: Payload,
    pub(crate) encoding: Encoding,
}
#[derive(Debug, Clone)]
pub struct PublicationBuilderDelete;

/// A builder for initializing  [`Session::put`](crate::session::Session::put), [`Session::delete`](crate::session::Session::delete),
/// [`Publisher::put`](crate::publication::Publisher::put), and [`Publisher::delete`](crate::publication::Publisher::delete) operations.
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh::prelude::r#async::*;
/// use zenoh::publication::CongestionControl;
/// use zenoh::sample::builder::{ValueBuilderTrait, QoSBuilderTrait};
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// session
///     .put("key/expression", "payload")
///     .encoding(Encoding::TEXT_PLAIN)
///     .congestion_control(CongestionControl::Block)
///     .res()
///     .await
///     .unwrap();
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
#[derive(Debug, Clone)]
pub struct PublicationBuilder<P, T> {
    pub(crate) publisher: P,
    pub(crate) kind: T,
    pub(crate) timestamp: Option<uhlc::Timestamp>,
    #[cfg(feature = "unstable")]
    pub(crate) source_info: SourceInfo,
    #[cfg(feature = "unstable")]
    pub(crate) attachment: Option<Attachment>,
}

impl<T> QoSBuilderTrait for PublicationBuilder<PublisherBuilder<'_, '_>, T> {
    #[inline]
    fn congestion_control(self, congestion_control: CongestionControl) -> Self {
        Self {
            publisher: self.publisher.congestion_control(congestion_control),
            ..self
        }
    }
    #[inline]
    fn priority(self, priority: Priority) -> Self {
        Self {
            publisher: self.publisher.priority(priority),
            ..self
        }
    }
    #[inline]
    fn express(self, is_express: bool) -> Self {
        Self {
            publisher: self.publisher.express(is_express),
            ..self
        }
    }
}

impl<T> PublicationBuilder<PublisherBuilder<'_, '_>, T> {
    /// Restrict the matching subscribers that will receive the published data
    /// to the ones that have the given [`Locality`](crate::prelude::Locality).
    #[zenoh_macros::unstable]
    #[inline]
    pub fn allowed_destination(mut self, destination: Locality) -> Self {
        self.publisher = self.publisher.allowed_destination(destination);
        self
    }
}

impl<P> ValueBuilderTrait for PublicationBuilder<P, PublicationBuilderPut> {
    fn encoding<T: Into<Encoding>>(self, encoding: T) -> Self {
        Self {
            kind: PublicationBuilderPut {
                encoding: encoding.into(),
                ..self.kind
            },
            ..self
        }
    }

    fn payload<IntoPayload>(self, payload: IntoPayload) -> Self
    where
        IntoPayload: Into<Payload>,
    {
        Self {
            kind: PublicationBuilderPut {
                payload: payload.into(),
                ..self.kind
            },
            ..self
        }
    }
    fn value<T: Into<Value>>(self, value: T) -> Self {
        let Value { payload, encoding } = value.into();
        Self {
            kind: PublicationBuilderPut { payload, encoding },
            ..self
        }
    }
}

impl<P, T> SampleBuilderTrait for PublicationBuilder<P, T> {
    #[cfg(feature = "unstable")]
    fn source_info(self, source_info: SourceInfo) -> Self {
        Self {
            source_info,
            ..self
        }
    }
    #[cfg(feature = "unstable")]
    fn attachment<TA: Into<Option<Attachment>>>(self, attachment: TA) -> Self {
        Self {
            attachment: attachment.into(),
            ..self
        }
    }
}

impl<P, T> TimestampBuilderTrait for PublicationBuilder<P, T> {
    fn timestamp<TS: Into<Option<uhlc::Timestamp>>>(self, timestamp: TS) -> Self {
        Self {
            timestamp: timestamp.into(),
            ..self
        }
    }
}

impl<P, T> Resolvable for PublicationBuilder<P, T> {
    type To = ZResult<()>;
}

impl SyncResolve for PublicationBuilder<PublisherBuilder<'_, '_>, PublicationBuilderPut> {
    #[inline]
    fn res_sync(self) -> <Self as Resolvable>::To {
        let publisher = self.publisher.create_one_shot_publisher()?;
        publisher.resolve_put(
            self.kind.payload,
            SampleKind::Put,
            self.kind.encoding,
            self.timestamp,
            #[cfg(feature = "unstable")]
            self.source_info,
            #[cfg(feature = "unstable")]
            self.attachment,
        )
    }
}

impl SyncResolve for PublicationBuilder<PublisherBuilder<'_, '_>, PublicationBuilderDelete> {
    #[inline]
    fn res_sync(self) -> <Self as Resolvable>::To {
        let publisher = self.publisher.create_one_shot_publisher()?;
        publisher.resolve_put(
            Payload::empty(),
            SampleKind::Delete,
            Encoding::ZENOH_BYTES,
            self.timestamp,
            #[cfg(feature = "unstable")]
            self.source_info,
            #[cfg(feature = "unstable")]
            self.attachment,
        )
    }
}

impl AsyncResolve for PublicationBuilder<PublisherBuilder<'_, '_>, PublicationBuilderPut> {
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}

impl AsyncResolve for PublicationBuilder<PublisherBuilder<'_, '_>, PublicationBuilderDelete> {
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}

/// A builder for initializing a [`Publisher`].
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh::prelude::r#async::*;
/// use zenoh::publication::CongestionControl;
/// use zenoh::sample::builder::QoSBuilderTrait;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// let publisher = session
///     .declare_publisher("key/expression")
///     .congestion_control(CongestionControl::Block)
///     .res()
///     .await
///     .unwrap();
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
#[derive(Debug)]
pub struct PublisherBuilder<'a, 'b: 'a> {
    pub(crate) session: SessionRef<'a>,
    pub(crate) key_expr: ZResult<KeyExpr<'b>>,
    pub(crate) congestion_control: CongestionControl,
    pub(crate) priority: Priority,
    pub(crate) is_express: bool,
    pub(crate) destination: Locality,
}

impl<'a, 'b> Clone for PublisherBuilder<'a, 'b> {
    fn clone(&self) -> Self {
        Self {
            session: self.session.clone(),
            key_expr: match &self.key_expr {
                Ok(k) => Ok(k.clone()),
                Err(e) => Err(zerror!("Cloned KE Error: {}", e).into()),
            },
            congestion_control: self.congestion_control,
            priority: self.priority,
            is_express: self.is_express,
            destination: self.destination,
        }
    }
}

impl QoSBuilderTrait for PublisherBuilder<'_, '_> {
    /// Change the `congestion_control` to apply when routing the data.
    #[inline]
    fn congestion_control(self, congestion_control: CongestionControl) -> Self {
        Self {
            congestion_control,
            ..self
        }
    }

    /// Change the priority of the written data.
    #[inline]
    fn priority(self, priority: Priority) -> Self {
        Self { priority, ..self }
    }

    /// Change the `express` policy to apply when routing the data.
    /// When express is set to `true`, then the message will not be batched.
    /// This usually has a positive impact on latency but negative impact on throughput.
    #[inline]
    fn express(self, is_express: bool) -> Self {
        Self { is_express, ..self }
    }
}

impl<'a, 'b> PublisherBuilder<'a, 'b> {
    /// Restrict the matching subscribers that will receive the published data
    /// to the ones that have the given [`Locality`](crate::prelude::Locality).
    #[zenoh_macros::unstable]
    #[inline]
    pub fn allowed_destination(mut self, destination: Locality) -> Self {
        self.destination = destination;
        self
    }

    // internal function for perfroming the publication
    fn create_one_shot_publisher(self) -> ZResult<Publisher<'a>> {
        Ok(Publisher {
            session: self.session,
            #[cfg(feature = "unstable")]
            eid: 0, // This is a one shot Publisher
            key_expr: self.key_expr?,
            congestion_control: self.congestion_control,
            priority: self.priority,
            is_express: self.is_express,
            destination: self.destination,
        })
    }
}

impl<'a, 'b> Resolvable for PublisherBuilder<'a, 'b> {
    type To = ZResult<Publisher<'a>>;
}

impl<'a, 'b> SyncResolve for PublisherBuilder<'a, 'b> {
    fn res_sync(self) -> <Self as Resolvable>::To {
        let mut key_expr = self.key_expr?;
        if !key_expr.is_fully_optimized(&self.session) {
            let session_id = self.session.id;
            let expr_id = self.session.declare_prefix(key_expr.as_str()).res_sync();
            let prefix_len = key_expr
                .len()
                .try_into()
                .expect("How did you get a key expression with a length over 2^32!?");
            key_expr = match key_expr.0 {
                crate::api::key_expr::KeyExprInner::Borrowed(key_expr)
                | crate::api::key_expr::KeyExprInner::BorrowedWire { key_expr, .. } => {
                    KeyExpr(crate::api::key_expr::KeyExprInner::BorrowedWire {
                        key_expr,
                        expr_id,
                        mapping: Mapping::Sender,
                        prefix_len,
                        session_id,
                    })
                }
                crate::api::key_expr::KeyExprInner::Owned(key_expr)
                | crate::api::key_expr::KeyExprInner::Wire { key_expr, .. } => {
                    KeyExpr(crate::api::key_expr::KeyExprInner::Wire {
                        key_expr,
                        expr_id,
                        mapping: Mapping::Sender,
                        prefix_len,
                        session_id,
                    })
                }
            }
        }
        self.session
            .declare_publication_intent(key_expr.clone())
            .res_sync()?;
        #[cfg(feature = "unstable")]
        let eid = self.session.runtime.next_id();
        let publisher = Publisher {
            session: self.session,
            #[cfg(feature = "unstable")]
            eid,
            key_expr,
            congestion_control: self.congestion_control,
            priority: self.priority,
            is_express: self.is_express,
            destination: self.destination,
        };
        log::trace!("publish({:?})", publisher.key_expr);
        Ok(publisher)
    }
}

impl<'a, 'b> AsyncResolve for PublisherBuilder<'a, 'b> {
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}

impl SyncResolve for PublicationBuilder<&Publisher<'_>, PublicationBuilderPut> {
    fn res_sync(self) -> <Self as Resolvable>::To {
        self.publisher.resolve_put(
            self.kind.payload,
            SampleKind::Put,
            self.kind.encoding,
            self.timestamp,
            #[cfg(feature = "unstable")]
            self.source_info,
            #[cfg(feature = "unstable")]
            self.attachment,
        )
    }
}

impl SyncResolve for PublicationBuilder<&Publisher<'_>, PublicationBuilderDelete> {
    fn res_sync(self) -> <Self as Resolvable>::To {
        self.publisher.resolve_put(
            Payload::empty(),
            SampleKind::Delete,
            Encoding::ZENOH_BYTES,
            self.timestamp,
            #[cfg(feature = "unstable")]
            self.source_info,
            #[cfg(feature = "unstable")]
            self.attachment,
        )
    }
}

impl AsyncResolve for PublicationBuilder<&Publisher<'_>, PublicationBuilderPut> {
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}

impl AsyncResolve for PublicationBuilder<&Publisher<'_>, PublicationBuilderDelete> {
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}
