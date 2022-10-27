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
#[zenoh_core::unstable]
use {
    async_std::channel::{bounded, Sender},
    async_std::task,
    futures::select,
    futures::{FutureExt, StreamExt},
    std::collections::{HashMap, VecDeque},
    std::convert::TryInto,
    std::future::Ready,
    zenoh::buffers::reader::HasReader,
    zenoh::prelude::r#async::*,
    zenoh::queryable::{Query, Queryable},
    zenoh::subscriber::FlumeSubscriber,
    zenoh::Session,
    zenoh_core::{bail, AsyncResolve, Resolvable, Result as ZResult, SyncResolve},
    zenoh_protocol::io::ZBufCodec,
    zenoh_util::core::ResolveFuture,
};

/// The builder of ReliabilityCache, allowing to configure it.
#[zenoh_core::unstable]
pub struct ReliabilityCacheBuilder<'a, 'b, 'c> {
    session: &'a Session,
    pub_key_expr: ZResult<KeyExpr<'b>>,
    queryable_prefix: Option<ZResult<KeyExpr<'c>>>,
    subscriber_origin: Locality,
    queryable_origin: Locality,
    history: usize,
    resources_limit: Option<usize>,
}

#[zenoh_core::unstable]
impl<'a, 'b, 'c> ReliabilityCacheBuilder<'a, 'b, 'c> {
    pub(crate) fn new(
        session: &'a Session,
        pub_key_expr: ZResult<KeyExpr<'b>>,
    ) -> ReliabilityCacheBuilder<'a, 'b, 'c> {
        ReliabilityCacheBuilder {
            session,
            pub_key_expr,
            queryable_prefix: None,
            subscriber_origin: Locality::default(),
            queryable_origin: Locality::default(),
            history: 1024,
            resources_limit: None,
        }
    }

    /// Change the prefix used for queryable.
    pub fn queryable_prefix<TryIntoKeyExpr>(mut self, queryable_prefix: TryIntoKeyExpr) -> Self
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'c>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'c>>>::Error: Into<zenoh_core::Error>,
    {
        self.queryable_prefix = Some(queryable_prefix.try_into().map_err(Into::into));
        self
    }

    /// Restrict the matching publications that will be cached by this [`ReliabilityCache`]
    /// to the ones that have the given [`Locality`](crate::prelude::Locality).
    #[inline]
    pub fn subscriber_allowed_origin(mut self, origin: Locality) -> Self {
        self.subscriber_origin = origin;
        self
    }

    /// Restrict the matching queries that will be receive by this [`ReliabilityCache`]'s queryable
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
}

#[zenoh_core::unstable]
impl<'a> Resolvable for ReliabilityCacheBuilder<'a, '_, '_> {
    type To = ZResult<ReliabilityCache<'a>>;
}

#[zenoh_core::unstable]
impl SyncResolve for ReliabilityCacheBuilder<'_, '_, '_> {
    fn res_sync(self) -> <Self as Resolvable>::To {
        ReliabilityCache::new(self)
    }
}

#[zenoh_core::unstable]
impl<'a> AsyncResolve for ReliabilityCacheBuilder<'a, '_, '_> {
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}

#[zenoh_core::unstable]
fn decode_range(range: &str) -> (Option<ZInt>, Option<ZInt>) {
    let mut split = range.split("..");
    let start = split.next().and_then(|s| s.parse::<u64>().ok());
    let end = split.next().map(|s| s.parse::<u64>().ok()).unwrap_or(start);
    (start, end)
}

#[zenoh_core::unstable]
fn sample_in_range(sample: &Sample, start: Option<ZInt>, end: Option<ZInt>) -> bool {
    if start.is_none() && end.is_none() {
        true
    } else {
        let mut buf = sample.value.payload.reader();
        let _id = buf.read_zid().unwrap(); //TODO
        let seq_num = buf.read_zint().unwrap(); //TODO
        match (start, end) {
            (Some(start), Some(end)) => seq_num >= start && seq_num <= end,
            (Some(start), None) => seq_num >= start,
            (None, Some(end)) => seq_num <= end,
            (None, None) => true,
        }
    }
}

#[zenoh_core::unstable]
pub struct ReliabilityCache<'a> {
    _sub: FlumeSubscriber<'a>,
    _queryable: Queryable<'a, flume::Receiver<Query>>,
    _stoptx: Sender<bool>,
}

