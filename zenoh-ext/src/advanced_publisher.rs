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
    fmt,
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
            EncodingBuilderTrait, FragInfoBuilderTrait, QoSBuilderTrait, SampleBuilderTrait,
            TimestampBuilderTrait, TimestampInstrumentationBuilderTrait,
        },
        zerror, TerminatableTask,
    },
    key_expr::{keyexpr, KeyExpr},
    liveliness::LivelinessToken,
    pubsub::{
        PublicationBuilder, PublicationBuilderDelete, PublicationBuilderPut, Publisher,
        PublisherBuilder, PublisherPutBuilder, PublisherUndeclaration,
    },
    qos::{CongestionControl, Priority, Reliability},
    sample::{FragInfo, Locality, Sample, SourceInfo},
    session::EntityGlobalId,
    Resolvable, Resolve, Result as ZResult, Session, Wait, KE_ADV_PREFIX, KE_EMPTY,
};
use zenoh_macros::ke;

use crate::{
    advanced_cache::{AdvancedCache, AdvancedCacheBuilder, CacheConfig, KE_UHLC},
    fragmentation::fragment_count,
    z_serialize,
};

pub(crate) static KE_PUB: &keyexpr = ke!("pub");

#[derive(Debug, PartialEq)]
#[zenoh_macros::unstable]
pub(crate) enum Sequencing {
    None,
    Timestamp,
    SequenceNumber,
}

/// Configure sample miss detection.
///
/// By default, misses are detected when later publications arrive. Enable a heartbeat
/// to also allow detection of missing trailing publications. Recovery additionally
/// requires a publisher [`cache`](AdvancedPublisherBuilder::cache).
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

/// Configure an [`AdvancedPublisher`].
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
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
    history: Option<CacheConfig>,
    fragmentation: Option<usize>,
}

#[zenoh_macros::unstable]
impl fmt::Debug for AdvancedPublisherBuilder<'_, '_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AdvancedPublisherBuilder")
            .field("session", &"..")
            .field("pub_key_expr", &self.pub_key_expr)
            .field("encoding", &self.encoding)
            .field("destination", &self.destination)
            .field("reliability", &self.reliability)
            .field("congestion_control", &self.congestion_control)
            .field("priority", &self.priority)
            .field("is_express", &self.is_express)
            .field("meta_key_expr", &self.meta_key_expr)
            .field("sequencing", &self.sequencing)
            .field("miss_config", &self.miss_config)
            .field("liveliness", &self.liveliness)
            .field("history", &self.history)
            .field("fragmentation", &self.fragmentation)
            .finish()
    }
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
            history: None,
            fragmentation: None,
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
    /// The cache serves history and recovery while this publisher is alive. Without
    /// [`sample_miss_detection`](Self::sample_miss_detection), the session must enable
    /// `timestamping/enabled` or publisher declaration fails. Recovery is limited to
    /// publications still retained in the cache.
    #[zenoh_macros::unstable]
    pub fn cache(mut self, config: CacheConfig) -> Self {
        if self.sequencing == Sequencing::None {
            self.sequencing = Sequencing::Timestamp;
        }
        self.history = Some(config);
        self
    }

    /// Allow this [`AdvancedPublisher`] to be detected by [`AdvancedSubscribers`](crate::AdvancedSubscriber).
    ///
    /// Subscribers using [`HistoryConfig::detect_late_publishers`](crate::HistoryConfig::detect_late_publishers)
    /// can retrieve this publisher's cached history when they discover it.
    #[zenoh_macros::unstable]
    pub fn publisher_detection(mut self) -> Self {
        self.liveliness = true;
        self
    }

    /// A key expression added to the liveliness token key expression and to the cache queryable key expression.
    ///
    /// It can be used to convey metadata.
    #[zenoh_macros::unstable]
    pub fn publisher_detection_metadata<TryIntoKeyExpr>(mut self, meta: TryIntoKeyExpr) -> Self
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'c>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'c>>>::Error: Into<zenoh::Error>,
    {
        self.meta_key_expr = Some(meta.try_into().map_err(Into::into));
        self
    }

    /// Split payloads larger than `size` bytes into independently delivered fragments.
    ///
    /// Requires [`sample_miss_detection`](Self::sample_miss_detection). Publisher
    /// declaration fails if it is disabled or `size` is zero. Payloads at or below
    /// `size` are sent whole. The size excludes metadata and attachments, so it is not a packet-size limit.
    ///
    /// [`AdvancedSubscriber`](crate::AdvancedSubscriber) automatically reassembles
    /// fragments; ordinary subscribers and direct cache queries receive partial payloads.
    /// The first fragment carries the publication's timestamp and attachment, which
    /// are preserved on the reassembled sample. Missing fragments can be recovered
    /// when the publisher enables [`cache`](Self::cache) and the subscriber enables
    /// [`recovery`](crate::AdvancedSubscriberBuilder::recovery).
    ///
    /// Choose `size` to stay within the receivers' [`max_fragments`](crate::AdvancedSubscriberBuilder::max_fragments)
    /// limit; receivers reject publications exceeding it. More than `u32::MAX`
    /// fragments causes a publication error.
    ///
    /// Once a fragmented publication is prepared, the publisher attempts to send
    /// every fragment and discards individual send errors. Preparation errors, including
    /// fragment-count overflow, are still returned to the caller.
    ///
    /// NOTE: to filter fragmented publications on ordinary subscribers, use the Sample's frag_info API
    /// to discard samples with fragmentation metadata.
    #[zenoh_macros::unstable]
    #[inline]
    pub fn fragmentation(self, size: usize) -> Self {
        Self {
            fragmentation: Some(size),
            ..self
        }
    }
}

