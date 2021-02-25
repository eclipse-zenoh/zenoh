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
#[macro_use]
extern crate criterion;
use async_std::sync::Arc;
use async_std::task;
use async_trait::async_trait;
use criterion::{BenchmarkId, Criterion};
use zenoh::net::protocol::core::{
    whatami, CongestionControl, PeerId, QueryConsolidation, QueryTarget, Reliability, ResKey,
    SubInfo, SubMode, ZInt,
};
use zenoh::net::protocol::io::RBuf;
use zenoh::net::protocol::proto::{DataInfo, RoutingContext};
use zenoh::net::protocol::session::Primitives;
use zenoh::net::routing::pubsub::*;
use zenoh::net::routing::resource::*;
use zenoh::net::routing::router::Tables;
use zenoh::net::routing::OutSession;

struct DummyPrimitives {}

#[async_trait]
impl Primitives for DummyPrimitives {
    async fn decl_resource(&self, _rid: ZInt, _reskey: &ResKey) {}
    async fn forget_resource(&self, _rid: ZInt) {}

    async fn decl_publisher(&self, _reskey: &ResKey, _routing_context: Option<RoutingContext>) {}
    async fn forget_publisher(&self, _reskey: &ResKey, _routing_context: Option<RoutingContext>) {}

    async fn decl_subscriber(
        &self,
        _reskey: &ResKey,
        _sub_info: &SubInfo,
        _routing_context: Option<RoutingContext>,
    ) {
    }
    async fn forget_subscriber(&self, _reskey: &ResKey, _routing_context: Option<RoutingContext>) {}

    async fn decl_queryable(&self, _reskey: &ResKey, _routing_context: Option<RoutingContext>) {}
    async fn forget_queryable(&self, _reskey: &ResKey, _routing_context: Option<RoutingContext>) {}

    async fn send_data(
        &self,
        _reskey: &ResKey,
        _payload: RBuf,
        _reliability: Reliability,
        _congestion_control: CongestionControl,
        _info: Option<DataInfo>,
        _routing_context: Option<RoutingContext>,
    ) {
    }
    async fn send_query(
        &self,
        _reskey: &ResKey,
        _predicate: &str,
        _qid: ZInt,
        _target: QueryTarget,
        _consolidation: QueryConsolidation,
        _routing_context: Option<RoutingContext>,
    ) {
    }
    async fn send_reply_data(
        &self,
        _qid: ZInt,
        _source_kind: ZInt,
        _replier_id: PeerId,
        _reskey: ResKey,
        _info: Option<DataInfo>,
        _payload: RBuf,
    ) {
    }
    async fn send_reply_final(&self, _qid: ZInt) {}
    async fn send_pull(
        &self,
        _is_final: bool,
        _reskey: &ResKey,
        _pull_id: ZInt,
        _max_samples: &Option<ZInt>,
    ) {
    }

    async fn send_close(&self) {}
}

fn tables_bench(c: &mut Criterion) {
    task::block_on(async {
        let mut tables = Tables::new(PeerId::new(0, [0; 16]), whatami::ROUTER, None);
        let primitives = Arc::new(DummyPrimitives {});

        let face0 = tables
            .open_face(
                PeerId::new(0, [0; 16]),
                whatami::CLIENT,
                OutSession::Primitives(primitives.clone()),
            )
            .await;
        declare_resource(
            &mut tables,
            &mut face0.upgrade().unwrap(),
            1,
            0,
            "/bench/tables",
        )
        .await;
        declare_resource(
            &mut tables,
            &mut face0.upgrade().unwrap(),
            2,
            0,
            "/bench/tables/*",
        )
        .await;

        let face1 = tables
            .open_face(
                PeerId::new(0, [0; 16]),
                whatami::CLIENT,
                OutSession::Primitives(primitives.clone()),
            )
            .await;

        let mut tables_bench = c.benchmark_group("tables_bench");
        let sub_info = SubInfo {
            reliability: Reliability::Reliable,
            mode: SubMode::Push,
            period: None,
        };

        for p in [8, 32, 256, 1024, 8192].iter() {
            for i in 1..(*p) {
                declare_resource(
                    &mut tables,
                    &mut face1.upgrade().unwrap(),
                    i,
                    0,
                    &["/bench/tables/AA", &i.to_string()].concat(),
                )
                .await;
                declare_client_subscription(
                    &mut tables,
                    &mut face1.upgrade().unwrap(),
                    i,
                    "",
                    &sub_info,
                )
                .await;
            }

            let face0 = face0.upgrade().unwrap();
            let payload = RBuf::new();

            tables_bench.bench_function(BenchmarkId::new("direct_route", p), |b| {
                b.iter(|| {
                    task::block_on(async {
                        route_data(
                            &mut tables,
                            &face0,
                            2 as u64,
                            "",
                            CongestionControl::Drop,
                            None,
                            payload.clone(),
                            None,
                        )
                        .await;
                    })
                })
            });

            tables_bench.bench_function(BenchmarkId::new("known_resource", p), |b| {
                b.iter(|| {
                    task::block_on(async {
                        route_data(
                            &mut tables,
                            &face0,
                            0 as u64,
                            "/bench/tables/*",
                            CongestionControl::Drop,
                            None,
                            payload.clone(),
                            None,
                        )
                        .await;
                    })
                })
            });

            tables_bench.bench_function(BenchmarkId::new("matches_lookup", p), |b| {
                b.iter(|| {
                    task::block_on(async {
                        route_data(
                            &mut tables,
                            &face0,
                            0 as u64,
                            "/bench/tables/A*",
                            CongestionControl::Drop,
                            None,
                            payload.clone(),
                            None,
                        )
                        .await;
                    })
                })
            });
        }
        tables_bench.finish();
    });
}

criterion_group!(benches, tables_bench);
criterion_main!(benches);
