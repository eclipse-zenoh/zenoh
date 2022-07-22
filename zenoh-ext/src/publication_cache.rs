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
use async_std::channel::{bounded, Sender};
use async_std::task;
use futures::FutureExt;
use futures::StreamExt;
use futures::{select, Future};
use std::collections::{HashMap, VecDeque};
use std::convert::TryInto;
use std::pin::Pin;
use zenoh::prelude::*;
use zenoh::queryable::{HandlerQueryable, Query};
use zenoh::subscriber::FlumeSubscriber;
use zenoh::Session;
use zenoh_core::{bail, AsyncResolve, Resolvable, Resolve};
use zenoh_core::{Result as ZResult, SyncResolve};

/// The builder of PublicationCache, allowing to configure it.
pub struct PublicationCacheBuilder<'a, 'b, 'c> {
    session: &'a Session,
    pub_key_expr: ZResult<KeyExpr<'b>>,
    queryable_prefix: Option<ZResult<KeyExpr<'c>>>,
    history: usize,
    resources_limit: Option<usize>,
}

impl<'a, 'b, 'c> PublicationCacheBuilder<'a, 'b, 'c> {
    pub(crate) fn new(
        session: &'a Session,
        pub_key_expr: ZResult<KeyExpr<'b>>,
    ) -> PublicationCacheBuilder<'a, 'b, 'c> {
        PublicationCacheBuilder {
            session,
            pub_key_expr,
            queryable_prefix: None,
            history: 1,
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

impl<'a> Resolvable for PublicationCacheBuilder<'a, '_, '_> {
    type Output = ZResult<PublicationCache<'a>>;
}
impl AsyncResolve for PublicationCacheBuilder<'_, '_, '_> {
    type Future = futures::future::Ready<Self::Output>;
    fn res_async(self) -> Self::Future {
        futures::future::ready(self.res_sync())
    }
}
impl SyncResolve for PublicationCacheBuilder<'_, '_, '_> {
    fn res_sync(self) -> Self::Output {
        PublicationCache::new(self)
    }
}

pub struct PublicationCache<'a> {
    _local_sub: FlumeSubscriber<'a>,
    _queryable: HandlerQueryable<'a, flume::Receiver<Query>>,
    _stoptx: Sender<bool>,
}

impl<'a> PublicationCache<'a> {
    fn new(conf: PublicationCacheBuilder<'a, '_, '_>) -> ZResult<PublicationCache<'a>> {
        let key_expr = conf.pub_key_expr?;
        // the queryable_prefix (optional), and the key_expr for PublicationCache's queryable ("[<queryable_prefix>]/<pub_key_expr>")
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
            "Create PublicationCache on {} with history={} resource_limit={:?}",
            &key_expr,
            conf.history,
            conf.resources_limit
        );

        if conf.session.hlc().is_none() {
            bail!(
                "Failed requirement for PublicationCache on {}: \
                     the Session is not configured with 'add_timestamp=true'",
                key_expr
            )
        }

        // declare the local subscriber that will store the local publications
        let local_sub = conf
            .session
            .declare_subscriber(&key_expr)
            .local()
            .res_sync()?;

        // declare the queryable that will answer to queries on cache
        let queryable = conf
            .session
            .declare_queryable(&queryable_key_expr)
            .res_sync()?;

        // take local ownership of stuff to be moved into task
        let sub_recv = local_sub.receiver.clone();
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
                                log::error!("PublicationCache on {}: resource_limit exceeded - can't cache publication for a new resource",
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
                            if !query.selector().key_expr.as_str().contains('*') {
                                if let Some(queue) = cache.get(query.selector().key_expr.as_keyexpr()) {
                                    for sample in queue {
                                        if let Err(e) = query.reply(Ok(sample.clone())).res_async().await {
                                            log::warn!("Error replying to query: {}", e);
                                        }
                                    }
                                }
                            } else {
                                for (key_expr, queue) in cache.iter() {
                                    if query.selector().key_expr.intersects(unsafe{ keyexpr::from_str_unchecked(key_expr) }) {
                                        for sample in queue {
                                            if let Err(e) = query.reply(Ok(sample.clone())).res_async().await {
                                                log::warn!("Error replying to query: {}", e);
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

        Ok(PublicationCache {
            _local_sub: local_sub,
            _queryable: queryable,
            _stoptx: stoptx,
        })
    }

    /// Close this PublicationCache
    #[inline]
    pub fn close(self) -> impl Resolve<ZResult<()>> + 'a {
        struct PublicationCacheCloser<'a> {
            p: PublicationCache<'a>,
        }
        impl Resolvable for PublicationCacheCloser<'_> {
            type Output = ZResult<()>;
        }
        impl SyncResolve for PublicationCacheCloser<'_> {
            fn res_sync(self) -> Self::Output {
                drop(self);
                Ok(())
            }
        }
        impl<'a> AsyncResolve for PublicationCacheCloser<'a> {
            type Future = Pin<Box<dyn Future<Output = ZResult<()>> + Send + 'a>>;
            fn res_async(self) -> Self::Future {
                Box::pin(async move {
                    self.p._queryable.undeclare().res_async().await?;
                    self.p._local_sub.undeclare().res_async().await?;
                    drop(self.p._stoptx);
                    Ok(())
                })
            }
        }
        PublicationCacheCloser { p: self }
    }
}
