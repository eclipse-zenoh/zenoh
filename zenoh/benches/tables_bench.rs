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
#[macro_use]
extern crate criterion;
use std::time::Duration;

use criterion::{BenchmarkId, Criterion};
use std::sync::Arc;
use zenoh::net::protocol::core::{
    Channel, CongestionControl, Reliability, SubInfo, SubMode, WhatAmI, ZenohId,
};
use zenoh::net::protocol::io::ZBuf;
use zenoh::net::routing::pubsub::*;
use zenoh::net::routing::resource::*;
use zenoh::net::routing::router::Tables;
use zenoh::net::transport::DummyPrimitives;
use zenoh_cfg_properties::config::ZN_QUERIES_DEFAULT_TIMEOUT_DEFAULT;

fn tables_bench(c: &mut Criterion) {
    let mut tables = Tables::new(
        ZenohId::new(0, [0; 16]),
        WhatAmI::Router,
        None,
        Duration::from_millis(ZN_QUERIES_DEFAULT_TIMEOUT_DEFAULT.parse().unwrap()),
    );
    let primitives = Arc::new(DummyPrimitives {});

    let face0 = tables.open_face(
        ZenohId::new(0, [0; 16]),
        WhatAmI::Client,
        primitives.clone(),
    );
    register_expr(
        &mut tables,
        &mut face0.upgrade().unwrap(),
        1,
        &"bench/tables".into(),
    );
    register_expr(
        &mut tables,
        &mut face0.upgrade().unwrap(),
        2,
        &"bench/tables/*".into(),
    );

    let face1 = tables.open_face(ZenohId::new(0, [0; 16]), WhatAmI::Client, primitives);

    let mut tables_bench = c.benchmark_group("tables_bench");
    let sub_info = SubInfo {
        reliability: Reliability::Reliable,
        mode: SubMode::Push,
        period: None,
    };

    for p in [8, 32, 256, 1024, 8192].iter() {
        for i in 1..(*p) {
            register_expr(
                &mut tables,
                &mut face1.upgrade().unwrap(),
                i,
                &["bench/tables/AA", &i.to_string()].concat().into(),
            );
            declare_client_subscription(
                &mut tables,
                &mut face1.upgrade().unwrap(),
                &i.into(),
                &sub_info,
            );
        }

        let face0 = face0.upgrade().unwrap();
        let payload = ZBuf::default();

        tables_bench.bench_function(BenchmarkId::new("direct_route", p), |b| {
            b.iter(|| {
                route_data(
                    &tables,
                    &face0,
                    &2.into(),
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
                    &"bench/tables/*".into(),
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
                    &"bench/tables/A*".into(),
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
