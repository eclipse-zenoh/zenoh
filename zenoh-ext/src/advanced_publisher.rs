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
        traits::{EncodingBuilderTrait, SampleBuilderTrait, TimestampBuilderTrait},
    },
    key_expr::KeyExpr,
    liveliness::LivelinessToken,
    pubsub::{PublicationBuilder, PublicationBuilderDelete, PublicationBuilderPut, Publisher},
    qos::{CongestionControl, Priority},
    sample::SourceInfo,
    session::EntityGlobalId,
    Resolvable, Resolve, Result as ZResult, Session, Wait, KE_ADV_PREFIX, KE_AT, KE_EMPTY,
};

use crate::advanced_cache::{AdvancedCache, AdvancedCacheBuilder, CacheConfig, KE_UHLC};

#[derive(PartialEq)]
pub(crate) enum Sequencing {
    None,
    Timestamp,
    SequenceNumber,
}

/// The builder of PublicationCache, allowing to configure it.
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
pub struct AdvancedPublisherBuilder<'a, 'b, 'c> {
    session: &'a Session,
    pub_key_expr: ZResult<KeyExpr<'b>>,
    meta_key_expr: Option<ZResult<KeyExpr<'c>>>,
    sequencing: Sequencing,
    liveliness: bool,
    cache: bool,
    history: CacheConfig,
}

impl<'a, 'b, 'c> AdvancedPublisherBuilder<'a, 'b, 'c> {
    pub(crate) fn new(
        session: &'a Session,
        pub_key_expr: ZResult<KeyExpr<'b>>,
    ) -> AdvancedPublisherBuilder<'a, 'b, 'c> {
        AdvancedPublisherBuilder {
            session,
            pub_key_expr,
            meta_key_expr: None,
            sequencing: Sequencing::None,
            liveliness: false,
            cache: false,
            history: CacheConfig::default(),
        }
    }

    /// Allow matching Subscribers to detect lost samples and optionally ask for retransimission.
    ///
    /// Retransmission can only be achieved if history is enabled.
    pub fn sample_miss_detection(mut self) -> Self {
        self.sequencing = Sequencing::SequenceNumber;
        self
    }

    /// Change the history size for each resource.
    pub fn cache(mut self, config: CacheConfig) -> Self {
        self.cache = true;
        if self.sequencing == Sequencing::None {
            self.sequencing = Sequencing::Timestamp;
        }
        self.history = config;
        self
    }

    /// Allow this publisher to be detected by subscribers.
    ///
    /// This allows Subscribers to retrieve the local history.
    pub fn publisher_detection(mut self) -> Self {
        self.liveliness = true;
        self
    }

    /// A key expression added to the liveliness token key expression
    /// and to the cache queryable key expression.
    /// It can be used to convey meta data.
    pub fn publisher_detection_metadata<TryIntoKeyExpr>(mut self, meta: TryIntoKeyExpr) -> Self
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'c>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'c>>>::Error: Into<zenoh::Error>,
    {
        self.meta_key_expr = Some(meta.try_into().map_err(Into::into));
        self
    }
}

impl<'a> Resolvable for AdvancedPublisherBuilder<'a, '_, '_> {
    type To = ZResult<AdvancedPublisher<'a>>;
}

impl Wait for AdvancedPublisherBuilder<'_, '_, '_> {
    fn wait(self) -> <Self as Resolvable>::To {
        AdvancedPublisher::new(self)
    }
}

impl IntoFuture for AdvancedPublisherBuilder<'_, '_, '_> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

/// [`AdvancedPublisher`].
pub struct AdvancedPublisher<'a> {
    publisher: Publisher<'a>,
    seqnum: Option<AtomicU32>,
    cache: Option<AdvancedCache>,
    _token: Option<LivelinessToken>,
}

impl<'a> AdvancedPublisher<'a> {
    fn new(conf: AdvancedPublisherBuilder<'a, '_, '_>) -> ZResult<Self> {
        let key_expr = conf.pub_key_expr?;
        let meta = match conf.meta_key_expr {
            Some(meta) => Some(meta?),
            None => None,
        };

        let publisher = conf
            .session
            .declare_publisher(key_expr.clone().into_owned())
            .wait()?;
        let id = publisher.id();
        let prefix = KE_ADV_PREFIX / &id.zid().into_keyexpr();
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
                AdvancedCacheBuilder::new(conf.session, Ok(key_expr.clone().into_owned()))
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
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let publisher = session.declare_publisher("key/expression")
    ///     .await
    ///     .unwrap();
    /// let publisher_id = publisher.id();
    /// # }
    /// ```
    pub fn id(&self) -> EntityGlobalId {
        self.publisher.id()
    }

    #[inline]
    pub fn key_expr(&self) -> &KeyExpr<'a> {
        self.publisher.key_expr()
    }

