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

use std::{
    collections::HashSet,
    convert::TryFrom,
    fmt,
    future::{IntoFuture, Ready},
    mem::ManuallyDrop,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

use futures::Sink;
use zenoh_core::{zread, Resolvable, Resolve, Wait};
use zenoh_protocol::{
    core::CongestionControl,
    network::{push::ext, Push},
    zenoh::{Del, PushBody, Put},
};
use zenoh_result::{Error, ZResult};
#[zenoh_macros::unstable]
use {
    crate::api::handlers::{Callback, DefaultHandler, IntoHandler},
    crate::api::sample::SourceInfo,
    zenoh_protocol::core::EntityGlobalId,
};

use super::{
    builders::publisher::{
        PublicationBuilder, PublicationBuilderDelete, PublicationBuilderPut,
        PublisherDeleteBuilder, PublisherPutBuilder,
    },
    bytes::ZBytes,
    encoding::Encoding,
    key_expr::KeyExpr,
    sample::{DataInfo, Locality, QoS, Sample, SampleFields, SampleKind},
    session::{SessionRef, Undeclarable},
};
use crate::{api::Id, net::primitives::Primitives};

pub(crate) struct PublisherState {
    pub(crate) id: Id,
    pub(crate) remote_id: Id,
    pub(crate) key_expr: KeyExpr<'static>,
    pub(crate) destination: Locality,
}

impl fmt::Debug for PublisherState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Publisher")
            .field("id", &self.id)
            .field("key_expr", &self.key_expr)
            .finish()
    }
}

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
/// use zenoh::prelude::*;
///
/// let session = zenoh::open(zenoh::config::peer()).await.unwrap().into_arc();
/// let publisher = session.declare_publisher("key/expression").await.unwrap();
/// publisher.put("value").await.unwrap();
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
/// use zenoh::prelude::*;
///
/// let session = zenoh::open(zenoh::config::peer()).await.unwrap().into_arc();
/// let mut subscriber = session.declare_subscriber("key/expression").await.unwrap();
/// let publisher = session.declare_publisher("another/key/expression").await.unwrap();
/// subscriber.stream().map(Ok).forward(publisher).await.unwrap();
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct Publisher<'a> {
    pub(crate) session: SessionRef<'a>,
    pub(crate) id: Id,
    pub(crate) key_expr: KeyExpr<'a>,
    pub(crate) congestion_control: CongestionControl,
    pub(crate) priority: Priority,
    pub(crate) is_express: bool,
    pub(crate) destination: Locality,
    pub(crate) matching_listeners: Arc<Mutex<HashSet<Id>>>,
}

impl<'a> Publisher<'a> {
    /// Returns the [`EntityGlobalId`] of this Publisher.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// let publisher = session.declare_publisher("key/expression")
    ///     .await
    ///     .unwrap();
    /// let publisher_id = publisher.id();
    /// # }
    /// ```
    #[zenoh_macros::unstable]
    pub fn id(&self) -> EntityGlobalId {
        EntityGlobalId {
            zid: self.session.zid().into(),
            eid: self.id,
        }
    }

