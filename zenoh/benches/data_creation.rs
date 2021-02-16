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

use async_std::sync::Arc;
use criterion::Criterion;

use zenoh::net::protocol::core::{CongestionControl, PeerId, Reliability, ResKey};
use zenoh::net::protocol::io::RBuf;
use zenoh::net::protocol::proto::{DataInfo, ZenohMessage};

fn consume_message(msg: ZenohMessage) {
    drop(msg);
}

fn consume_message_arc(msg: Arc<ZenohMessage>) {
    drop(msg);
}

fn criterion_benchmark(c: &mut Criterion) {
    let size = [8, 16, 32, 64, 128, 256, 512, 1_024, 2_048, 4_096, 8_192];
    for s in size.iter() {
        c.bench_function(format!("{} msg_creation_yes_info", s).as_str(), |b| {
            b.iter(|| {
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
                let payload = RBuf::from(vec![0; *s]);

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
                consume_message(msg);
            })
        });

        c.bench_function(format!("{} msg_creation_no_info", s).as_str(), |b| {
            b.iter(|| {
                let reliability = Reliability::Reliable;
                let congestion_control = CongestionControl::Block;

                let res_key = ResKey::RIdWithSuffix(18, String::from("/com/acme/sensors/temp"));
                let info = None;
                let payload = RBuf::from(vec![0; *s]);

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
                consume_message(msg);
            })
        });
    }

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
            uhlc::ID::new(16, [0u8; uhlc::ID::MAX_SIZE]),
        )),
        kind: Some(0),
        encoding: Some(0),
    });
    let payload = RBuf::from(vec![0; 1024]);
    let msg = Arc::new(ZenohMessage::make_data(
        res_key.clone(),
        payload.clone(),
        reliability,
        congestion_control,
        info.clone(),
        None,
        None,
        None,
    ));

    let amsg = msg.clone();
    c.bench_function(format!("arc_msg_clone").as_str(), |b| {
        b.iter(|| {
            consume_message_arc(amsg.clone());
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
