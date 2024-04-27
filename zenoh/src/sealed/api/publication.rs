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

use super::{
    builders::publication::{
        PublicationBuilder, PublicationBuilderDelete, PublicationBuilderPut,
        PublisherDeleteBuilder, PublisherPutBuilder,
    },
    bytes::ZBytes,
    encoding::Encoding,
    key_expr::KeyExpr,
    sample::{DataInfo, Locality, QoS, Sample, SampleFields, SampleKind},
    session::{SessionRef, Undeclarable},
};
use crate::sealed::net::primitives::Primitives;
use futures::Sink;
use std::{
    convert::TryFrom,
    future::Ready,
    pin::Pin,
    task::{Context, Poll},
};
use zenoh_core::{zread, AsyncResolve, Resolvable, Resolve, SyncResolve};
use zenoh_keyexpr::keyexpr;
use zenoh_protocol::{
    core::CongestionControl,
    network::{push::ext, Push},
    zenoh::{Del, PushBody, Put},
};
use zenoh_result::{Error, ZResult};

#[zenoh_macros::unstable]
use {
    super::handlers::{callback::locked, callback::Callback, DefaultHandler, IntoHandler},
    super::sample::SourceInfo,
    super::Id,
    zenoh_protocol::core::EntityGlobalId,
    zenoh_protocol::core::EntityId,
};

#[zenoh_macros::unstable]
#[derive(Clone)]
pub enum PublisherRef<'a> {
    Borrow(&'a Publisher<'a>),
    Shared(std::sync::Arc<Publisher<'static>>),
}

#[zenoh_macros::unstable]
impl<'a> std::ops::Deref for PublisherRef<'a> {
    type Target = Publisher<'a>;

    fn deref(&self) -> &Self::Target {
        match self {
            PublisherRef::Borrow(b) => b,
            PublisherRef::Shared(s) => s,
        }
    }
}

#[zenoh_macros::unstable]
impl std::fmt::Debug for PublisherRef<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PublisherRef::Borrow(b) => Publisher::fmt(b, f),
            PublisherRef::Shared(s) => Publisher::fmt(s, f),
        }
    }
}

/// A publisher that allows to send data through a stream.
///
/// Publishers are automatically undeclared when dropped.
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh::prelude::r#async::*;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap().into_arc();
/// let publisher = session.declare_publisher("key/expression").res().await.unwrap();
/// publisher.put("value").res().await.unwrap();
/// # }
/// ```
///
///
/// `Publisher` implements the `Sink` trait which is useful to forward
/// streams to zenoh.
/// ```no_run
/// # #[tokio::main]
/// # async fn main() {
/// use futures::StreamExt;
/// use zenoh::prelude::r#async::*;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap().into_arc();
/// let mut subscriber = session.declare_subscriber("key/expression").res().await.unwrap();
/// let publisher = session.declare_publisher("another/key/expression").res().await.unwrap();
/// subscriber.stream().map(Ok).forward(publisher).await.unwrap();
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct Publisher<'a> {
    pub(in crate::sealed) session: SessionRef<'a>,
    #[cfg(feature = "unstable")]
    pub(in crate::sealed) eid: EntityId,
    pub(in crate::sealed) key_expr: KeyExpr<'a>,
    pub(in crate::sealed) congestion_control: CongestionControl,
    pub(in crate::sealed) priority: Priority,
    pub(in crate::sealed) is_express: bool,
    pub(in crate::sealed) destination: Locality,
}

impl<'a> Publisher<'a> {
    /// Returns the [`EntityGlobalId`] of this Publisher.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let publisher = session.declare_publisher("key/expression")
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// let publisher_id = publisher.id();
    /// # }
    /// ```
    #[zenoh_macros::unstable]
    pub fn id(&self) -> EntityGlobalId {
        EntityGlobalId {
            zid: self.session.zid(),
            eid: self.eid,
        }
    }

