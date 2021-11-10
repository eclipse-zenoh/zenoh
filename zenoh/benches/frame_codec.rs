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

use criterion::Criterion;
use zenoh::net::protocol::core::{Channel, CongestionControl, KeyExpr, Priority, Reliability};
use zenoh::net::protocol::io::{WBuf, ZBuf};
use zenoh::net::protocol::proto::defaults::BATCH_SIZE;
use zenoh::net::protocol::proto::ZenohMessage;

fn criterion_benchmark(c: &mut Criterion) {
    let batch_size = BATCH_SIZE as usize;
    let mut current = 8;
    let mut pld = vec![];
    while current < batch_size {
        pld.push(current);
        current *= 2;
    }

    let key_expr_set = [
        KeyExpr::from(1),
        KeyExpr::from("/frame/bench"),
        KeyExpr::from(1).with_suffix("/frame/bench"),
    ];

    for p in &pld {
        for r in &key_expr_set {
            let key_expr = r.clone();
            let payload = ZBuf::from(vec![0; *p]);
            let channel = Channel {
                priority: Priority::default(),
                reliability: Reliability::Reliable,
            };
            let congestion_control = CongestionControl::default();
            let info = None;

            let mut msg = ZenohMessage::make_data(
                key_expr,
                payload,
                channel,
                congestion_control,
                info,
                None,
                None,
                None,
            );

            let mut wbuf = WBuf::new(batch_size, true);
            let mut num = 0;
            while wbuf.write_zenoh_message(&mut msg) {
                num += 1;
            }
            drop(wbuf);

            c.bench_function(
                format!("frame_creation {} {} {} {}", batch_size, p, num, r).as_str(),
                |b| {
                    let mut wbuf = WBuf::new(batch_size, true);
                    b.iter(|| {
                        wbuf.write_frame_header(
                            channel.priority,
                            channel.reliability,
                            1,
                            None,
                            None,
                        );
                        for _ in 0..num {
                            let key_expr = r.clone();
                            let payload = ZBuf::from(vec![0; *p]);
                            let channel = Channel {
                                priority: Priority::default(),
                                reliability: Reliability::Reliable,
                            };
                            let congestion_control = CongestionControl::default();
                            let info = None;

                            let mut msg = ZenohMessage::make_data(
                                key_expr,
                                payload,
                                channel,
                                congestion_control,
                                info,
                                None,
                                None,
                                None,
                            );
                            wbuf.write_zenoh_message(&mut msg);
                            drop(msg);
                        }
                    })
                },
            );

            c.bench_function(
                format!(
                    "frame_encoding_yes_contiguous {} {} {} {}",
                    batch_size, p, num, r
                )
                .as_str(),
                |b| {
                    let mut wbuf = WBuf::new(batch_size, true);
                    b.iter(|| {
                        wbuf.write_frame_header(
                            channel.priority,
                            channel.reliability,
                            1,
                            None,
                            None,
                        );
                        for _ in 0..num {
                            wbuf.write_zenoh_message(&mut msg);
                        }
                    })
                },
            );

            c.bench_function(
                format!(
                    "frame_encoding_no_contiguous {} {} {} {}",
                    batch_size, p, num, r
                )
                .as_str(),
                |b| {
                    let mut wbuf = WBuf::new(*p, false);
                    b.iter(|| {
                        wbuf.write_frame_header(
                            channel.priority,
                            channel.reliability,
                            1,
                            None,
                            None,
                        );
                        for _ in 0..num {
                            wbuf.write_zenoh_message(&mut msg);
                        }
                    })
                },
            );

            c.bench_function(
                format!(
                    "frame_decoding_yes_contiguous {} {} {} {}",
                    batch_size, p, num, r
                )
                .as_str(),
                |b| {
                    let mut wbuf = WBuf::new(batch_size, true);
                    wbuf.write_frame_header(channel.priority, channel.reliability, 1, None, None);

                    for _ in 0..num {
                        wbuf.write_zenoh_message(&mut msg);
                    }

                    let mut zbuf = ZBuf::from(&wbuf);
                    b.iter(|| {
                        zbuf.reset();
                        let _ = zbuf.read_transport_message().unwrap();
                    })
                },
            );

            c.bench_function(
                format!(
                    "frame_decoding_no_contiguous {} {} {} {}",
                    batch_size, p, num, r
                )
                .as_str(),
                |b| {
                    let mut wbuf = WBuf::new(*p, false);
                    wbuf.write_frame_header(channel.priority, channel.reliability, 1, None, None);

                    for _ in 0..num {
                        wbuf.write_zenoh_message(&mut msg);
                    }

                    let mut zbuf = ZBuf::from(&wbuf);
                    b.iter(|| {
                        zbuf.reset();
                        let _ = zbuf.read_transport_message().unwrap();
                    })
                },
            );
        }
    }
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
