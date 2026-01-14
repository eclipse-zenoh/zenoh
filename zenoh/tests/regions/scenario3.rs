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

// Scenario 3
//   R      R      R
// P P P  P P P  P P P

use std::time::Duration;

use zenoh_config::WhatAmI::{Peer, Router};
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
async fn test_regions_scenario3_order1_putsub() {
    init_tracing_subscriber();

    let _z9100 = ztimeout!(Node::new(Router, "31aa9100")
        .endpoints("tcp/[::]:9100", &[])
        .open());
    let _z9200 = ztimeout!(Node::new(Router, "31aa9200")
        .endpoints("tcp/[::]:9200", &["tcp/[::]:9100"])
        .open());
    let _z9300 = ztimeout!(Node::new(Router, "31aa9300")
        .endpoints("tcp/[::]:9300", &["tcp/[::]:9100", "tcp/[::]:9200"])
        .open());

    let _z9110 = ztimeout!(Node::new(Peer, "31aa9110")
        .listen("tcp/[::]:9110")
        .connect(&["tcp/[::]:9100"])
        .open());
    let _z9120 = ztimeout!(Node::new(Peer, "31aa9120")
        .listen("tcp/[::]:9120")
        .connect(&["tcp/[::]:9100", "tcp/[::]:9110"])
        .open());
    let _z9130 = ztimeout!(Node::new(Peer, "31aa9130")
        .listen("tcp/[::]:9130")
        .connect(&["tcp/[::]:9100", "tcp/[::]:9110", "tcp/[::]:9120"])
        .open());

    let _z9210 = ztimeout!(Node::new(Peer, "31aa9210")
        .listen("tcp/[::]:9210")
        .connect(&["tcp/[::]:9200"])
        .open());
    let _z9220 = ztimeout!(Node::new(Peer, "31aa9220")
        .listen("tcp/[::]:9220")
        .connect(&["tcp/[::]:9200", "tcp/[::]:9210"])
        .open());
    let _z9230 = ztimeout!(Node::new(Peer, "31aa9230")
        .listen("tcp/[::]:9230")
        .connect(&["tcp/[::]:9200", "tcp/[::]:9210", "tcp/[::]:9220"])
        .open());

    let _z9310 = ztimeout!(Node::new(Peer, "31aa9310")
        .listen("tcp/[::]:9310")
        .connect(&["tcp/[::]:9300"])
        .open());
    let _z9320 = ztimeout!(Node::new(Peer, "31aa9320")
        .listen("tcp/[::]:9320")
        .connect(&["tcp/[::]:9300", "tcp/[::]:9310"])
        .open());
    let _z9330 = ztimeout!(Node::new(Peer, "31aa9330")
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
        "31aa9110", "31aa9120", "31aa9130", "31aa9210", "31aa9220", "31aa9230", "31aa9310",
        "31aa9320", "31aa9330",
    ] {
        assert_eq!(
            count!(s, demux{zid=i src="north..."}:declare_subscriber{expr="test/**"}: "()"),
            2
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario3_order1_pubsub() {
    init_tracing_subscriber();

    let _z9100 = ztimeout!(Node::new(Router, "31ab9100")
        .endpoints("tcp/[::]:9100", &[])
        .open());
    let _z9200 = ztimeout!(Node::new(Router, "31ab9200")
        .endpoints("tcp/[::]:9200", &["tcp/[::]:9100"])
        .open());
    let _z9300 = ztimeout!(Node::new(Router, "31ab9300")
        .endpoints("tcp/[::]:9300", &["tcp/[::]:9100", "tcp/[::]:9200"])
        .open());

    let _z9110 = ztimeout!(Node::new(Peer, "31ab9110")
        .listen("tcp/[::]:9110")
        .connect(&["tcp/[::]:9100"])
        .open());
    let _z9120 = ztimeout!(Node::new(Peer, "31ab9120")
        .listen("tcp/[::]:9120")
        .connect(&["tcp/[::]:9100", "tcp/[::]:9110"])
        .open());
    let _z9130 = ztimeout!(Node::new(Peer, "31ab9130")
        .listen("tcp/[::]:9130")
        .connect(&["tcp/[::]:9100", "tcp/[::]:9110", "tcp/[::]:9120"])
        .open());

    let _z9210 = ztimeout!(Node::new(Peer, "31ab9210")
        .listen("tcp/[::]:9210")
        .connect(&["tcp/[::]:9200"])
        .open());
    let _z9220 = ztimeout!(Node::new(Peer, "31ab9220")
        .listen("tcp/[::]:9220")
        .connect(&["tcp/[::]:9200", "tcp/[::]:9210"])
        .open());
    let _z9230 = ztimeout!(Node::new(Peer, "31ab9230")
        .listen("tcp/[::]:9230")
        .connect(&["tcp/[::]:9200", "tcp/[::]:9210", "tcp/[::]:9220"])
        .open());

    let _z9310 = ztimeout!(Node::new(Peer, "31ab9310")
        .listen("tcp/[::]:9310")
        .connect(&["tcp/[::]:9300"])
        .open());
    let _z9320 = ztimeout!(Node::new(Peer, "31ab9320")
        .listen("tcp/[::]:9320")
        .connect(&["tcp/[::]:9300", "tcp/[::]:9310"])
        .open());
    let _z9330 = ztimeout!(Node::new(Peer, "31ab9330")
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
        "31ab9110", "31ab9120", "31ab9130", "31ab9210", "31ab9220", "31ab9230", "31ab9310",
        "31ab9320", "31ab9330",
    ] {
        assert_eq!(
            count!(s, demux{zid=i src="north..."}:declare_subscriber{expr="test/**"}: "()"),
            3
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario3_order2_putsub() {
    init_tracing_subscriber();

    let _z9110 = ztimeout!(Node::new(Peer, "32aa9110")
        .listen("tcp/[::]:9110")
        .connect(&["tcp/[::]:9100"])
        .open());
    let _z9120 = ztimeout!(Node::new(Peer, "32aa9120")
        .listen("tcp/[::]:9120")
        .connect(&["tcp/[::]:9100", "tcp/[::]:9110"])
        .open());
    let _z9130 = ztimeout!(Node::new(Peer, "32aa9130")
        .listen("tcp/[::]:9130")
        .connect(&["tcp/[::]:9100", "tcp/[::]:9110", "tcp/[::]:9120"])
        .open());

    let _z9210 = ztimeout!(Node::new(Peer, "32aa9210")
        .listen("tcp/[::]:9210")
        .connect(&["tcp/[::]:9200"])
        .open());
    let _z9220 = ztimeout!(Node::new(Peer, "32aa9220")
        .listen("tcp/[::]:9220")
        .connect(&["tcp/[::]:9200", "tcp/[::]:9210"])
        .open());
    let _z9230 = ztimeout!(Node::new(Peer, "32aa9230")
        .listen("tcp/[::]:9230")
        .connect(&["tcp/[::]:9200", "tcp/[::]:9210", "tcp/[::]:9220"])
        .open());

    let _z9310 = ztimeout!(Node::new(Peer, "32aa9310")
        .listen("tcp/[::]:9310")
        .connect(&["tcp/[::]:9300"])
        .open());
    let _z9320 = ztimeout!(Node::new(Peer, "32aa9320")
        .listen("tcp/[::]:9320")
        .connect(&["tcp/[::]:9300", "tcp/[::]:9310"])
        .open());
    let _z9330 = ztimeout!(Node::new(Peer, "32aa9330")
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

    let _z9100 = ztimeout!(Node::new(Router, "32aa9100")
        .endpoints("tcp/[::]:9100", &[])
        .open());
    let _z9200 = ztimeout!(Node::new(Router, "32aa9200")
        .endpoints("tcp/[::]:9200", &["tcp/[::]:9100"])
        .open());
    let _z9300 = ztimeout!(Node::new(Router, "32aa9300")
        .endpoints("tcp/[::]:9300", &["tcp/[::]:9100", "tcp/[::]:9200"])
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
        "32aa9110", "32aa9120", "32aa9130", "32aa9210", "32aa9220", "32aa9230", "32aa9310",
        "32aa9320", "32aa9330",
    ] {
        assert_eq!(
            count!(s, demux{zid=i src="north..."}:declare_subscriber{expr="test/**"}: "()"),
            2
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario3_order2_pubsub() {
    init_tracing_subscriber();

    let _z9110 = ztimeout!(Node::new(Peer, "32ab9110")
        .listen("tcp/[::]:9110")
        .connect(&["tcp/[::]:9100"])
        .open());
    let _z9120 = ztimeout!(Node::new(Peer, "32ab9120")
        .listen("tcp/[::]:9120")
        .connect(&["tcp/[::]:9100", "tcp/[::]:9110"])
        .open());
    let _z9130 = ztimeout!(Node::new(Peer, "32ab9130")
        .listen("tcp/[::]:9130")
        .connect(&["tcp/[::]:9100", "tcp/[::]:9110", "tcp/[::]:9120"])
        .open());

    let _z9210 = ztimeout!(Node::new(Peer, "32ab9210")
        .listen("tcp/[::]:9210")
        .connect(&["tcp/[::]:9200"])
        .open());
    let _z9220 = ztimeout!(Node::new(Peer, "32ab9220")
        .listen("tcp/[::]:9220")
        .connect(&["tcp/[::]:9200", "tcp/[::]:9210"])
        .open());
    let _z9230 = ztimeout!(Node::new(Peer, "32ab9230")
        .listen("tcp/[::]:9230")
        .connect(&["tcp/[::]:9200", "tcp/[::]:9210", "tcp/[::]:9220"])
        .open());

    let _z9310 = ztimeout!(Node::new(Peer, "32ab9310")
        .listen("tcp/[::]:9310")
        .connect(&["tcp/[::]:9300"])
        .open());
    let _z9320 = ztimeout!(Node::new(Peer, "32ab9320")
        .listen("tcp/[::]:9320")
        .connect(&["tcp/[::]:9300", "tcp/[::]:9310"])
        .open());
    let _z9330 = ztimeout!(Node::new(Peer, "32ab9330")
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

    let _z9100 = ztimeout!(Node::new(Router, "32ab9100")
        .endpoints("tcp/[::]:9100", &[])
        .open());
    let _z9200 = ztimeout!(Node::new(Router, "32ab9200")
        .endpoints("tcp/[::]:9200", &["tcp/[::]:9100"])
        .open());
    let _z9300 = ztimeout!(Node::new(Router, "32ab9300")
        .endpoints("tcp/[::]:9300", &["tcp/[::]:9100", "tcp/[::]:9200"])
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
        "32ab9110", "32ab9120", "32ab9130", "32ab9210", "32ab9220", "32ab9230", "32ab9310",
        "32ab9320", "32ab9330",
    ] {
        assert_eq!(
            count!(s, demux{zid=i src="north..."}:declare_subscriber{expr="test/**"}: "()"),
            3
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario3_order3_putsub() {
    init_tracing_subscriber();

    let _z9100 = ztimeout!(Node::new(Router, "33aa9100")
        .endpoints("tcp/[::]:9100", &[])
        .open());
    let _z9200 = ztimeout!(Node::new(Router, "33aa9200")
        .endpoints("tcp/[::]:9200", &["tcp/[::]:9100"])
        .open());

    let _z9110 = ztimeout!(Node::new(Peer, "33aa9110")
        .listen("tcp/[::]:9110")
        .connect(&["tcp/[::]:9100"])
        .open());
    let _z9120 = ztimeout!(Node::new(Peer, "33aa9120")
        .listen("tcp/[::]:9120")
        .connect(&["tcp/[::]:9100", "tcp/[::]:9110"])
        .open());
    let _z9130 = ztimeout!(Node::new(Peer, "33aa9130")
        .listen("tcp/[::]:9130")
        .connect(&["tcp/[::]:9100", "tcp/[::]:9110", "tcp/[::]:9120"])
        .open());

    let _z9210 = ztimeout!(Node::new(Peer, "33aa9210")
        .listen("tcp/[::]:9210")
        .connect(&["tcp/[::]:9200"])
        .open());
    let _z9220 = ztimeout!(Node::new(Peer, "33aa9220")
        .listen("tcp/[::]:9220")
        .connect(&["tcp/[::]:9200", "tcp/[::]:9210"])
        .open());
    let _z9230 = ztimeout!(Node::new(Peer, "33aa9230")
        .listen("tcp/[::]:9230")
        .connect(&["tcp/[::]:9200", "tcp/[::]:9210", "tcp/[::]:9220"])
        .open());

    let _z9310 = ztimeout!(Node::new(Peer, "33aa9310")
        .listen("tcp/[::]:9310")
        .connect(&["tcp/[::]:9300"])
        .open());
    let _z9320 = ztimeout!(Node::new(Peer, "33aa9320")
        .listen("tcp/[::]:9320")
        .connect(&["tcp/[::]:9300", "tcp/[::]:9310"])
        .open());
    let _z9330 = ztimeout!(Node::new(Peer, "33aa9330")
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

    let _z9300 = ztimeout!(Node::new(Router, "33aa9300")
        .endpoints("tcp/[::]:9300", &["tcp/[::]:9100", "tcp/[::]:9200"])
        .open());

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
        "33aa9110", "33aa9120", "33aa9130", "33aa9210", "33aa9220", "33aa9230", "33aa9310",
        "33aa9320", "33aa9330",
    ] {
        assert_eq!(
            count!(s, demux{zid=i src="north..."}:declare_subscriber{expr="test/**"}: "()"),
            2
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario3_order3_pubsub() {
    init_tracing_subscriber();

    let _z9100 = ztimeout!(Node::new(Router, "33ab9100")
        .endpoints("tcp/[::]:9100", &[])
        .open());
    let _z9200 = ztimeout!(Node::new(Router, "33ab9200")
        .endpoints("tcp/[::]:9200", &["tcp/[::]:9100"])
        .open());

    let _z9110 = ztimeout!(Node::new(Peer, "33ab9110")
        .listen("tcp/[::]:9110")
        .connect(&["tcp/[::]:9100"])
        .open());
    let _z9120 = ztimeout!(Node::new(Peer, "33ab9120")
        .listen("tcp/[::]:9120")
        .connect(&["tcp/[::]:9100", "tcp/[::]:9110"])
        .open());
    let _z9130 = ztimeout!(Node::new(Peer, "33ab9130")
        .listen("tcp/[::]:9130")
        .connect(&["tcp/[::]:9100", "tcp/[::]:9110", "tcp/[::]:9120"])
        .open());

    let _z9210 = ztimeout!(Node::new(Peer, "33ab9210")
        .listen("tcp/[::]:9210")
        .connect(&["tcp/[::]:9200"])
        .open());
    let _z9220 = ztimeout!(Node::new(Peer, "33ab9220")
        .listen("tcp/[::]:9220")
        .connect(&["tcp/[::]:9200", "tcp/[::]:9210"])
        .open());
    let _z9230 = ztimeout!(Node::new(Peer, "33ab9230")
        .listen("tcp/[::]:9230")
        .connect(&["tcp/[::]:9200", "tcp/[::]:9210", "tcp/[::]:9220"])
        .open());

    let _z9310 = ztimeout!(Node::new(Peer, "33ab9310")
        .listen("tcp/[::]:9310")
        .connect(&["tcp/[::]:9300"])
        .open());
    let _z9320 = ztimeout!(Node::new(Peer, "33ab9320")
        .listen("tcp/[::]:9320")
        .connect(&["tcp/[::]:9300", "tcp/[::]:9310"])
        .open());
    let _z9330 = ztimeout!(Node::new(Peer, "33ab9330")
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

    let _z9300 = ztimeout!(Node::new(Router, "33ab9300")
        .endpoints("tcp/[::]:9300", &["tcp/[::]:9100", "tcp/[::]:9200"])
        .open());

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
        "33ab9110", "33ab9120", "33ab9130", "33ab9210", "33ab9220", "33ab9230", "33ab9310",
        "33ab9320", "33ab9330",
    ] {
        assert_eq!(
            count!(s, demux{zid=i src="north..."}:declare_subscriber{expr="test/**"}: "()"),
            3
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario3_order4_putsub() {
    init_tracing_subscriber();

    let _z9100 = ztimeout!(Node::new(Router, "34aa9100")
        .endpoints("tcp/[::]:9100", &[])
        .open());
    let _z9200 = ztimeout!(Node::new(Router, "34aa9200")
        .endpoints("tcp/[::]:9200", &["tcp/[::]:9100"])
        .open());

    let _z9110 = ztimeout!(Node::new(Peer, "34aa9110")
        .listen("tcp/[::]:9110")
        .connect(&["tcp/[::]:9100"])
        .open());
    let _z9120 = ztimeout!(Node::new(Peer, "34aa9120")
        .listen("tcp/[::]:9120")
        .connect(&["tcp/[::]:9100", "tcp/[::]:9110"])
        .open());
    let _z9130 = ztimeout!(Node::new(Peer, "34aa9130")
        .listen("tcp/[::]:9130")
        .connect(&["tcp/[::]:9100", "tcp/[::]:9110", "tcp/[::]:9120"])
        .open());

    let _z9210 = ztimeout!(Node::new(Peer, "34aa9210")
        .listen("tcp/[::]:9210")
        .connect(&["tcp/[::]:9200"])
        .open());
    let _z9220 = ztimeout!(Node::new(Peer, "34aa9220")
        .listen("tcp/[::]:9220")
        .connect(&["tcp/[::]:9200", "tcp/[::]:9210"])
        .open());
    let _z9230 = ztimeout!(Node::new(Peer, "34aa9230")
        .listen("tcp/[::]:9230")
        .connect(&["tcp/[::]:9200", "tcp/[::]:9210", "tcp/[::]:9220"])
        .open());

    let s9110 = _z9110.declare_subscriber("test/**").await.unwrap();
    let s9120 = _z9120.declare_subscriber("test/**").await.unwrap();
    let s9130 = _z9130.declare_subscriber("test/**").await.unwrap();
    let s9210 = _z9210.declare_subscriber("test/**").await.unwrap();
    let s9220 = _z9220.declare_subscriber("test/**").await.unwrap();
    let s9230 = _z9230.declare_subscriber("test/**").await.unwrap();

    let _z9300 = ztimeout!(Node::new(Router, "34aa9300")
        .endpoints("tcp/[::]:9300", &["tcp/[::]:9100", "tcp/[::]:9200"])
        .open());

    let _z9310 = ztimeout!(Node::new(Peer, "34aa9310")
        .listen("tcp/[::]:9310")
        .connect(&["tcp/[::]:9300"])
        .open());
    let _z9320 = ztimeout!(Node::new(Peer, "34aa9320")
        .listen("tcp/[::]:9320")
        .connect(&["tcp/[::]:9300", "tcp/[::]:9310"])
        .open());
    let _z9330 = ztimeout!(Node::new(Peer, "34aa9330")
        .listen("tcp/[::]:9330")
        .connect(&["tcp/[::]:9300", "tcp/[::]:9310", "tcp/[::]:9320"])
        .open());

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
        "34aa9110", "34aa9120", "34aa9130", "34aa9210", "34aa9220", "34aa9230", "34aa9310",
        "34aa9320", "34aa9330",
    ] {
        assert_eq!(
            count!(s, demux{zid=i src="north..."}:declare_subscriber{expr="test/**"}: "()"),
            2
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario3_order4_pubsub() {
    init_tracing_subscriber();

    let _z9100 = ztimeout!(Node::new(Router, "34ab9100")
        .endpoints("tcp/[::]:9100", &[])
        .open());
    let _z9200 = ztimeout!(Node::new(Router, "34ab9200")
        .endpoints("tcp/[::]:9200", &["tcp/[::]:9100"])
        .open());

    let _z9110 = ztimeout!(Node::new(Peer, "34ab9110")
        .listen("tcp/[::]:9110")
        .connect(&["tcp/[::]:9100"])
        .open());
    let _z9120 = ztimeout!(Node::new(Peer, "34ab9120")
        .listen("tcp/[::]:9120")
        .connect(&["tcp/[::]:9100", "tcp/[::]:9110"])
        .open());
    let _z9130 = ztimeout!(Node::new(Peer, "34ab9130")
        .listen("tcp/[::]:9130")
        .connect(&["tcp/[::]:9100", "tcp/[::]:9110", "tcp/[::]:9120"])
        .open());

    let _z9210 = ztimeout!(Node::new(Peer, "34ab9210")
        .listen("tcp/[::]:9210")
        .connect(&["tcp/[::]:9200"])
        .open());
    let _z9220 = ztimeout!(Node::new(Peer, "34ab9220")
        .listen("tcp/[::]:9220")
        .connect(&["tcp/[::]:9200", "tcp/[::]:9210"])
        .open());
    let _z9230 = ztimeout!(Node::new(Peer, "34ab9230")
        .listen("tcp/[::]:9230")
        .connect(&["tcp/[::]:9200", "tcp/[::]:9210", "tcp/[::]:9220"])
        .open());

    let s9110 = _z9110.declare_subscriber("test/**").await.unwrap();
    let s9120 = _z9120.declare_subscriber("test/**").await.unwrap();
    let s9130 = _z9130.declare_subscriber("test/**").await.unwrap();
    let s9210 = _z9210.declare_subscriber("test/**").await.unwrap();
    let s9220 = _z9220.declare_subscriber("test/**").await.unwrap();
    let s9230 = _z9230.declare_subscriber("test/**").await.unwrap();

    let p9110 = _z9110.declare_publisher("test/9110").await.unwrap();
    let p9120 = _z9120.declare_publisher("test/9120").await.unwrap();
    let p9130 = _z9130.declare_publisher("test/9130").await.unwrap();
    let p9210 = _z9210.declare_publisher("test/9210").await.unwrap();
    let p9220 = _z9220.declare_publisher("test/9220").await.unwrap();
    let p9230 = _z9230.declare_publisher("test/9230").await.unwrap();

    let _z9300 = ztimeout!(Node::new(Router, "34ab9300")
        .endpoints("tcp/[::]:9300", &["tcp/[::]:9100", "tcp/[::]:9200"])
        .open());

    let _z9310 = ztimeout!(Node::new(Peer, "34ab9310")
        .listen("tcp/[::]:9310")
        .connect(&["tcp/[::]:9300"])
        .open());
    let _z9320 = ztimeout!(Node::new(Peer, "34ab9320")
        .listen("tcp/[::]:9320")
        .connect(&["tcp/[::]:9300", "tcp/[::]:9310"])
        .open());
    let _z9330 = ztimeout!(Node::new(Peer, "34ab9330")
        .listen("tcp/[::]:9330")
        .connect(&["tcp/[::]:9300", "tcp/[::]:9310", "tcp/[::]:9320"])
        .open());

    let s9310 = _z9310.declare_subscriber("test/**").await.unwrap();
    let s9320 = _z9320.declare_subscriber("test/**").await.unwrap();
    let s9330 = _z9330.declare_subscriber("test/**").await.unwrap();

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
        "34ab9110", "34ab9120", "34ab9130", "34ab9210", "34ab9220", "34ab9230", "34ab9310",
        "34ab9320", "34ab9330",
    ] {
        assert_eq!(
            count!(s, demux{zid=i src="north..."}:declare_subscriber{expr="test/**"}: "()"),
            3
        );
    }
}
