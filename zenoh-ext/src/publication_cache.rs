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
    handlers::FifoChannelHandler,
    internal::{bail, runtime::ZRuntime, ResolveFuture, TerminatableTask},
    key_expr::{keyexpr, KeyExpr, OwnedKeyExpr},
    pubsub::Subscriber,
    query::{Query, Queryable, ZenohParameters},
    sample::{Locality, Sample},
    Error, Resolvable, Resolve, Result as ZResult, Session, Wait,
};

/// The builder of PublicationCache, allowing to configure it.
#[zenoh_macros::unstable]
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
#[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
pub struct PublicationCacheBuilder<'a, 'b, 'c, const BACKGROUND: bool = false> {
    session: &'a Session,
    pub_key_expr: ZResult<KeyExpr<'b>>,
    queryable_suffix: Option<ZResult<KeyExpr<'c>>>,
    queryable_origin: Option<Locality>,
    complete: Option<bool>,
    history: usize,
    resources_limit: Option<usize>,
}

#[allow(deprecated)]
#[zenoh_macros::unstable]
impl<'a, 'b, 'c> PublicationCacheBuilder<'a, 'b, 'c> {
    pub(crate) fn new(
        session: &'a Session,
        pub_key_expr: ZResult<KeyExpr<'b>>,
    ) -> PublicationCacheBuilder<'a, 'b, 'c> {
        PublicationCacheBuilder {
            session,
            pub_key_expr,
            queryable_suffix: None,
            queryable_origin: None,
            complete: None,
            history: 1,
            resources_limit: None,
        }
    }

    /// Change the suffix used for queryable.
    #[zenoh_macros::unstable]
    #[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
    pub fn queryable_suffix<TryIntoKeyExpr>(mut self, queryable_suffix: TryIntoKeyExpr) -> Self
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'c>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'c>>>::Error: Into<Error>,
    {
        self.queryable_suffix = Some(queryable_suffix.try_into().map_err(Into::into));
        self
    }

    /// Restrict the matching queries that will be receive by this [`PublicationCache`]'s queryable
    /// to the ones that have the given [`Locality`](zenoh::prelude::Locality).
    #[zenoh_macros::unstable]
    #[inline]
    #[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
    pub fn queryable_allowed_origin(mut self, origin: Locality) -> Self {
        self.queryable_origin = Some(origin);
        self
    }

    /// Set completeness option for the queryable.
    #[zenoh_macros::unstable]
    #[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
    pub fn queryable_complete(mut self, complete: bool) -> Self {
        self.complete = Some(complete);
        self
    }

    /// Change the history size for each resource.
    #[zenoh_macros::unstable]
    #[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
    pub fn history(mut self, history: usize) -> Self {
        self.history = history;
        self
    }

    /// Change the limit number of cached resources.
    #[zenoh_macros::unstable]
    #[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
    pub fn resources_limit(mut self, limit: usize) -> Self {
        self.resources_limit = Some(limit);
        self
    }

    #[zenoh_macros::unstable]
    #[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
    pub fn background(self) -> PublicationCacheBuilder<'a, 'b, 'c, true> {
        PublicationCacheBuilder {
            session: self.session,
            pub_key_expr: self.pub_key_expr,
            queryable_suffix: self.queryable_suffix,
            queryable_origin: self.queryable_origin,
            complete: self.complete,
            history: self.history,
            resources_limit: self.resources_limit,
        }
    }
}

#[zenoh_macros::unstable]
#[allow(deprecated)]
impl Resolvable for PublicationCacheBuilder<'_, '_, '_> {
    type To = ZResult<PublicationCache>;
}

#[zenoh_macros::unstable]
#[allow(deprecated)]
impl Wait for PublicationCacheBuilder<'_, '_, '_> {
    #[zenoh_macros::unstable]
    fn wait(self) -> <Self as Resolvable>::To {
        PublicationCache::new(self)
    }
}

#[zenoh_macros::unstable]
#[allow(deprecated)]
impl IntoFuture for PublicationCacheBuilder<'_, '_, '_> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    #[zenoh_macros::unstable]
    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

#[zenoh_macros::unstable]
#[allow(deprecated)]
impl Resolvable for PublicationCacheBuilder<'_, '_, '_, true> {
    type To = ZResult<()>;
}

