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
use criterion::{BenchmarkId, Criterion};
use zenoh::net::protocol::core::{
    whatami, CongestionControl, PeerId, Reliability, SubInfo, SubMode,
};
use zenoh::net::protocol::io::RBuf;
use zenoh::net::protocol::session::DummyPrimitives;
use zenoh::net::routing::pubsub::*;
use zenoh::net::routing::resource::*;
use zenoh::net::routing::router::Tables;
use zenoh::net::routing::OutSession;

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
                            &tables,
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
                            &tables,
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
                            &tables,
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
