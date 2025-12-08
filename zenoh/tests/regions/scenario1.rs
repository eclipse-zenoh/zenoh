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
use zenoh_core::{lazy_static, ztimeout};

use crate::{count, Node};

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);

lazy_static! {
    static ref STORAGE: tracing_capture::SharedStorage = tracing_capture::SharedStorage::default();
}

fn init_tracing_subscriber() {
    use tracing_subscriber::layer::SubscriberExt;
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter("debug,zenoh::net::routing::dispatcher=trace")
        .finish();
    let subscriber = subscriber.with(tracing_capture::CaptureLayer::new(&STORAGE));
    tracing::subscriber::set_global_default(subscriber).ok();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[serial]
async fn test_regions_scenario1_order1_putsub() {
    init_tracing_subscriber();

    let _z9000 = ztimeout!(Node::new(Router, "aa9000")
        .endpoints("tcp/[::]:9000", &[])
        .open());

    let _z9100 = ztimeout!(Node::new(Client, "aa9100")
        .endpoints("tcp/[::]:9100", &["tcp/[::]:9000"])
        .gateway("{north:{filters:[{zids:[\"aa9000\"]}]},south:[{filters:[{modes:[\"client\", \"peer\"]}]}]}")
        .open());
    let _z9200 = ztimeout!(Node::new(Client, "aa9200")
        .endpoints("tcp/[::]:9200", &["tcp/[::]:9000"])
        .gateway("{north:{filters:[{zids:[\"aa9000\"]}]},south:[{filters:[{modes:[\"client\", \"peer\"]}]}]}")
        .open());
    let _z9300 = ztimeout!(Node::new(Client, "aa9300")
        .endpoints("tcp/[::]:9300", &["tcp/[::]:9000"])
        .gateway("{north:{filters:[{zids:[\"aa9000\"]}]},south:[{filters:[{modes:[\"client\", \"peer\"]}]}]}")
        .open());

    let _z9110 = ztimeout!(Node::new(Peer, "aa9110")
        .listen("tcp/[::]:9110")
        .connect(&["tcp/[::]:9100"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());
    let _z9120 = ztimeout!(Node::new(Peer, "aa9120")
        .listen("tcp/[::]:9120")
        .connect(&["tcp/[::]:9100", "tcp/[::]:9110"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());
    let _z9130 = ztimeout!(Node::new(Peer, "aa9130")
        .listen("tcp/[::]:9130")
        .connect(&["tcp/[::]:9100", "tcp/[::]:9110", "tcp/[::]:9120"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());

    let _z9210 = ztimeout!(Node::new(Peer, "aa9210")
        .listen("tcp/[::]:9210")
        .connect(&["tcp/[::]:9200"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());
    let _z9220 = ztimeout!(Node::new(Peer, "aa9220")
        .listen("tcp/[::]:9220")
        .connect(&["tcp/[::]:9200", "tcp/[::]:9210"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());
    let _z9230 = ztimeout!(Node::new(Peer, "aa9230")
        .listen("tcp/[::]:9230")
        .connect(&["tcp/[::]:9200", "tcp/[::]:9210", "tcp/[::]:9220"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());

    let _z9310 = ztimeout!(Node::new(Peer, "aa9310")
        .listen("tcp/[::]:9310")
        .connect(&["tcp/[::]:9300"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());
    let _z9320 = ztimeout!(Node::new(Peer, "aa9320")
        .listen("tcp/[::]:9320")
        .connect(&["tcp/[::]:9300", "tcp/[::]:9310"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());
    let _z9330 = ztimeout!(Node::new(Peer, "aa9330")
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

    let s = STORAGE.lock();

    for i in [
        "aa9110", "aa9120", "aa9130", "aa9210", "aa9220", "aa9230", "aa9310", "aa9320", "aa9330",
    ] {
        assert_eq!(
            count!(s, ["zid" = i], "Declare subscriber", "test/**", ":N"),
            2
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[serial]
async fn test_regions_scenario1_order1_pubsub() {
    init_tracing_subscriber();

    let _z9000 = ztimeout!(Node::new(Router, "ab9000")
        .endpoints("tcp/[::]:9000", &[])
        .open());

    let _z9100 = ztimeout!(Node::new(Client, "ab9100")
        .endpoints("tcp/[::]:9100", &["tcp/[::]:9000"])
        .gateway("{north:{filters:[{zids:[\"ab9000\"]}]},south:[{filters:[{modes:[\"client\", \"peer\"]}]}]}")
        .open());
    let _z9200 = ztimeout!(Node::new(Client, "ab9200")
        .endpoints("tcp/[::]:9200", &["tcp/[::]:9000"])
        .gateway("{north:{filters:[{zids:[\"ab9000\"]}]},south:[{filters:[{modes:[\"client\", \"peer\"]}]}]}")
        .open());
    let _z9300 = ztimeout!(Node::new(Client, "ab9300")
        .endpoints("tcp/[::]:9300", &["tcp/[::]:9000"])
        .gateway("{north:{filters:[{zids:[\"ab9000\"]}]},south:[{filters:[{modes:[\"client\", \"peer\"]}]}]}")
        .open());

    let _z9110 = ztimeout!(Node::new(Peer, "ab9110")
        .listen("tcp/[::]:9110")
        .connect(&["tcp/[::]:9100"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());
    let _z9120 = ztimeout!(Node::new(Peer, "ab9120")
        .listen("tcp/[::]:9120")
        .connect(&["tcp/[::]:9100", "tcp/[::]:9110"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());
    let _z9130 = ztimeout!(Node::new(Peer, "ab9130")
        .listen("tcp/[::]:9130")
        .connect(&["tcp/[::]:9100", "tcp/[::]:9110", "tcp/[::]:9120"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());

    let _z9210 = ztimeout!(Node::new(Peer, "ab9210")
        .listen("tcp/[::]:9210")
        .connect(&["tcp/[::]:9200"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());
    let _z9220 = ztimeout!(Node::new(Peer, "ab9220")
        .listen("tcp/[::]:9220")
        .connect(&["tcp/[::]:9200", "tcp/[::]:9210"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());
    let _z9230 = ztimeout!(Node::new(Peer, "ab9230")
        .listen("tcp/[::]:9230")
        .connect(&["tcp/[::]:9200", "tcp/[::]:9210", "tcp/[::]:9220"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());

    let _z9310 = ztimeout!(Node::new(Peer, "ab9310")
        .listen("tcp/[::]:9310")
        .connect(&["tcp/[::]:9300"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());
    let _z9320 = ztimeout!(Node::new(Peer, "ab9320")
        .listen("tcp/[::]:9320")
        .connect(&["tcp/[::]:9300", "tcp/[::]:9310"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());
    let _z9330 = ztimeout!(Node::new(Peer, "ab9330")
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

    let p9110 = _z9110.declare_publisher("test/9110").await.unwrap();
    let p9120 = _z9120.declare_publisher("test/9120").await.unwrap();
    let p9130 = _z9130.declare_publisher("test/9130").await.unwrap();
    let p9210 = _z9210.declare_publisher("test/9210").await.unwrap();
    let p9220 = _z9220.declare_publisher("test/9220").await.unwrap();
    let p9230 = _z9230.declare_publisher("test/9230").await.unwrap();
    let p9310 = _z9310.declare_publisher("test/9310").await.unwrap();
    let p9320 = _z9320.declare_publisher("test/9320").await.unwrap();
    let p9330 = _z9330.declare_publisher("test/9330").await.unwrap();

    tokio::time::sleep(SLEEP).await;

    p9110.put("9110").await.unwrap();
    p9120.put("9120").await.unwrap();
    p9130.put("9130").await.unwrap();
    p9210.put("9210").await.unwrap();
    p9220.put("9220").await.unwrap();
    p9230.put("9230").await.unwrap();
    p9310.put("9310").await.unwrap();
    p9320.put("9320").await.unwrap();
    p9330.put("9330").await.unwrap();

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

    let s = STORAGE.lock();

    for i in [
        "ab9110", "ab9120", "ab9130", "ab9210", "ab9220", "ab9230", "ab9310", "ab9320", "ab9330",
    ] {
        assert_eq!(
            count!(s, ["zid" = i], "Declare subscriber", "test/**", ":N"),
            3
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[serial]
async fn test_regions_scenario1_order2_putsub() {
    init_tracing_subscriber();

    let _z9110 = ztimeout!(Node::new(Peer, "ac9110")
        .listen("tcp/[::]:9110")
        .connect(&["tcp/[::]:9100"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());
    let _z9120 = ztimeout!(Node::new(Peer, "ac9120")
        .listen("tcp/[::]:9120")
        .connect(&["tcp/[::]:9100", "tcp/[::]:9110"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());
    let _z9130 = ztimeout!(Node::new(Peer, "ac9130")
        .listen("tcp/[::]:9130")
        .connect(&["tcp/[::]:9100", "tcp/[::]:9110", "tcp/[::]:9120"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());

    let _z9210 = ztimeout!(Node::new(Peer, "ac9210")
        .listen("tcp/[::]:9210")
        .connect(&["tcp/[::]:9200"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());
    let _z9220 = ztimeout!(Node::new(Peer, "ac9220")
        .listen("tcp/[::]:9220")
        .connect(&["tcp/[::]:9200", "tcp/[::]:9210"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());
    let _z9230 = ztimeout!(Node::new(Peer, "ac9230")
        .listen("tcp/[::]:9230")
        .connect(&["tcp/[::]:9200", "tcp/[::]:9210", "tcp/[::]:9220"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());

    let _z9310 = ztimeout!(Node::new(Peer, "ac9310")
        .listen("tcp/[::]:9310")
        .connect(&["tcp/[::]:9300"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());
    let _z9320 = ztimeout!(Node::new(Peer, "ac9320")
        .listen("tcp/[::]:9320")
        .connect(&["tcp/[::]:9300", "tcp/[::]:9310"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());
    let _z9330 = ztimeout!(Node::new(Peer, "ac9330")
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

    let _z9000 = ztimeout!(Node::new(Router, "ac9000")
        .endpoints("tcp/[::]:9000", &[])
        .open());

    let _z9100 = ztimeout!(Node::new(Client, "ac9100")
        .endpoints("tcp/[::]:9100", &["tcp/[::]:9000"])
        .gateway("{north:{filters:[{zids:[\"ac9000\"]}]},south:[{filters:[{modes:[\"client\", \"peer\"]}]}]}")
        .open());
    let _z9200 = ztimeout!(Node::new(Client, "ac9200")
        .endpoints("tcp/[::]:9200", &["tcp/[::]:9000"])
        .gateway("{north:{filters:[{zids:[\"ac9000\"]}]},south:[{filters:[{modes:[\"client\", \"peer\"]}]}]}")
        .open());
    let _z9300 = ztimeout!(Node::new(Client, "ac9300")
        .endpoints("tcp/[::]:9300", &["tcp/[::]:9000"])
        .gateway("{north:{filters:[{zids:[\"ac9000\"]}]},south:[{filters:[{modes:[\"client\", \"peer\"]}]}]}")
        .open());

    tokio::time::sleep(Duration::from_secs(8)).await;

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

    let s = STORAGE.lock();

    for i in [
        "ac9110", "ac9120", "ac9130", "ac9210", "ac9220", "ac9230", "ac9310", "ac9320", "ac9330",
    ] {
        assert_eq!(
            count!(s, ["zid" = i], "Declare subscriber", "test/**", ":N"),
            2
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[serial]
async fn test_regions_scenario1_order2_pubsub() {
    init_tracing_subscriber();

    let _z9110 = ztimeout!(Node::new(Peer, "ad9110")
        .listen("tcp/[::]:9110")
        .connect(&["tcp/[::]:9100"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());
    let _z9120 = ztimeout!(Node::new(Peer, "ad9120")
        .listen("tcp/[::]:9120")
        .connect(&["tcp/[::]:9100", "tcp/[::]:9110"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());
    let _z9130 = ztimeout!(Node::new(Peer, "ad9130")
        .listen("tcp/[::]:9130")
        .connect(&["tcp/[::]:9100", "tcp/[::]:9110", "tcp/[::]:9120"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());

    let _z9210 = ztimeout!(Node::new(Peer, "ad9210")
        .listen("tcp/[::]:9210")
        .connect(&["tcp/[::]:9200"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());
    let _z9220 = ztimeout!(Node::new(Peer, "ad9220")
        .listen("tcp/[::]:9220")
        .connect(&["tcp/[::]:9200", "tcp/[::]:9210"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());
    let _z9230 = ztimeout!(Node::new(Peer, "ad9230")
        .listen("tcp/[::]:9230")
        .connect(&["tcp/[::]:9200", "tcp/[::]:9210", "tcp/[::]:9220"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());

    let _z9310 = ztimeout!(Node::new(Peer, "ad9310")
        .listen("tcp/[::]:9310")
        .connect(&["tcp/[::]:9300"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());
    let _z9320 = ztimeout!(Node::new(Peer, "ad9320")
        .listen("tcp/[::]:9320")
        .connect(&["tcp/[::]:9300", "tcp/[::]:9310"])
        .gateway("{north:{filters:[{}]},south:[]}")
        .open());
    let _z9330 = ztimeout!(Node::new(Peer, "ad9330")
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

    let p9110 = _z9110.declare_publisher("test/9110").await.unwrap();
    let p9120 = _z9120.declare_publisher("test/9120").await.unwrap();
    let p9130 = _z9130.declare_publisher("test/9130").await.unwrap();
    let p9210 = _z9210.declare_publisher("test/9210").await.unwrap();
    let p9220 = _z9220.declare_publisher("test/9220").await.unwrap();
    let p9230 = _z9230.declare_publisher("test/9230").await.unwrap();
    let p9310 = _z9310.declare_publisher("test/9310").await.unwrap();
    let p9320 = _z9320.declare_publisher("test/9320").await.unwrap();
    let p9330 = _z9330.declare_publisher("test/9330").await.unwrap();

    let _z9000 = ztimeout!(Node::new(Router, "ad9000")
        .endpoints("tcp/[::]:9000", &[])
        .open());

    let _z9100 = ztimeout!(Node::new(Client, "ad9100")
        .endpoints("tcp/[::]:9100", &["tcp/[::]:9000"])
        .gateway("{north:{filters:[{zids:[\"ad9000\"]}]},south:[{filters:[{modes:[\"client\", \"peer\"]}]}]}")
        .open());
    let _z9200 = ztimeout!(Node::new(Client, "ad9200")
        .endpoints("tcp/[::]:9200", &["tcp/[::]:9000"])
        .gateway("{north:{filters:[{zids:[\"ad9000\"]}]},south:[{filters:[{modes:[\"client\", \"peer\"]}]}]}")
        .open());
    let _z9300 = ztimeout!(Node::new(Client, "ad9300")
        .endpoints("tcp/[::]:9300", &["tcp/[::]:9000"])
        .gateway("{north:{filters:[{zids:[\"ad9000\"]}]},south:[{filters:[{modes:[\"client\", \"peer\"]}]}]}")
        .open());

    tokio::time::sleep(Duration::from_secs(8)).await;

    p9110.put("9110").await.unwrap();
    p9120.put("9120").await.unwrap();
    p9130.put("9130").await.unwrap();
    p9210.put("9210").await.unwrap();
    p9220.put("9220").await.unwrap();
    p9230.put("9230").await.unwrap();
    p9310.put("9310").await.unwrap();
    p9320.put("9320").await.unwrap();
    p9330.put("9330").await.unwrap();

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

    let s = STORAGE.lock();

    for i in [
        "ad9110", "ad9120", "ad9130", "ad9210", "ad9220", "ad9230", "ad9310", "ad9320", "ad9330",
    ] {
        assert_eq!(
            count!(s, ["zid" = i], "Declare subscriber", "test/**", ":N"),
            3
        );
    }
}
