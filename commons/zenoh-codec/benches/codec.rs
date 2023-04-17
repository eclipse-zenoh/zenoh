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

use criterion::Criterion;
use zenoh_buffers::{
    reader::{DidntRead, HasReader},
    writer::HasWriter,
    BBuf, ZBuf, ZSlice,
};
use zenoh_codec::*;
use zenoh_protocol::{
    core::{Channel, CongestionControl, ZInt},
    defaults::BATCH_SIZE,
    transport::{Frame, FrameHeader, FrameKind},
    zenoh::Data,
};

fn criterion_benchmark(c: &mut Criterion) {
    // ZInt Vec<u8>
    let mut buff = vec![];
    let codec = Zenoh060::default();
    c.bench_function("ZInt Vec<u8>", |b| {
        b.iter(|| {
            buff.clear();
            let mut writer = buff.writer();
            codec.write(&mut writer, ZInt::MAX).unwrap();
            let mut reader = buff.reader();
            let _: ZInt = codec.read(&mut reader).unwrap();
        })
    });

    // ZInt BBuf
    let mut buff = BBuf::with_capacity(BATCH_SIZE as usize);
    let codec = Zenoh060::default();
    c.bench_function("ZInt BBuf", |b| {
        b.iter(|| {
            buff.clear();
            let mut writer = buff.writer();
            codec.write(&mut writer, ZInt::MAX).unwrap();
            let mut reader = buff.reader();
            let _: ZInt = codec.read(&mut reader).unwrap();
        })
    });

    // ZInt ZBuf
    let mut buff = ZBuf::default();
    let codec = Zenoh060::default();
    c.bench_function("ZInt ZBuf", |b| {
        b.iter(|| {
            buff.clear();
            let mut writer = buff.writer();
            codec.write(&mut writer, ZInt::MAX).unwrap();
            let mut reader = buff.reader();
            let _: ZInt = codec.read(&mut reader).unwrap();
        })
    });

    // Batch BBuf Write
    let mut buff = BBuf::with_capacity(u16::MAX as usize);
    let codec = Zenoh060::default();

    let frame = FrameHeader {
        channel: Channel::default(),
        sn: ZInt::MIN,
        kind: FrameKind::Messages,
    };

    let data = Data {
        key: 0.into(),
        data_info: None,
        payload: ZBuf::from(vec![0u8; 8]),
        congestion_control: CongestionControl::default(),
        reply_context: None,
    };

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
    let codec = Zenoh060::default();

    let frame = FrameHeader {
        channel: Channel::default(),
        sn: ZInt::MIN,
        kind: FrameKind::Messages,
    };

    let data = Data {
        key: 0.into(),
        data_info: None,
        payload: ZBuf::from(vec![0u8; 8]),
        congestion_control: CongestionControl::default(),
        reply_context: None,
    };

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
    let codec = Zenoh060::default();

    let frame = FrameHeader {
        channel: Channel::default(),
        sn: ZInt::MIN,
        kind: FrameKind::Messages,
    };

    let data = Data {
        key: 0.into(),
        data_info: None,
        payload: ZBuf::from(vec![0u8; 8]),
        congestion_control: CongestionControl::default(),
        reply_context: None,
    };

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
                let res: Result<Data, DidntRead> = codec.read(&mut reader);
                if res.is_err() {
                    break;
                }
            }
        })
    });

    // Fragmentation ZBuf Write
    let mut buff = ZBuf::default();
    let codec = Zenoh060::default();

    let data = Data {
        key: 0.into(),
        data_info: None,
        payload: ZBuf::from(vec![0u8; 1_000_000]),
        congestion_control: CongestionControl::default(),
        reply_context: None,
    };
    c.bench_function("Fragmentation ZBuf Write", |b| {
        b.iter(|| {
            let mut writer = buff.writer();
            codec.write(&mut writer, &data).unwrap();
        })
    });

    // Fragmentation ZBuf Read
    let mut buff = vec![];
    let codec = Zenoh060::default();

    let data = Data {
        key: 0.into(),
        data_info: None,
        payload: ZBuf::from(vec![0u8; 1_000_000]),
        congestion_control: CongestionControl::default(),
        reply_context: None,
    };

    let mut writer = buff.writer();
    codec.write(&mut writer, &data).unwrap();

    let mut zbuf = ZBuf::default();
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
            let _data: Data = codec.read(&mut reader).unwrap();
        })
    });

    // Fragmentation ZSlice ZBuf Read
    let mut buff = vec![];
    let codec = Zenoh060::default();

    let data = Data {
        key: 0.into(),
        data_info: None,
        payload: ZBuf::from(vec![0u8; 1_000_000]),
        congestion_control: CongestionControl::default(),
        reply_context: None,
    };

    let mut writer = buff.writer();
    codec.write(&mut writer, &data).unwrap();

    let zslice: ZSlice = buff.into();

    c.bench_function("Fragmentation ZSlice ZBuf Read", |b| {
        b.iter(|| {
            let mut zbuf = ZBuf::default();
            let chunk = u16::MAX as usize;
            let mut idx = 0;
            while idx < zslice.len() {
                let len = (zslice.len() - idx).min(chunk);
                zbuf.push_zslice(ZSlice::make(zslice.buf.clone(), idx, idx + len).unwrap());
                idx += len;
            }

            let mut reader = zbuf.reader();
            let _data: Data = codec.read(&mut reader).unwrap();
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