#[zenoh_macros::internal_trait]
#[zenoh_macros::unstable]
impl EncodingBuilderTrait for AdvancedPublisherBuilder<'_, '_, '_> {
    /// Set the [`Encoding`]
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
    /// Changes the [`CongestionControl`] to apply when routing the data.
    #[inline]
    #[zenoh_macros::unstable]
    fn congestion_control(self, congestion_control: CongestionControl) -> Self {
        Self {
            congestion_control,
            ..self
        }
    }

    /// Changes the [`Priority`] to apply when routing the data.
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

/// A publisher supporting caching, loss detection, discovery, and fragmentation.
///
/// Create one through [`AdvancedPublisherBuilderExt`](crate::AdvancedPublisherBuilderExt).
/// Features are opt-in:
///
/// - [`cache`](AdvancedPublisherBuilder::cache): retain publications for history and recovery.
/// - [`sample_miss_detection`](AdvancedPublisherBuilder::sample_miss_detection): enable subscriber loss detection.
/// - [`publisher_detection`](AdvancedPublisherBuilder::publisher_detection): advertise presence for discovery.
/// - [`fragmentation`](AdvancedPublisherBuilder::fragmentation): split large payloads for advanced subscribers.
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
    fragmentation: Option<usize>,
    _token: Option<LivelinessToken>,
    _state_publisher: Option<TerminatableTask>,
}