    #[inline]
    pub fn key_expr(&self) -> &KeyExpr<'a> {
        &self.key_expr
    }

    #[inline]
    /// Get the `congestion_control` applied when routing the data.
    pub fn congestion_control(&self) -> CongestionControl {
        self.congestion_control
    }

    /// Change the `congestion_control` to apply when routing the data.
    #[inline]
    pub fn set_congestion_control(&mut self, congestion_control: CongestionControl) {
        self.congestion_control = congestion_control;
    }

    /// Get the priority of the written data.
    #[inline]
    pub fn priority(&self) -> Priority {
        self.priority
    }

    /// Change the priority of the written data.
    #[inline]
    pub fn set_priority(&mut self, priority: Priority) {
        self.priority = priority;
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
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap().into_arc();
    /// let publisher = session.declare_publisher("key/expression").await.unwrap().into_arc();
    /// let matching_listener = publisher.matching_listener().await.unwrap();
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
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap().into_arc();
    /// let publisher = session.declare_publisher("key/expression").await.unwrap();
    /// publisher.put("value").await.unwrap();
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
            attachment: None,
        }
    }

    /// Delete data.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap().into_arc();
    /// let publisher = session.declare_publisher("key/expression").await.unwrap();
    /// publisher.delete().await.unwrap();
    /// # }
    /// ```
    pub fn delete(&self) -> PublisherDeleteBuilder<'_> {
        PublicationBuilder {
            publisher: self,
            kind: PublicationBuilderDelete,
            timestamp: None,
            #[cfg(feature = "unstable")]
            source_info: SourceInfo::empty(),
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
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap().into_arc();
    /// let publisher = session.declare_publisher("key/expression").await.unwrap();
    /// let matching_subscribers: bool = publisher
    ///     .matching_status()
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
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// let publisher = session.declare_publisher("key/expression").await.unwrap();
    /// let matching_listener = publisher.matching_listener().await.unwrap();
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
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// let publisher = session.declare_publisher("key/expression").await.unwrap();
    /// publisher.undeclare().await.unwrap();
    /// # }
    /// ```
    pub fn undeclare(self) -> impl Resolve<ZResult<()>> + 'a {
        Undeclarable::undeclare_inner(self, ())
    }

    fn undeclare_matching_listeners(&self) -> ZResult<()> {
        for id in zlock!(self.matching_listeners).drain() {
            self.session.undeclare_matches_listener_inner(id)?
        }
        Ok(())
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
/// use zenoh::prelude::*;
///
/// let session = zenoh::open(zenoh::config::peer()).await.unwrap().into_arc();
/// let publisher = session.declare_publisher("key/expression").await.unwrap().into_arc();
/// let matching_listener = publisher.matching_listener().await.unwrap();
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
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap().into_arc();
    /// let publisher = session.declare_publisher("key/expression").await.unwrap().into_arc();
    /// let matching_listener = publisher.matching_listener().await.unwrap();
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
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap().into_arc();
    /// let publisher = session.declare_publisher("key/expression").await.unwrap().into_arc();
    /// let matching_listener = publisher.matching_listener().await.unwrap();
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
        PublisherUndeclaration {
            publisher: ManuallyDrop::new(self),
        }
    }
}

/// A [`Resolvable`] returned when undeclaring a publisher.
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh::prelude::*;
///
/// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
/// let publisher = session.declare_publisher("key/expression").await.unwrap();
/// publisher.undeclare().await.unwrap();
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
pub struct PublisherUndeclaration<'a> {
    // ManuallyDrop wrapper prevents the drop code to be executed,
    // which would lead to a double undeclaration
    publisher: ManuallyDrop<Publisher<'a>>,
}

impl Resolvable for PublisherUndeclaration<'_> {
    type To = ZResult<()>;
}

impl Wait for PublisherUndeclaration<'_> {
    fn wait(self) -> <Self as Resolvable>::To {
        self.publisher.undeclare_matching_listeners()?;
        self.publisher
            .session
            .undeclare_publisher_inner(self.publisher.id)
    }
}

impl IntoFuture for PublisherUndeclaration<'_> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

