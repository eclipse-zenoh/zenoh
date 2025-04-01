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
    ops::{Bound, RangeBounds},
    sync::{Arc, RwLock},
};

use zenoh::{
    internal::{bail, traits::QoSBuilderTrait},
    key_expr::{
        format::{ke, kedefine},
        keyexpr, KeyExpr,
    },
    liveliness::LivelinessToken,
    qos::{CongestionControl, Priority},
    query::{Queryable, ZenohParameters},
    sample::{Locality, Sample, SampleBuilder},
    Resolvable, Result as ZResult, Session, Wait, KE_ADV_PREFIX, KE_STARSTAR,
};

pub(crate) static KE_UHLC: &keyexpr = ke!("uhlc");
#[zenoh_macros::unstable]
kedefine!(
    pub(crate) ke_liveliness: "${remaining:**}/@adv/${entity:*}/${zid:*}/${eid:*}/${meta:**}",
);

#[zenoh_macros::unstable]
/// Configure replies.
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
    fn congestion_control(mut self, congestion_control: CongestionControl) -> Self {
        self.congestion_control = congestion_control;
        self
    }

    #[allow(unused_mut)]
    #[zenoh_macros::unstable]
    fn priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    #[allow(unused_mut)]
    #[zenoh_macros::unstable]
    fn express(mut self, is_express: bool) -> Self {
        self.is_express = is_express;
        self
    }
}

#[derive(Debug, Clone)]
/// Configure an [`AdvancedPublisher`](crate::AdvancedPublisher) cache.
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
    /// Specify how many samples to keep for each resource.
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

#[zenoh_macros::unstable]
fn decode_range(range: &str) -> (Bound<u32>, Bound<u32>) {
    let mut split = range.split("..");
    let start = split
        .next()
        .and_then(|s| s.parse::<u32>().ok().map(Bound::Included))
        .unwrap_or(Bound::Unbounded);
    let end = split
        .next()
        .map(|s| {
            s.parse::<u32>()
                .ok()
                .map(Bound::Included)
                .unwrap_or(Bound::Unbounded)
        })
        .unwrap_or(start);
    (start, end)
}

/// [`AdvancedCache`].
#[zenoh_macros::unstable]
pub struct AdvancedCache {
    cache: Arc<RwLock<VecDeque<Sample>>>,
    max_samples: usize,
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
        let cache = Arc::new(RwLock::new(VecDeque::<Sample>::new()));

        // declare the queryable that will answer to queries on cache
        let queryable = conf
            .session
            .declare_queryable(&queryable_key_expr)
            .allowed_origin(conf.queryable_origin)
            .callback({
                let cache = cache.clone();
                move |query| {
                    tracing::trace!("AdvancedCache{{}} Handle query {}", query.selector());
                    let range = query
                        .parameters()
                        .get("_sn")
                        .map(decode_range)
                        .unwrap_or((Bound::Unbounded, Bound::Unbounded));
                    let max = query
                        .parameters()
                        .get("_max")
                        .and_then(|s| s.parse::<u32>().ok());
                    if let Ok(queue) = cache.read() {
                        if let Some(max) = max {
                            let mut samples = VecDeque::new();
                            for sample in queue.iter() {
                                if range == (Bound::Unbounded, Bound::Unbounded)
                                    || sample
                                        .source_info()
                                        .source_sn()
                                        .is_some_and(|sn| range.contains(&sn))
                                {
                                    if let (Some(Ok(time_range)), Some(timestamp)) =
                                        (query.parameters().time_range(), sample.timestamp())
                                    {
                                        if !time_range
                                            .contains(timestamp.get_time().to_system_time())
                                        {
                                            continue;
                                        }
                                    }
                                    samples.push_front(sample);
                                    samples.truncate(max as usize);
                                }
                            }
                            for sample in samples.drain(..).rev() {
                                if let Err(e) = query
                                    .reply_sample(
                                        SampleBuilder::from(sample.clone())
                                            .congestion_control(
                                                conf.history.replies_config.congestion_control,
                                            )
                                            .priority(conf.history.replies_config.priority)
                                            .express(conf.history.replies_config.is_express)
                                            .into(),
                                    )
                                    .wait()
                                {
                                    tracing::warn!("AdvancedCache{{}} Error replying to query: {}", e);
                                } else {
                                    tracing::trace!(
                                        "AdvancedCache{{}} Replied to query {} with Sample{{info:{:?}, ts:{:?}}}",
                                        query.selector(),
                                        sample.source_info(),
                                        sample.timestamp()
                                    );
                                }
                            }
                        } else {
                            for sample in queue.iter() {
                                if range == (Bound::Unbounded, Bound::Unbounded)
                                    || sample
                                        .source_info()
                                        .source_sn()
                                        .is_some_and(|sn| range.contains(&sn))
                                {
                                    if let (Some(Ok(time_range)), Some(timestamp)) =
                                        (query.parameters().time_range(), sample.timestamp())
                                    {
                                        if !time_range
                                            .contains(timestamp.get_time().to_system_time())
                                        {
                                            continue;
                                        }
                                    }
                                    if let Err(e) = query
                                        .reply_sample(
                                            SampleBuilder::from(sample.clone())
                                                .congestion_control(
                                                    conf.history.replies_config.congestion_control,
                                                )
                                                .priority(conf.history.replies_config.priority)
                                                .express(conf.history.replies_config.is_express)
                                                .into(),
                                        )
                                        .wait()
                                    {
                                        tracing::warn!("AdvancedCache{{}} Error replying to query: {}", e);
                                    } else {
                                        tracing::trace!(
                                            "AdvancedCache{{}} Replied to query {} with Sample{{info:{:?}, ts:{:?}}}",
                                            query.selector(),
                                            sample.source_info(),
                                            sample.timestamp()
                                        );
                                    }
                                }
                            }
                        }
                    } else {
                        tracing::error!("AdvancedCache{{}} Unable to take AdvancedPublisher cache read lock");
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

        Ok(AdvancedCache {
            cache,
            max_samples: conf.history.max_samples,
            _queryable: queryable,
            _token: token,
        })
    }

    #[zenoh_macros::unstable]
    pub(crate) fn cache_sample(&self, sample: Sample) {
        if let Ok(mut queue) = self.cache.write() {
            if queue.len() >= self.max_samples {
                queue.pop_front();
            }
            queue.push_back(sample);
        } else {
            tracing::error!("AdvancedCache{{}} Unable to take AdvancedPublisher cache write lock");
        }
    }
}
