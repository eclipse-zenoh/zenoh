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

use zenoh_core::{Resolvable, Result as ZResult, Wait};
#[cfg(feature = "unstable")]
use zenoh_protocol::core::Reliability;
use zenoh_protocol::{core::CongestionControl, network::Mapping};

#[cfg(feature = "unstable")]
use crate::api::sample::SourceInfo;
use crate::api::{
    builders::sample::{
        EncodingBuilderTrait, QoSBuilderTrait, SampleBuilderTrait, TimestampBuilderTrait,
    },
    bytes::{OptionZBytes, ZBytes},
    encoding::Encoding,
    key_expr::KeyExpr,
    publisher::{Priority, Publisher},
    sample::{Locality, SampleKind},
    session::SessionRef,
};

pub type SessionPutBuilder<'a, 'b> =
    PublicationBuilder<PublisherBuilder<'a, 'b>, PublicationBuilderPut>;

pub type SessionDeleteBuilder<'a, 'b> =
    PublicationBuilder<PublisherBuilder<'a, 'b>, PublicationBuilderDelete>;

pub type PublisherPutBuilder<'a> = PublicationBuilder<&'a Publisher<'a>, PublicationBuilderPut>;

pub type PublisherDeleteBuilder<'a> =
    PublicationBuilder<&'a Publisher<'a>, PublicationBuilderDelete>;

#[derive(Debug, Clone)]
pub struct PublicationBuilderPut {
    pub(crate) payload: ZBytes,
    pub(crate) encoding: Encoding,
}
#[derive(Debug, Clone)]
pub struct PublicationBuilderDelete;

/// A builder for initializing  [`Session::put`](crate::session::Session::put), [`Session::delete`](crate::session::Session::delete),
/// [`Publisher::put`](crate::pubsub::Publisher::put), and [`Publisher::delete`](crate::pubsub::Publisher::delete) operations.
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh::{bytes::Encoding, prelude::*, qos::CongestionControl};
///
/// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
/// session
///     .put("key/expression", "payload")
///     .encoding(Encoding::TEXT_PLAIN)
///     .congestion_control(CongestionControl::Block)
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
    pub(crate) attachment: Option<ZBytes>,
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
    /// Change the `reliability` to apply when routing the data.
    /// NOTE: Currently `reliability` does not trigger any data retransmission on the wire. 
    ///             It is rather used as a marker on the wire and it may be used to select the best link available (e.g. TCP for reliable data and UDP for best effort data).
    #[zenoh_macros::unstable]
    #[inline]
    pub fn reliability(self, reliability: Reliability) -> Self {
        Self {
            publisher: self.publisher.reliability(reliability),
            ..self
        }
    }
}

impl EncodingBuilderTrait for PublisherBuilder<'_, '_> {
    fn encoding<T: Into<Encoding>>(self, encoding: T) -> Self {
        Self {
            encoding: encoding.into(),
            ..self
        }
    }
}