impl Drop for Publisher<'_> {
    fn drop(&mut self) {
        let _ = self.undeclare_matching_listeners();
        let _ = self.session.undeclare_publisher_inner(self.id);
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
    pub(crate) fn resolve_put(
        &self,
        payload: ZBytes,
        kind: SampleKind,
        encoding: Encoding,
        timestamp: Option<uhlc::Timestamp>,
        #[cfg(feature = "unstable")] source_info: SourceInfo,
        attachment: Option<ZBytes>,
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
                    SampleKind::Put => PushBody::Put(Put {
                        timestamp,
                        encoding: encoding.clone().into(),
                        #[cfg(feature = "unstable")]
                        ext_sinfo: source_info.into(),
                        #[cfg(not(feature = "unstable"))]
                        ext_sinfo: None,
                        #[cfg(feature = "shared-memory")]
                        ext_shm: None,
                        ext_attachment: attachment.clone().map(|a| a.into()),
                        ext_unknown: vec![],
                        payload: payload.clone().into(),
                    }),
                    SampleKind::Delete => PushBody::Del(Del {
                        timestamp,
                        #[cfg(feature = "unstable")]
                        ext_sinfo: source_info.into(),
                        #[cfg(not(feature = "unstable"))]
                        ext_sinfo: None,
                        ext_attachment: attachment.clone().map(|a| a.into()),
                        ext_unknown: vec![],
                    }),
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
    #[zenoh_macros::internal]
    pub const MIN: Self = Self::MIN_;
    const MIN_: Self = Self::Background;
    /// The highest Priority
    #[zenoh_macros::internal]
    pub const MAX: Self = Self::MAX_;
    const MAX_: Self = Self::RealTime;
    /// The number of available priorities
    #[zenoh_macros::internal]
    pub const NUM: usize = 1 + Self::MIN_ as usize - Self::MAX_ as usize;
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
                Self::MAX_ as u8,
                Self::MIN_ as u8
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
/// use zenoh::prelude::*;
///
/// let session = zenoh::open(zenoh::config::peer()).await.unwrap().into_arc();
/// let publisher = session.declare_publisher("key/expression").await.unwrap();
/// let matching_status = publisher.matching_status().await.unwrap();
/// # }
/// ```
#[zenoh_macros::unstable]
#[derive(Copy, Clone, Debug)]
pub struct MatchingStatus {
    pub(crate) matching: bool,
}

#[zenoh_macros::unstable]
impl MatchingStatus {
    /// Return true if there exist Subscribers matching the Publisher's key expression.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap().into_arc();
    /// let publisher = session.declare_publisher("key/expression").await.unwrap();
    /// let matching_subscribers: bool = publisher
    ///     .matching_status()
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
    pub(crate) publisher: PublisherRef<'a>,
    pub handler: Handler,
}

#[zenoh_macros::unstable]
impl<'a> MatchingListenerBuilder<'a, DefaultHandler> {
    /// Receive the MatchingStatuses for this listener with a callback.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// let publisher = session.declare_publisher("key/expression").await.unwrap();
    /// let matching_listener = publisher
    ///     .matching_listener()
    ///     .callback(|matching_status| {
    ///         if matching_status.matching_subscribers() {
    ///             println!("Publisher has matching subscribers.");
    ///         } else {
    ///             println!("Publisher has NO MORE matching subscribers.");
    ///         }
    ///     })
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
    /// use zenoh::prelude::*;
    ///
    /// let mut n = 0;
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// let publisher = session.declare_publisher("key/expression").await.unwrap();
    /// let matching_listener = publisher
    ///     .matching_listener()
    ///     .callback_mut(move |_matching_status| { n += 1; })
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
        self.callback(crate::api::handlers::locked(callback))
    }

    /// Receive the MatchingStatuses for this listener with a [`Handler`](crate::prelude::IntoHandler).
    ///
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// let publisher = session.declare_publisher("key/expression").await.unwrap();
    /// let matching_listener = publisher
    ///     .matching_listener()
    ///     .with(flume::bounded(32))
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
impl<'a, Handler> Wait for MatchingListenerBuilder<'a, Handler>
where
    Handler: IntoHandler<'static, MatchingStatus> + Send,
    Handler::Handler: Send,
{
    #[zenoh_macros::unstable]
    fn wait(self) -> <Self as Resolvable>::To {
        let (callback, receiver) = self.handler.into_handler();
        let state = self
            .publisher
            .session
            .declare_matches_listener_inner(&self.publisher, callback)?;
        zlock!(self.publisher.matching_listeners).insert(state.id);
        Ok(MatchingListener {
            listener: MatchingListenerInner {
                publisher: self.publisher,
                state,
                background: false,
            },
            receiver,
        })
    }
}

#[zenoh_macros::unstable]
impl<'a, Handler> IntoFuture for MatchingListenerBuilder<'a, Handler>
where
    Handler: IntoHandler<'static, MatchingStatus> + Send,
    Handler::Handler: Send,
{
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    #[zenoh_macros::unstable]
    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

#[zenoh_macros::unstable]
pub(crate) struct MatchingListenerState {
    pub(crate) id: Id,
    pub(crate) current: std::sync::Mutex<bool>,
    pub(crate) key_expr: KeyExpr<'static>,
    pub(crate) destination: Locality,
    pub(crate) callback: Callback<'static, MatchingStatus>,
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
pub(crate) struct MatchingListenerInner<'a> {
    pub(crate) publisher: PublisherRef<'a>,
    pub(crate) state: std::sync::Arc<MatchingListenerState>,
    background: bool,
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
        MatchingListenerUndeclaration {
            subscriber: ManuallyDrop::new(self),
        }
    }
}

/// A listener that sends notifications when the [`MatchingStatus`] of a
/// publisher changes.
///
/// # Examples
/// ```no_run
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh::prelude::*;
///
/// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
/// let publisher = session.declare_publisher("key/expression").await.unwrap();
/// let matching_listener = publisher.matching_listener().await.unwrap();
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
    pub(crate) listener: MatchingListenerInner<'a>,
    pub(crate) receiver: Receiver,
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
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// let publisher = session.declare_publisher("key/expression").await.unwrap();
    /// let matching_listener = publisher.matching_listener().await.unwrap();
    /// matching_listener.undeclare().await.unwrap();
    /// # }
    /// ```
    #[inline]
    pub fn undeclare(self) -> MatchingListenerUndeclaration<'a> {
        self.listener.undeclare()
    }

    /// Make the matching listener run in background, until the publisher is undeclared.
    #[inline]
    #[zenoh_macros::unstable]
    pub fn background(mut self) {
        self.listener.background = true;
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
    // ManuallyDrop wrapper prevents the drop code to be executed,
    // which would lead to a double undeclaration
    subscriber: ManuallyDrop<MatchingListenerInner<'a>>,
}

#[zenoh_macros::unstable]
impl Resolvable for MatchingListenerUndeclaration<'_> {
    type To = ZResult<()>;
}