    pub fn key_expr(&self) -> &KeyExpr<'a> {
        &self.key_expr
    }

    /// Change the `congestion_control` to apply when routing the data.
    #[inline]
    pub fn set_congestion_control(&mut self, congestion_control: CongestionControl) {
        self.congestion_control = congestion_control;
    }

    /// Change the priority of the written data.
    #[inline]
    pub fn set_priority(&mut self, priority: Priority) {
        self.priority = priority;
    }

    /// Restrict the matching subscribers that will receive the published data
    /// to the ones that have the given [`Locality`](crate::prelude::Locality).
    #[zenoh_macros::unstable]
    #[inline]
    pub fn set_allowed_destination(&mut self, destination: Locality) {
        self.destination = destination;
    }

    /// Consumes the given `Publisher`, returning a thread-safe reference-counting
    /// pointer to it (`Arc<Publisher>`). This is equivalent to `Arc::new(Publisher)`.
    ///
    /// This is useful to share ownership of the `Publisher` between several threads
    /// and tasks. It also alows to create [`MatchingListener`] with static
    /// lifetime that can be moved to several threads and tasks.
    ///
    /// Note: the given zenoh `Publisher` will be undeclared when the last reference to
    /// it is dropped.
    ///
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap().into_arc();
    /// let publisher = session.declare_publisher("key/expression").res().await.unwrap().into_arc();
    /// let matching_listener = publisher.matching_listener().res().await.unwrap();
    ///
    /// tokio::task::spawn(async move {
    ///     while let Ok(matching_status) = matching_listener.recv_async().await {
    ///         if matching_status.matching_subscribers() {
    ///             println!("Publisher has matching subscribers.");
    ///         } else {
    ///             println!("Publisher has NO MORE matching subscribers.");
    ///         }
    ///     }
    /// }).await;
    /// # }
    /// ```
    #[zenoh_macros::unstable]
    pub fn into_arc(self) -> std::sync::Arc<Self> {
        std::sync::Arc::new(self)
    }

    /// Put data.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap().into_arc();
    /// let publisher = session.declare_publisher("key/expression").res().await.unwrap();
    /// publisher.put("value").res().await.unwrap();
    /// # }
    /// ```
    #[inline]
    pub fn put<IntoZBytes>(&self, payload: IntoZBytes) -> PublisherPutBuilder<'_>
    where
        IntoZBytes: Into<ZBytes>,
    {
        PublicationBuilder {
            publisher: self,
            kind: PublicationBuilderPut {
                payload: payload.into(),
                encoding: Encoding::ZENOH_BYTES,
            },
            timestamp: None,
            #[cfg(feature = "unstable")]
            source_info: SourceInfo::empty(),
            #[cfg(feature = "unstable")]
            attachment: None,
        }
    }

    /// Delete data.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap().into_arc();
    /// let publisher = session.declare_publisher("key/expression").res().await.unwrap();
    /// publisher.delete().res().await.unwrap();
    /// # }
    /// ```
    pub fn delete(&self) -> PublisherDeleteBuilder<'_> {
        PublicationBuilder {
            publisher: self,
            kind: PublicationBuilderDelete,
            timestamp: None,
            #[cfg(feature = "unstable")]
            source_info: SourceInfo::empty(),
            #[cfg(feature = "unstable")]
            attachment: None,
        }
    }

    /// Return the [`MatchingStatus`] of the publisher.
    ///
    /// [`MatchingStatus::matching_subscribers`] will return true if there exist Subscribers
    /// matching the Publisher's key expression and false otherwise.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap().into_arc();
    /// let publisher = session.declare_publisher("key/expression").res().await.unwrap();
    /// let matching_subscribers: bool = publisher
    ///     .matching_status()
    ///     .res()
    ///     .await
    ///     .unwrap()
    ///     .matching_subscribers();
    /// # }
    /// ```
    #[zenoh_macros::unstable]
    pub fn matching_status(&self) -> impl Resolve<ZResult<MatchingStatus>> + '_ {
        zenoh_core::ResolveFuture::new(async move {
            self.session
                .matching_status(self.key_expr(), self.destination)
        })
    }

    /// Return a [`MatchingListener`] for this Publisher.
    ///
    /// The [`MatchingListener`] that will send a notification each time the [`MatchingStatus`] of
    /// the Publisher changes.
    ///
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let publisher = session.declare_publisher("key/expression").res().await.unwrap();
    /// let matching_listener = publisher.matching_listener().res().await.unwrap();
    /// while let Ok(matching_status) = matching_listener.recv_async().await {
    ///     if matching_status.matching_subscribers() {
    ///         println!("Publisher has matching subscribers.");
    ///     } else {
    ///         println!("Publisher has NO MORE matching subscribers.");
    ///     }
    /// }
    /// # }
    /// ```
    #[zenoh_macros::unstable]
    pub fn matching_listener(&self) -> MatchingListenerBuilder<'_, DefaultHandler> {
        MatchingListenerBuilder {
            publisher: PublisherRef::Borrow(self),
            handler: DefaultHandler::default(),
        }
    }

    /// Undeclares the [`Publisher`], informing the network that it needn't optimize publications for its key expression anymore.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let publisher = session.declare_publisher("key/expression").res().await.unwrap();
    /// publisher.undeclare().res().await.unwrap();
    /// # }
    /// ```
    pub fn undeclare(self) -> impl Resolve<ZResult<()>> + 'a {
        Undeclarable::undeclare_inner(self, ())
    }
}