#[zenoh_core::unstable]
impl<'a> ReliabilityCache<'a> {
    fn new(conf: ReliabilityCacheBuilder<'a, '_, '_>) -> ZResult<ReliabilityCache<'a>> {
        let key_expr = conf.pub_key_expr?;
        // the queryable_prefix (optional), and the key_expr for ReliabilityCache's queryable ("[<queryable_prefix>]/<pub_key_expr>")
        let (queryable_prefix, queryable_key_expr): (Option<OwnedKeyExpr>, KeyExpr) =
            match conf.queryable_prefix {
                None => (None, key_expr.clone()),
                Some(Ok(ke)) => {
                    let queryable_key_expr = (&ke) / &key_expr;
                    (Some(ke.into()), queryable_key_expr)
                }
                Some(Err(e)) => bail!("Invalid key expression for queryable_prefix: {}", e),
            };
        log::debug!(
            "Create ReliabilityCache on {} with history={} resource_limit={:?}",
            &key_expr,
            conf.history,
            conf.resources_limit
        );

        // declare the local subscriber that will store the local publications
        let sub = conf
            .session
            .declare_subscriber(&key_expr)
            .allowed_origin(conf.subscriber_origin)
            .res_sync()?;

        // declare the queryable that will answer to queries on cache
        let queryable = conf
            .session
            .declare_queryable(&queryable_key_expr)
            .allowed_origin(conf.queryable_origin)
            .res_sync()?;

        // take local ownership of stuff to be moved into task
        let sub_recv = sub.receiver.clone();
        let quer_recv = queryable.receiver.clone();
        let pub_key_expr = key_expr.into_owned();
        let resources_limit = conf.resources_limit;
        let history = conf.history;

        let (stoptx, mut stoprx) = bounded::<bool>(1);
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
                                prefix.join(&sample.key_expr).unwrap().into()
                            } else {
                                sample.key_expr.clone()
                            };

                            if let Some(queue) = cache.get_mut(queryable_key_expr.as_keyexpr()) {
                                if queue.len() >= history {
                                    queue.pop_front();
                                }
                                queue.push_back(sample);
                            } else if cache.len() >= limit {
                                log::error!("ReliabilityCache on {}: resource_limit exceeded - can't cache publication for a new resource",
                                pub_key_expr);
                            } else {
                                let mut queue: VecDeque<Sample> = VecDeque::new();
                                queue.push_back(sample);
                                cache.insert(queryable_key_expr.into(), queue);
                            }
                        }
                    },

                    // on query, reply with cach content
                    query = quer_recv.recv_async() => {
                        if let Ok(query) = query {
                            let (start, end) = query.selector().parameters_cowmap().ok().and_then(|map| map.get("_sn").map(|range|decode_range(range))).unwrap_or((None, None));
                            if !query.selector().key_expr.as_str().contains('*') {
                                if let Some(queue) = cache.get(query.selector().key_expr.as_keyexpr()) {
                                    for sample in queue {
                                        if sample_in_range(sample, start, end) {
                                            if let Err(e) = query.reply(Ok(sample.clone())).res_async().await {
                                                log::warn!("Error replying to query: {}", e);
                                            }
                                        }
                                    }
                                }
                            } else {
                                for (key_expr, queue) in cache.iter() {
                                    if query.selector().key_expr.intersects(unsafe{ keyexpr::from_str_unchecked(key_expr) }) {
                                        for sample in queue {
                                            if sample_in_range(sample, start, end) {
                                                if let Err(e) = query.reply(Ok(sample.clone())).res_async().await {
                                                    log::warn!("Error replying to query: {}", e);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    },

                    // When stoptx is dropped, stop the task
                    _ = stoprx.next().fuse() => {
                        return
                    }
                );
            }
        });

        Ok(ReliabilityCache {
            _sub: sub,
            _queryable: queryable,
            _stoptx: stoptx,
        })
    }

    /// Close this ReliabilityCache
    #[inline]
    pub fn close(self) -> impl Resolve<ZResult<()>> + 'a {
        ResolveFuture::new(async move {
            let ReliabilityCache {
                _queryable,
                _sub,
                _stoptx,
            } = self;
            _queryable.undeclare().res_async().await?;
            _sub.undeclare().res_async().await?;
            drop(_stoptx);
            Ok(())
        })
    }
}
