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

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn qos_pubsub_overwrite_config() {
    use zenoh::{qos::Reliability, sample::Locality};

    let qos_config_overwrite = zenoh::Config::from_json5(
        r#"
        {
            qos: {
                publication: [
                    {
                        key_exprs: ["test/qos_overwrite/overwritten", "test/not_applicable/**"],
                        config: {
                            congestion_control: "drop",
                            express: false,
                            reliability: "best_effort",
                            allowed_destination: "any",
                        },
                    },
                    {
                        key_exprs: ["test/not_applicable"],
                        config: {
                            congestion_control: "drop",
                            express: false,
                            reliability: "best_effort",
                            allowed_destination: "any",
                        },
                    },
                ]
            }
        }
        "#,
    )
    .unwrap();
    let session1 = ztimeout!(zenoh::open(qos_config_overwrite)).unwrap();
    let session2 = ztimeout!(zenoh::open(zenoh::Config::default())).unwrap();

    let subscriber = ztimeout!(session2.declare_subscriber("test/qos_overwrite/**")).unwrap();
    tokio::time::sleep(SLEEP).await;

    // Session API tests

    // Session API - overwritten PUT
    ztimeout!(session1
        .put("test/qos_overwrite/overwritten", "qos")
        .congestion_control(CongestionControl::Block)
        .priority(Priority::DataLow)
        .express(true)
        .reliability(Reliability::Reliable)
        .allowed_destination(Locality::SessionLocal))
    .unwrap();
    let sample = ztimeout!(subscriber.recv_async()).unwrap();

    assert_eq!(sample.congestion_control(), CongestionControl::Drop);
    assert_eq!(sample.priority(), Priority::DataLow);
    assert!(!sample.express());
    assert_eq!(sample.reliability(), Reliability::BestEffort);

    // Session API - overwritten DELETE
    ztimeout!(session1
        .delete("test/qos_overwrite/overwritten")
        .congestion_control(CongestionControl::Block)
        .priority(Priority::DataLow)
        .express(true)
        .reliability(Reliability::Reliable)
        .allowed_destination(Locality::SessionLocal))
    .unwrap();
    let sample = ztimeout!(subscriber.recv_async()).unwrap();

    assert_eq!(sample.congestion_control(), CongestionControl::Drop);
    assert_eq!(sample.priority(), Priority::DataLow);
    assert!(!sample.express());
    assert_eq!(sample.reliability(), Reliability::BestEffort);

    // Session API - non-overwritten PUT
    ztimeout!(session1
        .put("test/qos_overwrite/no_overwrite", "qos")
        .congestion_control(CongestionControl::Block)
        .priority(Priority::DataLow)
        .express(true)
        .reliability(Reliability::Reliable))
    .unwrap();
    let sample = ztimeout!(subscriber.recv_async()).unwrap();

    assert_eq!(sample.congestion_control(), CongestionControl::Block);
    assert_eq!(sample.priority(), Priority::DataLow);
    assert!(sample.express());
    assert_eq!(sample.reliability(), Reliability::Reliable);

    // Session API - non-overwritten DELETE
    ztimeout!(session1
        .delete("test/qos_overwrite/no_overwrite")
        .congestion_control(CongestionControl::Block)
        .priority(Priority::DataLow)
        .express(true)
        .reliability(Reliability::Reliable))
    .unwrap();
    let sample = ztimeout!(subscriber.recv_async()).unwrap();

    assert_eq!(sample.congestion_control(), CongestionControl::Block);
    assert_eq!(sample.priority(), Priority::DataLow);
    assert!(sample.express());
    assert_eq!(sample.reliability(), Reliability::Reliable);

    // Publisher API tests

    let overwrite_config_publisher = ztimeout!(session1
        .declare_publisher("test/qos_overwrite/overwritten")
        .congestion_control(CongestionControl::Block)
        .priority(Priority::DataLow)
        .express(true)
        .reliability(Reliability::Reliable)
        .allowed_destination(Locality::SessionLocal))
    .unwrap();

    let no_overwrite_config_publisher = ztimeout!(session1
        .declare_publisher("test/qos_overwrite/no_overwrite")
        .congestion_control(CongestionControl::Block)
        .priority(Priority::DataLow)
        .express(true)
        .reliability(Reliability::Reliable))
    .unwrap();

    // PublisherBuilder API - overwritten PUT
    ztimeout!(overwrite_config_publisher.put("qos")).unwrap();
    let sample = ztimeout!(subscriber.recv_async()).unwrap();

    assert_eq!(sample.congestion_control(), CongestionControl::Drop);
    assert_eq!(sample.priority(), Priority::DataLow);
    assert!(!sample.express());
    assert_eq!(sample.reliability(), Reliability::BestEffort);

    // PublisherBuilder API - overwritten DELETE
    ztimeout!(overwrite_config_publisher.delete()).unwrap();
    let sample = ztimeout!(subscriber.recv_async()).unwrap();

    assert_eq!(sample.congestion_control(), CongestionControl::Drop);
    assert_eq!(sample.priority(), Priority::DataLow);
    assert!(!sample.express());
    assert_eq!(sample.reliability(), Reliability::BestEffort);

    // PublisherBuilder API - non-overwritten PUT
    ztimeout!(no_overwrite_config_publisher.put("qos")).unwrap();
    let sample = ztimeout!(subscriber.recv_async()).unwrap();

    assert_eq!(sample.congestion_control(), CongestionControl::Block);
    assert_eq!(sample.priority(), Priority::DataLow);
    assert!(sample.express());
    assert_eq!(sample.reliability(), Reliability::Reliable);

    // PublisherBuilder API - non-overwritten DELETE
    ztimeout!(no_overwrite_config_publisher.delete()).unwrap();
    let sample = ztimeout!(subscriber.recv_async()).unwrap();

    assert_eq!(sample.congestion_control(), CongestionControl::Block);
    assert_eq!(sample.priority(), Priority::DataLow);
    assert!(sample.express());
    assert_eq!(sample.reliability(), Reliability::Reliable);
}