/// Functions to create zenoh entities with `'static` lifetime.
///
/// This trait contains functions to create zenoh entities like
/// [`MatchingListener`] with a `'static` lifetime.
/// This is useful to move zenoh entities to several threads and tasks.
///
/// This trait is implemented for `Arc<Publisher>`.
///
/// # Examples
/// ```no_run
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh::prelude::r#async::*;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap().into_arc();
/// let publisher = session.declare_publisher("key/expression").res().await.unwrap().into_arc();
/// let matching_listener = publisher.matching_listener().res().await.unwrap();
///
/// tokio::task::spawn(async move {
///     while let Ok(matching_status) = matching_listener.recv_async().await {
///         if matching_status.matching_subscribers() {
///             println!("Publisher has matching subscribers.");
///         } else {
///             println!("Publisher has NO MORE matching subscribers.");
///         }
///     }
/// }).await;
/// # }
/// ```
#[zenoh_macros::unstable]
pub trait PublisherDeclarations {
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap().into_arc();
    /// let publisher = session.declare_publisher("key/expression").res().await.unwrap().into_arc();
    /// let matching_listener = publisher.matching_listener().res().await.unwrap();
    ///
    /// tokio::task::spawn(async move {
    ///     while let Ok(matching_status) = matching_listener.recv_async().await {
    ///         if matching_status.matching_subscribers() {
    ///             println!("Publisher has matching subscribers.");
    ///         } else {
    ///             println!("Publisher has NO MORE matching subscribers.");
    ///         }
    ///     }
    /// }).await;
    /// # }
    /// ```
    #[zenoh_macros::unstable]
    fn matching_listener(&self) -> MatchingListenerBuilder<'static, DefaultHandler>;
}

#[zenoh_macros::unstable]
impl PublisherDeclarations for std::sync::Arc<Publisher<'static>> {
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap().into_arc();
    /// let publisher = session.declare_publisher("key/expression").res().await.unwrap().into_arc();
    /// let matching_listener = publisher.matching_listener().res().await.unwrap();
    ///
    /// tokio::task::spawn(async move {
    ///     while let Ok(matching_status) = matching_listener.recv_async().await {
    ///         if matching_status.matching_subscribers() {
    ///             println!("Publisher has matching subscribers.");
    ///         } else {
    ///             println!("Publisher has NO MORE matching subscribers.");
    ///         }
    ///     }
    /// }).await;
    /// # }
    /// ```
    #[zenoh_macros::unstable]
    fn matching_listener(&self) -> MatchingListenerBuilder<'static, DefaultHandler> {
        MatchingListenerBuilder {
            publisher: PublisherRef::Shared(self.clone()),
            handler: DefaultHandler::default(),
        }
    }
}

impl<'a> Undeclarable<(), PublisherUndeclaration<'a>> for Publisher<'a> {
    fn undeclare_inner(self, _: ()) -> PublisherUndeclaration<'a> {
        PublisherUndeclaration { publisher: self }
    }
}

/// A [`Resolvable`] returned when undeclaring a publisher.
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh::prelude::r#async::*;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// let publisher = session.declare_publisher("key/expression").res().await.unwrap();
/// publisher.undeclare().res().await.unwrap();
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
pub struct PublisherUndeclaration<'a> {
    publisher: Publisher<'a>,
}

impl Resolvable for PublisherUndeclaration<'_> {
    type To = ZResult<()>;
}

