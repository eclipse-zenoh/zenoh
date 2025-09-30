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

use itertools::Itertools;
use zenoh_config::qos::PublisherQoSConfig;
use zenoh_core::{Resolvable, Result as ZResult, Wait};
use zenoh_keyexpr::keyexpr_tree::{IKeyExprTree, IKeyExprTreeNode};
use zenoh_protocol::core::CongestionControl;
#[cfg(feature = "unstable")]
use zenoh_protocol::core::Reliability;

#[cfg(feature = "unstable")]
use crate::api::sample::SourceInfo;
use crate::{
    api::{
        builders::sample::{
            EncodingBuilderTrait, QoSBuilderTrait, SampleBuilderTrait, TimestampBuilderTrait,
        },
        bytes::{OptionZBytes, ZBytes},
        encoding::Encoding,
        key_expr::KeyExpr,
        publisher::{Priority, Publisher},
        sample::{Locality, SampleKind},
    },
    Session,
};

/// The alias for [`PublicationBuilder`]
/// returned by the [`Session::put`](crate::session::Session::put) method.
pub type SessionPutBuilder<'a, 'b> =
    PublicationBuilder<PublisherBuilder<'a, 'b>, PublicationBuilderPut>;

/// The alias for [`PublicationBuilder`]
/// returned by the [`Session::delete`](crate::session::Session::delete) method.
pub type SessionDeleteBuilder<'a, 'b> =
    PublicationBuilder<PublisherBuilder<'a, 'b>, PublicationBuilderDelete>;

/// The alias for [`PublicationBuilder`]
/// returned by the [`Publisher::put`](crate::pubsub::Publisher::put) method.
pub type PublisherPutBuilder<'a> = PublicationBuilder<&'a Publisher<'a>, PublicationBuilderPut>;

/// The alias for [`PublicationBuilder`]
/// returned by the [`Publisher::delete`](crate::pubsub::Publisher::delete) method.
pub type PublisherDeleteBuilder<'a> =
    PublicationBuilder<&'a Publisher<'a>, PublicationBuilderDelete>;

/// The type-modifier for a [`PublicationBuilder`] for a `Put` operation.
///
/// Makes the publication builder make a sample of a [`kind`](crate::sample::Sample::kind) [`SampleKind::Put`].
#[derive(Debug, Clone)]
pub struct PublicationBuilderPut {
    pub(crate) payload: ZBytes,
    pub(crate) encoding: Encoding,
}

/// The type-modifier for a [`PublicationBuilder`] for a `Delete` operation.
///
/// Makes the publication builder make a sample of a [`kind`](crate::sample::Sample::kind) [`SampleKind::Delete`].
#[derive(Debug, Clone)]
pub struct PublicationBuilderDelete;

/// Publication builder
///
/// A publication builder object is returned by the following methods:
/// - [`Session::put`](crate::session::Session::put)
/// - [`Session::delete`](crate::session::Session::delete)
/// - [`Publisher::put`](crate::pubsub::Publisher::put)
/// - [`Publisher::delete`](crate::pubsub::Publisher::delete)
///
/// It resolves to `ZResult<()>` when awaited or when calling `.wait()`.
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh::{bytes::Encoding, qos::CongestionControl};
///
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// session
///     .put("key/expression", "payload")
///     .encoding(Encoding::TEXT_PLAIN)
///     .congestion_control(CongestionControl::Block)
///     .await
///     .unwrap();
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
#[derive(Debug, Clone)]
pub struct PublicationBuilder<P, T> {
    pub(crate) publisher: P,
    pub(crate) kind: T,
    pub(crate) timestamp: Option<uhlc::Timestamp>,
    #[cfg(feature = "unstable")]
    pub(crate) source_info: SourceInfo,
    pub(crate) attachment: Option<ZBytes>,
}

#[zenoh_macros::internal_trait]
impl<T> QoSBuilderTrait for PublicationBuilder<PublisherBuilder<'_, '_>, T> {
    /// Changes the [`CongestionControl`](crate::qos::CongestionControl) to apply when routing the data.
    #[inline]
    fn congestion_control(self, congestion_control: CongestionControl) -> Self {
        Self {
            publisher: self.publisher.congestion_control(congestion_control),
            ..self
        }
    }

