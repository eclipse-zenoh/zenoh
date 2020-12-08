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

use zenoh_protocol::core::{Channel, CongestionControl, PeerId, Reliability, ResKey};
use zenoh_protocol::io::{RBuf, WBuf};
use zenoh_protocol::proto::{DataInfo, ZenohMessage};

fn criterion_benchmark(c: &mut Criterion) {
    let batch_size = 16_384;
    let pld = [8, 16, 32, 64, 128, 256, 512, 1_024];

    for p in &pld {
        let channel = Channel::Reliable;
        let reliability = Reliability::Reliable;
        let congestion_control = CongestionControl::Block;

        let res_key = ResKey::RIdWithSuffix(18, String::from("/com/acme/sensors/temp"));
        let info = Some(DataInfo {
            source_id: Some(PeerId::new(16, [0u8; PeerId::MAX_SIZE])),
            source_sn: Some(12345),
            first_router_id: Some(PeerId::new(16, [0u8; PeerId::MAX_SIZE])),
            first_router_sn: Some(12345),
            timestamp: Some(uhlc::Timestamp::new(
                Default::default(),
                uhlc::ID::new(16, [1u8; uhlc::ID::MAX_SIZE]),
            )),
            kind: Some(0),
            encoding: Some(0),
        });
        let payload = RBuf::from(vec![0; *p]);

        let msg = ZenohMessage::make_data(
            res_key,
            payload,
            reliability,
            congestion_control,
            info,
            None,
            None,
        );

        let mut wbuf = WBuf::new(batch_size, true);
        let mut nums = 0;
        while wbuf.write_zenoh_message(&msg) {
            nums += 1;
        }
        drop(wbuf);

        c.bench_function(
            format!("frame decoding contigous {} {} {}", batch_size, p, nums).as_str(),
            |b| {
                let mut wbuf = WBuf::new(batch_size, true);
                wbuf.write_frame_header(channel, 1, None, None);

                for _ in 0..nums {
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
            format!("frame decoding non-contigous {} {} {}", batch_size, p, nums).as_str(),
            |b| {
                let mut wbuf = WBuf::new(*p, false);
                wbuf.write_frame_header(channel, 1, None, None);

                for _ in 0..nums {
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
