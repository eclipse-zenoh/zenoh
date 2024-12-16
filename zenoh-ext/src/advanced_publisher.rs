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
    future::{IntoFuture, Ready},
    sync::atomic::{AtomicU32, Ordering},
};

use zenoh::{
    bytes::{Encoding, OptionZBytes, ZBytes},
    internal::{
        bail,
        traits::{
            EncodingBuilderTrait, QoSBuilderTrait, SampleBuilderTrait, TimestampBuilderTrait,
        },
    },
    key_expr::{keyexpr, KeyExpr},
    liveliness::LivelinessToken,
    pubsub::{
        PublicationBuilder, PublicationBuilderDelete, PublicationBuilderPut, Publisher,
        PublisherBuilder,
    },
    qos::{CongestionControl, Priority, Reliability},
    sample::{Locality, SourceInfo},
    session::EntityGlobalId,
    Resolvable, Resolve, Result as ZResult, Session, Wait, KE_ADV_PREFIX, KE_AT, KE_EMPTY,
};
use zenoh_macros::ke;

use crate::advanced_cache::{AdvancedCache, AdvancedCacheBuilder, CacheConfig, KE_UHLC};

pub(crate) static KE_PUB: &keyexpr = ke!("pub");

#[derive(PartialEq)]
#[zenoh_macros::unstable]
pub(crate) enum Sequencing {
    None,
    Timestamp,
    SequenceNumber,
}

/// The builder of PublicationCache, allowing to configure it.
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
#[zenoh_macros::unstable]
pub struct AdvancedPublisherBuilder<'a, 'b, 'c> {
    session: &'a Session,
    pub_key_expr: ZResult<KeyExpr<'b>>,
    encoding: Encoding,
    destination: Locality,
    reliability: Reliability,
    congestion_control: CongestionControl,
    priority: Priority,
    is_express: bool,
    meta_key_expr: Option<ZResult<KeyExpr<'c>>>,
    sequencing: Sequencing,
    liveliness: bool,
    cache: bool,
    history: CacheConfig,
}

#[zenoh_macros::unstable]
impl<'a, 'b, 'c> AdvancedPublisherBuilder<'a, 'b, 'c> {
    #[zenoh_macros::unstable]
    pub(crate) fn new(builder: PublisherBuilder<'a, 'b>) -> AdvancedPublisherBuilder<'a, 'b, 'c> {
        AdvancedPublisherBuilder {
            session: builder.session,
            pub_key_expr: builder.key_expr,
            encoding: builder.encoding,
            destination: builder.destination,
            reliability: builder.reliability,
            congestion_control: builder.congestion_control,
            priority: builder.priority,
            is_express: builder.is_express,
            meta_key_expr: None,
            sequencing: Sequencing::None,
            liveliness: false,
            cache: false,
            history: CacheConfig::default(),
        }
    }

    /// Changes the [`zenoh::sample::Locality`] applied when routing the data.
    ///
    /// This restricts the matching subscribers that will receive the published data to the ones
    /// that have the given [`zenoh::sample::Locality`].
    #[zenoh_macros::unstable]
    #[inline]
    pub fn allowed_destination(mut self, destination: Locality) -> Self {
        self.destination = destination;
        self
    }

    /// Changes the [`zenoh::qos::Reliability`] to apply when routing the data.
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

    /// Allow matching [`AdvancedSubscribers`](crate::AdvancedSubscriber) to detect lost samples and optionally ask for retransimission.
    ///
    /// Retransmission can only be achieved if [`cache`](crate::AdvancedPublisherBuilder::cache) is enabled.
    #[zenoh_macros::unstable]
    pub fn sample_miss_detection(mut self) -> Self {
        self.sequencing = Sequencing::SequenceNumber;
        self
    }

    /// Attach a cache to this [`AdvancedPublisher`].
    ///
    /// The cache can be used for history and/or recovery.
    #[zenoh_macros::unstable]
    pub fn cache(mut self, config: CacheConfig) -> Self {
        self.cache = true;
        if self.sequencing == Sequencing::None {
            self.sequencing = Sequencing::Timestamp;
        }
        self.history = config;
        self
    }

    /// Allow this [`AdvancedPublisher`] to be detected by [`AdvancedSubscribers`](crate::AdvancedSubscriber).
    ///
    /// This allows [`AdvancedSubscribers`](crate::AdvancedSubscriber) to retrieve the local history.
    #[zenoh_macros::unstable]
    pub fn publisher_detection(mut self) -> Self {
        self.liveliness = true;
        self
    }

    /// A key expression added to the liveliness token key expression
    /// and to the cache queryable key expression.
    /// It can be used to convey meta data.
    #[zenoh_macros::unstable]
    pub fn publisher_detection_metadata<TryIntoKeyExpr>(mut self, meta: TryIntoKeyExpr) -> Self
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'c>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'c>>>::Error: Into<zenoh::Error>,
    {
        self.meta_key_expr = Some(meta.try_into().map_err(Into::into));
        self
    }
}