    /// Changes the [`Priority`](crate::qos::Priority) of the written data.
    #[inline]
    fn priority(self, priority: Priority) -> Self {
        Self {
            publisher: self.publisher.priority(priority),
            ..self
        }
    }

    /// Changes the Express policy to apply when routing the data.
    ///
    /// When express is set to `true`, then the message will not be batched.
    /// This usually has a positive impact on latency but a negative impact on throughput.
    #[inline]
    fn express(self, is_express: bool) -> Self {
        Self {
            publisher: self.publisher.express(is_express),
            ..self
        }
    }
}

impl<T> PublicationBuilder<PublisherBuilder<'_, '_>, T> {
    /// Changes the [`Locality`](crate::sample::Locality) applied when routing the data.
    ///
    /// This restricts the matching subscribers that will receive the published data to the ones
    /// that have the given [`Locality`](crate::sample::Locality).
    #[zenoh_macros::unstable]
    #[inline]
    pub fn allowed_destination(mut self, destination: Locality) -> Self {
        self.publisher = self.publisher.allowed_destination(destination);
        self
    }

    /// Changes the [`Reliability`](crate::qos::Reliability) to apply when routing the data.
    ///
    /// **NOTE**: Currently `reliability` does not trigger any data retransmission on the wire. It
    ///   is rather used as a marker on the wire and it may be used to select the best link
    ///   available (e.g. TCP for reliable data and UDP for best effort data).
    #[zenoh_macros::unstable]
    #[inline]
    pub fn reliability(self, reliability: Reliability) -> Self {
        Self {
            publisher: self.publisher.reliability(reliability),
            ..self
        }
    }
}

#[zenoh_macros::internal_trait]
impl EncodingBuilderTrait for PublisherBuilder<'_, '_> {
    /// Sets the default [`Encoding`](crate::bytes::Encoding) of the payload generated by this publisher.
    fn encoding<T: Into<Encoding>>(self, encoding: T) -> Self {
        Self {
            encoding: encoding.into(),
            ..self
        }
    }
}

