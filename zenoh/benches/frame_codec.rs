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

use zenoh::net::protocol::core::{Channel, CongestionControl, Reliability, ResKey};
use zenoh::net::protocol::io::{RBuf, WBuf};
use zenoh::net::protocol::proto::ZenohMessage;
use zenoh::net::protocol::session::defaults::SESSION_BATCH_SIZE;

fn criterion_benchmark(c: &mut Criterion) {
    let batch_size = *SESSION_BATCH_SIZE;
    let mut current = 8;
    let mut pld = Vec::new();
    while current < batch_size {
        pld.push(current);
        current *= 2;
    }

    for p in &pld {
        let channel = Channel::Reliable;
        let reliability = Reliability::Reliable;
        let congestion_control = CongestionControl::Block;

        let res_key = ResKey::RIdWithSuffix(18, String::from("/frame/bench"));
        let info = None;
        let payload = RBuf::from(vec![0; *p]);

        let msg = ZenohMessage::make_data(
            res_key,
            payload,
            reliability,
            congestion_control,
            info,
            None,
            None,
            None,
        );

        let mut wbuf = WBuf::new(batch_size, true);
        let mut num = 0;
        while wbuf.write_zenoh_message(&msg) {
            num += 1;
        }
        drop(wbuf);

        c.bench_function(
            format!("frame_creation {} {} {}", batch_size, p, num).as_str(),
            |b| {
                let mut wbuf = WBuf::new(batch_size, true);
                b.iter(|| {
                    wbuf.write_frame_header(channel, 1, None, None);
                    for _ in 0..num {
                        let reliability = Reliability::Reliable;
                        let congestion_control = CongestionControl::Block;
                        let res_key = ResKey::RIdWithSuffix(18, String::from("/frame/bench"));
                        let info = None;
                        let payload = RBuf::from(vec![0; *p]);

                        let msg = ZenohMessage::make_data(
                            res_key,
                            payload,
                            reliability,
                            congestion_control,
                            info,
                            None,
                            None,
                            None,
                        );
                        wbuf.write_zenoh_message(&msg);
                        drop(msg);
                    }
                })
            },
        );

        c.bench_function(
            format!("frame_encoding_yes_contigous {} {} {}", batch_size, p, num).as_str(),
            |b| {
                let mut wbuf = WBuf::new(batch_size, true);
                b.iter(|| {
                    wbuf.write_frame_header(channel, 1, None, None);
                    for _ in 0..num {
                        wbuf.write_zenoh_message(&msg);
                    }
                })
            },
        );

        c.bench_function(
            format!("frame_encoding_no_contigous {} {} {}", batch_size, p, num).as_str(),
            |b| {
                let mut wbuf = WBuf::new(*p, false);
                b.iter(|| {
                    wbuf.write_frame_header(channel, 1, None, None);
                    for _ in 0..num {
                        wbuf.write_zenoh_message(&msg);
                    }
                })
            },
        );

        c.bench_function(
            format!("frame_decoding_yes_contigous {} {} {}", batch_size, p, num).as_str(),
            |b| {
                let mut wbuf = WBuf::new(batch_size, true);
                wbuf.write_frame_header(channel, 1, None, None);

                for _ in 0..num {
                    wbuf.write_zenoh_message(&msg);
                }

                let mut rbuf = RBuf::from(&wbuf);
                b.iter(|| {
                    rbuf.reset_pos();
                    let _ = rbuf.read_session_message().unwrap();
                })
            },
        );

        c.bench_function(
            format!("frame_decoding_no_contigous {} {} {}", batch_size, p, num).as_str(),
            |b| {
                let mut wbuf = WBuf::new(*p, false);
                wbuf.write_frame_header(channel, 1, None, None);

                for _ in 0..num {
                    wbuf.write_zenoh_message(&msg);
                }

                let mut rbuf = RBuf::from(&wbuf);
                b.iter(|| {
                    rbuf.reset_pos();
                    let _ = rbuf.read_session_message().unwrap();
                })
            },
        );
    }
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