impl<P> EncodingBuilderTrait for PublicationBuilder<P, PublicationBuilderPut> {
    fn encoding<T: Into<Encoding>>(self, encoding: T) -> Self {
        Self {
            kind: PublicationBuilderPut {
                encoding: encoding.into(),
                ..self.kind
            },
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
    fn attachment<TA: Into<OptionZBytes>>(self, attachment: TA) -> Self {
        let attachment: OptionZBytes = attachment.into();
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

impl Wait for PublicationBuilder<PublisherBuilder<'_, '_>, PublicationBuilderPut> {
    #[inline]
    fn wait(self) -> <Self as Resolvable>::To {
        let publisher = self.publisher.create_one_shot_publisher()?;
        publisher.resolve_put(
            self.kind.payload,
            SampleKind::Put,
            self.kind.encoding,
            self.timestamp,
            #[cfg(feature = "unstable")]
            self.source_info,
            self.attachment,
        )
    }
}

impl Wait for PublicationBuilder<PublisherBuilder<'_, '_>, PublicationBuilderDelete> {
    #[inline]
    fn wait(self) -> <Self as Resolvable>::To {
        let publisher = self.publisher.create_one_shot_publisher()?;
        publisher.resolve_put(
            ZBytes::empty(),
            SampleKind::Delete,
            Encoding::ZENOH_BYTES,
            self.timestamp,
            #[cfg(feature = "unstable")]
            self.source_info,
            self.attachment,
        )
    }
}

impl IntoFuture for PublicationBuilder<PublisherBuilder<'_, '_>, PublicationBuilderPut> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

impl IntoFuture for PublicationBuilder<PublisherBuilder<'_, '_>, PublicationBuilderDelete> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

/// A builder for initializing a [`Publisher`].
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh::{prelude::*, qos::CongestionControl};
///
/// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
/// let publisher = session
///     .declare_publisher("key/expression")
///     .congestion_control(CongestionControl::Block)
///     .await
///     .unwrap();
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
#[derive(Debug)]
pub struct PublisherBuilder<'a, 'b: 'a> {
    pub(crate) session: SessionRef<'a>,
    pub(crate) key_expr: ZResult<KeyExpr<'b>>,
    pub(crate) encoding: Encoding,
    pub(crate) congestion_control: CongestionControl,
    pub(crate) priority: Priority,
    pub(crate) is_express: bool,
    #[cfg(feature = "unstable")]
    pub(crate) reliability: Reliability,
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
            encoding: self.encoding.clone(),
            congestion_control: self.congestion_control,
            priority: self.priority,
            is_express: self.is_express,
            #[cfg(feature = "unstable")]
            reliability: self.reliability,
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

    /// Change the `reliability`` to apply when routing the data.
    /// NOTE: Currently `reliability` does not trigger any data retransmission on the wire. 
    ///             It is rather used as a marker on the wire and it may be used to select the best link available (e.g. TCP for reliable data and UDP for best effort data).
    #[zenoh_macros::unstable]
    #[inline]
    pub fn reliability(self, reliability: Reliability) -> Self {
        Self {
            reliability,
            ..self
        }
    }

    // internal function for performing the publication
    fn create_one_shot_publisher(self) -> ZResult<Publisher<'a>> {
        Ok(Publisher {
            session: self.session,
            id: 0, // This is a one shot Publisher
            key_expr: self.key_expr?,
            encoding: self.encoding,
            congestion_control: self.congestion_control,
            priority: self.priority,
            is_express: self.is_express,
            destination: self.destination,
            #[cfg(feature = "unstable")]
            reliability: self.reliability,
            #[cfg(feature = "unstable")]
            matching_listeners: Default::default(),
            undeclare_on_drop: true,
        })
    }
}

impl<'a, 'b> Resolvable for PublisherBuilder<'a, 'b> {
    type To = ZResult<Publisher<'a>>;
}

impl<'a, 'b> Wait for PublisherBuilder<'a, 'b> {
    fn wait(self) -> <Self as Resolvable>::To {
        let mut key_expr = self.key_expr?;
        if !key_expr.is_fully_optimized(&self.session) {
            let session_id = self.session.id;
            let expr_id = self.session.declare_prefix(key_expr.as_str()).wait();
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
            .declare_publisher_inner(key_expr.clone(), self.destination)
            .map(|id| Publisher {
                session: self.session,
                id,
                key_expr,
                encoding: self.encoding,
                congestion_control: self.congestion_control,
                priority: self.priority,
                is_express: self.is_express,
                destination: self.destination,
                #[cfg(feature = "unstable")]
                reliability: self.reliability,
                #[cfg(feature = "unstable")]
                matching_listeners: Default::default(),
                undeclare_on_drop: true,
            })
    }
}

impl<'a, 'b> IntoFuture for PublisherBuilder<'a, 'b> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

impl Wait for PublicationBuilder<&Publisher<'_>, PublicationBuilderPut> {
    fn wait(self) -> <Self as Resolvable>::To {
        self.publisher.resolve_put(
            self.kind.payload,
            SampleKind::Put,
            self.kind.encoding,
            self.timestamp,
            #[cfg(feature = "unstable")]
            self.source_info,
            self.attachment,
        )
    }
}

impl Wait for PublicationBuilder<&Publisher<'_>, PublicationBuilderDelete> {
    fn wait(self) -> <Self as Resolvable>::To {
        self.publisher.resolve_put(
            ZBytes::empty(),
            SampleKind::Delete,
            Encoding::ZENOH_BYTES,
            self.timestamp,
            #[cfg(feature = "unstable")]
            self.source_info,
            self.attachment,
        )
    }
}

impl IntoFuture for PublicationBuilder<&Publisher<'_>, PublicationBuilderPut> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

impl IntoFuture for PublicationBuilder<&Publisher<'_>, PublicationBuilderDelete> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}
