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
extern crate rand;

use criterion::{black_box, Criterion};

use zenoh::net::protocol::core::ZInt;
use zenoh::net::protocol::io::WBuf;

fn bench_foo((v, buf): (ZInt, &mut WBuf)) {
    buf.write_zint(v);
}

fn criterion_benchmark(c: &mut Criterion) {
    let mut buf = WBuf::new(64, true);
    let rs3: [ZInt; 3] = [
        ZInt::from(rand::random::<u8>()),
        ZInt::from(rand::random::<u8>()),
        ZInt::from(rand::random::<u8>()),
    ];
    let _rs2: [ZInt; 2] = [
        ZInt::from(rand::random::<u8>()),
        ZInt::from(rand::random::<u8>()),
    ];
    let _ns: [ZInt; 4] = [0; 4];
    let _len = String::from("u8");

    c.bench_function("bench_foo u8", |b| {
        b.iter(|| {
            let _ = bench_foo(black_box((rs3[0], &mut buf)));
            buf.clear();
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
