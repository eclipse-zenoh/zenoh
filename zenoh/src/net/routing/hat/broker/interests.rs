//
// Copyright (c) 2025 ZettaScale Technology
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
    collections::{HashMap, HashSet},
    sync::{atomic::Ordering, Arc},
};

use itertools::Itertools;
#[allow(unused_imports)]
use zenoh_core::polyfill::*;
use zenoh_protocol::network::{
    declare::{
        self, common::ext::WireExprType, queryable::ext::QueryableInfoType, QueryableId,
        SubscriberId, TokenId,
    },
    interest::{InterestId, InterestMode},
    Declare, DeclareBody, DeclareFinal, DeclareQueryable, DeclareSubscriber, DeclareToken,
    Interest, UndeclareQueryable, UndeclareSubscriber,
};

use super::Hat;
use crate::net::routing::{
    dispatcher::{
        interests::{CurrentInterest, RemoteInterest},
        local_resources::LocalResourceInfoTrait,
        queries::merge_qabl_infos,
        resource::Resource,
        tables::TablesData,
    },
    gateway::SubscriberInfo,
    hat::{
        DispatcherContext, HatBaseTrait, HatInterestTrait, Remote, RouteCurrentDeclareResult,
        RouteInterestResult,
    },
    RoutingContext,
};

impl HatInterestTrait for Hat {
    fn route_interest(
        &mut self,
        _ctx: DispatcherContext,
        _msg: &Interest,
        _res: Option<Arc<Resource>>,
        _remote: &Remote,
    ) -> RouteInterestResult {
        unreachable!("north-bound broker hat")
    }

    fn route_interest_final(
        &mut self,
        _ctx: DispatcherContext,
        _msg: &Interest,
        _remote_interest: &RemoteInterest,
    ) {
        unreachable!("north-bound broker hat")
    }

    fn route_declare_final(
        &mut self,
        _ctx: DispatcherContext,
        _interest_id: InterestId,
    ) -> RouteCurrentDeclareResult {
        unreachable!("north-bound broker hat")
    }

    fn route_current_token(
        &mut self,
        _ctx: DispatcherContext,
        _interest_id: InterestId,
        _res: Arc<Resource>,
    ) -> RouteCurrentDeclareResult {
        unreachable!("north-bound broker hat")
    }

