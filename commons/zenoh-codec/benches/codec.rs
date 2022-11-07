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
use zenoh_buffers::ZBuf;
use zenoh_buffers::{reader::HasReader, writer::HasWriter};
use zenoh_codec::*;
// use zenoh_buffers::reader::HasReader;
// use zenoh_protocol::core::{Channel, CongestionControl, Priority, Reliability, WireExpr};
// use zenoh_protocol::io::{WBuf, ZBuf};
use zenoh_protocol::{core::ZInt, defaults::BATCH_SIZE};
// use zenoh_protocol::proto::ZenohMessage;
// use zenoh_protocol::proto::{MessageReader, MessageWriter};

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("ZInt Vec<u8>", |b| {
        let mut buff = vec![0u8; BATCH_SIZE as usize];
        let codec = Zenoh060::default();

        b.iter(|| {
            buff.clear();
            let mut writer = (&mut buff).writer();
            codec.write(&mut writer, ZInt::MAX).unwrap();
            let mut reader = (&buff).reader();
            let _: ZInt = codec.read(&mut reader).unwrap();
        })
    });

    c.bench_function("ZInt ZBuf", |b| {
        let mut buff = ZBuf::default();
        buff.push_zslice(vec![0u8; BATCH_SIZE as usize].into());
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
