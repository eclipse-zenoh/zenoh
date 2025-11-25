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
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

use futures::Sink;
use serde::Deserialize;
use tracing::error;
use zenoh_config::qos::PriorityConf;
use zenoh_core::{Resolvable, Resolve, Wait};
use zenoh_protocol::core::CongestionControl;
use zenoh_result::{Error, ZResult};
#[cfg(feature = "unstable")]
use {
    crate::api::sample::SourceInfo, zenoh_config::wrappers::EntityGlobalId,
    zenoh_protocol::core::EntityGlobalIdProto, zenoh_protocol::core::Reliability,
};

use crate::api::{
    builders::{
        matching_listener::MatchingListenerBuilder,
        publisher::{
            PublicationBuilder, PublicationBuilderDelete, PublicationBuilderPut,
            PublisherDeleteBuilder, PublisherPutBuilder,
        },
    },
    bytes::ZBytes,
    encoding::Encoding,
    handlers::DefaultHandler,
    key_expr::KeyExpr,
    matching::{MatchingStatus, MatchingStatusType},
    sample::{Locality, Sample, SampleFields},
    session::{UndeclarableSealed, WeakSession},
    Id,
};

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

/// A publisher that allows sending data through a stream.
///
/// Publishers are automatically undeclared when dropped.
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
///
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let publisher = session.declare_publisher("key/expression").await.unwrap();
/// publisher.put("value").await.unwrap();
/// # format!("{publisher:?}");
/// # }
/// ```
///
///
/// `Publisher` implements the `Sink` trait which is useful for forwarding
/// streams to zenoh.
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// use futures::StreamExt;
///
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let mut subscriber = session.declare_subscriber("key/expression").await.unwrap();
/// let publisher = session.declare_publisher("another/key/expression").await.unwrap();
/// # tokio::task::spawn(async move {
/// #     session.put("key/expression", "value").await.unwrap();
/// #     tokio::time::sleep(std::time::Duration::from_secs(1)).await;
/// #     session.close().await.unwrap();
/// # });
/// subscriber.stream().map(Ok).forward(publisher).await.unwrap();
/// # }
/// ```
#[derive(Debug)]
pub struct Publisher<'a> {
    pub(crate) session: WeakSession,
    pub(crate) id: Id,
    pub(crate) key_expr: KeyExpr<'a>,
    pub(crate) encoding: Encoding,
    pub(crate) congestion_control: CongestionControl,
    pub(crate) priority: Priority,
    pub(crate) is_express: bool,
    pub(crate) destination: Locality,
    #[cfg(feature = "unstable")]
    pub(crate) reliability: Reliability,
    pub(crate) matching_listeners: Arc<Mutex<HashSet<Id>>>,
    pub(crate) undeclare_on_drop: bool,
}

impl<'a> Publisher<'a> {
    /// Returns the [`EntityGlobalId`] of this Publisher.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let publisher = session.declare_publisher("key/expression")
    ///     .await
    ///     .unwrap();
    /// let publisher_id = publisher.id();
    /// # }
    /// ```
    #[zenoh_macros::unstable]
    pub fn id(&self) -> EntityGlobalId {
        EntityGlobalIdProto {
            zid: self.session.zid().into(),
            eid: self.id,
        }
        .into()
    }

