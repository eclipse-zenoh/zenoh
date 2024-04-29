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
    collections::{HashMap, VecDeque},
    convert::TryInto,
    future::{IntoFuture, Ready},
    time::Duration,
};

use zenoh::{
    core::{Error, Resolvable, Resolve, Result as ZResult},
    internal::{bail, ResolveFuture, TerminatableTask},
    key_expr::{keyexpr, KeyExpr, OwnedKeyExpr},
    prelude::Wait,
    queryable::{Query, Queryable},
    runtime::ZRuntime,
    sample::{Locality, Sample},
    session::{SessionDeclarations, SessionRef},
    subscriber::FlumeSubscriber,
};

/// The builder of PublicationCache, allowing to configure it.
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
pub struct PublicationCacheBuilder<'a, 'b, 'c> {
    session: SessionRef<'a>,
    pub_key_expr: ZResult<KeyExpr<'b>>,
    queryable_prefix: Option<ZResult<KeyExpr<'c>>>,
    queryable_origin: Option<Locality>,
    complete: Option<bool>,
    history: usize,
    resources_limit: Option<usize>,
}

impl<'a, 'b, 'c> PublicationCacheBuilder<'a, 'b, 'c> {
    pub(crate) fn new(
        session: SessionRef<'a>,
        pub_key_expr: ZResult<KeyExpr<'b>>,
    ) -> PublicationCacheBuilder<'a, 'b, 'c> {
        PublicationCacheBuilder {
            session,
            pub_key_expr,
            queryable_prefix: None,
            queryable_origin: None,
            complete: None,
            history: 1,
            resources_limit: None,
        }
    }

    /// Change the prefix used for queryable.
    pub fn queryable_prefix<TryIntoKeyExpr>(mut self, queryable_prefix: TryIntoKeyExpr) -> Self
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'c>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'c>>>::Error: Into<Error>,
    {
        self.queryable_prefix = Some(queryable_prefix.try_into().map_err(Into::into));
        self
    }

    /// Restrict the matching queries that will be receive by this [`PublicationCache`]'s queryable
    /// to the ones that have the given [`Locality`](zenoh::prelude::Locality).
    #[zenoh_macros::unstable]
    #[inline]
    pub fn queryable_allowed_origin(mut self, origin: Locality) -> Self {
        self.queryable_origin = Some(origin);
        self
    }

    /// Set completeness option for the queryable.
    pub fn queryable_complete(mut self, complete: bool) -> Self {
        self.complete = Some(complete);
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
    type To = ZResult<PublicationCache<'a>>;
}

impl Wait for PublicationCacheBuilder<'_, '_, '_> {
    fn wait(self) -> <Self as Resolvable>::To {
        PublicationCache::new(self)
    }
}

impl<'a> IntoFuture for PublicationCacheBuilder<'a, '_, '_> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

pub struct PublicationCache<'a> {
    local_sub: FlumeSubscriber<'a>,
    _queryable: Queryable<'a, flume::Receiver<Query>>,
    task: TerminatableTask,
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
        tracing::debug!(
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
            .allowed_origin(Locality::SessionLocal)
            .wait()?;

        // declare the queryable which returns the cached publications
        let mut queryable = conf.session.declare_queryable(&queryable_key_expr);
        if let Some(origin) = conf.queryable_origin {
            queryable = queryable.allowed_origin(origin);
        }
        if let Some(complete) = conf.complete {
            queryable = queryable.complete(complete);
        }
        let queryable = queryable.wait()?;

        // take local ownership of stuff to be moved into task
        let sub_recv = local_sub.handler().clone();
        let quer_recv = queryable.handler().clone();
        let pub_key_expr = key_expr.into_owned();
        let resources_limit = conf.resources_limit;
        let history = conf.history;

        // TODO(yuyuan): use CancellationToken to manage it
        let token = TerminatableTask::create_cancellation_token();
        let token2 = token.clone();
        let task = TerminatableTask::spawn(
            ZRuntime::Application,
            async move {
                let mut cache: HashMap<OwnedKeyExpr, VecDeque<Sample>> =
                    HashMap::with_capacity(resources_limit.unwrap_or(32));
                let limit = resources_limit.unwrap_or(usize::MAX);
                loop {
                    tokio::select! {
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
                                    tracing::error!("PublicationCache on {}: resource_limit exceeded - can't cache publication for a new resource",
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
                                if !query.selector().key_expr().as_str().contains('*') {
                                    if let Some(queue) = cache.get(query.selector().key_expr().as_keyexpr()) {
                                        for sample in queue {
                                            if let (Ok(Some(time_range)), Some(timestamp)) = (query.parameters().time_range(), sample.timestamp()) {
                                                if !time_range.contains(timestamp.get_time().to_system_time()){
                                                    continue;
                                                }
                                            }
                                            if let Err(e) = query.reply_sample(sample.clone()).await {
                                                tracing::warn!("Error replying to query: {}", e);
                                            }
                                        }
                                    }
                                } else {
                                    for (key_expr, queue) in cache.iter() {
                                        if query.selector().key_expr().intersects(unsafe{ keyexpr::from_str_unchecked(key_expr) }) {
                                            for sample in queue {
                                                if let (Ok(Some(time_range)), Some(timestamp)) = (query.parameters().time_range(), sample.timestamp()) {
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
                        },
                        _ = token2.cancelled() => return
                    }
                }
            },
            token,
        );

        Ok(PublicationCache {
            local_sub,
            _queryable: queryable,
            task,
        })
    }

    /// Close this PublicationCache
    #[inline]
    pub fn close(self) -> impl Resolve<ZResult<()>> + 'a {
        ResolveFuture::new(async move {
            let PublicationCache {
                _queryable,
                local_sub,
                task,
            } = self;
            _queryable.undeclare().await?;
            local_sub.undeclare().await?;
            task.terminate(Duration::from_secs(10));
            Ok(())
        })
    }

    pub fn key_expr(&self) -> &KeyExpr<'static> {
        self.local_sub.key_expr()
    }
}
