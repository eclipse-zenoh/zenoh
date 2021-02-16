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

use zenoh::net::protocol::core::{Channel, CongestionControl, Reliability, ResKey, ZInt};
use zenoh::net::protocol::io::{RBuf, WBuf};
use zenoh::net::protocol::proto::{Attachment, FramePayload, SessionMessage, ZenohMessage};

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
    RBuf::from(&*buf).read_zint()
}

fn bench_two_zint_codec((v, buf): (&[ZInt; 2], &mut WBuf)) -> Option<ZInt> {
    buf.write_zint(v[0]);
    buf.write_zint(v[1]);
    let mut rbuf = RBuf::from(&*buf);
    let _ = rbuf.read_zint()?;
    rbuf.read_zint()
}

fn bench_three_zint_codec((v, buf): (&[ZInt; 3], &mut WBuf)) -> Option<ZInt> {
    buf.write_zint(v[0]);
    buf.write_zint(v[1]);
    buf.write_zint(v[2]);
    let mut rbuf = RBuf::from(&*buf);
    let _ = rbuf.read_zint()?;
    let _ = rbuf.read_zint()?;
    rbuf.read_zint()
}

fn bench_make_data(payload: RBuf) {
    let _ = ZenohMessage::make_data(
        ResKey::RId(10),
        payload,
        Reliability::Reliable,
        CongestionControl::Block,
        None,
        None,
        None,
        None,
    );
}

fn bench_write_data(buf: &mut WBuf, data: &ZenohMessage) {
    buf.write_zenoh_message(data);
}

fn bench_make_frame_header(ch: Channel, is_fragment: Option<bool>) {
    let _ = SessionMessage::make_frame_header(ch, is_fragment);
}

fn bench_write_frame_header(
    buf: &mut WBuf,
    ch: Channel,
    sn: ZInt,
    is_fragment: Option<bool>,
    attachment: Option<Attachment>,
) {
    buf.write_frame_header(ch, sn, is_fragment, attachment);
}

fn bench_make_frame_data(payload: FramePayload) {
    let _ = SessionMessage::make_frame(Channel::BestEffort, 42, payload, None);
}

fn bench_write_frame_data(buf: &mut WBuf, data: &SessionMessage) {
    buf.write_session_message(data);
}

fn bench_make_frame_frag(payload: FramePayload) {
    let _ = SessionMessage::make_frame(Channel::BestEffort, 42, payload, None);
}

fn bench_write_frame_frag(buf: &mut WBuf, data: &SessionMessage) {
    buf.write_session_message(data);
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
    let payload = RBuf::from(vec![0u8, 32]);
    let data = ZenohMessage::make_data(
        ResKey::RId(10),
        payload.clone(),
        Reliability::BestEffort,
        CongestionControl::Drop,
        None,
        None,
        None,
        None,
    );

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

    c.bench_function("bench_make_data", |b| {
        b.iter(|| {
            let _ = bench_make_data(payload.clone());
            buf.clear();
        })
    });

    c.bench_function("bench_write_data", |b| {
        b.iter(|| {
            let _ = bench_write_data(&mut buf, &data);
            buf.clear();
        })
    });

    // Frame benchmark
    let ch = Channel::Reliable;
    let is_fragment = Some(true);
    c.bench_function("bench_make_frame_header", |b| {
        b.iter(|| {
            let _ = bench_make_frame_header(ch, is_fragment);
            buf.clear();
        })
    });

    c.bench_function("bench_write_frame_header", |b| {
        b.iter(|| {
            let _ = bench_write_frame_header(&mut buf, ch, 42, is_fragment, None);
            buf.clear();
        })
    });

    c.bench_function("bench_make_frame_data", |b| {
        b.iter(|| {
            let frame_data_payload = FramePayload::Messages {
                messages: vec![data.clone(); 1],
            };
            let _ = bench_make_frame_data(frame_data_payload.clone());
        })
    });

    let frame_data_payload = FramePayload::Messages {
        messages: vec![data; 1],
    };
    let frame_data =
        SessionMessage::make_frame(Channel::BestEffort, 42, frame_data_payload.clone(), None);
    c.bench_function("bench_write_frame_data", |b| {
        b.iter(|| {
            let _ = bench_write_frame_data(&mut buf, &frame_data);
            buf.clear();
        })
    });

    c.bench_function("bench_make_frame_frag", |b| {
        b.iter(|| {
            let frame_frag_payload = FramePayload::Fragment {
                buffer: payload.clone(),
                is_final: false,
            };
            let _ = bench_make_frame_frag(frame_frag_payload);
        })
    });

    let frame_frag_payload = FramePayload::Fragment {
        buffer: payload.clone(),
        is_final: false,
    };
    let frame_frag = SessionMessage::make_frame(Channel::BestEffort, 42, frame_frag_payload, None);
    c.bench_function("bench_write_frame_frag", |b| {
        b.iter(|| {
            let _ = bench_write_frame_frag(&mut buf, &frame_frag);
            buf.clear();
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