#[zenoh_macros::unstable]
impl fmt::Debug for AdvancedPublisher<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AdvancedPublisher")
            .field("publisher", &self.publisher)
            .field(
                "seqnum",
                &self
                    .seqnum
                    .as_ref()
                    .map(|seqnum| seqnum.load(Ordering::Relaxed)),
            )
            .field("cache", &self.cache.as_ref().map(|_| ".."))
            .field("token", &self._token.as_ref().map(|_| ".."))
            .field("state_publisher", &self._state_publisher)
            .finish()
    }
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
        if let Some(fragmentation_size) = conf.fragmentation {
            if fragmentation_size == 0 {
                bail!("fragmentation size must be non-zero");
            }
            if conf.sequencing != Sequencing::SequenceNumber {
                bail!(
                    "fragmentation requires sample_miss_detection (sequence-number sequencing) \
                     for fragment reassembly on the subscriber side"
                );
            }
        }
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

        let cache = conf
            .history
            .map(|h| {
                AdvancedCacheBuilder::new(conf.session, Ok(key_expr.clone()))
                    .history(h)
                    .queryable_suffix(&suffix)
                    .wait()
            })
            .transpose()?;

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
            fragmentation: conf.fragmentation,
            _token: token,
            _state_publisher: state_publisher,
        })
    }

    fn publication_identity(&self, operation: &str) -> Option<SourceInfo> {
        let seqnum = self.seqnum.as_ref()?;
        let info = Some(SourceInfo::new(
            self.id(),
            seqnum.fetch_add(1, Ordering::Relaxed),
        ));
        tracing::trace!(
            "AdvancedPublisher{{key_expr: {}}}: {} data with {:?}",
            self.key_expr(),
            operation,
            info
        );
        info
    }

    fn publication_timestamp(&self) -> Option<uhlc::Timestamp> {
        self.publisher
            .session()
            .hlc()
            .map(|hlc| hlc.new_timestamp())
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
        AdvancedPublisherPutBuilder {
            publisher: self,
            builder: self.publisher.put(payload),
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
        AdvancedPublisherDeleteBuilder {
            publisher: self,
            builder: self.publisher.delete(),
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

    /// Return a [`MatchingListener`](zenoh::matching::MatchingListener) for this Publisher.
    ///
    /// Wraps [`Publisher::matching_listener`](zenoh::pubsub::Publisher::matching_listener) method.
    ///
    /// The [`MatchingListener`](zenoh::matching::MatchingListener) will send a notification each time
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
    pub fn undeclare(self) -> PublisherUndeclaration<'a> {
        tracing::debug!(
            "AdvancedPublisher{{key_expr: {}}}: Undeclare",
            self.key_expr()
        );
        self.publisher.undeclare()
    }
}

/// Builder for publishing data through an [`AdvancedPublisher`].
#[zenoh_macros::unstable]
pub type AdvancedPublisherPutBuilder<'a> = AdvancedPublicationBuilder<'a, PublicationBuilderPut>;
/// Builder for publishing a deletion through an [`AdvancedPublisher`].
#[zenoh_macros::unstable]
pub type AdvancedPublisherDeleteBuilder<'a> =
    AdvancedPublicationBuilder<'a, PublicationBuilderDelete>;

/// Configure an advanced publication and send it with `.await` or [`Wait::wait`].
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
#[derive(Clone)]
#[zenoh_macros::unstable]
pub struct AdvancedPublicationBuilder<'a, P> {
    pub(crate) publisher: &'a AdvancedPublisher<'a>,
    pub(crate) builder: PublicationBuilder<&'a Publisher<'a>, P>,
}

#[zenoh_macros::unstable]
impl<P> fmt::Debug for AdvancedPublicationBuilder<'_, P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AdvancedPublicationBuilder")
            .field("builder", &"..")
            .field("has_cache", &self.publisher.cache.is_some())
            .finish()
    }
}

#[zenoh_macros::internal_trait]
#[zenoh_macros::unstable]
impl EncodingBuilderTrait for AdvancedPublicationBuilder<'_, PublicationBuilderPut> {
    /// Set the [`Encoding`]
    #[zenoh_macros::unstable]
    fn encoding<T: Into<Encoding>>(self, encoding: T) -> Self {
        Self {
            builder: self.builder.encoding(encoding),
            ..self
        }
    }
}

#[zenoh_macros::unstable]
impl<P> AdvancedPublicationBuilder<'_, P> {
    /// Sets an optional attachment to be sent along with the publication.
    ///
    /// The argument is converted via [`OptionZBytes`], which supports both `T: Into<ZBytes>`
    /// and `Option<T>` where `T: Into<ZBytes>`.
    #[zenoh_macros::unstable]
    pub fn attachment<TA: Into<OptionZBytes>>(self, attachment: TA) -> Self {
        let attachment: OptionZBytes = attachment.into();
        Self {
            builder: self.builder.attachment(attachment),
            ..self
        }
    }
}