#[zenoh_macros::internal_trait]
impl<P> EncodingBuilderTrait for PublicationBuilder<P, PublicationBuilderPut> {
    /// Sets the [`Encoding`](crate::bytes::Encoding) of the payload of this publication.
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

#[zenoh_macros::internal_trait]
impl<P, T> SampleBuilderTrait for PublicationBuilder<P, T> {
    /// Sets an optional [`SourceInfo`](crate::sample::SourceInfo) to be sent along with the publication.
    #[zenoh_macros::unstable]
    fn source_info(self, source_info: SourceInfo) -> Self {
        Self {
            source_info,
            ..self
        }
    }
    /// Sets an optional attachment to be sent along with the publication.
    /// The method accepts both `Into<ZBytes>` and `Option<Into<ZBytes>>`.
    fn attachment<TA: Into<OptionZBytes>>(self, attachment: TA) -> Self {
        let attachment: OptionZBytes = attachment.into();
        Self {
            attachment: attachment.into(),
            ..self
        }
    }
}

#[zenoh_macros::internal_trait]
impl<P, T> TimestampBuilderTrait for PublicationBuilder<P, T> {
    /// Sets an optional timestamp to be sent along with the publication.
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
    fn wait(mut self) -> <Self as Resolvable>::To {
        self.publisher = self.publisher.apply_qos_overwrites();
        self.publisher.session.0.resolve_put(
            &self.publisher.key_expr?,
            self.kind.payload,
            SampleKind::Put,
            self.kind.encoding,
            self.publisher.congestion_control,
            self.publisher.priority,
            self.publisher.is_express,
            self.publisher.destination,
            #[cfg(feature = "unstable")]
            self.publisher.reliability,
            self.timestamp,
            #[cfg(feature = "unstable")]
            self.source_info,
            self.attachment,
        )
    }
}

impl Wait for PublicationBuilder<PublisherBuilder<'_, '_>, PublicationBuilderDelete> {
    #[inline]
    fn wait(mut self) -> <Self as Resolvable>::To {
        self.publisher = self.publisher.apply_qos_overwrites();
        self.publisher.session.0.resolve_put(
            &self.publisher.key_expr?,
            ZBytes::new(),
            SampleKind::Delete,
            Encoding::ZENOH_BYTES,
            self.publisher.congestion_control,
            self.publisher.priority,
            self.publisher.is_express,
            self.publisher.destination,
            #[cfg(feature = "unstable")]
            self.publisher.reliability,
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
/// Returned by the
/// [`Session::declare_publisher`](crate::Session::declare_publisher) method.
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh::qos::CongestionControl;
///
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let publisher = session
///     .declare_publisher("key/expression")
///     .congestion_control(CongestionControl::Block)
///     .await
///     .unwrap();
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
#[derive(Debug)]
pub struct PublisherBuilder<'a, 'b> {
    #[cfg(feature = "internal")]
    pub session: &'a Session,
    #[cfg(not(feature = "internal"))]
    pub(crate) session: &'a Session,

    #[cfg(feature = "internal")]
    pub key_expr: ZResult<KeyExpr<'b>>,
    #[cfg(not(feature = "internal"))]
    pub(crate) key_expr: ZResult<KeyExpr<'b>>,

    #[cfg(feature = "internal")]
    pub encoding: Encoding,
    #[cfg(not(feature = "internal"))]
    pub(crate) encoding: Encoding,
    #[cfg(feature = "internal")]
    pub congestion_control: CongestionControl,
    #[cfg(not(feature = "internal"))]
    pub(crate) congestion_control: CongestionControl,
    #[cfg(feature = "internal")]
    pub priority: Priority,
    #[cfg(not(feature = "internal"))]
    pub(crate) priority: Priority,
    #[cfg(feature = "internal")]
    pub is_express: bool,
    #[cfg(not(feature = "internal"))]
    pub(crate) is_express: bool,
    #[cfg(feature = "internal")]
    #[cfg(feature = "unstable")]
    pub reliability: Reliability,
    #[cfg(not(feature = "internal"))]
    #[cfg(feature = "unstable")]
    pub(crate) reliability: Reliability,
    #[cfg(feature = "internal")]
    pub destination: Locality,
    #[cfg(not(feature = "internal"))]
    pub(crate) destination: Locality,
}

impl Clone for PublisherBuilder<'_, '_> {
    fn clone(&self) -> Self {
        Self {
            session: self.session,
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

#[zenoh_macros::internal_trait]
impl QoSBuilderTrait for PublisherBuilder<'_, '_> {
    /// Changes the [`CongestionControl`](crate::qos::CongestionControl) to apply when routing the data.
    #[inline]
    fn congestion_control(self, congestion_control: CongestionControl) -> Self {
        Self {
            congestion_control,
            ..self
        }
    }

    /// Changes the [`Priority`](crate::qos::Priority) of the written data.
    #[inline]
    fn priority(self, priority: Priority) -> Self {
        Self { priority, ..self }
    }

    /// Changes the Express policy to apply when routing the data.
    ///
    /// When express is set to `true`, then the message will not be batched.
    /// This usually has a positive impact on latency but a negative impact on throughput.
    #[inline]
    fn express(self, is_express: bool) -> Self {
        Self { is_express, ..self }
    }
}

impl PublisherBuilder<'_, '_> {
    /// Looks up whether any configured QoS overwrites apply to the builder's key expression.
    /// Returns a new builder with the overwritten QoS parameters.
    pub(crate) fn apply_qos_overwrites(self) -> Self {
        let mut qos_overwrites = PublisherQoSConfig::default();
        if let Ok(key_expr) = &self.key_expr {
            // get overwritten builder
            let state = zread!(self.session.0.state);
            let nodes_including = state
                .publisher_qos_tree
                .nodes_including(key_expr)
                .filter(|n| n.weight().is_some())
                .collect_vec();
            if let Some(node) = nodes_including.first() {
                qos_overwrites = node
                    .weight()
                    .expect("first node weight should not be None")
                    .clone();
                if nodes_including.len() > 1 {
                    tracing::warn!(
                        "Publisher declared on `{}` which is included by multiple key_exprs in qos config ({}). Using qos config for `{}`",
                        key_expr,
                        nodes_including.iter().map(|n| n.keyexpr().to_string()).join(", "),
                        node.keyexpr(),
                    );
                }
            }
        }

        Self {
            congestion_control: qos_overwrites
                .congestion_control
                .map(|cc| cc.into())
                .unwrap_or(self.congestion_control),
            priority: qos_overwrites
                .priority
                .map(|p| p.into())
                .unwrap_or(self.priority),
            is_express: qos_overwrites.express.unwrap_or(self.is_express),
            #[cfg(feature = "unstable")]
            reliability: qos_overwrites
                .reliability
                .map(|r| r.into())
                .unwrap_or(self.reliability),
            #[cfg(feature = "unstable")]
            destination: qos_overwrites
                .allowed_destination
                .map(|d| d.into())
                .unwrap_or(self.destination),
            ..self
        }
    }

    /// Changes the [`Locality`](crate::sample::Locality) applied when routing the data.
    ///
    /// This restricts the matching subscribers that will receive the published data to the ones
    /// that have the given [`Locality`](crate::sample::Locality).
    #[inline]
    pub fn allowed_destination(mut self, destination: Locality) -> Self {
        self.destination = destination;
        self
    }

    /// Changes the [`Reliability`](crate::qos::Reliability) to apply when routing the data.
    ///
    /// **NOTE**: Currently `reliability` does not trigger any data retransmission on the wire. It
    ///   is rather used as a marker on the wire and it may be used to select the best link
    ///   available (e.g. TCP for reliable data and UDP for best effort data).
    #[zenoh_macros::unstable]
    #[inline]
    pub fn reliability(self, reliability: Reliability) -> Self {
        Self {
            reliability,
            ..self
        }
    }
}

impl<'b> Resolvable for PublisherBuilder<'_, 'b> {
    type To = ZResult<Publisher<'b>>;
}

impl Wait for PublisherBuilder<'_, '_> {
    fn wait(mut self) -> <Self as Resolvable>::To {
        self = self.apply_qos_overwrites();
        let mut key_expr = self.key_expr?;
        if !key_expr.is_fully_optimized(&self.session.0) {
            key_expr = self.session.declare_keyexpr(key_expr).wait()?;
        }
        let id = self
            .session
            .0
            .declare_publisher_inner(key_expr.clone(), self.destination)?;
        Ok(Publisher {
            session: self.session.downgrade(),
            id,
            key_expr,
            encoding: self.encoding,
            congestion_control: self.congestion_control,
            priority: self.priority,
            is_express: self.is_express,
            destination: self.destination,
            #[cfg(feature = "unstable")]
            reliability: self.reliability,
            matching_listeners: Default::default(),
            undeclare_on_drop: true,
        })
    }
}

impl IntoFuture for PublisherBuilder<'_, '_> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

impl Wait for PublicationBuilder<&Publisher<'_>, PublicationBuilderPut> {
    fn wait(self) -> <Self as Resolvable>::To {
        self.publisher.session.resolve_put(
            &self.publisher.key_expr,
            self.kind.payload,
            SampleKind::Put,
            self.kind.encoding,
            self.publisher.congestion_control,
            self.publisher.priority,
            self.publisher.is_express,
            self.publisher.destination,
            #[cfg(feature = "unstable")]
            self.publisher.reliability,
            self.timestamp,
            #[cfg(feature = "unstable")]
            self.source_info,
            self.attachment,
        )
    }
}

impl Wait for PublicationBuilder<&Publisher<'_>, PublicationBuilderDelete> {
    fn wait(self) -> <Self as Resolvable>::To {
        self.publisher.session.resolve_put(
            &self.publisher.key_expr,
            ZBytes::new(),
            SampleKind::Delete,
            Encoding::ZENOH_BYTES,
            self.publisher.congestion_control,
            self.publisher.priority,
            self.publisher.is_express,
            self.publisher.destination,
            #[cfg(feature = "unstable")]
            self.publisher.reliability,
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
