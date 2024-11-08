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
    borrow::Borrow,
    collections::{HashMap, VecDeque},
    future::{IntoFuture, Ready},
};

use flume::{bounded, Sender};
use futures::{select, FutureExt};
use tokio::task;
use zenoh::{
    handlers::FifoChannelHandler,
    internal::{bail, ResolveFuture},
    key_expr::{
        format::{ke, kedefine},
        keyexpr, KeyExpr, OwnedKeyExpr,
    },
    liveliness::LivelinessToken,
    pubsub::Subscriber,
    query::{Query, Queryable, ZenohParameters},
    sample::{Locality, Sample},
    Resolvable, Resolve, Result as ZResult, Session, Wait,
};

#[zenoh_macros::unstable]
pub(crate) static KE_STAR: &keyexpr = ke!("*");
#[zenoh_macros::unstable]
pub(crate) static KE_PREFIX: &keyexpr = ke!("@cache");
#[zenoh_macros::unstable]
kedefine!(
    pub(crate) ke_liveliness: "@cache/${zid:*}/${eid:*}/${remaining:**}",
);

/// The builder of AdvancedCache, allowing to configure it.
pub struct AdvancedCacheBuilder<'a, 'b, 'c> {
    session: &'a Session,
    pub_key_expr: ZResult<KeyExpr<'b>>,
    queryable_prefix: Option<ZResult<KeyExpr<'c>>>,
    subscriber_origin: Locality,
    queryable_origin: Locality,
    history: usize,
    liveliness: bool,
    resources_limit: Option<usize>,
}