#[zenoh_macros::internal_trait]
#[zenoh_macros::unstable]
impl EncodingBuilderTrait for AdvancedPublisherBuilder<'_, '_, '_> {
    #[zenoh_macros::unstable]
    fn encoding<T: Into<Encoding>>(self, encoding: T) -> Self {
        Self {
            encoding: encoding.into(),
            ..self
        }
    }
}

#[zenoh_macros::internal_trait]
#[zenoh_macros::unstable]
impl QoSBuilderTrait for AdvancedPublisherBuilder<'_, '_, '_> {
    /// Changes the [`zenoh::qos::CongestionControl`] to apply when routing the data.
    #[inline]
    #[zenoh_macros::unstable]
    fn congestion_control(self, congestion_control: CongestionControl) -> Self {
        Self {
            congestion_control,
            ..self
        }
    }

    /// Changes the [`zenoh::qos::Priority`] of the written data.
    #[inline]
    #[zenoh_macros::unstable]
    fn priority(self, priority: Priority) -> Self {
        Self { priority, ..self }
    }

    /// Changes the Express policy to apply when routing the data.
    ///
    /// When express is set to `true`, then the message will not be batched.
    /// This usually has a positive impact on latency but negative impact on throughput.
    #[inline]
    #[zenoh_macros::unstable]
    fn express(self, is_express: bool) -> Self {
        Self { is_express, ..self }
    }
}

#[zenoh_macros::unstable]
impl<'b> Resolvable for AdvancedPublisherBuilder<'_, 'b, '_> {
    type To = ZResult<AdvancedPublisher<'b>>;
}

#[zenoh_macros::unstable]
impl Wait for AdvancedPublisherBuilder<'_, '_, '_> {
    #[zenoh_macros::unstable]
    fn wait(self) -> <Self as Resolvable>::To {
        AdvancedPublisher::new(self)
    }
}

#[zenoh_macros::unstable]
impl IntoFuture for AdvancedPublisherBuilder<'_, '_, '_> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    #[zenoh_macros::unstable]
    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

/// [`AdvancedPublisher`].
#[zenoh_macros::unstable]
pub struct AdvancedPublisher<'a> {
    publisher: Publisher<'a>,
    seqnum: Option<AtomicU32>,
    cache: Option<AdvancedCache>,
    _token: Option<LivelinessToken>,
}

#[zenoh_macros::unstable]
impl<'a> AdvancedPublisher<'a> {
    #[zenoh_macros::unstable]
    fn new(conf: AdvancedPublisherBuilder<'_, 'a, '_>) -> ZResult<Self> {
        let key_expr = conf.pub_key_expr?;
        let meta = match conf.meta_key_expr {
            Some(meta) => Some(meta?),
            None => None,
        };

        let publisher = conf
            .session
            .declare_publisher(key_expr.clone())
            .encoding(conf.encoding)
            .allowed_destination(conf.destination)
            .reliability(conf.reliability)
            .congestion_control(conf.congestion_control)
            .priority(conf.priority)
            .express(conf.is_express)
            .wait()?;
        let id = publisher.id();
        let prefix = KE_ADV_PREFIX / KE_PUB / &id.zid().into_keyexpr();
        let prefix = match conf.sequencing {
            Sequencing::SequenceNumber => {
                prefix / &KeyExpr::try_from(id.eid().to_string()).unwrap()
            }
            _ => prefix / KE_UHLC,
        };
        let prefix = match meta {
            Some(meta) => prefix / &meta / KE_AT,
            // We need this empty chunk because af a routing matching bug
            _ => prefix / KE_EMPTY / KE_AT,
        };

        let seqnum = match conf.sequencing {
            Sequencing::SequenceNumber => Some(AtomicU32::new(0)),
            Sequencing::Timestamp => {
                if conf.session.hlc().is_none() {
                    bail!(
                        "Cannot create AdvancedPublisher {} with Sequencing::Timestamp: \
                            the 'timestamping' setting must be enabled in the Zenoh configuration.",
                        key_expr,
                    )
                }
                None
            }
            _ => None,
        };

        let cache = if conf.cache {
            Some(
                AdvancedCacheBuilder::new(conf.session, Ok(key_expr.clone()))
                    .history(conf.history)
                    .queryable_prefix(&prefix)
                    .wait()?,
            )
        } else {
            None
        };

        let token = if conf.liveliness {
            Some(
                conf.session
                    .liveliness()
                    .declare_token(prefix / &key_expr)
                    .wait()?,
            )
        } else {
            None
        };

        Ok(AdvancedPublisher {
            publisher,
            seqnum,
            cache,
            _token: token,
        })
    }