impl SyncResolve for PublisherUndeclaration<'_> {
    fn res_sync(mut self) -> <Self as Resolvable>::To {
        let Publisher {
            session, key_expr, ..
        } = &self.publisher;
        session
            .undeclare_publication_intent(key_expr.clone())
            .res_sync()?;
        self.publisher.key_expr = unsafe { keyexpr::from_str_unchecked("") }.into();
        Ok(())
    }
}

impl AsyncResolve for PublisherUndeclaration<'_> {
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}

impl Drop for Publisher<'_> {
    fn drop(&mut self) {
        if !self.key_expr.is_empty() {
            let _ = self
                .session
                .undeclare_publication_intent(self.key_expr.clone())
                .res_sync();
        }
    }
}

impl<'a> Sink<Sample> for Publisher<'a> {
    type Error = Error;

    #[inline]
    fn poll_ready(self: Pin<&mut Self>, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    #[inline]
    fn start_send(self: Pin<&mut Self>, item: Sample) -> Result<(), Self::Error> {
        let SampleFields {
            payload,
            kind,
            encoding,
            #[cfg(feature = "unstable")]
            attachment,
            ..
        } = item.into();
        self.resolve_put(
            payload,
            kind,
            encoding,
            None,
            #[cfg(feature = "unstable")]
            SourceInfo::empty(),
            #[cfg(feature = "unstable")]
            attachment,
        )
    }

    #[inline]
    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    #[inline]
    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}

impl Publisher<'_> {
    pub fn resolve_put(
        &self,
        payload: ZBytes,
        kind: SampleKind,
        encoding: Encoding,
        timestamp: Option<uhlc::Timestamp>,
        #[cfg(feature = "unstable")] source_info: SourceInfo,
        #[cfg(feature = "unstable")] attachment: Option<ZBytes>,
    ) -> ZResult<()> {
        tracing::trace!("write({:?}, [...])", &self.key_expr);
        let primitives = zread!(self.session.state)
            .primitives
            .as_ref()
            .unwrap()
            .clone();
        let timestamp = if timestamp.is_none() {
            self.session.runtime.new_timestamp()
        } else {
            timestamp
        };
        if self.destination != Locality::SessionLocal {
            primitives.send_push(Push {
                wire_expr: self.key_expr.to_wire(&self.session).to_owned(),
                ext_qos: ext::QoSType::new(
                    self.priority.into(),
                    self.congestion_control,
                    self.is_express,
                ),
                ext_tstamp: None,
                ext_nodeid: ext::NodeIdType::DEFAULT,
                payload: match kind {
                    SampleKind::Put => {
                        #[allow(unused_mut)]
                        let mut ext_attachment = None;
                        #[cfg(feature = "unstable")]
                        {
                            if let Some(attachment) = attachment.clone() {
                                ext_attachment = Some(attachment.into());
                            }
                        }
                        PushBody::Put(Put {
                            timestamp,
                            encoding: encoding.clone().into(),
                            #[cfg(feature = "unstable")]
                            ext_sinfo: source_info.into(),
                            #[cfg(not(feature = "unstable"))]
                            ext_sinfo: None,
                            #[cfg(feature = "shared-memory")]
                            ext_shm: None,
                            ext_attachment,
                            ext_unknown: vec![],
                            payload: payload.clone().into(),
                        })
                    }
                    SampleKind::Delete => {
                        #[allow(unused_mut)]
                        let mut ext_attachment = None;
                        #[cfg(feature = "unstable")]
                        {
                            if let Some(attachment) = attachment.clone() {
                                ext_attachment = Some(attachment.into());
                            }
                        }
                        PushBody::Del(Del {
                            timestamp,
                            #[cfg(feature = "unstable")]
                            ext_sinfo: source_info.into(),
                            #[cfg(not(feature = "unstable"))]
                            ext_sinfo: None,
                            ext_attachment,
                            ext_unknown: vec![],
                        })
                    }
                },
            });
        }
        if self.destination != Locality::Remote {
            let data_info = DataInfo {
                kind,
                encoding: Some(encoding),
                timestamp,
                source_id: None,
                source_sn: None,
                qos: QoS::from(ext::QoSType::new(
                    self.priority.into(),
                    self.congestion_control,
                    self.is_express,
                )),
            };

            self.session.handle_data(
                true,
                &self.key_expr.to_wire(&self.session),
                Some(data_info),
                payload.into(),
                #[cfg(feature = "unstable")]
                attachment,
            );
        }
        Ok(())
    }
}