#[zenoh_macros::unstable]
impl<P> SampleBuilderTrait for AdvancedPublicationBuilder<'_, P> {
    #[zenoh_macros::unstable]
    fn source_info<TS: Into<Option<SourceInfo>>>(self, source_info: TS) -> Self {
        Self {
            builder: self.builder.source_info(source_info),
            ..self
        }
    }

    #[zenoh_macros::unstable]
    fn attachment<TA: Into<OptionZBytes>>(self, attachment: TA) -> Self {
        self.attachment(attachment)
    }
}

#[zenoh_macros::internal_trait]
#[zenoh_macros::unstable]
impl<P> FragInfoBuilderTrait for AdvancedPublicationBuilder<'_, P> {
    /// Attach fragmentation metadata without splitting the payload.
    ///
    /// This is an internal API used by [`AdvancedPublisher`]. Applications should use
    /// [`AdvancedPublisherBuilder::fragmentation`] instead of setting metadata manually.
    /// Fragments of one publication share the same [`SourceInfo`] and fragment count,
    /// with zero-based indices less than that count.
    /// [`AdvancedSubscriber`](crate::AdvancedSubscriber) discards fragments without
    /// [`SourceInfo`].
    #[zenoh_macros::unstable]
    fn frag_info<TF: Into<Option<FragInfo>>>(self, frag_info: TF) -> Self {
        Self {
            builder: self.builder.frag_info(frag_info),
            ..self
        }
    }
}

#[zenoh_macros::internal_trait]
#[zenoh_macros::unstable]
impl<P> TimestampBuilderTrait for AdvancedPublicationBuilder<'_, P> {
    /// Sets an optional timestamp to be sent along with the publication.
    #[zenoh_macros::unstable]
    fn timestamp<TS: Into<Option<uhlc::Timestamp>>>(self, timestamp: TS) -> Self {
        Self {
            builder: self.builder.timestamp(timestamp),
            ..self
        }
    }
}

#[zenoh_macros::internal_trait]
#[zenoh_macros::unstable]
impl<P> TimestampInstrumentationBuilderTrait for AdvancedPublicationBuilder<'_, P> {
    #[zenoh_macros::unstable]
    fn timestamp_instrumentation<
        TS: Into<Option<zenoh::timestamp_stack::TimestampInstrumentation>>,
    >(
        self,
        instrumentation: TS,
    ) -> Self {
        Self {
            builder: self.builder.timestamp_instrumentation(instrumentation),
            ..self
        }
    }
}

#[zenoh_macros::unstable]
impl<P> Resolvable for AdvancedPublicationBuilder<'_, P> {
    type To = ZResult<()>;
}

impl<'a> AdvancedPublisherPutBuilder<'a> {
    fn prepare_fragments(
        publisher: &AdvancedPublisher<'_>,
        publication: &PublisherPutBuilder<'a>,
        size: usize,
    ) -> ZResult<Vec<PublisherPutBuilder<'a>>> {
        // `to_bytes` is only called if fragmentation is necessary,
        // to avoid copy for small messages.
        let bytes = publication.payload_ref().to_bytes();
        // NOTE: this consumes a sequence number even if the
        // publication is refused, leaving a gap that subscribers will
        // account for.
        let frag_count = fragment_count(bytes.len(), size).map_err(|e| {
            zerror!(
                "AdvancedPublisher{{key_expr: {}}}: {e}",
                publisher.key_expr()
            )
        })?;
        let mut builders = Vec::with_capacity(frag_count as usize);
        for (i, chunk) in bytes.chunks(size).enumerate() {
            let mut builder = publication
                .clone()
                .payload(chunk)
                .frag_info(FragInfo::new(frag_count, i as u32));
            match i {
                0 => {
                    // fragment 0 carries timestamp and attachment of original message
                    if let Some(timestamp) = publisher.publication_timestamp() {
                        builder = builder.timestamp(timestamp);
                    }
                }
                _ => {
                    // clear timestamp and attachment for fragments that aren't frag 0
                    // NOTE: timestamp is cleared here to avoid duplicate HLC clock values
                    builder = builder.timestamp(None).attachment(None::<ZBytes>);
                }
            }
            builders.push(builder);
        }
        Ok(builders)
    }

    fn send_fragments(builders: impl IntoIterator<Item = impl Wait<To = ZResult<()>>>) {
        // Attempt every fragment and discard individual send errors. Preparation
        // errors still propagate before anything is cached or sent.
        for builder in builders {
            let _ = builder.wait();
        }
    }
}

