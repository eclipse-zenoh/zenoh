//
// Copyright (c) 2022 ZettaScale Technology
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
    collections::VecDeque,
    future::{IntoFuture, Ready},
    num::NonZeroUsize,
    sync::{Arc, RwLock},
};

use zenoh::{
    internal::{bail, traits::QoSBuilderTrait, zerror},
    key_expr::{
        format::{ke, kedefine},
        keyexpr, KeyExpr,
    },
    liveliness::LivelinessToken,
    qos::{CongestionControl, Priority},
    query::{Parameters, Query, Queryable, TimeRange, ZenohParameters},
    sample::{Locality, Sample, SampleBuilder},
    Resolvable, Result as ZResult, Session, Wait, KE_ADV_PREFIX, KE_STARSTAR,
};

use crate::advanced_query::QueryRange;

pub(crate) static KE_UHLC: &keyexpr = ke!("uhlc");
#[zenoh_macros::unstable]
kedefine!(
    pub(crate) ke_liveliness: "${remaining:**}/@adv/${entity:*}/${zid:*}/${eid:*}/${meta:**}",
);

#[zenoh_macros::unstable]
/// Configure the QoS of cache replies for history and recovery.
///
/// Defaults to [`Priority::Data`], [`CongestionControl::Block`], and batching enabled.
#[derive(Clone, Debug)]
pub struct RepliesConfig {
    priority: Priority,
    congestion_control: CongestionControl,
    is_express: bool,
}

#[zenoh_macros::unstable]
impl Default for RepliesConfig {
    fn default() -> Self {
        Self {
            priority: Priority::Data,
            congestion_control: CongestionControl::Block,
            is_express: false,
        }
    }
}

#[zenoh_macros::internal_trait]
#[zenoh_macros::unstable]
impl QoSBuilderTrait for RepliesConfig {
    #[allow(unused_mut)]
    #[zenoh_macros::unstable]
    /// Changes the [`CongestionControl`] to apply when routing the data.
    fn congestion_control(mut self, congestion_control: CongestionControl) -> Self {
        self.congestion_control = congestion_control;
        self
    }

    #[allow(unused_mut)]
    #[zenoh_macros::unstable]
    /// Changes the [`Priority`] to apply when routing the data.
    fn priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    #[allow(unused_mut)]
    #[zenoh_macros::unstable]
    /// Changes the Express policy to apply when routing the data.
    ///
    /// When express is set to `true`, then the message will not be batched.
    /// This usually has a positive impact on latency but a negative impact on throughput.
    fn express(mut self, is_express: bool) -> Self {
        self.is_express = is_express;
        self
    }
}

#[derive(Debug, Clone)]
/// Configure an [`AdvancedPublisher`](crate::AdvancedPublisher) cache.
///
/// Keeps the most recent publications in memory while the publisher is alive.
#[zenoh_macros::unstable]
pub struct CacheConfig {
    max_samples: usize,
    replies_config: RepliesConfig,
}

#[zenoh_macros::unstable]
impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_samples: 1,
            replies_config: RepliesConfig::default(),
        }
    }
}

#[zenoh_macros::unstable]
impl CacheConfig {
    /// Set the number of publications retained by this publisher's cache (default: 1).
    ///
    /// A fragmented publication occupies one entry; its fragments are evicted together.
    /// This is a publication-count limit, not a byte limit. Publisher declaration fails
    /// if `depth` is zero.
    #[zenoh_macros::unstable]
    pub fn max_samples(mut self, depth: usize) -> Self {
        self.max_samples = depth;
        self
    }

    /// The QoS to apply to replies.
    #[zenoh_macros::unstable]
    pub fn replies_config(mut self, qos: RepliesConfig) -> Self {
        self.replies_config = qos;
        self
    }
}

/// The builder of an [`AdvancedCache`], allowing to configure it.
#[zenoh_macros::unstable]
pub struct AdvancedCacheBuilder<'a, 'b, 'c> {
    session: &'a Session,
    pub_key_expr: ZResult<KeyExpr<'b>>,
    queryable_suffix: Option<ZResult<KeyExpr<'c>>>,
    queryable_origin: Locality,
    history: CacheConfig,
    liveliness: bool,
}