/// The Priority of zenoh messages.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum Priority {
    RealTime = 1,
    InteractiveHigh = 2,
    InteractiveLow = 3,
    DataHigh = 4,
    #[default]
    Data = 5,
    DataLow = 6,
    Background = 7,
}

impl Priority {
    /// Default
    pub const DEFAULT: Self = Self::Data;
    /// The lowest Priority
    pub const MIN: Self = Self::Background;
    /// The highest Priority
    pub const MAX: Self = Self::RealTime;
    /// The number of available priorities
    pub const NUM: usize = 1 + Self::MIN as usize - Self::MAX as usize;
}

impl TryFrom<u8> for Priority {
    type Error = zenoh_result::Error;

    /// A Priority is identified by a numeric value.
    /// Lower the value, higher the priority.
    /// Higher the value, lower the priority.
    ///
    /// Admitted values are: 1-7.
    ///
    /// Highest priority: 1
    /// Lowest priority: 7
    fn try_from(priority: u8) -> Result<Self, Self::Error> {
        match priority {
            1 => Ok(Priority::RealTime),
            2 => Ok(Priority::InteractiveHigh),
            3 => Ok(Priority::InteractiveLow),
            4 => Ok(Priority::DataHigh),
            5 => Ok(Priority::Data),
            6 => Ok(Priority::DataLow),
            7 => Ok(Priority::Background),
            unknown => bail!(
                "{} is not a valid priority value. Admitted values are: [{}-{}].",
                unknown,
                Self::MAX as u8,
                Self::MIN as u8
            ),
        }
    }
}

type ProtocolPriority = zenoh_protocol::core::Priority;
impl From<Priority> for ProtocolPriority {
    fn from(prio: Priority) -> Self {
        // The Priority in the prelude differs from the Priority in the core protocol only from
        // the missing Control priority. The Control priority is reserved for zenoh internal use
        // and as such it is not exposed by the zenoh API. Nevertheless, the values of the
        // priorities which are common to the internal and public Priority enums are the same. Therefore,
        // it is possible to safely transmute from the public Priority enum toward the internal
        // Priority enum without risking to be in an invalid state.
        // For better robusteness, the correctness of the unsafe transmute operation is covered
        // by the unit test below.
        unsafe { std::mem::transmute::<Priority, zenoh_protocol::core::Priority>(prio) }
    }
}

impl TryFrom<ProtocolPriority> for Priority {
    type Error = zenoh_result::Error;
    fn try_from(priority: ProtocolPriority) -> Result<Self, Self::Error> {
        match priority {
            ProtocolPriority::Control => bail!("'Control' is not a valid priority value."),
            ProtocolPriority::RealTime => Ok(Priority::RealTime),
            ProtocolPriority::InteractiveHigh => Ok(Priority::InteractiveHigh),
            ProtocolPriority::InteractiveLow => Ok(Priority::InteractiveLow),
            ProtocolPriority::DataHigh => Ok(Priority::DataHigh),
            ProtocolPriority::Data => Ok(Priority::Data),
            ProtocolPriority::DataLow => Ok(Priority::DataLow),
            ProtocolPriority::Background => Ok(Priority::Background),
        }
    }
}

/// A struct that indicates if there exist Subscribers matching the Publisher's key expression.
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh::prelude::r#async::*;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap().into_arc();
/// let publisher = session.declare_publisher("key/expression").res().await.unwrap();
/// let matching_status = publisher.matching_status().res().await.unwrap();
/// # }
/// ```
#[zenoh_macros::unstable]
#[derive(Copy, Clone, Debug)]
pub struct MatchingStatus {
    pub(in crate::sealed) matching: bool,
}

#[zenoh_macros::unstable]
impl MatchingStatus {
    /// Return true if there exist Subscribers matching the Publisher's key expression.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap().into_arc();
    /// let publisher = session.declare_publisher("key/expression").res().await.unwrap();
    /// let matching_subscribers: bool = publisher
    ///     .matching_status()
    ///     .res()
    ///     .await
    ///     .unwrap()
    ///     .matching_subscribers();
    /// # }
    /// ```
    pub fn matching_subscribers(&self) -> bool {
        self.matching
    }
}

