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
    Resolvable, Result as ZResult, Session, Wait, KE_ADV_PREFIX, KE_AT, KE_STARSTAR,
};

pub(crate) static KE_UHLC: &keyexpr = ke!("uhlc");
#[zenoh_macros::unstable]
kedefine!(
    pub(crate) ke_liveliness: "@adv/${zid:*}/${eid:*}/${meta:**}/@/${remaining:**}",
);

#[zenoh_macros::unstable]
/// Configure replies QoS.
#[derive(Clone, Debug)]
pub struct QoS {
    priority: Priority,
    congestion_control: CongestionControl,
    is_express: bool,
}

impl Default for QoS {
    fn default() -> Self {
        Self {
            priority: Priority::Data,
            congestion_control: CongestionControl::Block,
            is_express: false,
        }
    }
}

#[zenoh_macros::internal_trait]
impl QoSBuilderTrait for QoS {
    #[allow(unused_mut)]
    fn congestion_control(mut self, congestion_control: CongestionControl) -> Self {
        self.congestion_control = congestion_control;
        self
    }

    #[allow(unused_mut)]
    fn priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    #[allow(unused_mut)]
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
    replies_qos: QoS,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_samples: 1,
            replies_qos: QoS::default(),
        }
    }
}

impl CacheConfig {
    /// Specify how many samples to keep for each resource.
    pub fn max_samples(mut self, depth: usize) -> Self {
        self.max_samples = depth;
        self
    }

    /// The QoS to apply to replies.
    pub fn replies_qos(mut self, qos: QoS) -> Self {
        self.replies_qos = qos;
        self
    }
}

/// The builder of an [`AdvancedCache`], allowing to configure it.
#[zenoh_macros::unstable]
pub struct AdvancedCacheBuilder<'a, 'b, 'c> {
    session: &'a Session,
    pub_key_expr: ZResult<KeyExpr<'b>>,
    queryable_prefix: Option<ZResult<KeyExpr<'c>>>,
    queryable_origin: Locality,
    history: CacheConfig,
    liveliness: bool,
}

impl<'a, 'b, 'c> AdvancedCacheBuilder<'a, 'b, 'c> {
    pub(crate) fn new(
        session: &'a Session,
        pub_key_expr: ZResult<KeyExpr<'b>>,
    ) -> AdvancedCacheBuilder<'a, 'b, 'c> {
        AdvancedCacheBuilder {
            session,
            pub_key_expr,
            queryable_prefix: Some(Ok((KE_ADV_PREFIX / KE_STARSTAR / KE_AT).into())),
            queryable_origin: Locality::default(),
            history: CacheConfig::default(),
            liveliness: false,
        }
    }

    /// Change the prefix used for queryable.
    pub fn queryable_prefix<TryIntoKeyExpr>(mut self, queryable_prefix: TryIntoKeyExpr) -> Self
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'c>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'c>>>::Error: Into<zenoh::Error>,
    {
        self.queryable_prefix = Some(queryable_prefix.try_into().map_err(Into::into));
        self
    }

    /// Change the history size for each resource.
    pub fn history(mut self, history: CacheConfig) -> Self {
        self.history = history;
        self
    }
}

impl Resolvable for AdvancedCacheBuilder<'_, '_, '_> {
    type To = ZResult<AdvancedCache>;
}

impl Wait for AdvancedCacheBuilder<'_, '_, '_> {
    fn wait(self) -> <Self as Resolvable>::To {
        AdvancedCache::new(self)
    }
}

impl IntoFuture for AdvancedCacheBuilder<'_, '_, '_> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

fn decode_range(range: &str) -> (Option<u32>, Option<u32>) {
    let mut split = range.split("..");
    let start = split.next().and_then(|s| s.parse::<u32>().ok());
    let end = split.next().map(|s| s.parse::<u32>().ok()).unwrap_or(start);
    (start, end)
}

fn sample_in_range(sample: &Sample, start: Option<u32>, end: Option<u32>) -> bool {
    if start.is_none() && end.is_none() {
        true
    } else if let Some(source_sn) = sample.source_info().source_sn() {
        match (start, end) {
            (Some(start), Some(end)) => source_sn >= start && source_sn <= end,
            (Some(start), None) => source_sn >= start,
            (None, Some(end)) => source_sn <= end,
            (None, None) => true,
        }
    } else {
        false
    }
}

/// [`AdvancedCache`].
#[zenoh_macros::unstable]
pub struct AdvancedCache {
    cache: Arc<RwLock<VecDeque<Sample>>>,
    max_samples: usize,
    _queryable: Queryable<()>,
    _token: Option<LivelinessToken>,
}

impl AdvancedCache {
    fn new(conf: AdvancedCacheBuilder<'_, '_, '_>) -> ZResult<AdvancedCache> {
        let key_expr = conf.pub_key_expr?.into_owned();
        // the queryable_prefix (optional), and the key_expr for AdvancedCache's queryable ("[<queryable_prefix>]/<pub_key_expr>")
        let queryable_key_expr = match conf.queryable_prefix {
            None => key_expr.clone(),
            Some(Ok(ke)) => (&ke) / &key_expr,
            Some(Err(e)) => bail!("Invalid key expression for queryable_prefix: {}", e),
        };
        tracing::debug!(
            "Create AdvancedCache on {} with max_samples={:?}",
            &key_expr,
            conf.history,
        );
        let cache = Arc::new(RwLock::new(VecDeque::new()));

        // declare the queryable that will answer to queries on cache
        let queryable = conf
            .session
            .declare_queryable(&queryable_key_expr)
            .allowed_origin(conf.queryable_origin)
            .callback({
                let cache = cache.clone();
                move |query| {
                    let (start, end) = query
                        .parameters()
                        .get("_sn")
                        .map(decode_range)
                        .unwrap_or((None, None));
                    let max = query
                        .parameters()
                        .get("_max")
                        .and_then(|s| s.parse::<u32>().ok());
                    if let Ok(queue) = cache.read() {
                        if let Some(max) = max {
                            let mut samples = VecDeque::new();
                            for sample in queue.iter() {
                                if sample_in_range(sample, start, end) {
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
                                                conf.history.replies_qos.congestion_control,
                                            )
                                            .priority(conf.history.replies_qos.priority)
                                            .express(conf.history.replies_qos.is_express)
                                            .into(),
                                    )
                                    .wait()
                                {
                                    tracing::warn!("Error replying to query: {}", e);
                                }
                            }
                        } else {
                            for sample in queue.iter() {
                                if sample_in_range(sample, start, end) {
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
                                                    conf.history.replies_qos.congestion_control,
                                                )
                                                .priority(conf.history.replies_qos.priority)
                                                .express(conf.history.replies_qos.is_express)
                                                .into(),
                                        )
                                        .wait()
                                    {
                                        tracing::warn!("Error replying to query: {}", e);
                                    }
                                }
                            }
                        }
                    } else {
                        tracing::error!("Unable to take AdvancedPublisher cache read lock");
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

    pub fn cache_sample(&self, sample: Sample) {
        if let Ok(mut queue) = self.cache.write() {
            if queue.len() >= self.max_samples {
                queue.pop_front();
            }
            queue.push_back(sample);
        } else {
            tracing::error!("Unable to take AdvancedPublisher cache write lock");
        }
    }
}
