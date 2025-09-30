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
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    time::Duration,
};

use zenoh::{
    bytes::{Encoding, OptionZBytes, ZBytes},
    internal::{
        bail,
        runtime::ZRuntime,
        traits::{
            EncodingBuilderTrait, QoSBuilderTrait, SampleBuilderTrait, TimestampBuilderTrait,
        },
        TerminatableTask,
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
    Resolvable, Resolve, Result as ZResult, Session, Wait, KE_ADV_PREFIX, KE_EMPTY,
};
use zenoh_macros::ke;

use crate::{
    advanced_cache::{AdvancedCache, AdvancedCacheBuilder, CacheConfig, KE_UHLC},
    z_serialize,
};

pub(crate) static KE_PUB: &keyexpr = ke!("pub");

#[derive(PartialEq)]
#[zenoh_macros::unstable]
pub(crate) enum Sequencing {
    None,
    Timestamp,
    SequenceNumber,
}

/// Configuration for sample miss detection
///
/// Enabling [`sample_miss_detection`](crate::AdvancedPublisherBuilder::sample_miss_detection) in [`AdvancedPublisher`](crate::AdvancedPublisher)
/// allows [`AdvancedSubscribers`](crate::AdvancedSubscriber) to detect missed samples
/// through [`sample_miss_listener`](crate::AdvancedSubscriber::sample_miss_listener)
/// and to recover missed samples through [`recovery`](crate::AdvancedSubscriberBuilder::recovery).
#[zenoh_macros::unstable]
#[derive(Debug, Default, Clone)]
pub struct MissDetectionConfig {
    pub(crate) state_publisher: Option<(Duration, bool)>,
}

#[zenoh_macros::unstable]
impl MissDetectionConfig {
    /// Allow last sample miss detection through periodic heartbeat.
    ///
    /// Periodically send the last published sample's sequence number to allow last sample recovery.
    ///
    /// [`heartbeat`](MissDetectionConfig::heartbeat) and [`sporadic_heartbeat`](MissDetectionConfig::sporadic_heartbeat)
    /// are mutually exclusive. Enabling one will disable the other.
    ///
    /// [`AdvancedSubscribers`](crate::AdvancedSubscriber) can recover last sample with the
    /// [`heartbeat`](crate::advanced_subscriber::RecoveryConfig::heartbeat) option.
    #[zenoh_macros::unstable]
    pub fn heartbeat(mut self, period: Duration) -> Self {
        self.state_publisher = Some((period, false));
        self
    }

    /// Allow last sample miss detection through sporadic heartbeat.
    ///
    /// Each period, the last published sample's sequence number is sent with [`CongestionControl::Block`]
    /// but only if it changed since the last period.
    ///
    /// [`heartbeat`](MissDetectionConfig::heartbeat) and [`sporadic_heartbeat`](MissDetectionConfig::sporadic_heartbeat)
    /// are mutually exclusive. Enabling one will disable the other.
    ///
    /// [`AdvancedSubscribers`](crate::AdvancedSubscriber) can recover last sample with the
    /// [`heartbeat`](crate::advanced_subscriber::RecoveryConfig::heartbeat) option.
    #[zenoh_macros::unstable]
    pub fn sporadic_heartbeat(mut self, period: Duration) -> Self {
        self.state_publisher = Some((period, true));
        self
    }
}

/// The builder of AdvancedPublisher, allowing to configure it.
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
    miss_config: Option<MissDetectionConfig>,
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
            miss_config: None,
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

    /// Allow matching [`AdvancedSubscribers`](crate::AdvancedSubscriber) to detect lost samples and optionally ask for retransmission.
    ///
    /// Retransmission can only be achieved if [`cache`](crate::AdvancedPublisherBuilder::cache) is enabled.
    #[zenoh_macros::unstable]
    pub fn sample_miss_detection(mut self, config: MissDetectionConfig) -> Self {
        self.sequencing = Sequencing::SequenceNumber;
        self.miss_config = Some(config);
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

/// The extension to a [`Publisher`](zenoh::pubsub::Publisher) providing advanced functionalities.
///
/// The `AdvancedPublisher` is constructed over a regular [`Publisher`](zenoh::pubsub::Publisher) through
/// [`advanced`](crate::AdvancedPublisherBuilderExt::advanced) method or by using
/// any other method of [`AdvancedPublisherBuilder`](crate::AdvancedPublisherBuilder).
///
/// The `AdvancedPublisher` works with [`AdvancedSubscriber`](crate::AdvancedSubscriber) to provide additional functionalities such as:
/// - [`cache`](crate::AdvancedPublisherBuilderExt::cache) last published samples to be retrieved by
///   [`AdvancedSubscriber`](crate::AdvancedSubscriber)'s [`history`](crate::AdvancedSubscriberBuilderExt::history) mechanism
/// - [`sample_miss_detection`](crate::AdvancedPublisherBuilderExt::sample_miss_detection) to allow detecting missed samples
///   using [`AdvancedSubscriber`](crate::AdvancedSubscriber)'s [`sample_miss_listener`](crate::AdvancedSubscriber::sample_miss_listener)
/// - [`publisher_detection`](crate::AdvancedPublisherBuilderExt::publisher_detection) to create a Liveliness token to assert its presence and
///   allow it to be requested for missed samples if [`detect_late_publishers`](crate::HistoryConfig::detect_late_publishers) is enabled
///
/// # Example
/// ```no_run
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh_ext::{AdvancedPublisherBuilderExt, CacheConfig, MissDetectionConfig};
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let publisher = session
///     .declare_publisher("key/expression")
///     .cache(CacheConfig::default().max_samples(10))
///     .sample_miss_detection(
///         MissDetectionConfig::default().heartbeat(std::time::Duration::from_secs(1))
///     )
///     .publisher_detection()
///     .await
///     .unwrap();
/// publisher.put("Value").await.unwrap();
/// # }
/// ```
#[zenoh_macros::unstable]
pub struct AdvancedPublisher<'a> {
    publisher: Publisher<'a>,
    seqnum: Option<Arc<AtomicU32>>,
    cache: Option<AdvancedCache>,
    _token: Option<LivelinessToken>,
    _state_publisher: Option<TerminatableTask>,
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
        tracing::debug!("Create AdvancedPublisher{{key_expr: {}}}", &key_expr);

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
        let suffix = KE_ADV_PREFIX / KE_PUB / &id.zid().into_keyexpr();
        let suffix = match conf.sequencing {
            Sequencing::SequenceNumber => {
                suffix / &KeyExpr::try_from(id.eid().to_string()).unwrap()
            }
            _ => suffix / KE_UHLC,
        };
        let suffix = match meta {
            Some(meta) => suffix / &meta,
            // We need this empty chunk because of a routing matching bug
            _ => suffix / KE_EMPTY,
        };

        let seqnum = match conf.sequencing {
            Sequencing::SequenceNumber => Some(Arc::new(AtomicU32::new(0))),
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
                    .queryable_suffix(&suffix)
                    .wait()?,
            )
        } else {
            None
        };

        let token = if conf.liveliness {
            tracing::debug!(
                "AdvancedPublisher{{key_expr: {}}}: Declare liveliness token {}",
                key_expr,
                &key_expr / &suffix,
            );
            Some(
                conf.session
                    .liveliness()
                    .declare_token(&key_expr / &suffix)
                    .wait()?,
            )
        } else {
            None
        };

        let state_publisher = if let Some((period, sporadic)) =
            conf.miss_config.as_ref().and_then(|c| c.state_publisher)
        {
            if let Some(seqnum) = seqnum.as_ref() {
                tracing::debug!(
                    "AdvancedPublisher{{key_expr: {}}}: Enable {}heartbeat on {} with period {:?}",
                    key_expr,
                    if sporadic { "sporadic " } else { "" },
                    &key_expr / &suffix,
                    period
                );
                let seqnum = seqnum.clone();
                if !sporadic {
                    let publisher = conf.session.declare_publisher(&key_expr / &suffix).wait()?;
                    Some(TerminatableTask::spawn_abortable(
                        ZRuntime::Net,
                        async move {
                            loop {
                                tokio::time::sleep(period).await;
                                let seqnum = seqnum.load(Ordering::Relaxed);
                                if seqnum > 0 {
                                    let _ = publisher.put(z_serialize(&(seqnum - 1))).await;
                                }
                            }
                        },
                    ))
                } else {
                    let mut last_seqnum = 0;
                    let publisher = conf
                        .session
                        .declare_publisher(&key_expr / &suffix)
                        .congestion_control(CongestionControl::Block)
                        .wait()?;
                    Some(TerminatableTask::spawn_abortable(
                        ZRuntime::Net,
                        async move {
                            loop {
                                tokio::time::sleep(period).await;
                                let seqnum = seqnum.load(Ordering::Relaxed);
                                if seqnum > last_seqnum {
                                    let _ = publisher.put(z_serialize(&(seqnum - 1))).await;
                                    last_seqnum = seqnum;
                                }
                            }
                        },
                    ))
                }
            } else {
                None
            }
        } else {
            None
        };

        Ok(AdvancedPublisher {
            publisher,
            seqnum,
            cache,
            _token: token,
            _state_publisher: state_publisher,
        })
    }

    /// Returns the [`EntityGlobalId`] of this Publisher.
    ///
    /// Wraps [`Publisher::id`](zenoh::pubsub::Publisher::id) method
    #[zenoh_macros::unstable]
    pub fn id(&self) -> EntityGlobalId {
        self.publisher.id()
    }

    /// Returns the [`KeyExpr`] of this Publisher.
    ///
    /// Wraps [`Publisher::key_expr`](zenoh::pubsub::Publisher::key_expr) method
    #[inline]
    #[zenoh_macros::unstable]
    pub fn key_expr(&self) -> &KeyExpr<'a> {
        self.publisher.key_expr()
    }

    /// Get the [`Encoding`] used when publishing data.
    ///
    /// Wraps [`Publisher::encoding`](zenoh::pubsub::Publisher::encoding) method
    #[inline]
    #[zenoh_macros::unstable]
    pub fn encoding(&self) -> &Encoding {
        self.publisher.encoding()
    }

    /// Get the `congestion_control` applied when routing the data.
    ///
    /// Wraps [`Publisher::congestion_control`](zenoh::pubsub::Publisher::congestion_control) method
    #[inline]
    #[zenoh_macros::unstable]
    pub fn congestion_control(&self) -> CongestionControl {
        self.publisher.congestion_control()
    }

    /// Get the priority of the written data.
    ///
    /// Wraps [`Publisher::priority`](zenoh::pubsub::Publisher::priority) method
    #[inline]
    #[zenoh_macros::unstable]
    pub fn priority(&self) -> Priority {
        self.publisher.priority()
    }

    /// Put data.
    ///
    /// Wraps [`Publisher::put`](zenoh::pubsub::Publisher::put) method
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
            let info = SourceInfo::new(
                Some(self.publisher.id()),
                Some(seqnum.fetch_add(1, Ordering::Relaxed)),
            );
            tracing::trace!(
                "AdvancedPublisher{{key_expr: {}}}: Put data with {:?}",
                self.publisher.key_expr(),
                info
            );
            builder = builder.source_info(info);
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
    /// Wraps [`Publisher::delete`](zenoh::pubsub::Publisher::delete) method
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
    /// Wraps [`Publisher::matching_status`](zenoh::pubsub::Publisher::matching_status) method.
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
    /// Wraps [`Publisher::matching_listener`](zenoh::pubsub::Publisher::matching_listener) method.
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
    /// Wraps [`Publisher::undeclare`](zenoh::pubsub::Publisher::undeclare) method
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
        tracing::debug!(
            "AdvancedPublisher{{key_expr: {}}}: Undeclare",
            self.key_expr()
        );
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