/// A builder for initializing a [`MatchingListener`].
#[zenoh_macros::unstable]
#[derive(Debug)]
pub struct MatchingListenerBuilder<'a, Handler> {
    pub(in crate::sealed) publisher: PublisherRef<'a>,
    pub(in crate::sealed) handler: Handler,
}

#[zenoh_macros::unstable]
impl<'a> MatchingListenerBuilder<'a, DefaultHandler> {
    /// Receive the MatchingStatuses for this listener with a callback.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let publisher = session.declare_publisher("key/expression").res().await.unwrap();
    /// let matching_listener = publisher
    ///     .matching_listener()
    ///     .callback(|matching_status| {
    ///         if matching_status.matching_subscribers() {
    ///             println!("Publisher has matching subscribers.");
    ///         } else {
    ///             println!("Publisher has NO MORE matching subscribers.");
    ///         }
    ///     })
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
    #[inline]
    #[zenoh_macros::unstable]
    pub fn callback<Callback>(self, callback: Callback) -> MatchingListenerBuilder<'a, Callback>
    where
        Callback: Fn(MatchingStatus) + Send + Sync + 'static,
    {
        let MatchingListenerBuilder {
            publisher,
            handler: _,
        } = self;
        MatchingListenerBuilder {
            publisher,
            handler: callback,
        }
    }

    /// Receive the MatchingStatuses for this listener with a mutable callback.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let mut n = 0;
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let publisher = session.declare_publisher("key/expression").res().await.unwrap();
    /// let matching_listener = publisher
    ///     .matching_listener()
    ///     .callback_mut(move |_matching_status| { n += 1; })
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
    #[inline]
    #[zenoh_macros::unstable]
    pub fn callback_mut<CallbackMut>(
        self,
        callback: CallbackMut,
    ) -> MatchingListenerBuilder<'a, impl Fn(MatchingStatus) + Send + Sync + 'static>
    where
        CallbackMut: FnMut(MatchingStatus) + Send + Sync + 'static,
    {
        self.callback(locked(callback))
    }

    /// Receive the MatchingStatuses for this listener with a [`Handler`](crate::prelude::IntoHandler).
    ///
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let publisher = session.declare_publisher("key/expression").res().await.unwrap();
    /// let matching_listener = publisher
    ///     .matching_listener()
    ///     .with(flume::bounded(32))
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// while let Ok(matching_status) = matching_listener.recv_async().await {
    ///     if matching_status.matching_subscribers() {
    ///         println!("Publisher has matching subscribers.");
    ///     } else {
    ///         println!("Publisher has NO MORE matching subscribers.");
    ///     }
    /// }
    /// # }
    /// ```
    #[inline]
    #[zenoh_macros::unstable]
    pub fn with<Handler>(self, handler: Handler) -> MatchingListenerBuilder<'a, Handler>
    where
        Handler: IntoHandler<'static, MatchingStatus>,
    {
        let MatchingListenerBuilder {
            publisher,
            handler: _,
        } = self;
        MatchingListenerBuilder { publisher, handler }
    }
}

#[zenoh_macros::unstable]
impl<'a, Handler> Resolvable for MatchingListenerBuilder<'a, Handler>
where
    Handler: IntoHandler<'static, MatchingStatus> + Send,
    Handler::Handler: Send,
{
    type To = ZResult<MatchingListener<'a, Handler::Handler>>;
}

#[zenoh_macros::unstable]
impl<'a, Handler> SyncResolve for MatchingListenerBuilder<'a, Handler>
where
    Handler: IntoHandler<'static, MatchingStatus> + Send,
    Handler::Handler: Send,
{
    #[zenoh_macros::unstable]
    fn res_sync(self) -> <Self as Resolvable>::To {
        let (callback, receiver) = self.handler.into_handler();
        self.publisher
            .session
            .declare_matches_listener_inner(&self.publisher, callback)
            .map(|listener_state| MatchingListener {
                listener: MatchingListenerInner {
                    publisher: self.publisher,
                    state: listener_state,
                    alive: true,
                },
                receiver,
            })
    }
}

