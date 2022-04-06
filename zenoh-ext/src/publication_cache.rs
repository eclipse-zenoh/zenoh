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
use async_std::pin::Pin;
use async_std::task;
use async_std::task::{Context, Poll};
use futures::select;
use futures::FutureExt;
use futures::StreamExt;
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use zenoh::prelude::*;
use zenoh::queryable::Queryable;
use zenoh::subscriber::Subscriber;
use zenoh::sync::zready;
use zenoh::utils::key_expr;
use zenoh::Session;
use zenoh_core::bail;
use zenoh_core::Result as ZResult;

/// The builder of PublicationCache, allowing to configure it.
#[derive(Clone)]
pub struct PublicationCacheBuilder<'a, 'b> {
    session: &'a Session,
    pub_key_expr: KeyExpr<'b>,
    queryable_prefix: Option<String>,
    history: usize,
    resources_limit: Option<usize>,
}

impl<'a, 'b> PublicationCacheBuilder<'a, 'b> {
    pub(crate) fn new(
        session: &'a Session,
        pub_key_expr: KeyExpr<'b>,
    ) -> PublicationCacheBuilder<'a, 'b> {
        PublicationCacheBuilder {
            session,
            pub_key_expr,
            queryable_prefix: None,
            history: 1,
            resources_limit: None,
        }
    }

    /// Change the prefix used for queryable.
    pub fn queryable_prefix(mut self, queryable_prefix: String) -> Self {
        self.queryable_prefix = Some(queryable_prefix);
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

impl<'a, 'b> Future for PublicationCacheBuilder<'a, '_> {
    type Output = ZResult<PublicationCache<'a>>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Ready(PublicationCache::new(Pin::into_inner(self).clone()))
    }
}

impl<'a> ZFuture for PublicationCacheBuilder<'a, '_> {
    fn wait(self) -> ZResult<PublicationCache<'a>> {
        PublicationCache::new(self)
    }
}

pub struct PublicationCache<'a> {
    _local_sub: Subscriber<'a>,
    _queryable: Queryable<'a>,
    _stoptx: Sender<bool>,
}

impl<'a> PublicationCache<'a> {
    pub const QUERYABLE_KIND: ZInt = 0x08;

    fn new(conf: PublicationCacheBuilder<'a, '_>) -> ZResult<PublicationCache<'a>> {
        log::debug!(
            "Create PublicationCache on {} with history={} resource_limit={:?}",
            conf.pub_key_expr,
            conf.history,
            conf.resources_limit
        );

        if conf.session.hlc().is_none() {
            bail!(
                "Failed requirement for PublicationCache on {}: \
                     the Session is not configured with 'add_timestamp=true'",
                conf.pub_key_expr
            )
        }

        // declare the local subscriber that will store the local publications
        let mut local_sub = conf.session.subscribe(&conf.pub_key_expr).local().wait()?;

        // declare the queryable that will answer to queries on cache
        let queryable_key_expr = if let Some(prefix) = &conf.queryable_prefix {
            KeyExpr::from(format!(
                "{}{}",
                prefix,
                conf.session.key_expr_to_expr(&conf.pub_key_expr)?
            ))
        } else {
            conf.pub_key_expr.clone()
        };
        let mut queryable = conf
            .session
            .queryable(&queryable_key_expr)
            .kind(PublicationCache::QUERYABLE_KIND)
            .wait()?;

        // take local ownership of stuff to be moved into task
        let mut sub_recv = local_sub.receiver().clone();
        let mut quer_recv = queryable.receiver().clone();
        let pub_key_expr = conf.pub_key_expr.to_owned();
        let resources_limit = conf.resources_limit;
        let queryable_prefix = conf.queryable_prefix;
        let history = conf.history;

        let (stoptx, mut stoprx) = bounded::<bool>(1);
        task::spawn(async move {
            let mut cache: HashMap<String, VecDeque<Sample>> =
                HashMap::with_capacity(resources_limit.unwrap_or(32));
            let limit = resources_limit.unwrap_or(usize::MAX);

            loop {
                select!(
                    // on publication received by the local subscriber, store it
                    sample = sub_recv.next().fuse() => {
                        if let Some(sample) = sample {
                            let queryable_key_expr = if let Some(prefix) = &queryable_prefix {
                                format!("{}{}", prefix, sample.key_expr)
                            } else {
                                sample.key_expr.to_string()
                            };

                            if let Some(queue) = cache.get_mut(&queryable_key_expr) {
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
                                cache.insert(queryable_key_expr, queue);
                            }
                        }
                    },

                    // on query, reply with cach content
                    query = quer_recv.next().fuse() => {
                        if let Some(query) = query {
                            if !query.selector().key_selector.as_str().contains('*') {
                                if let Some(queue) = cache.get(query.selector().key_selector.as_str()) {
                                    for sample in queue {
                                        query.reply(sample.clone());
                                    }
                                }
                            } else {
                                for (key_expr, queue) in cache.iter() {
                                    if key_expr::intersect(query.selector().key_selector.as_str(), key_expr) {
                                        for sample in queue {
                                            query.reply(sample.clone());
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
    pub fn close(self) -> impl ZFuture<Output = ZResult<()>> {
        // just drop self and all its content
        zready(Ok(()))
    }
}
