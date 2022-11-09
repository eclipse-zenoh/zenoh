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

use criterion::Criterion;
use zenoh_buffers::{reader::HasReader, writer::HasWriter, BBuf, ZBuf};
use zenoh_codec::*;
use zenoh_protocol::{core::ZInt, defaults::BATCH_SIZE};

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("ZInt Vec<u8>", |b| {
        let mut buff = Vec::with_capacity(BATCH_SIZE as usize);
        let codec = Zenoh060::default();

        b.iter(|| {
            buff.clear();
            let mut writer = buff.writer();
            codec.write(&mut writer, ZInt::MAX).unwrap();
            let mut reader = buff.reader();
            let _: ZInt = codec.read(&mut reader).unwrap();
        })
    });

    c.bench_function("ZInt BBuf", |b| {
        let mut buff = BBuf::with_capacity(BATCH_SIZE as usize);
        let codec = Zenoh060::default();

        b.iter(|| {
            buff.clear();
            let mut writer = buff.writer();
            codec.write(&mut writer, ZInt::MAX).unwrap();
            let mut reader = buff.reader();
            let _: ZInt = codec.read(&mut reader).unwrap();
        })
    });

    c.bench_function("ZInt ZBuf", |b| {
        let mut buff = ZBuf::default();
        let codec = Zenoh060::default();

        b.iter(|| {
            buff.clear();
            let mut writer = buff.writer();
            codec.write(&mut writer, ZInt::MAX).unwrap();
            let mut reader = buff.reader();
            let _: ZInt = codec.read(&mut reader).unwrap();
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