#[zenoh_macros::unstable]
impl<'a, Handler> AsyncResolve for MatchingListenerBuilder<'a, Handler>
where
    Handler: IntoHandler<'static, MatchingStatus> + Send,
    Handler::Handler: Send,
{
    type Future = Ready<Self::To>;

    #[zenoh_macros::unstable]
    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}

#[zenoh_macros::unstable]
pub struct MatchingListenerState {
    pub(in crate::sealed) id: Id,
    pub(in crate::sealed) current: std::sync::Mutex<bool>,
    pub(in crate::sealed) key_expr: KeyExpr<'static>,
    pub(in crate::sealed) destination: Locality,
    pub(in crate::sealed) callback: Callback<'static, MatchingStatus>,
}

#[zenoh_macros::unstable]
impl std::fmt::Debug for MatchingListenerState {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("MatchingListener")
            .field("id", &self.id)
            .field("key_expr", &self.key_expr)
            .finish()
    }
}

#[zenoh_macros::unstable]
pub(in crate::sealed) struct MatchingListenerInner<'a> {
    pub(in crate::sealed) publisher: PublisherRef<'a>,
    pub(in crate::sealed) state: std::sync::Arc<MatchingListenerState>,
    pub(in crate::sealed) alive: bool,
}

#[zenoh_macros::unstable]
impl<'a> MatchingListenerInner<'a> {
    #[inline]
    pub fn undeclare(self) -> MatchingListenerUndeclaration<'a> {
        Undeclarable::undeclare_inner(self, ())
    }
}

#[zenoh_macros::unstable]
impl<'a> Undeclarable<(), MatchingListenerUndeclaration<'a>> for MatchingListenerInner<'a> {
    fn undeclare_inner(self, _: ()) -> MatchingListenerUndeclaration<'a> {
        MatchingListenerUndeclaration { subscriber: self }
    }
}

/// A listener that sends notifications when the [`MatchingStatus`] of a
/// publisher changes.
///
/// # Examples
/// ```no_run
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh::prelude::r#async::*;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// let publisher = session.declare_publisher("key/expression").res().await.unwrap();
/// let matching_listener = publisher.matching_listener().res().await.unwrap();
/// while let Ok(matching_status) = matching_listener.recv_async().await {
///     if matching_status.matching_subscribers() {
///         println!("Publisher has matching subscribers.");
///     } else {
///         println!("Publisher has NO MORE matching subscribers.");
///     }
/// }
/// # }
/// ```
#[zenoh_macros::unstable]
pub struct MatchingListener<'a, Receiver> {
    pub(in crate::sealed) listener: MatchingListenerInner<'a>,
    pub(in crate::sealed) receiver: Receiver,
}

#[zenoh_macros::unstable]
impl<'a, Receiver> MatchingListener<'a, Receiver> {
    /// Close a [`MatchingListener`].
    ///
    /// MatchingListeners are automatically closed when dropped, but you may want to use this function to handle errors or
    /// close the MatchingListener asynchronously.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let publisher = session.declare_publisher("key/expression").res().await.unwrap();
    /// let matching_listener = publisher.matching_listener().res().await.unwrap();
    /// matching_listener.undeclare().res().await.unwrap();
    /// # }
    /// ```
    #[inline]
    pub fn undeclare(self) -> MatchingListenerUndeclaration<'a> {
        self.listener.undeclare()
    }
}

#[zenoh_macros::unstable]
impl<'a, T> Undeclarable<(), MatchingListenerUndeclaration<'a>> for MatchingListener<'a, T> {
    fn undeclare_inner(self, _: ()) -> MatchingListenerUndeclaration<'a> {
        Undeclarable::undeclare_inner(self.listener, ())
    }
}

#[zenoh_macros::unstable]
impl<Receiver> std::ops::Deref for MatchingListener<'_, Receiver> {
    type Target = Receiver;

    fn deref(&self) -> &Self::Target {
        &self.receiver
    }
}
#[zenoh_macros::unstable]
impl<Receiver> std::ops::DerefMut for MatchingListener<'_, Receiver> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.receiver
    }
}

#[zenoh_macros::unstable]
pub struct MatchingListenerUndeclaration<'a> {
    subscriber: MatchingListenerInner<'a>,
}