    /// Returns the [`EntityGlobalId`] of this Publisher.
    #[zenoh_macros::unstable]
    pub fn id(&self) -> EntityGlobalId {
        self.publisher.id()
    }

    /// Returns the [`KeyExpr`] of this Publisher.
    #[inline]
    #[zenoh_macros::unstable]
    pub fn key_expr(&self) -> &KeyExpr<'a> {
        self.publisher.key_expr()
    }

    /// Get the [`Encoding`] used when publishing data.
    #[inline]
    #[zenoh_macros::unstable]
    pub fn encoding(&self) -> &Encoding {
        self.publisher.encoding()
    }

    /// Get the `congestion_control` applied when routing the data.
    #[inline]
    #[zenoh_macros::unstable]
    pub fn congestion_control(&self) -> CongestionControl {
        self.publisher.congestion_control()
    }

    /// Get the priority of the written data.
    #[inline]
    #[zenoh_macros::unstable]
    pub fn priority(&self) -> Priority {
        self.publisher.priority()
    }

    /// Put data.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh_ext::AdvancedPublisherBuilderExt;
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let publisher = session.declare_publisher("key/expression").advanced().await.unwrap();
    /// publisher.put("value").await.unwrap();
    /// # }
    /// ```
    #[inline]
    #[zenoh_macros::unstable]
    pub fn put<IntoZBytes>(&self, payload: IntoZBytes) -> AdvancedPublisherPutBuilder<'_>
    where
        IntoZBytes: Into<ZBytes>,
    {
        let mut builder = self.publisher.put(payload);
        if let Some(seqnum) = &self.seqnum {
            builder = builder.source_info(SourceInfo::new(
                Some(self.publisher.id()),
                Some(seqnum.fetch_add(1, Ordering::Relaxed)),
            ));
        }
        if let Some(hlc) = self.publisher.session().hlc() {
            builder = builder.timestamp(hlc.new_timestamp());
        }
        AdvancedPublisherPutBuilder {
            builder,
            cache: self.cache.as_ref(),
        }
    }

    /// Delete data.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh_ext::AdvancedPublisherBuilderExt;
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let publisher = session.declare_publisher("key/expression").advanced().await.unwrap();
    /// publisher.delete().await.unwrap();
    /// # }
    /// ```
    #[zenoh_macros::unstable]
    pub fn delete(&self) -> AdvancedPublisherDeleteBuilder<'_> {
        let mut builder = self.publisher.delete();
        if let Some(seqnum) = &self.seqnum {
            builder = builder.source_info(SourceInfo::new(
                Some(self.publisher.id()),
                Some(seqnum.fetch_add(1, Ordering::Relaxed)),
            ));
        }
        if let Some(hlc) = self.publisher.session().hlc() {
            builder = builder.timestamp(hlc.new_timestamp());
        }
        AdvancedPublisherDeleteBuilder {
            builder,
            cache: self.cache.as_ref(),
        }
    }

    /// Return the [`MatchingStatus`](zenoh::matching::MatchingStatus) of the publisher.
    ///
    /// [`MatchingStatus::matching`](zenoh::matching::MatchingStatus::matching)
    /// will return true if there exist Subscribers matching the Publisher's key expression and false otherwise.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh_ext::AdvancedPublisherBuilderExt;
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let publisher = session.declare_publisher("key/expression").advanced().await.unwrap();
    /// let matching_subscribers: bool = publisher
    ///     .matching_status()
    ///     .await
    ///     .unwrap()
    ///     .matching();
    /// # }
    /// ```
    #[zenoh_macros::unstable]
    pub fn matching_status(&self) -> impl Resolve<ZResult<zenoh::matching::MatchingStatus>> + '_ {
        self.publisher.matching_status()
    }

    /// Return a [`MatchingListener`](zenoh::matching::MatchingStatus) for this Publisher.
    ///
    /// The [`MatchingListener`](zenoh::matching::MatchingStatus) that will send a notification each time
    /// the [`MatchingStatus`](zenoh::matching::MatchingStatus) of the Publisher changes.
    ///
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh_ext::AdvancedPublisherBuilderExt;
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let publisher = session.declare_publisher("key/expression").advanced().await.unwrap();
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
    #[zenoh_macros::unstable]
    pub fn matching_listener(
        &self,
    ) -> zenoh::matching::MatchingListenerBuilder<'_, zenoh::handlers::DefaultHandler> {
        self.publisher.matching_listener()
    }

    /// Undeclares the [`Publisher`], informing the network that it needn't optimize publications for its key expression anymore.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh_ext::AdvancedPublisherBuilderExt;
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let publisher = session.declare_publisher("key/expression").advanced().await.unwrap();
    /// publisher.undeclare().await.unwrap();
    /// # }
    /// ```
    #[zenoh_macros::unstable]
    pub fn undeclare(self) -> impl Resolve<ZResult<()>> + 'a {
        self.publisher.undeclare()
    }
}