#[zenoh_macros::unstable]
impl<'a, 'b, 'c> AdvancedCacheBuilder<'a, 'b, 'c> {
    #[zenoh_macros::unstable]
    pub(crate) fn new(
        session: &'a Session,
        pub_key_expr: ZResult<KeyExpr<'b>>,
    ) -> AdvancedCacheBuilder<'a, 'b, 'c> {
        AdvancedCacheBuilder {
            session,
            pub_key_expr,
            queryable_suffix: Some(Ok((KE_ADV_PREFIX / KE_STARSTAR).into())),
            queryable_origin: Locality::default(),
            history: CacheConfig::default(),
            liveliness: false,
        }
    }

    /// Change the suffix used for queryable.
    #[zenoh_macros::unstable]
    pub fn queryable_suffix<TryIntoKeyExpr>(mut self, queryable_suffix: TryIntoKeyExpr) -> Self
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'c>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'c>>>::Error: Into<zenoh::Error>,
    {
        self.queryable_suffix = Some(queryable_suffix.try_into().map_err(Into::into));
        self
    }

    /// Change the history size for each resource.
    #[zenoh_macros::unstable]
    pub fn history(mut self, history: CacheConfig) -> Self {
        self.history = history;
        self
    }
}

#[zenoh_macros::unstable]
impl Resolvable for AdvancedCacheBuilder<'_, '_, '_> {
    type To = ZResult<AdvancedCache>;
}

#[zenoh_macros::unstable]
impl Wait for AdvancedCacheBuilder<'_, '_, '_> {
    fn wait(self) -> <Self as Resolvable>::To {
        AdvancedCache::new(self)
    }
}

#[zenoh_macros::unstable]
impl IntoFuture for AdvancedCacheBuilder<'_, '_, '_> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    #[zenoh_macros::unstable]
    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

/// A complete publication occupies one cache entry, regardless of fragment count.
// Keep unfragmented samples inline to avoid an extra allocation.
#[allow(clippy::large_enum_variant)]
enum CachedPublication {
    Single(Sample),
    /// All fragments of one publication, in fragment-number order.
    Fragmented(Vec<Sample>),
}

impl CachedPublication {
    fn fragments(&self) -> &[Sample] {
        match self {
            Self::Single(sample) => std::slice::from_ref(sample),
            Self::Fragmented(fragments) => fragments,
        }
    }
}

struct CacheQuery {
    sequence_range: QueryRange,
    fragment_range: QueryRange,
    time_range: Option<TimeRange>,
    max: Option<u32>,
}

impl CacheQuery {
    fn from_parameters(parameters: &Parameters<'_>) -> Self {
        Self {
            sequence_range: QueryRange::from_parameter(parameters.get("_sn")),
            fragment_range: QueryRange::from_parameter(parameters.get("_fn")),
            time_range: parameters.time_range().and_then(Result::ok),
            max: parameters.get("_max").and_then(|s| s.parse().ok()),
        }
    }

    fn matches_fragment(&self, fragment: &Sample) -> bool {
        self.fragment_range.is_unbounded()
            || fragment
                .frag_info()
                .is_some_and(|info| self.fragment_range.contains(info.frag_num()))
    }

    fn matches_publication(&self, publication: &CachedPublication) -> bool {
        let head = publication.fragments().first();
        if !(self.sequence_range.is_unbounded()
            || head
                .and_then(|fragment| fragment.source_info())
                .is_some_and(|info| self.sequence_range.contains(info.source_sn())))
        {
            return false;
        }
        if let (Some(time_range), Some(timestamp)) = (
            self.time_range.as_ref(),
            head.and_then(|fragment| fragment.timestamp()),
        ) {
            if !time_range.contains(timestamp.get_time().to_system_time()) {
                return false;
            }
        }
        self.fragment_range.is_unbounded()
            || publication
                .fragments()
                .iter()
                .any(|fragment| self.matches_fragment(fragment))
    }

    fn for_each_matching_publication(
        &self,
        queue: &VecDeque<CachedPublication>,
        mut reply: impl FnMut(&CachedPublication),
    ) {
        let publications = queue
            .iter()
            .filter(|publication| self.matches_publication(publication));
        if let Some(max) = self.max {
            // Count matching publications, not individual fragments. Select the
            // newest matches, then reply in ascending order.
            let selected: Vec<_> = publications.rev().take(max as usize).collect();
            for publication in selected.into_iter().rev() {
                reply(publication);
            }
        } else {
            for publication in publications {
                reply(publication);
            }
        }
    }
}