#[zenoh_macros::unstable]
impl Resolvable for MatchingListenerUndeclaration<'_> {
    type To = ZResult<()>;
}

#[zenoh_macros::unstable]
impl SyncResolve for MatchingListenerUndeclaration<'_> {
    fn res_sync(mut self) -> <Self as Resolvable>::To {
        self.subscriber.alive = false;
        self.subscriber
            .publisher
            .session
            .undeclare_matches_listener_inner(self.subscriber.state.id)
    }
}

#[zenoh_macros::unstable]
impl AsyncResolve for MatchingListenerUndeclaration<'_> {
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}

#[zenoh_macros::unstable]
impl Drop for MatchingListenerInner<'_> {
    fn drop(&mut self) {
        if self.alive {
            let _ = self
                .publisher
                .session
                .undeclare_matches_listener_inner(self.state.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::sealed::api::{sample::SampleKind, session::SessionDeclarations};
    use zenoh_config::Config;
    use zenoh_core::SyncResolve;

    #[test]
    fn priority_from() {
        use super::Priority as APrio;
        use std::convert::TryInto;
        use zenoh_protocol::core::Priority as TPrio;

        for i in APrio::MAX as u8..=APrio::MIN as u8 {
            let p: APrio = i.try_into().unwrap();

            match p {
                APrio::RealTime => assert_eq!(p as u8, TPrio::RealTime as u8),
                APrio::InteractiveHigh => assert_eq!(p as u8, TPrio::InteractiveHigh as u8),
                APrio::InteractiveLow => assert_eq!(p as u8, TPrio::InteractiveLow as u8),
                APrio::DataHigh => assert_eq!(p as u8, TPrio::DataHigh as u8),
                APrio::Data => assert_eq!(p as u8, TPrio::Data as u8),
                APrio::DataLow => assert_eq!(p as u8, TPrio::DataLow as u8),
                APrio::Background => assert_eq!(p as u8, TPrio::Background as u8),
            }

            let t: TPrio = p.into();
            assert_eq!(p as u8, t as u8);
        }
    }

    #[test]
    fn sample_kind_integrity_in_publication() {
        use crate::sealed::api::session::open;

        const KEY_EXPR: &str = "test/sample_kind_integrity/publication";
        const VALUE: &str = "zenoh";

        fn sample_kind_integrity_in_publication_with(kind: SampleKind) {
            let session = open(Config::default()).res().unwrap();
            let sub = session.declare_subscriber(KEY_EXPR).res().unwrap();
            let pub_ = session.declare_publisher(KEY_EXPR).res().unwrap();

            match kind {
                SampleKind::Put => pub_.put(VALUE).res().unwrap(),
                SampleKind::Delete => pub_.delete().res().unwrap(),
            }
            let sample = sub.recv().unwrap();

            assert_eq!(sample.kind, kind);
            if let SampleKind::Put = kind {
                assert_eq!(sample.payload.deserialize::<String>().unwrap(), VALUE);
            }
        }

        sample_kind_integrity_in_publication_with(SampleKind::Put);
        sample_kind_integrity_in_publication_with(SampleKind::Delete);
    }

    #[test]
    fn sample_kind_integrity_in_put_builder() {
        use crate::sealed::api::session::open;
        use zenoh_core::SyncResolve;

        const KEY_EXPR: &str = "test/sample_kind_integrity/put_builder";
        const VALUE: &str = "zenoh";

        fn sample_kind_integrity_in_put_builder_with(kind: SampleKind) {
            let session = open(Config::default()).res().unwrap();
            let sub = session.declare_subscriber(KEY_EXPR).res().unwrap();

            match kind {
                SampleKind::Put => session.put(KEY_EXPR, VALUE).res().unwrap(),
                SampleKind::Delete => session.delete(KEY_EXPR).res().unwrap(),
            }
            let sample = sub.recv().unwrap();

            assert_eq!(sample.kind, kind);
            if let SampleKind::Put = kind {
                assert_eq!(sample.payload.deserialize::<String>().unwrap(), VALUE);
            }
        }

        sample_kind_integrity_in_put_builder_with(SampleKind::Put);
        sample_kind_integrity_in_put_builder_with(SampleKind::Delete);
    }
}
