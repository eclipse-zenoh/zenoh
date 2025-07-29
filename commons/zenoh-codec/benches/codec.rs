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
#[macro_use]
extern crate criterion;

use std::sync::Arc;

use criterion::Criterion;
use zenoh_buffers::{
    reader::{DidntRead, HasReader},
    writer::HasWriter,
    BBuf, ZBuf, ZSlice,
};
use zenoh_codec::*;
use zenoh_protocol::{
    core::Reliability,
    network::Push,
    transport::{BatchSize, Frame, FrameHeader, TransportSn},
    zenoh::Put,
};

fn criterion_benchmark(c: &mut Criterion) {
    // u64 Vec<u8>
    let mut buff = vec![];
    let codec = Zenoh080::new();
    c.bench_function("u64 Vec<u8>", |b| {
        b.iter(|| {
            buff.clear();
            let mut writer = buff.writer();
            codec.write(&mut writer, u64::MAX).unwrap();
            let mut reader = buff.reader();
            let _: u64 = codec.read(&mut reader).unwrap();
        })
    });

    // u64 BBuf
    let mut buff = BBuf::with_capacity(BatchSize::MAX as usize);
    let codec = Zenoh080::new();
    c.bench_function("u64 BBuf", |b| {
        b.iter(|| {
            buff.clear();
            let mut writer = buff.writer();
            codec.write(&mut writer, u64::MAX).unwrap();
            let mut reader = buff.reader();
            let _: u64 = codec.read(&mut reader).unwrap();
        })
    });

    // u64 ZBuf
    let mut buff = ZBuf::empty();
    let codec = Zenoh080::new();
    c.bench_function("u64 ZBuf", |b| {
        b.iter(|| {
            buff.clear();
            let mut writer = buff.writer();
            codec.write(&mut writer, u64::MAX).unwrap();
            let mut reader = buff.reader();
            let _: u64 = codec.read(&mut reader).unwrap();
        })
    });

    // Batch BBuf Write
    let mut buff = BBuf::with_capacity(u16::MAX as usize);
    let codec = Zenoh080::new();

    let frame = FrameHeader {
        reliability: Reliability::DEFAULT,
        sn: TransportSn::MIN,
        ext_qos: zenoh_protocol::transport::frame::ext::QoSType::DEFAULT,
    };

    let data = Push::from(Put {
        payload: ZBuf::from(vec![0u8; 8]),
        ..Put::default()
    });

    // Calculate the number of messages
    // let mut writer = buff.writer();
    // codec.write(&mut writer, &frame).unwrap();
    // let mut count = 0;
    // while codec.write(&mut writer, &data).is_ok() {
    //     count += 1;
    // }
    // println!(">>> Batch with {} msgs", count);
    // // 2022-11-16: a batch contains 5967 messages
    c.bench_function("Batch BBuf Write", |b| {
        b.iter(|| {
            buff.clear();
            let mut writer = buff.writer();

            codec.write(&mut writer, &frame).unwrap();
            while codec.write(&mut writer, &data).is_ok() {}
        })
    });

    // Batch ZSlice Read NoAlloc
    let mut buff = BBuf::with_capacity(u16::MAX as usize);
    let codec = Zenoh080::new();

    let frame = FrameHeader {
        reliability: Reliability::DEFAULT,
        sn: TransportSn::MIN,
        ext_qos: zenoh_protocol::transport::frame::ext::QoSType::DEFAULT,
    };

    let data = Push::from(Put {
        payload: ZBuf::from(vec![0u8; 8]),
        ..Put::default()
    });

    let mut writer = buff.writer();
    codec.write(&mut writer, &frame).unwrap();
    while codec.write(&mut writer, &data).is_ok() {}

    let zslice = ZSlice::from(buff.as_slice().to_vec());
    c.bench_function("Batch ZSlice Read", |b| {
        b.iter(|| {
            let mut c = zslice.clone();
            let mut reader = c.reader();

            let _frame: Frame = codec.read(&mut reader).unwrap();
        })
    });

    // Batch ZSlice Read NoAlloc
    let mut buff = BBuf::with_capacity(u16::MAX as usize);
    let codec = Zenoh080::new();

    let frame = FrameHeader {
        reliability: Reliability::DEFAULT,
        sn: TransportSn::MIN,
        ext_qos: zenoh_protocol::transport::frame::ext::QoSType::DEFAULT,
    };

    let data = Push::from(Put {
        payload: ZBuf::from(vec![0u8; 8]),
        ..Put::default()
    });

    let mut writer = buff.writer();
    codec.write(&mut writer, &frame).unwrap();
    while codec.write(&mut writer, &data).is_ok() {}

    let zslice = ZSlice::from(buff.as_slice().to_vec());
    c.bench_function("Batch ZSlice Read NoAlloc", |b| {
        b.iter(|| {
            let mut c = zslice.clone();
            let mut reader = c.reader();

            let _header: FrameHeader = codec.read(&mut reader).unwrap();
            loop {
                let res: Result<Push, DidntRead> = codec.read(&mut reader);
                if res.is_err() {
                    break;
                }
            }
        })
    });

    // Fragmentation ZBuf Write
    let mut buff = ZBuf::empty();
    let codec = Zenoh080::new();

    let data = Push::from(Put {
        payload: ZBuf::from(vec![0u8; 1_000_000]),
        ..Put::default()
    });

    c.bench_function("Fragmentation ZBuf Write", |b| {
        b.iter(|| {
            let mut writer = buff.writer();
            codec.write(&mut writer, &data).unwrap();
        })
    });

    // Fragmentation ZBuf Read
    let mut buff = vec![];
    let codec = Zenoh080::new();

    let data = Push::from(Put {
        payload: ZBuf::from(vec![0u8; 1_000_000]),
        ..Put::default()
    });

    let mut writer = buff.writer();
    codec.write(&mut writer, &data).unwrap();

    let mut zbuf = ZBuf::empty();
    let chunk = u16::MAX as usize;
    let mut idx = 0;
    while idx < buff.len() {
        let len = (buff.len() - idx).min(chunk);
        zbuf.push_zslice(buff[idx..idx + len].to_vec().into());
        idx += len;
    }
    c.bench_function("Fragmentation ZBuf Read", |b| {
        b.iter(|| {
            let mut reader = zbuf.reader();
            let _data: Push = codec.read(&mut reader).unwrap();
        })
    });

    // Fragmentation ZSlice ZBuf Read
    let mut buff = vec![];
    let codec = Zenoh080::new();

    let data = Push::from(Put {
        payload: ZBuf::from(vec![0u8; 1_000_000]),
        ..Put::default()
    });

    let mut writer = buff.writer();
    codec.write(&mut writer, &data).unwrap();

    let buff = Arc::new(buff);
    let zslice: ZSlice = buff.clone().into();

    c.bench_function("Fragmentation ZSlice ZBuf Read", |b| {
        b.iter(|| {
            let mut zbuf = ZBuf::empty();
            let chunk = u16::MAX as usize;
            let mut idx = 0;
            while idx < zslice.len() {
                let len = (zslice.len() - idx).min(chunk);
                zbuf.push_zslice(ZSlice::new(buff.clone(), idx, idx + len).unwrap());
                idx += len;
            }

            let mut reader = zbuf.reader();
            let _data: Push = codec.read(&mut reader).unwrap();
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
