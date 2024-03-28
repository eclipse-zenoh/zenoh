//
// Copyright (c) 2024 ZettaScale Technology
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
use std::time::Duration;
use zenoh::prelude::r#async::*;
use zenoh::{publication::Priority, SessionDeclarations};
use zenoh_core::ztimeout;

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pubsub() {
    let session1 = ztimeout!(zenoh::open(zenoh_config::peer()).res_async()).unwrap();
    let session2 = ztimeout!(zenoh::open(zenoh_config::peer()).res_async()).unwrap();

    let publisher1 = ztimeout!(session1
        .declare_publisher("test/qos")
        .priority(Priority::DataHigh)
        .congestion_control(CongestionControl::Drop)
        .res())
    .unwrap();

    let publisher2 = ztimeout!(session1
        .declare_publisher("test/qos")
        .priority(Priority::DataLow)
        .congestion_control(CongestionControl::Block)
        .res())
    .unwrap();

    let subscriber = ztimeout!(session2.declare_subscriber("test/qos").res()).unwrap();
    tokio::time::sleep(SLEEP).await;

    ztimeout!(publisher1.put("qos").res_async()).unwrap();
    let sample = ztimeout!(subscriber.recv_async()).unwrap();
    let qos = sample.qos();

    assert_eq!(qos.priority(), Priority::DataHigh);
    assert_eq!(qos.congestion_control(), CongestionControl::Drop);

    ztimeout!(publisher2.put("qos").res_async()).unwrap();
    let sample = ztimeout!(subscriber.recv_async()).unwrap();
    let qos = sample.qos();

    assert_eq!(qos.priority(), Priority::DataLow);
    assert_eq!(qos.congestion_control(), CongestionControl::Block);
}