#[zenoh_macros::unstable]
#[allow(deprecated)]
impl Wait for PublicationCacheBuilder<'_, '_, '_, true> {
    #[zenoh_macros::unstable]
    fn wait(self) -> <Self as Resolvable>::To {
        PublicationCache::new(self).map(|_| ())
    }
}

#[zenoh_macros::unstable]
#[allow(deprecated)]
impl IntoFuture for PublicationCacheBuilder<'_, '_, '_, true> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    #[zenoh_macros::unstable]
    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

/// [`PublicationCache`].
#[zenoh_macros::unstable]
#[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
pub struct PublicationCache {
    local_sub: Subscriber<FifoChannelHandler<Sample>>,
    _queryable: Queryable<FifoChannelHandler<Query>>,
    task: TerminatableTask,
}

#[zenoh_macros::unstable]
#[allow(deprecated)]
impl PublicationCache {
    #[zenoh_macros::unstable]
    fn new<const BACKGROUND: bool>(
        conf: PublicationCacheBuilder<'_, '_, '_, BACKGROUND>,
    ) -> ZResult<PublicationCache> {
        let key_expr = conf.pub_key_expr?;
        // the queryable_suffix (optional), and the key_expr for PublicationCache's queryable ("<pub_key_expr>/[<queryable_suffix>]")
        let (queryable_suffix, queryable_key_expr): (Option<OwnedKeyExpr>, KeyExpr) =
            match conf.queryable_suffix {
                None => (None, key_expr.clone()),
                Some(Ok(ke)) => {
                    let queryable_key_expr = &key_expr / &ke;
                    (Some(ke.into()), queryable_key_expr)
                }
                Some(Err(e)) => bail!("Invalid key expression for queryable_suffix: {}", e),
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
                     the 'timestamping' setting must be enabled in the Zenoh configuration",
                key_expr,
            )
        }

        // declare the local subscriber that will store the local publications
        let mut local_sub = conf
            .session
            .declare_subscriber(&key_expr)
            .allowed_origin(Locality::SessionLocal)
            .wait()?;
        if BACKGROUND {
            local_sub.set_background(true);
        }

        // declare the queryable which returns the cached publications
        let mut queryable = conf.session.declare_queryable(&queryable_key_expr);
        if let Some(origin) = conf.queryable_origin {
            queryable = queryable.allowed_origin(origin);
        }
        if let Some(complete) = conf.complete {
            queryable = queryable.complete(complete);
        }
        let mut queryable = queryable.wait()?;
        if BACKGROUND {
            queryable.set_background(true);
        }

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
                                let queryable_key_expr: KeyExpr<'_> = if let Some(suffix) = &queryable_suffix {
                                    sample.key_expr() / suffix
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

                        // on query, reply with cached content
                        query = quer_recv.recv_async() => {
                            if let Ok(query) = query {
                                if !query.key_expr().as_str().contains('*') {
                                    if let Some(queue) = cache.get(query.key_expr().as_keyexpr()) {
                                        for sample in queue {
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
                                } else {
                                    for (key_expr, queue) in cache.iter() {
                                        if query.key_expr().intersects(unsafe{ keyexpr::from_str_unchecked(key_expr) }) {
                                            for sample in queue {
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

    /// Undeclare this [`PublicationCache`]`.
    #[zenoh_macros::unstable]
    #[inline]
    #[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
    pub fn undeclare(self) -> impl Resolve<ZResult<()>> {
        ResolveFuture::new(async move {
            let PublicationCache {
                _queryable,
                local_sub,
                mut task,
            } = self;
            _queryable.undeclare().await?;
            local_sub.undeclare().await?;
            task.terminate(Duration::from_secs(10));
            Ok(())
        })
    }

    #[zenoh_macros::internal]
    #[zenoh_macros::unstable]
    #[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
    pub fn set_background(&mut self, background: bool) {
        self.local_sub.set_background(background);
        self._queryable.set_background(background);
    }

    #[zenoh_macros::unstable]
    #[deprecated = "Use `AdvancedPublisher` and `AdvancedSubscriber` instead."]
    pub fn key_expr(&self) -> &KeyExpr<'static> {
        self.local_sub.key_expr()
    }
}