#[zenoh_macros::unstable]
impl Wait for AdvancedPublisherPutBuilder<'_> {
    #[inline]
    #[zenoh_macros::unstable]
    fn wait(self) -> <Self as Resolvable>::To {
        let Self {
            publisher,
            mut builder,
        } = self;
        if let Some(info) = publisher.publication_identity("Put") {
            builder = builder.source_info(info);
        }
        if let Some(size) = publisher.fragmentation {
            if builder.payload_ref().len() > size {
                let builders = Self::prepare_fragments(publisher, &builder, size)?;
                if let Some(cache) = publisher.cache.as_ref() {
                    // Cache the complete batch before sending even the first fragment.
                    cache.cache_fragments(builders.iter().map(Sample::from).collect());
                }
                Self::send_fragments(builders);
                return Ok(());
            }
        }
        if let Some(timestamp) = publisher.publication_timestamp() {
            builder = builder.timestamp(timestamp);
        }
        if let Some(cache) = publisher.cache.as_ref() {
            cache.cache_sample(Sample::from(&builder));
        }
        builder.wait()
    }
}

#[zenoh_macros::unstable]
impl Wait for AdvancedPublisherDeleteBuilder<'_> {
    #[inline]
    #[zenoh_macros::unstable]
    fn wait(self) -> <Self as Resolvable>::To {
        let Self {
            publisher,
            mut builder,
        } = self;
        if let Some(info) = publisher.publication_identity("Del") {
            builder = builder.source_info(info);
        }
        if let Some(timestamp) = publisher.publication_timestamp() {
            builder = builder.timestamp(timestamp);
        }
        if let Some(cache) = publisher.cache.as_ref() {
            cache.cache_sample(Sample::from(&builder));
        }
        builder.wait()
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

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, time::Duration};

    use zenoh::{sample::SampleKind, time::Timestamp, Config, Resolvable, Result as ZResult, Wait};

    use crate::{AdvancedPublisherBuilderExt, MissDetectionConfig};

    #[test]
    fn deferred_and_cloned_publications_get_identity_when_resolved() {
        let mut config = Config::default();
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
        config.listen.endpoints.set(vec![]).unwrap();
        let session = zenoh::open(config).wait().unwrap();
        let key = "test/advanced_publisher/deferred";
        let subscriber = session.declare_subscriber(key).wait().unwrap();
        let publisher = session
            .declare_publisher(key)
            .sample_miss_detection(MissDetectionConfig::default())
            .wait()
            .unwrap();

        let deferred = publisher.put("deferred");
        let cloned = deferred.clone();
        let deletion = publisher.delete();
        publisher.put("first").wait().unwrap();
        cloned.wait().unwrap();
        deletion.wait().unwrap();
        deferred.wait().unwrap();

        for (sn, kind, payload) in [
            (0, SampleKind::Put, "first"),
            (1, SampleKind::Put, "deferred"),
            (2, SampleKind::Delete, ""),
            (3, SampleKind::Put, "deferred"),
        ] {
            let sample = subscriber
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .unwrap();
            let info = sample.source_info().unwrap();
            assert_eq!(*info.source_id(), publisher.id());
            assert_eq!(info.source_sn(), sn);
            assert_eq!(sample.kind(), kind);
            assert_eq!(sample.payload().try_to_string().unwrap(), payload);
        }
        assert!(subscriber.try_recv().unwrap().is_none());
        session.close().wait().unwrap();
    }

    #[test]
    fn publication_metadata_survives_whole_fragmented_and_delete_paths() {
        for timestamping in [false, true] {
            for fragmentation in [false, true] {
                let mut config = Config::default();
                config.scouting.multicast.set_enabled(Some(false)).unwrap();
                config.listen.endpoints.set(vec![]).unwrap();
                config
                    .insert_json5("timestamping/enabled", &timestamping.to_string())
                    .unwrap();
                let session = zenoh::open(config).wait().unwrap();
                let key = "test/advanced_publisher/metadata";
                let subscriber = session.declare_subscriber(key).wait().unwrap();
                let mut publisher = session
                    .declare_publisher(key)
                    .sample_miss_detection(MissDetectionConfig::default());
                if fragmentation {
                    publisher = publisher.fragmentation(2);
                }
                let publisher = publisher.wait().unwrap();
                let timestamp = Timestamp::new(Duration::from_secs(1).into(), session.zid().into());

                for (sn, payload) in ["a", "ab", "abc"].into_iter().enumerate() {
                    publisher
                        .put(payload)
                        .timestamp(timestamp)
                        .attachment("metadata")
                        .wait()
                        .unwrap();
                    let chunks: Vec<_> = payload
                        .as_bytes()
                        .chunks(if fragmentation { 2 } else { payload.len() })
                        .collect();
                    for (index, chunk) in chunks.iter().enumerate() {
                        let sample = subscriber
                            .recv_timeout(Duration::from_secs(5))
                            .unwrap()
                            .unwrap();
                        assert_eq!(sample.payload().to_bytes().as_ref(), *chunk);
                        assert_eq!(sample.source_info().unwrap().source_sn(), sn as u32);
                        assert_eq!(
                            sample
                                .frag_info()
                                .map(|info| (info.frag_num(), info.frag_count())),
                            (chunks.len() > 1).then_some((index as u32, chunks.len() as u32))
                        );
                        if index == 0 {
                            assert_eq!(
                                sample.attachment().unwrap().try_to_string().unwrap(),
                                "metadata"
                            );
                            if timestamping {
                                assert!(sample.timestamp().unwrap() > &timestamp);
                            } else {
                                assert_eq!(sample.timestamp(), Some(&timestamp));
                            }
                        } else {
                            assert!(sample.attachment().is_none());
                            if !timestamping {
                                assert!(sample.timestamp().is_none());
                            }
                        }
                    }
                }
                publisher.delete().timestamp(timestamp).wait().unwrap();
                let sample = subscriber
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap()
                    .unwrap();
                assert_eq!(sample.kind(), SampleKind::Delete);
                assert_eq!(sample.source_info().unwrap().source_sn(), 3);
                if timestamping {
                    assert!(sample.timestamp().unwrap() > &timestamp);
                } else {
                    assert_eq!(sample.timestamp(), Some(&timestamp));
                }
                assert!(subscriber.try_recv().unwrap().is_none());
                session.close().wait().unwrap();
            }
        }
    }

    struct FailingFragment<'a> {
        index: usize,
        attempts: &'a RefCell<Vec<usize>>,
    }

    impl Resolvable for FailingFragment<'_> {
        type To = ZResult<()>;
    }

    impl Wait for FailingFragment<'_> {
        fn wait(self) -> Self::To {
            self.attempts.borrow_mut().push(self.index);
            Err(zenoh::internal::zerror!("simulated fragment send failure").into())
        }
    }

    #[test]
    fn fragment_send_failures_do_not_stop_later_attempts() {
        let attempts = RefCell::new(Vec::new());
        super::AdvancedPublisherPutBuilder::send_fragments((0..3).map(|index| FailingFragment {
            index,
            attempts: &attempts,
        }));
        assert_eq!(*attempts.borrow(), [0, 1, 2]);
    }

    #[test]
    fn fragmented_publication_discards_send_errors() {
        let mut config = Config::default();
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
        config.listen.endpoints.set(vec![]).unwrap();
        let session = zenoh::open(config).wait().unwrap();
        let publisher = session
            .declare_publisher("test/advanced_publisher/send_errors")
            .sample_miss_detection(MissDetectionConfig::default())
            .fragmentation(2)
            .wait()
            .unwrap();
        session.close().wait().unwrap();

        // Each fragment's underlying publication fails on the closed session,
        // but the fragmented publication still succeeds after attempting them.
        publisher.put("abcdef").wait().unwrap();
        assert!(publisher.put("ab").wait().is_err());
        assert!(publisher.delete().wait().is_err());
    }
}
