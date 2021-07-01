//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
use async_std::channel::{bounded, Sender};
use async_std::pin::Pin;
use async_std::task;
use async_std::task::{Context, Poll};
use futures::select;
use futures::FutureExt;
use futures_lite::StreamExt;
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::sync::Arc;
use zenoh::net::utils::resource_name;
use zenoh::net::*;
use zenoh_util::core::ZResult;
use zenoh_util::sync::ZFuture;
use zenoh_util::zresolved;

pub(crate) const PUBLISHER_CACHE_QUERYABLE_KIND: ZInt = 0x08;

/// The builder of PublicationCache, allowing to configure it.
#[derive(Clone)]
pub struct PublicationCacheBuilder {
    session: Arc<Session>,
    pub_reskey: ResKey,
    queryable_prefix: Option<String>,
    history: usize,
    resources_limit: Option<usize>,
}

impl PublicationCacheBuilder {
    pub(crate) fn new(session: &Session, pub_reskey: &ResKey) -> PublicationCacheBuilder {
        PublicationCacheBuilder {
            session: session.to_arc(),
            pub_reskey: pub_reskey.clone(),
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

impl<'a> Future for PublicationCacheBuilder {
    type Output = ZResult<PublicationCache>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Ready(PublicationCache::new(Pin::into_inner(self).clone()))
    }
}

impl<'a> ZFuture<ZResult<PublicationCache>> for PublicationCacheBuilder {
    fn wait(self) -> ZResult<PublicationCache> {
        PublicationCache::new(self)
    }
}

pub struct PublicationCache {
    _stoptx: Sender<bool>,
}

impl PublicationCache {
    fn new(conf: PublicationCacheBuilder) -> ZResult<PublicationCache> {
        log::debug!("Declare PublicationCache on {}", conf.pub_reskey);

        let (stoptx, stoprx) = bounded::<bool>(1);
        task::spawn(async move {
            // declare the publisher
            let publisher = conf
                .session
                .declare_publisher(&conf.pub_reskey)
                .wait()
                .unwrap();

            // declare the local subscriber that will store the local publications
            let mut local_sub = conf
                .session
                .declare_local_subscriber(&conf.pub_reskey)
                .wait()
                .unwrap();

            // declare the queryable that will answer to queries on cache
            let queryable_reskey = if let Some(prefix) = &conf.queryable_prefix {
                ResKey::from(format!(
                    "{}{}",
                    prefix,
                    conf.session.reskey_to_resname(&conf.pub_reskey).unwrap()
                ))
            } else {
                conf.pub_reskey.clone()
            };
            let mut queryable = conf
                .session
                .declare_queryable(&queryable_reskey, PUBLISHER_CACHE_QUERYABLE_KIND)
                .wait()
                .unwrap();

            let mut cache: HashMap<String, VecDeque<Sample>> =
                HashMap::with_capacity(conf.resources_limit.unwrap_or(32));
            let limit = conf.resources_limit.unwrap_or(usize::MAX);

            loop {
                select!(
                    // on publication received by the local subscriber, store it
                    sample = local_sub.receiver().next().fuse() => {
                        let sample = sample.unwrap();
                        let queryable_resname = if let Some(prefix) = &conf.queryable_prefix {
                            format!("{}{}", prefix, sample.res_name)
                        } else {
                            sample.res_name.clone()
                        };

                        if let Some(queue) = cache.get_mut(&queryable_resname) {
                            if queue.len() >= conf.history {
                                queue.pop_front();
                            }
                            queue.push_back(sample);
                        } else if cache.len() >= limit {
                            log::error!("PublicationCache on {}: resource_limit exceeded - can't cache publication for a new resource",
                            conf.pub_reskey);
                        } else {
                            let mut queue: VecDeque<Sample> = VecDeque::with_capacity(conf.history);
                            queue.push_back(sample);
                            cache.insert(queryable_resname, queue);
                        }

                    },

                    // on query, reply with cach content
                    query = queryable.receiver().next().fuse() => {
                        let query = query.unwrap();
                        if !query.res_name.contains('*') {
                            if let Some(queue) = cache.get(&query.res_name) {
                                for sample in queue {
                                    query.reply(sample.clone());
                                }
                            }
                        } else {
                            for (resname, queue) in cache.iter() {
                                if resource_name::intersect(&query.res_name, &resname) {
                                    for sample in queue {
                                        query.reply(sample.clone());
                                    }
                                }
                            }
                        }
                    },

                    // When stoptx is dropped, stop the task
                    _ = stoprx.recv().fuse() => {
                        log::debug!("Undeclare PublicationCache on {}", conf.pub_reskey);
                        if let Err(e) = publisher.undeclare().await {
                            log::warn!("Error undeclaring publisher for PublicationCache on {}: {}", conf.pub_reskey, e);
                        }
                        if let Err(e) = local_sub.undeclare().await {
                            log::warn!("Error undeclaring local subscriber for PublicationCache on {}: {}", conf.pub_reskey, e);
                        }
                        if let Err(e) = queryable.undeclare().await {
                            log::warn!("Error undeclaring queryable for PublicationCache on {}: {}", conf.pub_reskey, e);
                        }
                        return
                    }
                );
            }
        });

        Ok(PublicationCache { _stoptx: stoptx })
    }

    /// Undeclare this PublicationCache
    #[inline]
    pub fn undeclare(self) -> ZResolvedFuture<ZResult<()>> {
        // just dropping _stoptx will stop the task
        zresolved!(Ok(()))
    }
}