    /// Get the [`Encoding`] used when publishing data.
    #[inline]
    pub fn encoding(&self) -> &Encoding {
        self.publisher.encoding()
    }

    /// Get the `congestion_control` applied when routing the data.
    #[inline]
    pub fn congestion_control(&self) -> CongestionControl {
        self.publisher.congestion_control()
    }

    /// Get the priority of the written data.
    #[inline]
    pub fn priority(&self) -> Priority {
        self.publisher.priority()
    }

    /// Put data.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let publisher = session.declare_publisher("key/expression").await.unwrap();
    /// publisher.put("value").await.unwrap();
    /// # }
    /// ```
    #[inline]
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
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let publisher = session.declare_publisher("key/expression").await.unwrap();
    /// publisher.delete().await.unwrap();
    /// # }
    /// ```
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
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let publisher = session.declare_publisher("key/expression").await.unwrap();
    /// publisher.undeclare().await.unwrap();
    /// # }
    /// ```
    pub fn undeclare(self) -> impl Resolve<ZResult<()>> + 'a {
        self.publisher.undeclare()
    }
}

pub type AdvancedPublisherPutBuilder<'a> = AdvancedPublicationBuilder<'a, PublicationBuilderPut>;
pub type AdvancedPublisherDeleteBuilder<'a> =
    AdvancedPublicationBuilder<'a, PublicationBuilderDelete>;

#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
#[derive(Clone)]
pub struct AdvancedPublicationBuilder<'a, P> {
    pub(crate) builder: PublicationBuilder<&'a Publisher<'a>, P>,
    pub(crate) cache: Option<&'a AdvancedCache>,
}

#[zenoh_macros::internal_trait]
impl EncodingBuilderTrait for AdvancedPublicationBuilder<'_, PublicationBuilderPut> {
    fn encoding<T: Into<Encoding>>(self, encoding: T) -> Self {
        Self {
            builder: self.builder.encoding(encoding),
            ..self
        }
    }
}

#[zenoh_macros::internal_trait]
impl<P> SampleBuilderTrait for AdvancedPublicationBuilder<'_, P> {
    #[cfg(feature = "unstable")]
    fn source_info(self, source_info: SourceInfo) -> Self {
        Self {
            builder: self.builder.source_info(source_info),
            ..self
        }
    }
    fn attachment<TA: Into<OptionZBytes>>(self, attachment: TA) -> Self {
        let attachment: OptionZBytes = attachment.into();
        Self {
            builder: self.builder.attachment(attachment),
            ..self
        }
    }
}

#[zenoh_macros::internal_trait]
impl<P> TimestampBuilderTrait for AdvancedPublicationBuilder<'_, P> {
    fn timestamp<TS: Into<Option<uhlc::Timestamp>>>(self, timestamp: TS) -> Self {
        Self {
            builder: self.builder.timestamp(timestamp),
            ..self
        }
    }
}

impl<P> Resolvable for AdvancedPublicationBuilder<'_, P> {
    type To = ZResult<()>;
}

impl Wait for AdvancedPublisherPutBuilder<'_> {
    #[inline]
    fn wait(self) -> <Self as Resolvable>::To {
        if let Some(cache) = self.cache {
            cache.cache_sample(zenoh::sample::Sample::from(&self.builder));
        }
        self.builder.wait()
    }
}

impl Wait for AdvancedPublisherDeleteBuilder<'_> {
    #[inline]
    fn wait(self) -> <Self as Resolvable>::To {
        if let Some(cache) = self.cache {
            cache.cache_sample(zenoh::sample::Sample::from(&self.builder));
        }
        self.builder.wait()
    }
}

impl IntoFuture for AdvancedPublisherPutBuilder<'_> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

impl IntoFuture for AdvancedPublisherDeleteBuilder<'_> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}
