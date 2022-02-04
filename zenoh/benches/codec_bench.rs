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
use zenoh::net::protocol::io::{WBuf, ZBuf};

fn _bench_zint_write((v, buf): (ZInt, &mut WBuf)) {
    buf.write_zint(v);
}

fn _bench_zint_write_two((v, buf): (&[ZInt; 2], &mut WBuf)) {
    buf.write_zint(v[0]);
    buf.write_zint(v[1]);
}

fn _bench_zint_write_three((v, buf): (&[ZInt; 3], &mut WBuf)) {
    buf.write_zint(v[0]);
    buf.write_zint(v[1]);
    buf.write_zint(v[2]);
}

fn bench_one_zint_codec((v, buf): (ZInt, &mut WBuf)) -> Option<ZInt> {
    buf.write_zint(v);
    ZBuf::from(buf.clone()).read_zint()
}

fn bench_two_zint_codec((v, buf): (&[ZInt; 2], &mut WBuf)) -> Option<ZInt> {
    buf.write_zint(v[0]);
    buf.write_zint(v[1]);
    let mut zbuf = ZBuf::from(buf.clone());
    let _ = zbuf.read_zint()?;
    zbuf.read_zint()
}

fn bench_three_zint_codec((v, buf): (&[ZInt; 3], &mut WBuf)) -> Option<ZInt> {
    buf.write_zint(v[0]);
    buf.write_zint(v[1]);
    buf.write_zint(v[2]);
    let mut zbuf = ZBuf::from(buf.clone());
    let _ = zbuf.read_zint()?;
    let _ = zbuf.read_zint()?;
    zbuf.read_zint()
}

fn bench_write_10bytes1((v, buf): (u8, &mut WBuf)) {
    buf.write(v);
    buf.write(v);
    buf.write(v);
    buf.write(v);
    buf.write(v);
    buf.write(v);
    buf.write(v);
    buf.write(v);
    buf.write(v);
    buf.write(v);
}

fn criterion_benchmark(c: &mut Criterion) {
    let mut buf = WBuf::new(64, true);
    let rs3: [ZInt; 3] = [
        ZInt::from(rand::random::<u8>()),
        ZInt::from(rand::random::<u8>()),
        ZInt::from(rand::random::<u8>()),
    ];
    let rs2: [ZInt; 2] = [
        ZInt::from(rand::random::<u8>()),
        ZInt::from(rand::random::<u8>()),
    ];
    let _ns: [ZInt; 4] = [0; 4];
    let len = String::from("u8");

    c.bench_function(&format!("bench_one_zint_codec {}", len), |b| {
        b.iter(|| {
            let _ = bench_one_zint_codec(black_box((rs3[0], &mut buf)));
            buf.clear();
        })
    });

    c.bench_function(&format!("bench_two_zint_codec {}", len), |b| {
        b.iter(|| {
            let _ = bench_two_zint_codec(black_box((&rs2, &mut buf)));
            buf.clear();
        })
    });

    c.bench_function("bench_three_zint_codec u8", |b| {
        b.iter(|| {
            let _ = bench_three_zint_codec(black_box((&rs3, &mut buf)));
            buf.clear();
        })
    });

    let r4 = rand::random::<u8>();
    c.bench_function("bench_write_10bytes1", |b| {
        b.iter(|| {
            let _ = bench_write_10bytes1(black_box((r4, &mut buf)));
            buf.clear();
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