#[zenoh_macros::unstable]
impl Wait for MatchingListenerUndeclaration<'_> {
    fn wait(self) -> <Self as Resolvable>::To {
        zlock!(self.subscriber.publisher.matching_listeners).remove(&self.subscriber.state.id);
        self.subscriber
            .publisher
            .session
            .undeclare_matches_listener_inner(self.subscriber.state.id)
    }
}

#[zenoh_macros::unstable]
impl IntoFuture for MatchingListenerUndeclaration<'_> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

#[zenoh_macros::unstable]
impl Drop for MatchingListenerInner<'_> {
    fn drop(&mut self) {
        if !self.background {
            zlock!(self.publisher.matching_listeners).remove(&self.state.id);
            let _ = self
                .publisher
                .session
                .undeclare_matches_listener_inner(self.state.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use zenoh_config::Config;
    use zenoh_core::Wait;

    use crate::api::{sample::SampleKind, session::SessionDeclarations};

    #[test]
    fn priority_from() {
        use std::convert::TryInto;

        use zenoh_protocol::core::Priority as TPrio;

        use super::Priority as APrio;

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
        use crate::api::session::open;

        const KEY_EXPR: &str = "test/sample_kind_integrity/publication";
        const VALUE: &str = "zenoh";

        fn sample_kind_integrity_in_publication_with(kind: SampleKind) {
            let session = open(Config::default()).wait().unwrap();
            let sub = session.declare_subscriber(KEY_EXPR).wait().unwrap();
            let pub_ = session.declare_publisher(KEY_EXPR).wait().unwrap();

            match kind {
                SampleKind::Put => pub_.put(VALUE).wait().unwrap(),
                SampleKind::Delete => pub_.delete().wait().unwrap(),
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
        use crate::api::session::open;

        const KEY_EXPR: &str = "test/sample_kind_integrity/put_builder";
        const VALUE: &str = "zenoh";

        fn sample_kind_integrity_in_put_builder_with(kind: SampleKind) {
            let session = open(Config::default()).wait().unwrap();
            let sub = session.declare_subscriber(KEY_EXPR).wait().unwrap();

            match kind {
                SampleKind::Put => session.put(KEY_EXPR, VALUE).wait().unwrap(),
                SampleKind::Delete => session.delete(KEY_EXPR).wait().unwrap(),
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