    #[allow(clippy::incompatible_msrv)]
    #[tracing::instrument(level = "debug", skip(ctx, msg), ret)]
    fn send_current_subscribers(
        &self,
        ctx: DispatcherContext,
        msg: &Interest,
        res: Option<Arc<Resource>>,
        mut other_matches: HashMap<Arc<Resource>, SubscriberInfo>,
    ) {
        debug_assert!(self.owns(ctx.src_face));
        debug_assert!(ctx.src_face.region.bound().is_south());
        debug_assert_ne!(msg.mode, InterestMode::Final);

        let src_fid = ctx.src_face.id;

        let matches = {
            other_matches.extend(
                self.owned_faces(ctx.tables)
                    .filter(|face| face.id != src_fid)
                    .flat_map(|face| self.face_hat(face).remote_subs.values())
                    .filter(|sub| res.as_ref().is_none_or(|res| res.matches(sub)))
                    .cloned()
                    .zip(std::iter::repeat(SubscriberInfo)),
            );

            other_matches
        };
        let mut matches = matches.into_keys();

        if msg.options.aggregate() {
            if let Some(aggregated_res) = &res {
                let (sub_id, sub_info) = if msg.mode.is_future() {
                    let face_hat_mut = self.face_hat_mut(ctx.src_face);

                    for sub in matches {
                        face_hat_mut.local_subs.insert_simple_resource(
                            sub.clone(),
                            SubscriberInfo,
                            || face_hat_mut.next_id.fetch_add(1, Ordering::SeqCst),
                            HashSet::new(),
                        );
                    }

                    face_hat_mut.local_subs.insert_aggregated_resource(
                        aggregated_res.clone(),
                        || face_hat_mut.next_id.fetch_add(1, Ordering::SeqCst),
                        HashSet::from_iter([msg.id]),
                    )
                } else {
                    (
                        SubscriberId::default(),
                        matches.next().map(|_| SubscriberInfo),
                    )
                };

                if msg.mode.is_current() && sub_info.is_some() {
                    // send declare only if there is at least one resource matching the aggregate
                    let wire_expr = Resource::decl_key(aggregated_res, ctx.src_face);
                    tracing::debug!(dst = %ctx.src_face);
                    (ctx.send_declare)(
                        &ctx.src_face.primitives,
                        RoutingContext::with_expr(
                            Declare {
                                interest_id: Some(msg.id),
                                ext_qos: declare::ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                                body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                    id: sub_id,
                                    wire_expr,
                                }),
                            },
                            aggregated_res.expr().to_string(),
                        ),
                    );
                }
            }
        } else if !msg.options.aggregate() && msg.mode.is_current() {
            for sub in matches {
                let sub_id = if msg.mode.is_future() {
                    let face_hat_mut = self.face_hat_mut(ctx.src_face);
                    face_hat_mut
                        .local_subs
                        .insert_simple_resource(
                            sub.clone(),
                            SubscriberInfo,
                            || face_hat_mut.next_id.fetch_add(1, Ordering::SeqCst),
                            HashSet::from([msg.id]),
                        )
                        .0
                } else {
                    SubscriberId::default()
                };
                let wire_expr = Resource::decl_key(&sub, ctx.src_face);
                tracing::debug!(dst = %ctx.src_face);
                (ctx.send_declare)(
                    &ctx.src_face.primitives,
                    RoutingContext::with_expr(
                        Declare {
                            interest_id: Some(msg.id),
                            ext_qos: declare::ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                            body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                                id: sub_id,
                                wire_expr,
                            }),
                        },
                        sub.expr().to_string(),
                    ),
                );
            }
        }
    }

    #[allow(clippy::incompatible_msrv)]
    #[tracing::instrument(level = "debug", skip(ctx, msg), ret)]
    fn send_current_queryables(
        &self,
        ctx: DispatcherContext,
        msg: &Interest,
        res: Option<Arc<Resource>>,
        other_matches: HashMap<Arc<Resource>, QueryableInfoType>,
    ) {
        debug_assert!(self.owns(ctx.src_face));
        debug_assert!(ctx.src_face.region.bound().is_south());
        debug_assert_ne!(msg.mode, InterestMode::Final);

        let matches: HashMap<Arc<Resource>, QueryableInfoType> = other_matches
            .into_iter()
            .chain(
                self.owned_faces(ctx.tables)
                    .filter(|f| f.id != ctx.src_face.id)
                    .flat_map(|f| self.face_hat(f).remote_qabls.values().cloned())
                    .filter(|(r, _)| {
                        r.ctx.is_some() && res.as_ref().is_none_or(|res| res.matches(r))
                    }),
            )
            .fold(HashMap::new(), |mut acc, (res, info)| {
                acc.entry(res)
                    .and_modify(|i| {
                        *i = merge_qabl_infos(*i, info);
                    })
                    .or_insert(info);
                acc
            });

        if msg.options.aggregate() {
            if let Some(aggregated_res) = &res {
                let (resource_id, qabl_info) = if msg.mode.is_future() {
                    for (qabl, qabl_info) in matches {
                        let face_hat_mut = self.face_hat_mut(ctx.src_face);
                        face_hat_mut.local_qabls.insert_simple_resource(
                            qabl.clone(),
                            qabl_info,
                            || face_hat_mut.next_id.fetch_add(1, Ordering::SeqCst),
                            HashSet::new(),
                        );
                    }
                    let face_hat_mut = self.face_hat_mut(ctx.src_face);
                    face_hat_mut.local_qabls.insert_aggregated_resource(
                        aggregated_res.clone(),
                        || face_hat_mut.next_id.fetch_add(1, Ordering::SeqCst),
                        HashSet::from_iter([msg.id]),
                    )
                } else {
                    (
                        QueryableId::default(),
                        QueryableInfoType::aggregate_many(
                            aggregated_res,
                            matches.iter().map(|(res, info)| (res, *info)),
                        ),
                    )
                };
                if let Some(ext_info) = msg.mode.is_current().then_some(qabl_info).flatten() {
                    // send declare only if there is at least one resource matching the aggregate
                    let wire_expr = Resource::decl_key(aggregated_res, ctx.src_face);
                    tracing::debug!(dst = %ctx.src_face);
                    (ctx.send_declare)(
                        &ctx.src_face.primitives,
                        RoutingContext::with_expr(
                            Declare {
                                interest_id: Some(msg.id),
                                ext_qos: declare::ext::QoSType::DECLARE,
                                ext_tstamp: None,
                                ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                                body: DeclareBody::DeclareQueryable(DeclareQueryable {
                                    id: resource_id,
                                    wire_expr,
                                    ext_info,
                                }),
                            },
                            aggregated_res.expr().to_string(),
                        ),
                    );
                }
            }
        } else if !msg.options.aggregate() && msg.mode.is_current() {
            for (qabl, qabl_info) in matches {
                let resource_id = if msg.mode.is_future() {
                    let face_hat_mut = self.face_hat_mut(ctx.src_face);
                    face_hat_mut
                        .local_qabls
                        .insert_simple_resource(
                            qabl.clone(),
                            qabl_info,
                            || face_hat_mut.next_id.fetch_add(1, Ordering::SeqCst),
                            HashSet::from([msg.id]),
                        )
                        .0
                } else {
                    QueryableId::default()
                };
                let wire_expr = Resource::decl_key(&qabl, ctx.src_face);
                tracing::debug!(dst = %ctx.src_face);
                (ctx.send_declare)(
                    &ctx.src_face.primitives,
                    RoutingContext::with_expr(
                        Declare {
                            interest_id: Some(msg.id),
                            ext_qos: declare::ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                            body: DeclareBody::DeclareQueryable(DeclareQueryable {
                                id: resource_id,
                                wire_expr,
                                ext_info: qabl_info,
                            }),
                        },
                        qabl.expr().to_string(),
                    ),
                );
            }
        }
    }

    #[allow(clippy::incompatible_msrv)]
    #[tracing::instrument(level = "debug", skip(ctx, msg), ret)]
    fn send_current_tokens(
        &self,
        ctx: DispatcherContext,
        msg: &Interest,
        res: Option<Arc<Resource>>,
        mut other_matches: HashSet<Arc<Resource>>,
    ) {
        debug_assert!(self.owns(ctx.src_face));
        debug_assert!(ctx.src_face.region.bound().is_south());
        debug_assert!(!msg.options.aggregate());

        let matches = {
            // NOTE(regions): we don't exclude inbound tokens from the src face as the API includes
            // session-local tokens in liveliness queries/subscribers.
            other_matches.extend(
                self.owned_faces(ctx.tables)
                    .flat_map(|face| self.face_hat(face).remote_tokens.values())
                    .filter(|token| res.as_ref().is_none_or(|res| res.matches(token)))
                    .cloned(),
            );

            other_matches.into_iter()
        };

        for token in matches {
            let id = if msg.mode.is_future() {
                let face_hat = self.face_hat_mut(ctx.src_face);

                *face_hat
                    .local_tokens
                    .entry(token.clone())
                    .or_insert_with(|| face_hat.next_id.fetch_add(1, Ordering::SeqCst))
            } else {
                TokenId::default()
            };

            let wire_expr = Resource::decl_key(&token, ctx.src_face);
            tracing::debug!(dst = %ctx.src_face);
            (ctx.send_declare)(
                &ctx.src_face.primitives,
                RoutingContext::with_expr(
                    Declare {
                        interest_id: Some(msg.id),
                        ext_qos: declare::ext::QoSType::DECLARE,
                        ext_tstamp: None,
                        ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                        body: DeclareBody::DeclareToken(DeclareToken { id, wire_expr }),
                    },
                    token.expr().to_string(),
                ),
            );
        }
    }

    #[tracing::instrument(level = "debug", skip(ctx), ret)]
    fn propagate_current_token(
        &self,
        ctx: DispatcherContext,
        res: Arc<Resource>,
        interest: CurrentInterest,
    ) {
        debug_assert!(self.region().bound().is_south());

        let mut dst = self.hat_remote(&interest.src).clone();

        debug_assert!(dst.region.bound().is_south());

        if self.face_hat(&dst).local_tokens.contains_key(&res) {
            tracing::debug!("Already propagated");
            return;
        }

        let id = if interest.mode.is_future() {
            let id = self.face_hat(&dst).next_id.fetch_add(1, Ordering::SeqCst);
            self.face_hat_mut(&mut dst)
                .local_tokens
                .insert(res.clone(), id);
            id
        } else {
            TokenId::default()
        };

        let wire_expr = Resource::decl_key(&res, &mut dst);
        tracing::debug!(dst = %dst);
        (ctx.send_declare)(
            &dst.primitives,
            RoutingContext::with_expr(
                Declare {
                    interest_id: Some(interest.src_interest_id),
                    ext_qos: declare::ext::QoSType::DECLARE,
                    ext_tstamp: None,
                    ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                    body: DeclareBody::DeclareToken(DeclareToken { id, wire_expr }),
                },
                res.expr().to_string(),
            ),
        );
    }

    #[tracing::instrument(level = "debug", skip(ctx, dst), ret)]
    fn send_declare_final(&mut self, ctx: DispatcherContext, id: InterestId, dst: &Remote) {
        let dst_face = self.hat_remote(dst);

        tracing::debug!(dst = %dst_face);
        (ctx.send_declare)(
            &dst_face.primitives,
            RoutingContext::new(Declare {
                interest_id: Some(id),
                ext_qos: declare::ext::QoSType::DECLARE,
                ext_tstamp: None,
                ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                body: DeclareBody::DeclareFinal(DeclareFinal),
            }),
        );
    }

    #[tracing::instrument(level = "debug", skip(ctx, msg), ret)]
    fn register_interest(
        &mut self,
        ctx: DispatcherContext,
        msg: &Interest,
        res: Option<Arc<Resource>>,
    ) {
        if self
            .face_hat_mut(ctx.src_face)
            .remote_interests
            .contains_key(&msg.id)
        {
            tracing::error!("Interest ids cannot be re-used");
            return;
        }

        self.face_hat_mut(ctx.src_face).remote_interests.insert(
            msg.id,
            RemoteInterest {
                res,
                options: msg.options,
                mode: msg.mode,
            },
        );
    }

    #[tracing::instrument(level = "debug", skip(ctx, msg), fields(id = msg.id), ret)]
    fn unregister_interest(
        &mut self,
        ctx: DispatcherContext,
        msg: &Interest,
    ) -> Option<RemoteInterest> {
        debug_assert!(self.region().bound().is_south());
        debug_assert!(self.owns(ctx.src_face));

        let Some(remote_interest) = self
            .face_hat_mut(ctx.src_face)
            .remote_interests
            .remove(&msg.id)
        else {
            tracing::error!("Unknown interest");
            return None;
        };

        let aggregate = remote_interest.options.aggregate();
        // Aggregated interests without an expression never advertised a resource.
        if !aggregate || remote_interest.res.is_some() {
            let aggregate_res = aggregate.then_some(remote_interest.res.as_ref()).flatten();
            let subscribers = if remote_interest.options.subscribers() {
                self.face_hat_mut(ctx.src_face)
                    .local_subs
                    .remove_interest(msg.id, aggregate_res)
            } else {
                Vec::new()
            };
            let queryables = if remote_interest.options.queryables() {
                self.face_hat_mut(ctx.src_face)
                    .local_qabls
                    .remove_interest(msg.id, aggregate_res)
            } else {
                Vec::new()
            };
            for (id, resource) in subscribers {
                (ctx.send_declare)(
                    &ctx.src_face.primitives,
                    RoutingContext::with_expr(
                        Declare {
                            interest_id: None,
                            ext_qos: declare::ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                            body: DeclareBody::UndeclareSubscriber(UndeclareSubscriber {
                                id,
                                ext_wire_expr: WireExprType::null(),
                            }),
                        },
                        resource.expr().to_string(),
                    ),
                );
            }
            for (id, resource) in queryables {
                (ctx.send_declare)(
                    &ctx.src_face.primitives,
                    RoutingContext::with_expr(
                        Declare {
                            interest_id: None,
                            ext_qos: declare::ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                            body: DeclareBody::UndeclareQueryable(UndeclareQueryable {
                                id,
                                ext_wire_expr: WireExprType::null(),
                            }),
                        },
                        resource.expr().to_string(),
                    ),
                );
            }
        }

        self.owned_faces(ctx.tables)
            .all(|face| {
                !self
                    .face_hat(face)
                    .remote_interests
                    .values()
                    .contains(&remote_interest)
            })
            .then_some(remote_interest)
    }

    #[tracing::instrument(level = "trace", skip(tables), ret)]
    fn remote_interests(&self, tables: &TablesData) -> HashSet<RemoteInterest> {
        self.owned_faces(tables)
            .flat_map(|face| self.face_hat(face).remote_interests.values().cloned())
            .collect()
    }
}

