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

use zenoh::{
    bytes::Encoding,
    qos::{CongestionControl, Priority},
};
use zenoh_core::ztimeout;

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn qos_pubsub() {
    let session1 = ztimeout!(zenoh::open(zenoh::Config::default())).unwrap();
    let session2 = ztimeout!(zenoh::open(zenoh::Config::default())).unwrap();

    let publisher1 = ztimeout!(session1
        .declare_publisher("test/qos")
        .encoding("text/plain")
        .priority(Priority::DataHigh)
        .congestion_control(CongestionControl::Drop)
        .express(true))
    .unwrap();

    let publisher2 = ztimeout!(session1
        .declare_publisher("test/qos")
        .encoding(Encoding::ZENOH_STRING)
        .priority(Priority::DataLow)
        .congestion_control(CongestionControl::Block)
        .express(false))
    .unwrap();

    let subscriber = ztimeout!(session2.declare_subscriber("test/qos")).unwrap();
    tokio::time::sleep(SLEEP).await;

    ztimeout!(publisher1.put("qos")).unwrap();
    let sample = ztimeout!(subscriber.recv_async()).unwrap();

    assert_eq!(sample.encoding(), &Encoding::TEXT_PLAIN);
    assert_eq!(sample.priority(), Priority::DataHigh);
    assert_eq!(sample.congestion_control(), CongestionControl::Drop);
    assert!(sample.express());

    ztimeout!(publisher2.put("qos")).unwrap();
    let sample = ztimeout!(subscriber.recv_async()).unwrap();

    assert_eq!(sample.encoding(), &Encoding::ZENOH_STRING);
    assert_eq!(sample.priority(), Priority::DataLow);
    assert_eq!(sample.congestion_control(), CongestionControl::Block);
    assert!(!sample.express());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn qos_pubsub_overwrite_builder() {
    let builder_config_overwrite = zenoh::Config::from_json5(
        r#"
        {
            publishers: {
                default_builders: [
                    {
                        key_exprs: ["test/qos/overwritten", "test/not_applicable/**"],
                        config: {
                            priority: "real_time",
                            encoding: "zenoh/string",
                            congestion_control: "drop",
                            express: false,
                        },
                    },
                    {
                        key_exprs: ["test/not_applicable"],
                        config: {
                            priority: "data_high",
                            encoding: "zenoh/bytes",
                            congestion_control: "drop",
                            express: false,
                        },
                    },
                ]
            }
        }
    "#,
    )
    .unwrap();
    let session1 = ztimeout!(zenoh::open(builder_config_overwrite)).unwrap();
    let session2 = ztimeout!(zenoh::open(zenoh::Config::default())).unwrap();

    let overwrite_config_publisher = ztimeout!(session1
        .declare_publisher("test/qos/overwritten")
        .congestion_control(CongestionControl::Block)
        .express(true))
    .unwrap();

    let no_overwrite_config_publisher = ztimeout!(session1
        .declare_publisher("test/qos/no_overwrite")
        .encoding(Encoding::TEXT_PLAIN)
        .priority(Priority::DataLow)
        .congestion_control(CongestionControl::Drop)
        .express(false))
    .unwrap();

    let subscriber = ztimeout!(session2.declare_subscriber("test/qos/**")).unwrap();
    tokio::time::sleep(SLEEP).await;

    ztimeout!(overwrite_config_publisher.put("qos")).unwrap();
    let sample = ztimeout!(subscriber.recv_async()).unwrap();

    assert_eq!(sample.priority(), Priority::RealTime);
    assert_eq!(sample.encoding(), &Encoding::ZENOH_STRING);
    assert_eq!(sample.congestion_control(), CongestionControl::Block);
    assert!(sample.express());

    ztimeout!(no_overwrite_config_publisher.put("qos")).unwrap();
    let sample = ztimeout!(subscriber.recv_async()).unwrap();

    assert_eq!(sample.encoding(), &Encoding::TEXT_PLAIN);
    assert_eq!(sample.priority(), Priority::DataLow);
    assert_eq!(sample.congestion_control(), CongestionControl::Drop);
    assert!(!sample.express());
}
