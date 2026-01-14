//
// Copyright (c) 2023 ZettaScale Technology
//
// This program and the accompanying materials are made availbble under the
// terms of the Eclipse Public License 2.0 which is availbble at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is availbble at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//

// Scenario 2
//   P      P      P
// P P P  P P P  P P P

use std::time::Duration;

use zenoh_config::WhatAmI::Peer;
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
async fn test_regions_scenario2_order1_putsub() {
    init_tracing_subscriber();

    let _z9100 = ztimeout!(Node::new(Peer, "21aa9100")
        .endpoints("tcp/[::]:9100", &[])
        .gateway("{north:{filters:[{zids:[\"21aa9100\",\"21aa9200\",\"21aa9300\"]}]},south:[]}")
        .open());
    let _z9200 = ztimeout!(Node::new(Peer, "21aa9200")
        .endpoints("tcp/[::]:9200", &["tcp/[::]:9100"])
        .gateway("{north:{filters:[{zids:[\"21aa9100\",\"21aa9200\",\"21aa9300\"]}]},south:[]}")
        .open());
    let _z9300 = ztimeout!(Node::new(Peer, "21aa9300")
        .endpoints("tcp/[::]:9300", &["tcp/[::]:9100", "tcp/[::]:9200"])
        .gateway("{north:{filters:[{zids:[\"21aa9100\",\"21aa9200\",\"21aa9300\"]}]},south:[]}")
        .open());

    let _z9110 = ztimeout!(Node::new(Peer, "21aa9110")
        .listen("tcp/[::]:9110")
        .connect(&["tcp/[::]:9100"])
        .open());
    let _z9120 = ztimeout!(Node::new(Peer, "21aa9120")
        .listen("tcp/[::]:9120")
        .connect(&["tcp/[::]:9100", "tcp/[::]:9110"])
        .open());
    let _z9130 = ztimeout!(Node::new(Peer, "21aa9130")
        .listen("tcp/[::]:9130")
        .connect(&["tcp/[::]:9100", "tcp/[::]:9110", "tcp/[::]:9120"])
        .open());

    let _z9210 = ztimeout!(Node::new(Peer, "21aa9210")
        .listen("tcp/[::]:9210")
        .connect(&["tcp/[::]:9200"])
        .open());
    let _z9220 = ztimeout!(Node::new(Peer, "21aa9220")
        .listen("tcp/[::]:9220")
        .connect(&["tcp/[::]:9200", "tcp/[::]:9210"])
        .open());
    let _z9230 = ztimeout!(Node::new(Peer, "21aa9230")
        .listen("tcp/[::]:9230")
        .connect(&["tcp/[::]:9200", "tcp/[::]:9210", "tcp/[::]:9220"])
        .open());

    let _z9310 = ztimeout!(Node::new(Peer, "21aa9310")
        .listen("tcp/[::]:9310")
        .connect(&["tcp/[::]:9300"])
        .open());
    let _z9320 = ztimeout!(Node::new(Peer, "21aa9320")
        .listen("tcp/[::]:9320")
        .connect(&["tcp/[::]:9300", "tcp/[::]:9310"])
        .open());
    let _z9330 = ztimeout!(Node::new(Peer, "21aa9330")
        .listen("tcp/[::]:9330")
        .connect(&["tcp/[::]:9300", "tcp/[::]:9310", "tcp/[::]:9320"])
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
        "21aa9110", "21aa9120", "21aa9130", "21aa9210", "21aa9220", "21aa9230", "21aa9310",
        "21aa9320", "21aa9330",
    ] {
        assert_eq!(
            count!(s, demux{zid=i src="north..."}:declare_subscriber{expr="test/**"}: "()"),
            2
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario2_order1_pubsub() {
    init_tracing_subscriber();

    let _z9100 = ztimeout!(Node::new(Peer, "21ab9100")
        .endpoints("tcp/[::]:9100", &[])
        .gateway("{north:{filters:[{zids:[\"21ab9100\",\"21ab9200\",\"21ab9300\"]}]},south:[{filters:[]}]}")
        .open());
    let _z9200 = ztimeout!(Node::new(Peer, "21ab9200")
        .endpoints("tcp/[::]:9200", &["tcp/[::]:9100"])
        .gateway("{north:{filters:[{zids:[\"21ab9100\",\"21ab9200\",\"21ab9300\"]}]},south:[{filters:[]}]}")
        .open());
    let _z9300 = ztimeout!(Node::new(Peer, "21ab9300")
        .endpoints("tcp/[::]:9300", &["tcp/[::]:9100", "tcp/[::]:9200"])
        .gateway("{north:{filters:[{zids:[\"21ab9100\",\"21ab9200\",\"21ab9300\"]}]},south:[{filters:[]}]}")
        .open());

    let _z9110 = ztimeout!(Node::new(Peer, "21ab9110")
        .listen("tcp/[::]:9110")
        .connect(&["tcp/[::]:9100"])
        .open());
    let _z9120 = ztimeout!(Node::new(Peer, "21ab9120")
        .listen("tcp/[::]:9120")
        .connect(&["tcp/[::]:9100", "tcp/[::]:9110"])
        .open());
    let _z9130 = ztimeout!(Node::new(Peer, "21ab9130")
        .listen("tcp/[::]:9130")
        .connect(&["tcp/[::]:9100", "tcp/[::]:9110", "tcp/[::]:9120"])
        .open());

    let _z9210 = ztimeout!(Node::new(Peer, "21ab9210")
        .listen("tcp/[::]:9210")
        .connect(&["tcp/[::]:9200"])
        .open());
    let _z9220 = ztimeout!(Node::new(Peer, "21ab9220")
        .listen("tcp/[::]:9220")
        .connect(&["tcp/[::]:9200", "tcp/[::]:9210"])
        .open());
    let _z9230 = ztimeout!(Node::new(Peer, "21ab9230")
        .listen("tcp/[::]:9230")
        .connect(&["tcp/[::]:9200", "tcp/[::]:9210", "tcp/[::]:9220"])
        .open());

    let _z9310 = ztimeout!(Node::new(Peer, "21ab9310")
        .listen("tcp/[::]:9310")
        .connect(&["tcp/[::]:9300"])
        .open());
    let _z9320 = ztimeout!(Node::new(Peer, "21ab9320")
        .listen("tcp/[::]:9320")
        .connect(&["tcp/[::]:9300", "tcp/[::]:9310"])
        .open());
    let _z9330 = ztimeout!(Node::new(Peer, "21ab9330")
        .listen("tcp/[::]:9330")
        .connect(&["tcp/[::]:9300", "tcp/[::]:9310", "tcp/[::]:9320"])
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
        "21ab9110", "21ab9120", "21ab9130", "21ab9210", "21ab9220", "21ab9230", "21ab9310",
        "21ab9320", "21ab9330",
    ] {
        assert_eq!(
            count!(s, demux{zid=i src="north..."}:declare_subscriber{expr="test/**"}: "()"),
            3
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario2_order2_putsub() {
    init_tracing_subscriber();

    let _z9110 = ztimeout!(Node::new(Peer, "22aa9110")
        .listen("tcp/[::]:9110")
        .connect(&["tcp/[::]:9100"])
        .open());
    let _z9120 = ztimeout!(Node::new(Peer, "22aa9120")
        .listen("tcp/[::]:9120")
        .connect(&["tcp/[::]:9100", "tcp/[::]:9110"])
        .open());
    let _z9130 = ztimeout!(Node::new(Peer, "22aa9130")
        .listen("tcp/[::]:9130")
        .connect(&["tcp/[::]:9100", "tcp/[::]:9110", "tcp/[::]:9120"])
        .open());

    let _z9210 = ztimeout!(Node::new(Peer, "22aa9210")
        .listen("tcp/[::]:9210")
        .connect(&["tcp/[::]:9200"])
        .open());
    let _z9220 = ztimeout!(Node::new(Peer, "22aa9220")
        .listen("tcp/[::]:9220")
        .connect(&["tcp/[::]:9200", "tcp/[::]:9210"])
        .open());
    let _z9230 = ztimeout!(Node::new(Peer, "22aa9230")
        .listen("tcp/[::]:9230")
        .connect(&["tcp/[::]:9200", "tcp/[::]:9210", "tcp/[::]:9220"])
        .open());

    let _z9310 = ztimeout!(Node::new(Peer, "22aa9310")
        .listen("tcp/[::]:9310")
        .connect(&["tcp/[::]:9300"])
        .open());
    let _z9320 = ztimeout!(Node::new(Peer, "22aa9320")
        .listen("tcp/[::]:9320")
        .connect(&["tcp/[::]:9300", "tcp/[::]:9310"])
        .open());
    let _z9330 = ztimeout!(Node::new(Peer, "22aa9330")
        .listen("tcp/[::]:9330")
        .connect(&["tcp/[::]:9300", "tcp/[::]:9310", "tcp/[::]:9320"])
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

    let _z9100 = ztimeout!(Node::new(Peer, "22aa9100")
        .endpoints("tcp/[::]:9100", &[])
        .gateway("{north:{filters:[{zids:[\"22aa9100\",\"22aa9200\",\"22aa9300\"]}]},south:[]}")
        .open());
    let _z9200 = ztimeout!(Node::new(Peer, "22aa9200")
        .endpoints("tcp/[::]:9200", &["tcp/[::]:9100"])
        .gateway("{north:{filters:[{zids:[\"22aa9100\",\"22aa9200\",\"22aa9300\"]}]},south:[]}")
        .open());
    let _z9300 = ztimeout!(Node::new(Peer, "22aa9300")
        .endpoints("tcp/[::]:9300", &["tcp/[::]:9100", "tcp/[::]:9200"])
        .gateway("{north:{filters:[{zids:[\"22aa9100\",\"22aa9200\",\"22aa9300\"]}]},south:[]}")
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
        "22aa9110", "22aa9120", "22aa9130", "22aa9210", "22aa9220", "22aa9230", "22aa9310",
        "22aa9320", "22aa9330",
    ] {
        assert_eq!(
            count!(s, demux{zid=i src="north..."}:declare_subscriber{expr="test/**"}: "()"),
            2
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario2_order2_pubsub() {
    init_tracing_subscriber();

    let _z9110 = ztimeout!(Node::new(Peer, "22ab9110")
        .listen("tcp/[::]:9110")
        .connect(&["tcp/[::]:9100"])
        .open());
    let _z9120 = ztimeout!(Node::new(Peer, "22ab9120")
        .listen("tcp/[::]:9120")
        .connect(&["tcp/[::]:9100", "tcp/[::]:9110"])
        .open());
    let _z9130 = ztimeout!(Node::new(Peer, "22ab9130")
        .listen("tcp/[::]:9130")
        .connect(&["tcp/[::]:9100", "tcp/[::]:9110", "tcp/[::]:9120"])
        .open());

    let _z9210 = ztimeout!(Node::new(Peer, "22ab9210")
        .listen("tcp/[::]:9210")
        .connect(&["tcp/[::]:9200"])
        .open());
    let _z9220 = ztimeout!(Node::new(Peer, "22ab9220")
        .listen("tcp/[::]:9220")
        .connect(&["tcp/[::]:9200", "tcp/[::]:9210"])
        .open());
    let _z9230 = ztimeout!(Node::new(Peer, "22ab9230")
        .listen("tcp/[::]:9230")
        .connect(&["tcp/[::]:9200", "tcp/[::]:9210", "tcp/[::]:9220"])
        .open());

    let _z9310 = ztimeout!(Node::new(Peer, "22ab9310")
        .listen("tcp/[::]:9310")
        .connect(&["tcp/[::]:9300"])
        .open());
    let _z9320 = ztimeout!(Node::new(Peer, "22ab9320")
        .listen("tcp/[::]:9320")
        .connect(&["tcp/[::]:9300", "tcp/[::]:9310"])
        .open());
    let _z9330 = ztimeout!(Node::new(Peer, "22ab9330")
        .listen("tcp/[::]:9330")
        .connect(&["tcp/[::]:9300", "tcp/[::]:9310", "tcp/[::]:9320"])
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

    let _z9100 = ztimeout!(Node::new(Peer, "22ab9100")
        .endpoints("tcp/[::]:9100", &[])
        .gateway("{north:{filters:[{zids:[\"22ab9100\",\"22ab9200\",\"22ab9300\"]}]},south:[]}")
        .open());
    let _z9200 = ztimeout!(Node::new(Peer, "22ab9200")
        .endpoints("tcp/[::]:9200", &["tcp/[::]:9100"])
        .gateway("{north:{filters:[{zids:[\"22ab9100\",\"22ab9200\",\"22ab9300\"]}]},south:[]}")
        .open());
    let _z9300 = ztimeout!(Node::new(Peer, "22ab9300")
        .endpoints("tcp/[::]:9300", &["tcp/[::]:9100", "tcp/[::]:9200"])
        .gateway("{north:{filters:[{zids:[\"22ab9100\",\"22ab9200\",\"22ab9300\"]}]},south:[]}")
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
        "22ab9110", "22ab9120", "22ab9130", "22ab9210", "22ab9220", "22ab9230", "22ab9310",
        "22ab9320", "22ab9330",
    ] {
        assert_eq!(
            count!(s, demux{zid=i src="north..."}:declare_subscriber{expr="test/**"}: "()"),
            3
        );
    }
}
