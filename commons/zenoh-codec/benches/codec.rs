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

    c.bench_function("Batch BBuf Write", |b| {
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

        b.iter(|| {
            buff.clear();
            let mut writer = buff.writer();

            codec.write(&mut writer, &frame).unwrap();
            while codec.write(&mut writer, &data).is_ok() {}
        })
    });

    c.bench_function("Batch ZSlice Read", |b| {
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

        b.iter(|| {
            let mut c = zslice.clone();
            let mut reader = c.reader();

            let _frame: Frame = codec.read(&mut reader).unwrap();
        })
    });

    c.bench_function("Batch ZSlice Read NoAlloc", |b| {
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
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