    /// Returns the [`KeyExpr`] of this Publisher.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let publisher = session.declare_publisher("key/expression")
    ///     .await
    ///     .unwrap();
    /// let key_expr = publisher.key_expr();
    /// # }
    /// ```
    #[inline]
    pub fn key_expr(&self) -> &KeyExpr<'a> {
        &self.key_expr
    }

    /// Get the [`Encoding`] used when publishing data.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let publisher = session.declare_publisher("key/expression")
    ///     .await
    ///     .unwrap();
    /// let encoding = publisher.encoding();
    /// # }
    /// ```
    #[inline]
    pub fn encoding(&self) -> &Encoding {
        &self.encoding
    }

    /// Get the [`CongestionControl`] applied when routing the data.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let publisher = session.declare_publisher("key/expression")
    ///     .await
    ///     .unwrap();
    /// let congestion_control = publisher.congestion_control();
    /// # }
    /// ```
    #[inline]
    pub fn congestion_control(&self) -> CongestionControl {
        self.congestion_control
    }

    /// Get the [`Priority`] of the written data.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let publisher = session.declare_publisher("key/expression")
    ///     .await
    ///     .unwrap();
    /// let priority = publisher.priority();
    /// # }
    /// ```
    #[inline]
    pub fn priority(&self) -> Priority {
        self.priority
    }

    /// Get the [`Reliability`] applied when routing the data.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let publisher = session.declare_publisher("key/expression")
    ///     .await
    ///     .unwrap();
    /// let reliability = publisher.reliability();
    /// # }
    /// ```
    #[zenoh_macros::unstable]
    #[inline]
    pub fn reliability(&self) -> Reliability {
        self.reliability
    }

    /// Publish the data. The subscribers matching the Publisher's key expression will receive the
    /// [`Sample`] with [`kind`](crate::sample::Sample::kind) [SampleKind::Put](crate::sample::SampleKind::Put).
    ///
    /// The builder allows customizing the publication: add the timestamp,
    /// attachment, etc. Some fields are pre-filled with the Publisher's configuration and can
    /// be overridden.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
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
                encoding: self.encoding.clone(),
            },
            timestamp: None,
            #[cfg(feature = "unstable")]
            source_info: SourceInfo::empty(),
            attachment: None,
        }
    }

    /// Declare that the data associated with the Publisher's key expression is deleted.
    /// The subscribers will receive the [`Sample`] with
    /// [`kind`](crate::sample::Sample::kind) [SampleKind::Delete](crate::sample::SampleKind::Delete).
    ///
    /// The builder allows customizing the publication: add the timestamp, attachment, etc.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
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
    /// [`MatchingStatus::matching`] will return true if there exist Subscribers
    /// matching the Publisher's key expression and false otherwise.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let publisher = session.declare_publisher("key/expression").await.unwrap();
    /// let matching_subscribers: bool = publisher
    ///     .matching_status()
    ///     .await
    ///     .unwrap()
    ///     .matching();
    /// # }
    /// ```
    pub fn matching_status(&self) -> impl Resolve<ZResult<MatchingStatus>> + '_ {
        zenoh_core::ResolveFuture::new(async move {
            self.session.matching_status(
                self.key_expr(),
                self.destination,
                MatchingStatusType::Subscribers,
            )
        })
    }

    /// Return a [`MatchingListener`](crate::api::matching::MatchingListener) for this Publisher.
    ///
    /// The [`MatchingListener`](crate::api::matching::MatchingListener) will send a notification each time the [`MatchingStatus`](crate::api::matching::MatchingStatus) of
    /// the Publisher changes.
    ///
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let publisher = session.declare_publisher("key/expression").await.unwrap();
    /// let matching_listener = publisher.matching_listener().await.unwrap();
    /// while let Ok(matching_status) = matching_listener.recv_async().await {
    ///     if matching_status.matching() {
    ///         println!("Publisher has matching subscribers.");
    ///     } else {
    ///         println!("Publisher has NO MORE matching subscribers.");
    ///     }
    /// }
    /// # }
    /// ```
    pub fn matching_listener(&self) -> MatchingListenerBuilder<'_, DefaultHandler> {
        MatchingListenerBuilder {
            session: &self.session,
            key_expr: &self.key_expr,
            destination: self.destination,
            matching_listeners: &self.matching_listeners,
            matching_status_type: MatchingStatusType::Subscribers,
            handler: DefaultHandler::default(),
        }
    }

    /// Undeclare the [`Publisher`], informing the network that it needn't optimize publications for its key expression anymore.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let publisher = session.declare_publisher("key/expression").await.unwrap();
    /// publisher.undeclare().await.unwrap();
    /// # }
    /// ```
    pub fn undeclare(self) -> impl Resolve<ZResult<()>> + 'a {
        UndeclarableSealed::undeclare_inner(self, ())
    }

    fn undeclare_impl(&mut self) -> ZResult<()> {
        // set the flag first to avoid double panic if this function panics
        self.undeclare_on_drop = false;
        let ids: Vec<Id> = zlock!(self.matching_listeners).drain().collect();
        for id in ids {
            self.session.undeclare_matches_listener_inner(id)?
        }
        self.session.undeclare_publisher_inner(self.id)
    }

    #[zenoh_macros::internal]
    pub fn session(&self) -> &crate::Session {
        self.session.session()
    }
}

