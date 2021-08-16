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
use criterion::{BenchmarkId, Criterion};
use zenoh::net::protocol::core::{
    whatami, Channel, CongestionControl, PeerId, Reliability, SubInfo, SubMode,
};
use zenoh::net::protocol::io::ZBuf;
use zenoh::net::protocol::session::DummyPrimitives;
use zenoh::net::routing::pubsub::*;
use zenoh::net::routing::resource::*;
use zenoh::net::routing::router::Tables;

fn tables_bench(c: &mut Criterion) {
    let mut tables = Tables::new(PeerId::new(0, [0; 16]), whatami::ROUTER, None);
    let primitives = Arc::new(DummyPrimitives {});

    let face0 = tables.open_face(PeerId::new(0, [0; 16]), whatami::CLIENT, primitives.clone());
    declare_resource(
        &mut tables,
        &mut face0.upgrade().unwrap(),
        1,
        0,
        "/bench/tables",
    );
    declare_resource(
        &mut tables,
        &mut face0.upgrade().unwrap(),
        2,
        0,
        "/bench/tables/*",
    );

    let face1 = tables.open_face(PeerId::new(0, [0; 16]), whatami::CLIENT, primitives.clone());

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
            );
            declare_client_subscription(
                &mut tables,
                &mut face1.upgrade().unwrap(),
                i,
                "",
                &sub_info,
            );
        }

        let face0 = face0.upgrade().unwrap();
        let payload = ZBuf::new();

        tables_bench.bench_function(BenchmarkId::new("direct_route", p), |b| {
            b.iter(|| {
                route_data(
                    &tables,
                    &face0,
                    2 as u64,
                    "",
                    Channel::default(),
                    CongestionControl::default(),
                    None,
                    payload.clone(),
                    None,
                );
            })
        });

        tables_bench.bench_function(BenchmarkId::new("known_resource", p), |b| {
            b.iter(|| {
                route_data(
                    &tables,
                    &face0,
                    0 as u64,
                    "/bench/tables/*",
                    Channel::default(),
                    CongestionControl::default(),
                    None,
                    payload.clone(),
                    None,
                );
            })
        });

        tables_bench.bench_function(BenchmarkId::new("matches_lookup", p), |b| {
            b.iter(|| {
                route_data(
                    &tables,
                    &face0,
                    0 as u64,
                    "/bench/tables/A*",
                    Channel::default(),
                    CongestionControl::default(),
                    None,
                    payload.clone(),
                    None,
                );
            })
        });
    }
    tables_bench.finish();
}

criterion_group!(benches, tables_bench);
criterion_main!(benches);