fn reply_publication(
    query: &Query,
    publication: &CachedPublication,
    selection: &CacheQuery,
    replies: &RepliesConfig,
) {
    for frag in publication
        .fragments()
        .iter()
        .filter(|frag| selection.matches_fragment(frag))
    {
        if let Err(e) = query
            .reply_sample(
                SampleBuilder::from(frag.clone())
                    .congestion_control(replies.congestion_control)
                    .priority(replies.priority)
                    .express(replies.is_express)
                    .into(),
            )
            .wait()
        {
            tracing::warn!("AdvancedCache{{}} Error replying to query: {}", e);
        } else {
            tracing::trace!(
                "AdvancedCache{{}} Replied to query {} with Sample{{info:{:?}, ts:{:?}}}",
                query.selector(),
                frag.source_info(),
                frag.timestamp()
            );
        }
    }
}

/// [`AdvancedCache`].
#[zenoh_macros::unstable]
pub struct AdvancedCache {
    cache: Arc<RwLock<VecDeque<CachedPublication>>>,
    max_samples: NonZeroUsize,
    _queryable: Queryable<()>,
    _token: Option<LivelinessToken>,
}

#[zenoh_macros::unstable]
impl AdvancedCache {
    #[zenoh_macros::unstable]
    fn new(conf: AdvancedCacheBuilder<'_, '_, '_>) -> ZResult<AdvancedCache> {
        let key_expr = conf.pub_key_expr?.into_owned();
        // the queryable_suffix (optional), and the key_expr for AdvancedCache's queryable ("<pub_key_expr>/[<queryable_suffix>]")
        let queryable_key_expr = match conf.queryable_suffix {
            None => key_expr.clone(),
            Some(Ok(ke)) => &key_expr / &ke,
            Some(Err(e)) => bail!("Invalid key expression for queryable_suffix: {}", e),
        };
        tracing::debug!(
            "Create AdvancedCache{{key_expr: {}, max_samples: {:?}}}",
            &key_expr,
            conf.history,
        );
        let cache = Arc::new(RwLock::new(VecDeque::<CachedPublication>::new()));

        // declare the queryable that will answer to queries on cache
        let queryable = conf
            .session
            .declare_queryable(&queryable_key_expr)
            .allowed_origin(conf.queryable_origin)
            .callback({
                let cache = cache.clone();
                move |query| {
                    tracing::trace!("AdvancedCache{{}} Handle query {}", query.selector());
                    let selection = CacheQuery::from_parameters(query.parameters());
                    if let Ok(queue) = cache.read() {
                        selection.for_each_matching_publication(&queue, |publication| {
                            reply_publication(
                                &query,
                                publication,
                                &selection,
                                &conf.history.replies_config,
                            );
                        });
                    } else {
                        tracing::error!(
                            "AdvancedCache{{}} Unable to take AdvancedPublisher cache read lock"
                        );
                    }
                }
            })
            .wait()?;

        let token = if conf.liveliness {
            Some(
                conf.session
                    .liveliness()
                    .declare_token(queryable_key_expr)
                    .wait()?,
            )
        } else {
            None
        };

        let max_samples = conf
            .history
            .max_samples
            .try_into()
            .map_err(|_| zerror!("max_samples must not be zero"))?;
        Ok(AdvancedCache {
            cache,
            max_samples,
            _queryable: queryable,
            _token: token,
        })
    }

    #[zenoh_macros::unstable]
    pub(crate) fn cache_sample(&self, sample: Sample) {
        self.cache_publication(CachedPublication::Single(sample));
    }

    #[zenoh_macros::unstable]
    pub(crate) fn cache_fragments(&self, fragments: Vec<Sample>) {
        self.cache_publication(CachedPublication::Fragmented(fragments));
    }

