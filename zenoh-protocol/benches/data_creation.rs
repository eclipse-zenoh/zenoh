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

use zenoh_protocol::core::ResKey;
use zenoh_protocol::io::RBuf;
use zenoh_protocol::proto::ZenohMessage;

fn consume_message(msg: ZenohMessage) {
    drop(msg);
}

fn consume_message_arc(msg: Arc<ZenohMessage>) {
    drop(msg);
}

fn criterion_benchmark(c: &mut Criterion) {
    let iters = [
        1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 1, 8, 64, 128, 1024,
    ];
    let reliable = true;

    for n in &iters {
        let res_key = ResKey::RIdWithSuffix(18, String::from("/com/acme/sensors/temp"));
        let info = Some(RBuf::from(vec![0; 1024]));
        let payload = RBuf::from(vec![0; 1024]);

        c.bench_function(format!("{} msg_creation", n).as_str(), |b| {
            b.iter(|| {
                for _ in 0..*n {
                    let msg = ZenohMessage::make_data(
                        reliable,
                        res_key.clone(),
                        info.clone(),
                        payload.clone(),
                        None,
                        None,
                    );
                    consume_message(msg);
                }
            })
        });
    }

    let res_key = ResKey::RIdWithSuffix(18, String::from("/com/acme/sensors/temp"));
    let info = Some(RBuf::from(vec![0; 1024]));
    let payload = RBuf::from(vec![0; 1024]);
    let msg = Arc::new(ZenohMessage::make_data(
        reliable,
        res_key.clone(),
        info.clone(),
        payload.clone(),
        None,
        None,
    ));

    for n in &iters {
        let amsg = msg.clone();
        c.bench_function(format!("{} arc_msg_clone", n).as_str(), |b| {
            b.iter(|| {
                for _ in 0..*n {
                    consume_message_arc(amsg.clone());
                }
            })
        });
    }
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
