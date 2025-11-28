//
// Copyright (c) 2023 ZettaScale Technology
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

// Scenario 1
//          R
//   C      C      C
// P P P  P P P  P P P

use std::time::Duration;

use serial_test::serial;
use zenoh_config::WhatAmI::{Client, Peer, Router};
use zenoh_core::ztimeout;

use crate::Node;
use crate::count;

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[serial]
async fn test_regions_scenario1_order1_putsub() {
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter("debug,zenoh::net::routing::dispatcher=trace")
        .finish();
    let storage = tracing_capture::SharedStorage::default();
    use tracing_subscriber::layer::SubscriberExt;
    let subscriber = subscriber.with(tracing_capture::CaptureLayer::new(&storage));
    tracing::subscriber::set_global_default(subscriber).unwrap();

    let _z9000 = ztimeout!(Node::new(Router, "9000")
        .endpoints("tcp/[::]:9000", &[])
        .open());

    let _z9100 = ztimeout!(Node::new(Client, "9100")
        .endpoints("tcp/[::]:9100", &["tcp/[::]:9000"])
        .gateway("{north:{filters:[{zids:[\"9000\"]}]},south:[{filters:[{modes:[\"client\", \"peer\"]}]}]}")
        .open());
    let _z9200 = ztimeout!(Node::new(Client, "9200")
        .endpoints("tcp/[::]:9200", &["tcp/[::]:9000"])
        .gateway("{north:{filters:[{zids:[\"9000\"]}]},south:[{filters:[{modes:[\"client\", \"peer\"]}]}]}")
        .open());
    let _z9300 = ztimeout!(Node::new(Client, "9300")
        .endpoints("tcp/[::]:9300", &["tcp/[::]:9000"])
        .gateway("{north:{filters:[{zids:[\"9000\"]}]},south:[{filters:[{modes:[\"client\", \"peer\"]}]}]}")
        .open());

    let _z9110 = ztimeout!(Node::new(Peer, "9110")
        .listen("tcp/[::]:9110")
        .connect(&["tcp/[::]:9100"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());
    let _z9120 = ztimeout!(Node::new(Peer, "9120")
        .listen("tcp/[::]:9120")
        .connect(&["tcp/[::]:9100", "tcp/[::]:9110"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());
    let _z9130 = ztimeout!(Node::new(Peer, "9130")
        .listen("tcp/[::]:9130")
        .connect(&["tcp/[::]:9100", "tcp/[::]:9110", "tcp/[::]:9120"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());

    let _z9210 = ztimeout!(Node::new(Peer, "9210")
        .listen("tcp/[::]:9210")
        .connect(&["tcp/[::]:9200"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());
    let _z9220 = ztimeout!(Node::new(Peer, "9220")
        .listen("tcp/[::]:9220")
        .connect(&["tcp/[::]:9200", "tcp/[::]:9210"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());
    let _z9230 = ztimeout!(Node::new(Peer, "9230")
        .listen("tcp/[::]:9230")
        .connect(&["tcp/[::]:9200", "tcp/[::]:9210", "tcp/[::]:9220"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());

    let _z9310 = ztimeout!(Node::new(Peer, "9310")
        .listen("tcp/[::]:9310")
        .connect(&["tcp/[::]:9300"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());
    let _z9320 = ztimeout!(Node::new(Peer, "9320")
        .listen("tcp/[::]:9320")
        .connect(&["tcp/[::]:9300", "tcp/[::]:9310"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());
    let _z9330 = ztimeout!(Node::new(Peer, "9330")
        .listen("tcp/[::]:9330")
        .connect(&["tcp/[::]:9300", "tcp/[::]:9310", "tcp/[::]:9320"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());

    let s9110 = _z9110.declare_subscriber("test/**").await.unwrap();
    let s9120 = _z9120.declare_subscriber("test/**").await.unwrap();
    let s9130 = _z9130.declare_subscriber("test/**").await.unwrap();
    let s9210 = _z9210.declare_subscriber("test/**").await.unwrap();
    let s9220 = _z9220.declare_subscriber("test/**").await.unwrap();
    let s9230 = _z9230.declare_subscriber("test/**").await.unwrap();
    let s9310 = _z9310.declare_subscriber("test/**").await.unwrap();
    let s9320 = _z9320.declare_subscriber("test/**").await.unwrap();
    let s9330 = _z9330.declare_subscriber("test/**").await.unwrap();
    tokio::time::sleep(SLEEP).await;

    _z9110.put("test/9110", "9110").await.unwrap();
    _z9120.put("test/9120", "9120").await.unwrap();
    _z9130.put("test/9130", "9130").await.unwrap();
    _z9210.put("test/9210", "9210").await.unwrap();
    _z9220.put("test/9220", "9220").await.unwrap();
    _z9230.put("test/9230", "9230").await.unwrap();
    _z9310.put("test/9310", "9310").await.unwrap();
    _z9320.put("test/9320", "9320").await.unwrap();
    _z9330.put("test/9330", "9330").await.unwrap();
    tokio::time::sleep(SLEEP).await;

    assert_eq!(s9110.drain().count(), 9);
    assert_eq!(s9120.drain().count(), 9);
    assert_eq!(s9130.drain().count(), 9);
    assert_eq!(s9210.drain().count(), 9);
    assert_eq!(s9220.drain().count(), 9);
    assert_eq!(s9230.drain().count(), 9);
    assert_eq!(s9310.drain().count(), 9);
    assert_eq!(s9320.drain().count(), 9);
    assert_eq!(s9330.drain().count(), 9);

    let s = storage.lock();

    for i in [
        "9110", "9120", "9130", "9210", "9220", "9230", "9310", "9320", "9330",
    ] {
        assert_eq!(
            count!(s, ["zid" = i], "Declare subscriber", "test/**", ":N"),
            2
        );
    }
}