#[zenoh_macros::unstable]
pub type AdvancedPublisherPutBuilder<'a> = AdvancedPublicationBuilder<'a, PublicationBuilderPut>;
#[zenoh_macros::unstable]
pub type AdvancedPublisherDeleteBuilder<'a> =
    AdvancedPublicationBuilder<'a, PublicationBuilderDelete>;

#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
#[derive(Clone)]
#[zenoh_macros::unstable]
pub struct AdvancedPublicationBuilder<'a, P> {
    pub(crate) builder: PublicationBuilder<&'a Publisher<'a>, P>,
    pub(crate) cache: Option<&'a AdvancedCache>,
}

#[zenoh_macros::internal_trait]
#[zenoh_macros::unstable]
impl EncodingBuilderTrait for AdvancedPublicationBuilder<'_, PublicationBuilderPut> {
    #[zenoh_macros::unstable]
    fn encoding<T: Into<Encoding>>(self, encoding: T) -> Self {
        Self {
            builder: self.builder.encoding(encoding),
            ..self
        }
    }
}

#[zenoh_macros::internal_trait]
#[zenoh_macros::unstable]
impl<P> SampleBuilderTrait for AdvancedPublicationBuilder<'_, P> {
    #[zenoh_macros::unstable]
    fn source_info(self, source_info: SourceInfo) -> Self {
        Self {
            builder: self.builder.source_info(source_info),
            ..self
        }
    }
    #[zenoh_macros::unstable]
    fn attachment<TA: Into<OptionZBytes>>(self, attachment: TA) -> Self {
        let attachment: OptionZBytes = attachment.into();
        Self {
            builder: self.builder.attachment(attachment),
            ..self
        }
    }
}

#[zenoh_macros::internal_trait]
#[zenoh_macros::unstable]
impl<P> TimestampBuilderTrait for AdvancedPublicationBuilder<'_, P> {
    #[zenoh_macros::unstable]
    fn timestamp<TS: Into<Option<uhlc::Timestamp>>>(self, timestamp: TS) -> Self {
        Self {
            builder: self.builder.timestamp(timestamp),
            ..self
        }
    }
}

#[zenoh_macros::unstable]
impl<P> Resolvable for AdvancedPublicationBuilder<'_, P> {
    type To = ZResult<()>;
}

#[zenoh_macros::unstable]
impl Wait for AdvancedPublisherPutBuilder<'_> {
    #[inline]
    #[zenoh_macros::unstable]
    fn wait(self) -> <Self as Resolvable>::To {
        if let Some(cache) = self.cache {
            cache.cache_sample(zenoh::sample::Sample::from(&self.builder));
        }
        self.builder.wait()
    }
}

#[zenoh_macros::unstable]
impl Wait for AdvancedPublisherDeleteBuilder<'_> {
    #[inline]
    #[zenoh_macros::unstable]
    fn wait(self) -> <Self as Resolvable>::To {
        if let Some(cache) = self.cache {
            cache.cache_sample(zenoh::sample::Sample::from(&self.builder));
        }
        self.builder.wait()
    }
}

#[zenoh_macros::unstable]
impl IntoFuture for AdvancedPublisherPutBuilder<'_> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    #[zenoh_macros::unstable]
    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

#[zenoh_macros::unstable]
impl IntoFuture for AdvancedPublisherDeleteBuilder<'_> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    #[zenoh_macros::unstable]
    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}