impl<'a, 'b, 'c> AdvancedCacheBuilder<'a, 'b, 'c> {
    pub(crate) fn new(
        session: &'a Session,
        pub_key_expr: ZResult<KeyExpr<'b>>,
    ) -> AdvancedCacheBuilder<'a, 'b, 'c> {
        AdvancedCacheBuilder {
            session,
            pub_key_expr,
            queryable_prefix: Some(Ok((KE_PREFIX / KE_STAR / KE_STAR).into())),
            subscriber_origin: Locality::default(),
            queryable_origin: Locality::default(),
            history: 1024,
            liveliness: false,
            resources_limit: None,
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

    /// Restrict the matching publications that will be cached by this [`AdvancedCache`]
    /// to the ones that have the given [`Locality`](crate::prelude::Locality).
    #[inline]
    pub fn subscriber_allowed_origin(mut self, origin: Locality) -> Self {
        self.subscriber_origin = origin;
        self
    }

    /// Restrict the matching queries that will be receive by this [`AdvancedCache`]'s queryable
    /// to the ones that have the given [`Locality`](crate::prelude::Locality).
    #[inline]
    pub fn queryable_allowed_origin(mut self, origin: Locality) -> Self {
        self.queryable_origin = origin;
        self
    }

    /// Change the history size for each resource.
    pub fn history(mut self, history: usize) -> Self {
        self.history = history;
        self
    }

    /// Change the limit number of cached resources.
    pub fn resources_limit(mut self, limit: usize) -> Self {
        self.resources_limit = Some(limit);
        self
    }

    pub fn liveliness(mut self, enabled: bool) -> Self {
        self.liveliness = enabled;
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

impl<'a> IntoFuture for AdvancedCacheBuilder<'a, '_, '_> {
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

pub struct AdvancedCache {
    _sub: Subscriber<FifoChannelHandler<Sample>>,
    _queryable: Queryable<FifoChannelHandler<Query>>,
    _token: Option<LivelinessToken>,
    _stoptx: Sender<bool>,
}

impl AdvancedCache {
    fn new(conf: AdvancedCacheBuilder<'_, '_, '_>) -> ZResult<AdvancedCache> {
        let key_expr = conf.pub_key_expr?;
        // the queryable_prefix (optional), and the key_expr for AdvancedCache's queryable ("[<queryable_prefix>]/<pub_key_expr>")
        let (queryable_prefix, queryable_key_expr): (Option<OwnedKeyExpr>, KeyExpr) =
            match conf.queryable_prefix {
                None => (None, key_expr.clone()),
                Some(Ok(ke)) => {
                    let queryable_key_expr = (&ke) / &key_expr;
                    (Some(ke.into()), queryable_key_expr)
                }
                Some(Err(e)) => bail!("Invalid key expression for queryable_prefix: {}", e),
            };
        tracing::debug!(
            "Create AdvancedCache on {} with history={} resource_limit={:?}",
            &key_expr,
            conf.history,
            conf.resources_limit
        );

        // declare the local subscriber that will store the local publications
        let sub = conf
            .session
            .declare_subscriber(&key_expr)
            .allowed_origin(conf.subscriber_origin)
            .wait()?;

        // declare the queryable that will answer to queries on cache
        let queryable = conf
            .session
            .declare_queryable(&queryable_key_expr)
            .allowed_origin(conf.queryable_origin)
            .wait()?;

        // take local ownership of stuff to be moved into task
        let sub_recv = sub.handler().clone();
        let quer_recv = queryable.handler().clone();
        let pub_key_expr = key_expr.into_owned();
        let resources_limit = conf.resources_limit;
        let history = conf.history;

        let (stoptx, stoprx) = bounded::<bool>(1);
        task::spawn(async move {
            let mut cache: HashMap<OwnedKeyExpr, VecDeque<Sample>> =
                HashMap::with_capacity(resources_limit.unwrap_or(32));
            let limit = resources_limit.unwrap_or(usize::MAX);

            loop {
                select!(
                    // on publication received by the local subscriber, store it
                    sample = sub_recv.recv_async() => {
                        if let Ok(sample) = sample {
                            let queryable_key_expr: KeyExpr<'_> = if let Some(prefix) = &queryable_prefix {
                                prefix.join(&sample.key_expr()).unwrap().into()
                            } else {
                                sample.key_expr().clone()
                            };

                            if let Some(queue) = cache.get_mut(queryable_key_expr.as_keyexpr()) {
                                if queue.len() >= history {
                                    queue.pop_front();
                                }
                                queue.push_back(sample);
                            } else if cache.len() >= limit {
                                tracing::error!("AdvancedCache on {}: resource_limit exceeded - can't cache publication for a new resource",
                                pub_key_expr);
                            } else {
                                let mut queue: VecDeque<Sample> = VecDeque::new();
                                queue.push_back(sample);
                                cache.insert(queryable_key_expr.into(), queue);
                            }
                        }
                    },

                    // on query, reply with cache content
                    query = quer_recv.recv_async() => {
                        if let Ok(query) = query {
                            let (start, end) = query.parameters().get("_sn").map(decode_range).unwrap_or((None, None));
                            if !query.selector().key_expr().as_str().contains('*') {
                                if let Some(queue) = cache.get(query.selector().key_expr().as_keyexpr()) {
                                    for sample in queue {
                                        if sample_in_range(sample, start, end) {
                                            if let (Some(Ok(time_range)), Some(timestamp)) = (query.parameters().time_range(), sample.timestamp()) {
                                                if !time_range.contains(timestamp.get_time().to_system_time()){
                                                    continue;
                                                }
                                            }
                                            if let Err(e) = query.reply_sample(sample.clone()).await {
                                                tracing::warn!("Error replying to query: {}", e);
                                            }
                                        }
                                    }
                                }
                            } else {
                                for (key_expr, queue) in cache.iter() {
                                    if query.selector().key_expr().intersects(key_expr.borrow()) {
                                        for sample in queue {
                                            if sample_in_range(sample, start, end) {
                                                if let (Some(Ok(time_range)), Some(timestamp)) = (query.parameters().time_range(), sample.timestamp()) {
                                                    if !time_range.contains(timestamp.get_time().to_system_time()){
                                                        continue;
                                                    }
                                                }
                                                if let Err(e) = query.reply_sample(sample.clone()).await {
                                                    tracing::warn!("Error replying to query: {}", e);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    },

                    // When stoptx is dropped, stop the task
                    _ = stoprx.recv_async().fuse() => {
                        return
                    }
                );
            }
        });

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
            _sub: sub,
            _queryable: queryable,
            _token: token,
            _stoptx: stoptx,
        })
    }

    /// Close this AdvancedCache
    #[inline]
    pub fn close(self) -> impl Resolve<ZResult<()>> {
        ResolveFuture::new(async move {
            let AdvancedCache {
                _queryable,
                _sub,
                _token,
                _stoptx,
            } = self;
            _sub.undeclare().await?;
            if let Some(token) = _token {
                token.undeclare().await?;
            }
            _queryable.undeclare().await?;
            drop(_stoptx);
            Ok(())
        })
    }
}
