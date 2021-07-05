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
use zenoh::net::utils::resource_name;
use zenoh::net::*;
use zenoh_util::zerror;

pub(crate) const PUBLISHER_CACHE_QUERYABLE_KIND: ZInt = 0x08;

/// The builder of PublicationCache, allowing to configure it.
#[derive(Clone)]
pub struct PublicationCacheBuilder<'a> {
    session: &'a Session,
    pub_reskey: ResKey,
    queryable_prefix: Option<String>,
    history: usize,
    resources_limit: Option<usize>,
}

impl PublicationCacheBuilder<'_> {
    pub(crate) fn new<'a>(
        session: &'a Session,
        pub_reskey: &ResKey,
    ) -> PublicationCacheBuilder<'a> {
        PublicationCacheBuilder {
            session,
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

impl<'a> Future for PublicationCacheBuilder<'a> {
    type Output = ZResult<PublicationCache<'a>>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Ready(PublicationCache::new(Pin::into_inner(self).clone()))
    }
}

impl<'a> ZFuture for PublicationCacheBuilder<'a> {
    fn wait(self) -> ZResult<PublicationCache<'a>> {
        PublicationCache::new(self)
    }
}

pub struct PublicationCache<'a> {
    _publisher: Publisher<'a>,
    _local_sub: Subscriber<'a>,
    _queryable: Queryable<'a>,
    _stoptx: Sender<bool>,
}

impl PublicationCache<'_> {
    fn new(conf: PublicationCacheBuilder<'_>) -> ZResult<PublicationCache<'_>> {
        log::debug!("Declare PublicationCache on {}", conf.pub_reskey);

        if conf.session.hlc().is_none() {
            return zerror!(ZErrorKind::Other {
                descr: format!(
                    "Failed requirement for PublicationCache on {}: \
                     the Session is not configured with 'add_timestamp=true'",
                    conf.pub_reskey
                )
            });
        }

        // declare the publisher
        let publisher = conf.session.declare_publisher(&conf.pub_reskey).wait()?;

        // declare the local subscriber that will store the local publications
        let mut local_sub = conf
            .session
            .declare_local_subscriber(&conf.pub_reskey)
            .wait()?;

        // declare the queryable that will answer to queries on cache
        let queryable_reskey = if let Some(prefix) = &conf.queryable_prefix {
            ResKey::from(format!(
                "{}{}",
                prefix,
                conf.session.reskey_to_resname(&conf.pub_reskey)?
            ))
        } else {
            conf.pub_reskey.clone()
        };
        let mut queryable = conf
            .session
            .declare_queryable(&queryable_reskey, PUBLISHER_CACHE_QUERYABLE_KIND)
            .wait()?;

        // take local ownership of stuff to be moved into task
        let mut sub_recv = local_sub.receiver().clone();
        let mut quer_recv = queryable.receiver().clone();
        let pub_reskey = conf.pub_reskey;
        let resources_limit = conf.resources_limit;
        let queryable_prefix = conf.queryable_prefix;
        let history = conf.history;

        let (stoptx, stoprx) = bounded::<bool>(1);
        task::spawn(async move {
            let mut cache: HashMap<String, VecDeque<Sample>> =
                HashMap::with_capacity(resources_limit.unwrap_or(32));
            let limit = resources_limit.unwrap_or(usize::MAX);

            loop {
                select!(
                    // on publication received by the local subscriber, store it
                    sample = sub_recv.next().fuse() => {
                        if let Some(sample) = sample {
                            let queryable_resname = if let Some(prefix) = &queryable_prefix {
                                format!("{}{}", prefix, sample.res_name)
                            } else {
                                sample.res_name.clone()
                            };

                            if let Some(queue) = cache.get_mut(&queryable_resname) {
                                if queue.len() >= history {
                                    queue.pop_front();
                                }
                                queue.push_back(sample);
                            } else if cache.len() >= limit {
                                log::error!("PublicationCache on {}: resource_limit exceeded - can't cache publication for a new resource",
                                pub_reskey);
                            } else {
                                let mut queue: VecDeque<Sample> = VecDeque::with_capacity(history);
                                queue.push_back(sample);
                                cache.insert(queryable_resname, queue);
                            }
                        }
                    },

                    // on query, reply with cach content
                    query = quer_recv.next().fuse() => {
                        if let Some(query) = query {
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
                        }
                    },

                    // When stoptx is dropped, stop the task
                    _ = stoprx.recv().fuse() => {
                        return
                    }
                );
            }
        });

        Ok(PublicationCache {
            _publisher: publisher,
            _local_sub: local_sub,
            _queryable: queryable,
            _stoptx: stoptx,
        })
    }

    /// Undeclare this PublicationCache
    #[inline]
    pub fn undeclare(self) -> impl ZFuture<Output = ZResult<()>> {
        // just drop self and all its content
        zready(Ok(()))
    }
}