    fn cache_publication(&self, publication: CachedPublication) {
        if let Ok(mut queue) = self.cache.write() {
            if queue.len() >= self.max_samples.get() {
                drop(queue.drain(..1));
            }
            queue.push_back(publication);
        } else {
            tracing::error!("AdvancedCache{{}} Unable to take AdvancedPublisher cache write lock");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };

    use zenoh::{
        query::{ConsolidationMode, ReplyKeyExpr},
        Config, Session, Wait,
    };

    use crate::{AdvancedPublisherBuilderExt, CacheConfig, MissDetectionConfig};

    const KEY: &str = "test/advanced_cache/publications";

    fn open_session() -> Session {
        let mut config = Config::default();
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
        config.listen.endpoints.set(vec![]).unwrap();
        zenoh::open(config).wait().unwrap()
    }

    fn query_cache(session: &Session, parameters: &str) -> Vec<(u32, Option<u32>)> {
        session
            .get(format!("{KEY}/@adv/**?{parameters}"))
            .consolidation(ConsolidationMode::None)
            .accept_replies(ReplyKeyExpr::Any)
            .timeout(Duration::from_secs(5))
            .wait()
            .unwrap()
            .into_iter()
            .map(|reply| {
                let sample = reply.into_result().unwrap();
                (
                    sample.source_info().unwrap().source_sn(),
                    sample.frag_info().map(|info| info.frag_num()),
                )
            })
            .collect()
    }

    #[test]
    fn cache_max_counts_publications_and_preserves_reply_order() {
        let session = open_session();
        let publisher = session
            .declare_publisher(KEY)
            .cache(CacheConfig::default().max_samples(5))
            .sample_miss_detection(MissDetectionConfig::default())
            .fragmentation(2)
            .wait()
            .unwrap();
        for payload in ["old", "abcd", "ef", "ghijkl", "mn"] {
            publisher.put(payload).wait().unwrap();
        }

        assert_eq!(
            query_cache(&session, "_max=2"),
            [(3, Some(0)), (3, Some(1)), (3, Some(2)), (4, None)]
        );
        assert_eq!(
            query_cache(&session, "_max=3"),
            [
                (2, None),
                (3, Some(0)),
                (3, Some(1)),
                (3, Some(2)),
                (4, None)
            ]
        );
        assert!(query_cache(&session, "_max=0").is_empty());
        session.close().wait().unwrap();
    }

    #[test]
    fn cache_eviction_preserves_recent_whole_and_fragmented_publications() {
        for depth in [1, 3] {
            let session = open_session();
            let publisher = session
                .declare_publisher(KEY)
                .cache(CacheConfig::default().max_samples(depth))
                .sample_miss_detection(MissDetectionConfig::default())
                .fragmentation(2)
                .wait()
                .unwrap();
            for payload in ["a", "bcde", "fg", "hijkl", "m"] {
                publisher.put(payload).wait().unwrap();
            }
            let expected = if depth == 1 {
                vec![(4, None)]
            } else {
                vec![
                    (2, None),
                    (3, Some(0)),
                    (3, Some(1)),
                    (3, Some(2)),
                    (4, None),
                ]
            };
            assert_eq!(query_cache(&session, ""), expected);
            session.close().wait().unwrap();
        }
    }

    #[test]
    fn cache_fragment_filter_applies_before_publication_limit() {
        let session = open_session();
        let publisher = session
            .declare_publisher(KEY)
            .cache(CacheConfig::default().max_samples(5))
            .sample_miss_detection(MissDetectionConfig::default())
            .fragmentation(2)
            .wait()
            .unwrap();
        for payload in ["old", "abcd", "ef", "ghijkl", "mn"] {
            publisher.put(payload).wait().unwrap();
        }

        assert_eq!(
            query_cache(&session, "_fn=1..2;_max=2"),
            [(1, Some(1)), (3, Some(1)), (3, Some(2))]
        );
        assert_eq!(
            query_cache(&session, "_sn=0..1;_fn=1;_max=1"),
            [(1, Some(1))]
        );
        assert_eq!(
            query_cache(&session, "_fn=2..;_max=invalid"),
            [(3, Some(2))]
        );
        assert_eq!(query_cache(&session, "_fn=abc..def;_max=1"), [(4, None)]);
        assert!(query_cache(&session, "_fn=9;_max=1").is_empty());
        session.close().wait().unwrap();
    }

    #[test]
    fn cache_complete_batch_is_queryable_from_first_fragment() {
        let session = open_session();
        let cached_fragments = Arc::new(Mutex::new(Vec::new()));
        let _subscriber = session
            .declare_subscriber(KEY)
            .callback({
                let session = session.clone();
                let cached_fragments = cached_fragments.clone();
                move |fragment| {
                    if fragment
                        .frag_info()
                        .is_some_and(|info| info.frag_num() == 0)
                    {
                        *cached_fragments.lock().unwrap() = query_cache(&session, "_sn=0;_max=1");
                    }
                }
            })
            .wait()
            .unwrap();
        let publisher = session
            .declare_publisher(KEY)
            .cache(CacheConfig::default())
            .sample_miss_detection(MissDetectionConfig::default())
            .fragmentation(2)
            .wait()
            .unwrap();

        publisher.put("abcdef").wait().unwrap();
        assert_eq!(
            *cached_fragments.lock().unwrap(),
            [(0, Some(0)), (0, Some(1)), (0, Some(2))]
        );
        session.close().wait().unwrap();
    }
}
