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
extern crate rand;

use criterion::{black_box, Criterion};

use zenoh::net::protocol::core::{
    Channel, CongestionControl, Priority, Reliability, WireExpr, ZInt,
};
use zenoh::net::protocol::io::{WBuf, ZBuf, ZSlice};
use zenoh::net::protocol::proto::{
    Attachment, Frame, FramePayload, TransportMessage, ZenohMessage,
};
use zenoh_buffers::buffer::CopyBuffer;
use zenoh_buffers::reader::HasReader;
use zenoh_protocol::io::{WBufCodec, ZBufCodec};
use zenoh_protocol::proto::MessageWriter;

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
    ZBuf::from(buf.clone()).reader().read_zint()
}

fn bench_two_zint_codec((v, buf): (&[ZInt; 2], &mut WBuf)) -> Option<ZInt> {
    buf.write_zint(v[0]);
    buf.write_zint(v[1]);
    let zbuf = ZBuf::from(buf.clone());
    let mut zbuf = zbuf.reader();
    let _ = zbuf.read_zint()?;
    zbuf.read_zint()
}

fn bench_three_zint_codec((v, buf): (&[ZInt; 3], &mut WBuf)) -> Option<ZInt> {
    buf.write_zint(v[0]);
    buf.write_zint(v[1]);
    buf.write_zint(v[2]);
    let zbuf = ZBuf::from(buf.clone());
    let mut zbuf = zbuf.reader();
    let _ = zbuf.read_zint()?;
    let _ = zbuf.read_zint()?;
    zbuf.read_zint()
}

fn bench_make_data(payload: ZBuf) {
    let _ = ZenohMessage::make_data(
        WireExpr::from(10),
        payload,
        Channel::default(),
        CongestionControl::default(),
        None,
        None,
        None,
        None,
    );
}

fn bench_write_data(buf: &mut WBuf, data: &mut ZenohMessage) {
    buf.write_zenoh_message(data);
}

fn bench_make_frame_header(reliability: Reliability, is_fragment: Option<bool>) {
    let _ = Frame::make_header(reliability, is_fragment);
}

fn bench_write_frame_header(
    buf: &mut WBuf,
    priority: Priority,
    reliability: Reliability,
    sn: ZInt,
    is_fragment: Option<bool>,
    attachment: Option<Attachment>,
) {
    buf.write_frame_header(priority, reliability, sn, is_fragment, attachment);
}

fn bench_make_frame_data(payload: FramePayload) {
    let _ = TransportMessage::make_frame(Channel::default(), 42, payload, None);
}

fn bench_write_frame_data(buf: &mut WBuf, data: &mut TransportMessage) {
    buf.write_transport_message(data);
}

fn bench_make_frame_frag(payload: FramePayload) {
    let _ = TransportMessage::make_frame(Channel::default(), 42, payload, None);
}

fn bench_write_frame_frag(buf: &mut WBuf, data: &mut TransportMessage) {
    buf.write_transport_message(data);
}

fn bench_write_10bytes1((v, buf): (u8, &mut WBuf)) {
    buf.write_byte(v);
    buf.write_byte(v);
    buf.write_byte(v);
    buf.write_byte(v);
    buf.write_byte(v);
    buf.write_byte(v);
    buf.write_byte(v);
    buf.write_byte(v);
    buf.write_byte(v);
    buf.write_byte(v);
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
    let bytes = vec![0_u8; 32];
    let payload: ZBuf = bytes.clone().into();
    let fragment: ZSlice = bytes.into();
    let mut data = ZenohMessage::make_data(
        WireExpr::from(10),
        payload.clone(),
        Channel::default(),
        CongestionControl::default(),
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
            let _ = bench_write_data(&mut buf, &mut data);
            buf.clear();
        })
    });

    // Frame benchmark
    let conduit = Priority::default();
    let reliability = Reliability::Reliable;
    let is_fragment = Some(true);
    c.bench_function("bench_make_frame_header", |b| {
        b.iter(|| {
            let _ = bench_make_frame_header(reliability, is_fragment);
            buf.clear();
        })
    });

    c.bench_function("bench_write_frame_header", |b| {
        b.iter(|| {
            let _ = bench_write_frame_header(&mut buf, conduit, reliability, 42, is_fragment, None);
            buf.clear();
        })
    });

    c.bench_function("bench_make_frame_data", |b| {
        b.iter(|| {
            let frame_data_payload = FramePayload::Messages {
                messages: vec![data.clone(); 1],
            };
            let _ = bench_make_frame_data(frame_data_payload);
        })
    });

    let frame_data_payload = FramePayload::Messages {
        messages: vec![data; 1],
    };
    let mut frame_data =
        TransportMessage::make_frame(Channel::default(), 42, frame_data_payload, None);
    c.bench_function("bench_write_frame_data", |b| {
        b.iter(|| {
            let _ = bench_write_frame_data(&mut buf, &mut frame_data);
            buf.clear();
        })
    });

    c.bench_function("bench_make_frame_frag", |b| {
        b.iter(|| {
            let frame_frag_payload = FramePayload::Fragment {
                buffer: fragment.clone(),
                is_final: false,
            };
            let _ = bench_make_frame_frag(frame_frag_payload);
        })
    });

    let frame_frag_payload = FramePayload::Fragment {
        buffer: fragment,
        is_final: false,
    };
    let mut frame_frag =
        TransportMessage::make_frame(Channel::default(), 42, frame_frag_payload, None);
    c.bench_function("bench_write_frame_frag", |b| {
        b.iter(|| {
            let _ = bench_write_frame_frag(&mut buf, &mut frame_frag);
            buf.clear();
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