impl<'a> UndeclarableSealed<()> for Publisher<'a> {
    type Undeclaration = PublisherUndeclaration<'a>;

    fn undeclare_inner(self, _: ()) -> Self::Undeclaration {
        PublisherUndeclaration(self)
    }
}

/// A [`Resolvable`] returned by [`Publisher::undeclare`]
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
///
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let publisher = session.declare_publisher("key/expression").await.unwrap();
/// publisher.undeclare().await.unwrap();
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
pub struct PublisherUndeclaration<'a>(Publisher<'a>);

impl Resolvable for PublisherUndeclaration<'_> {
    type To = ZResult<()>;
}

impl Wait for PublisherUndeclaration<'_> {
    fn wait(mut self) -> <Self as Resolvable>::To {
        self.0.undeclare_impl()
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
        if self.undeclare_on_drop {
            if let Err(error) = self.undeclare_impl() {
                error!(error);
            }
        }
    }
}

impl Sink<Sample> for Publisher<'_> {
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
        self.session.resolve_put(
            &self.key_expr,
            payload,
            kind,
            encoding,
            self.congestion_control,
            self.priority,
            self.is_express,
            self.destination,
            #[cfg(feature = "unstable")]
            self.reliability,
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

/// Message priority.
///
/// If QoS is enabled in the [`Config`](crate::config::Config) (see the boolean
/// `transport.<transport_type>.qos` parameter),
/// Zenoh keeps one transmission queue per [`Priority`], where all messages in
/// the queue have the same [`Priority`]. These queues are serviced in the order of their assigned
/// [`Priority`] (i.e. from [`Priority::RealTime`] to [`Priority::Background`]).
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Deserialize)]
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
    /// The lower the value, the higher the priority.
    /// The higher the value, the lower the priority.
    ///
    /// Allowed values are: 1-7.
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

impl From<PriorityConf> for Priority {
    fn from(value: PriorityConf) -> Self {
        match value {
            PriorityConf::RealTime => Self::RealTime,
            PriorityConf::InteractiveHigh => Self::InteractiveHigh,
            PriorityConf::InteractiveLow => Self::InteractiveLow,
            PriorityConf::DataHigh => Self::DataHigh,
            PriorityConf::Data => Self::Data,
            PriorityConf::DataLow => Self::DataLow,
            PriorityConf::Background => Self::Background,
        }
    }
}

impl From<Priority> for PriorityConf {
    fn from(value: Priority) -> Self {
        match value {
            Priority::RealTime => Self::RealTime,
            Priority::InteractiveHigh => Self::InteractiveHigh,
            Priority::InteractiveLow => Self::InteractiveLow,
            Priority::DataHigh => Self::DataHigh,
            Priority::Data => Self::Data,
            Priority::DataLow => Self::DataLow,
            Priority::Background => Self::Background,
        }
    }
}

type ProtocolPriority = zenoh_protocol::core::Priority;
impl From<Priority> for ProtocolPriority {
    fn from(prio: Priority) -> Self {
        // The Priority in the prelude differs from the Priority in the core protocol only by
        // the missing Control priority. The Control priority is reserved for zenoh's internal use
        // and as such it is not exposed by the zenoh API. Nevertheless, the values of the
        // priorities which are common to the internal and public Priority enums are the same. Therefore,
        // it is possible to safely transmute from the public Priority enum toward the internal
        // Priority enum without risking being in an invalid state.
        // For better robustness, the correctness of the unsafe transmute operation is covered
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

#[cfg(test)]
mod tests {
    use crate::{sample::SampleKind, Config, Wait};

    #[cfg(feature = "internal")]
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
                assert_eq!(sample.payload.try_to_string().unwrap(), VALUE);
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
                assert_eq!(sample.payload.try_to_string().unwrap(), VALUE);
            }
        }

        sample_kind_integrity_in_put_builder_with(SampleKind::Put);
        sample_kind_integrity_in_put_builder_with(SampleKind::Delete);
    }
}
