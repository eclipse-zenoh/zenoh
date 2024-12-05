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
    bytes::{Encoding, ZBytes},
    internal::bail,
    key_expr::KeyExpr,
    liveliness::LivelinessToken,
    pubsub::{Publisher, PublisherDeleteBuilder, PublisherPutBuilder},
    qos::{CongestionControl, Priority},
    sample::{Locality, SourceInfo},
    session::EntityGlobalId,
    Resolvable, Resolve, Result as ZResult, Session, Wait,
};

use crate::advanced_cache::{
    AdvancedCache, AdvancedCacheBuilder, CacheConfig, KE_EMPTY, KE_PREFIX, KE_SEPARATOR, KE_UHLC,
};

#[derive(PartialEq)]
pub enum Sequencing {
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

pub struct AdvancedPublisher<'a> {
    publisher: Publisher<'a>,
    seqnum: Option<AtomicU32>,
    _cache: Option<AdvancedCache>,
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
        let prefix = KE_PREFIX / &id.zid().into_keyexpr();
        let prefix = match conf.sequencing {
            Sequencing::SequenceNumber => {
                prefix / &KeyExpr::try_from(id.eid().to_string()).unwrap()
            }
            _ => prefix / KE_UHLC,
        };
        let prefix = match meta {
            Some(meta) => prefix / &meta / KE_SEPARATOR,
            // We need this empty chunk because af a routing matching bug
            _ => prefix / KE_EMPTY / KE_SEPARATOR,
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
                    .subscriber_allowed_origin(Locality::SessionLocal)
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
            _cache: cache,
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
    pub fn put<IntoZBytes>(&self, payload: IntoZBytes) -> PublisherPutBuilder<'_>
    where
        IntoZBytes: Into<ZBytes>,
    {
        let mut put = self.publisher.put(payload);
        if let Some(seqnum) = &self.seqnum {
            put = put.source_info(SourceInfo::new(
                Some(self.publisher.id()),
                Some(seqnum.fetch_add(1, Ordering::Relaxed)),
            ));
        }
        put
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
    pub fn delete(&self) -> PublisherDeleteBuilder<'_> {
        let mut delete = self.publisher.delete();
        if let Some(seqnum) = &self.seqnum {
            delete = delete.source_info(SourceInfo::new(
                Some(self.publisher.id()),
                Some(seqnum.fetch_add(1, Ordering::Relaxed)),
            ));
        }
        delete
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