#[cfg(test)]
mod retention_tests {
    use zenoh_config::{Config, WhatAmI};
    use zenoh_protocol::{core::Region, network::interest::InterestOptions};

    use super::{super::HatFace, *};
    use crate::net::{
        primitives::{DummyPrimitives, Primitives},
        routing::gateway::{GatewayBuilder, NodeId},
    };

    #[test]
    fn final_interest_reclaims_broker_subscriber_and_queryable_references() {
        for aggregated in [false, true] {
            let mut config = Config::default().expanded();
            config.set_mode(Some(WhatAmI::Client)).unwrap();
            let gateway = GatewayBuilder::new(&config)
                .subregions(vec![Region::Local])
                .build()
                .unwrap();
            let source = gateway.new_session(Arc::new(DummyPrimitives {}));
            let requester = gateway.new_session(Arc::new(DummyPrimitives {}));
            source.declare_subscriber(
                1,
                &"retention/topic".into(),
                &SubscriberInfo,
                NodeId::default(),
                &mut |p, m| {
                    m.with_mut(|m| p.send_declare(m));
                },
            );
            source.declare_queryable(
                2,
                &"retention/topic".into(),
                &QueryableInfoType {
                    complete: true,
                    distance: 0,
                },
                NodeId::default(),
                &mut |p, m| {
                    m.with_mut(|m| p.send_declare(m));
                },
            );
            let counts = || {
                let _guard = gateway.tables.tables.read().unwrap();
                let hat = requester.state.hats[Region::Local]
                    .downcast_ref::<HatFace>()
                    .unwrap();
                (
                    hat.local_subs.interest_count(),
                    hat.local_qabls.interest_count(),
                )
            };
            let options = InterestOptions::SUBSCRIBERS
                + InterestOptions::QUERYABLES
                + if aggregated {
                    InterestOptions::AGGREGATE
                } else {
                    InterestOptions::empty()
                };
            for round in 0..256 {
                for id in [round * 2, round * 2 + 1] {
                    requester.send_interest(&mut Interest {
                        id,
                        mode: InterestMode::CurrentFuture,
                        options,
                        wire_expr: Some("retention/topic".into()),
                        ext_qos: Default::default(),
                        ext_tstamp: None,
                        ext_nodeid: Default::default(),
                    });
                }
                assert_eq!(counts(), (2, 2), "unexpected retained interest count");
                for (id, expected) in [(round * 2, 1), (round * 2 + 1, 0)] {
                    requester.send_interest(&mut Interest {
                        id,
                        mode: InterestMode::Final,
                        options: InterestOptions::empty(),
                        wire_expr: None,
                        ext_qos: Default::default(),
                        ext_tstamp: None,
                        ext_nodeid: Default::default(),
                    });
                    assert_eq!(counts(), (expected, expected), "final interest retained or removed a live sibling; aggregated={aggregated}");
                }
            }
            requester.send_close();
            source.send_close();
        }
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn repeated_interests_do_not_leave_stale_matches() {
        use std::time::Duration;

        use tokio::time::{sleep, timeout};

        async fn open(mode: WhatAmI, endpoint: &str) -> crate::Session {
            let mut config = Config::default();
            config.set_mode(Some(mode)).unwrap();
            config.scouting.multicast.set_enabled(Some(false)).unwrap();
            config.scouting.gossip.set_enabled(Some(false)).unwrap();
            if mode == WhatAmI::Router {
                config
                    .listen
                    .endpoints
                    .set(vec![endpoint.parse().unwrap()])
                    .unwrap();
            } else {
                config.listen.endpoints.set(vec![]).unwrap();
                config
                    .connect
                    .endpoints
                    .set(vec![endpoint.parse().unwrap()])
                    .unwrap();
            }
            crate::open(config).await.unwrap()
        }
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("tcp/127.0.0.1:{}", listener.local_addr().unwrap().port());
        drop(listener);
        let router = open(WhatAmI::Router, &endpoint).await;
        let source = open(WhatAmI::Client, &endpoint).await;
        let observer = open(WhatAmI::Client, &endpoint).await;
        let subscriber = source
            .declare_subscriber("retention/matching")
            .await
            .unwrap();
        let queryable = source
            .declare_queryable("retention/matching")
            .complete(true)
            .await
            .unwrap();
        for _ in 0..32 {
            let publisher = observer
                .declare_publisher("retention/matching")
                .await
                .unwrap();
            let querier = observer
                .declare_querier("retention/matching")
                .await
                .unwrap();
            timeout(Duration::from_secs(2), async {
                while !publisher.matching_status().await.unwrap().matching()
                    || !querier.matching_status().await.unwrap().matching()
                {
                    sleep(Duration::from_millis(1)).await;
                }
            })
            .await
            .expect("live endpoints were not discovered");
            drop(querier);
            drop(publisher);
            // Allow both Final interests to reach the router before the next generation.
            sleep(Duration::from_millis(5)).await;
        }
        let publisher = observer
            .declare_publisher("retention/matching")
            .await
            .unwrap();
        let querier = observer
            .declare_querier("retention/matching")
            .await
            .unwrap();
        timeout(Duration::from_secs(2), async {
            while !publisher.matching_status().await.unwrap().matching()
                || !querier.matching_status().await.unwrap().matching()
            {
                sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .unwrap();
        drop(subscriber);
        drop(queryable);
        timeout(Duration::from_secs(2), async {
            while publisher.matching_status().await.unwrap().matching()
                || querier.matching_status().await.unwrap().matching()
            {
                sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .expect("completed interests left stale matching declarations");
        drop(publisher);
        drop(querier);
        for session in [observer, source, router] {
            timeout(Duration::from_secs(5), session.close())
                .await
                .unwrap()
                .unwrap();
        }
    }
}
