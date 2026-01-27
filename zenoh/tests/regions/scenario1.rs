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

use zenoh_config::WhatAmI::{Client, Peer, Router};
use zenoh_core::{lazy_static, ztimeout};

use crate::{count, json, loc, Node};

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(5);

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
async fn test_regions_scenario1_order1_putsub() {
    init_tracing_subscriber();

    let _z9000 = ztimeout!(Node::new(Router, "11aa9000")
        .listen("tcp/[::]:11111")
        .open());

    let _z9100 = ztimeout!(Node::new(Client, "11aa9100")
        .endpoints("tcp/[::]:0", &[loc!(_z9000)])
        .multicast("224.1.1.1:9100")
        .gateway(json!({"south": [{"filters": [{"modes": ["client", "peer"]}]}]}))
        .open());
    let _z9200 = ztimeout!(Node::new(Client, "11aa9200")
        .endpoints("tcp/[::]:0", &[loc!(_z9000)])
        .multicast("224.1.1.1:9200")
        .gateway(json!({"south": [{"filters": [{"modes": ["client", "peer"]}]}]}))
        .open());
    let _z9300 = ztimeout!(Node::new(Client, "11aa9300")
        .endpoints("tcp/[::]:0", &[loc!(_z9000)])
        .multicast("224.1.1.1:9300")
        .gateway(json!({"south": [{"filters": [{"modes": ["client", "peer"]}]}]}))
        .open());

    let _z9110 = ztimeout!(Node::new(Peer, "11aa9110")
        .multicast("224.1.1.1:9100")
        .open());
    let _z9120 = ztimeout!(Node::new(Peer, "11aa9120")
        .multicast("224.1.1.1:9100")
        .open());
    let _z9130 = ztimeout!(Node::new(Peer, "11aa9130")
        .multicast("224.1.1.1:9100")
        .open());

    let _z9210 = ztimeout!(Node::new(Peer, "11aa9210")
        .multicast("224.1.1.1:9200")
        .open());
    let _z9220 = ztimeout!(Node::new(Peer, "11aa9220")
        .multicast("224.1.1.1:9200")
        .open());
    let _z9230 = ztimeout!(Node::new(Peer, "11aa9230")
        .multicast("224.1.1.1:9200")
        .open());

    let _z9310 = ztimeout!(Node::new(Peer, "11aa9310")
        .multicast("224.1.1.1:9300")
        .open());
    let _z9320 = ztimeout!(Node::new(Peer, "11aa9320")
        .multicast("224.1.1.1:9300")
        .open());
    let _z9330 = ztimeout!(Node::new(Peer, "11aa9330")
        .multicast("224.1.1.1:9300")
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
        "11aa9110", "11aa9120", "11aa9130", "11aa9210", "11aa9220", "11aa9230", "11aa9310",
        "11aa9320", "11aa9330",
    ] {
        assert_eq!(
            count!(s, demux{zid=i src="north..."}:declare_subscriber{expr="test/**"}: "()"),
            2
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario1_order1_pubsub() {
    init_tracing_subscriber();

    let _z9000 = ztimeout!(Node::new(Router, "11ab9000").listen("tcp/[::]:0").open());

    let _z9100 = ztimeout!(Node::new(Client, "11ab9100")
        .endpoints("tcp/[::]:0", &[loc!(_z9000)])
        .multicast("224.1.1.2:9100")
        .gateway(json!({"south": [{"filters": [{"modes": ["client", "peer"]}]}]}))
        .open());
    let _z9200 = ztimeout!(Node::new(Client, "11ab9200")
        .endpoints("tcp/[::]:0", &[loc!(_z9000)])
        .multicast("224.1.1.2:9200")
        .gateway(json!({"south": [{"filters": [{"modes": ["client", "peer"]}]}]}))
        .open());
    let _z9300 = ztimeout!(Node::new(Client, "11ab9300")
        .endpoints("tcp/[::]:0", &[loc!(_z9000)])
        .multicast("224.1.1.2:9300")
        .gateway(json!({"south": [{"filters": [{"modes": ["client", "peer"]}]}]}))
        .open());

    let _z9110 = ztimeout!(Node::new(Peer, "11ab9110")
        .multicast("224.1.1.2:9100")
        .open());
    let _z9120 = ztimeout!(Node::new(Peer, "11ab9120")
        .multicast("224.1.1.2:9100")
        .open());
    let _z9130 = ztimeout!(Node::new(Peer, "11ab9130")
        .multicast("224.1.1.2:9100")
        .open());

    let _z9210 = ztimeout!(Node::new(Peer, "11ab9210")
        .multicast("224.1.1.2:9200")
        .open());
    let _z9220 = ztimeout!(Node::new(Peer, "11ab9220")
        .multicast("224.1.1.2:9200")
        .open());
    let _z9230 = ztimeout!(Node::new(Peer, "11ab9230")
        .multicast("224.1.1.2:9200")
        .open());

    let _z9310 = ztimeout!(Node::new(Peer, "11ab9310")
        .multicast("224.1.1.2:9300")
        .open());
    let _z9320 = ztimeout!(Node::new(Peer, "11ab9320")
        .multicast("224.1.1.2:9300")
        .open());
    let _z9330 = ztimeout!(Node::new(Peer, "11ab9330")
        .multicast("224.1.1.2:9300")
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
    let p9330 = _z9330
        .declare_publisher("test/9330")
        .express(true)
        .await
        .unwrap();

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
        "11ab9110", "11ab9120", "11ab9130", "11ab9210", "11ab9220", "11ab9230", "11ab9310",
        "11ab9320", "11ab9330",
    ] {
        assert_eq!(
            count!(s, demux{zid=i src="north..."}:declare_subscriber{expr="test/**"}: "()"),
            3
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario1_order2_putsub() {
    init_tracing_subscriber();

    let _z9110 = ztimeout!(Node::new(Peer, "12aa9110")
        .multicast("224.1.2.1:9100")
        .open());
    let _z9120 = ztimeout!(Node::new(Peer, "12aa9120")
        .multicast("224.1.2.1:9100")
        .open());
    let _z9130 = ztimeout!(Node::new(Peer, "12aa9130")
        .multicast("224.1.2.1:9100")
        .open());

    let _z9210 = ztimeout!(Node::new(Peer, "12aa9210")
        .multicast("224.1.2.1:9200")
        .open());
    let _z9220 = ztimeout!(Node::new(Peer, "12aa9220")
        .multicast("224.1.2.1:9200")
        .open());
    let _z9230 = ztimeout!(Node::new(Peer, "12aa9230")
        .multicast("224.1.2.1:9200")
        .open());

    let _z9310 = ztimeout!(Node::new(Peer, "12aa9310")
        .multicast("224.1.2.1:9300")
        .open());
    let _z9320 = ztimeout!(Node::new(Peer, "12aa9320")
        .multicast("224.1.2.1:9300")
        .open());
    let _z9330 = ztimeout!(Node::new(Peer, "12aa9330")
        .multicast("224.1.2.1:9300")
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

    let _z9000 = ztimeout!(Node::new(Router, "12aa9000").listen("tcp/[::]:0").open());

    let _z9100 = ztimeout!(Node::new(Client, "12aa9100")
        .multicast("224.1.2.1:9100")
        .endpoints("tcp/[::]:0", &[loc!(_z9000)])
        .gateway(json!({"south": [{"filters": [{"modes": ["client", "peer"]}]}]}))
        .open());
    let _z9200 = ztimeout!(Node::new(Client, "12aa9200")
        .multicast("224.1.2.1:9200")
        .endpoints("tcp/[::]:0", &[loc!(_z9000)])
        .gateway(json!({"south": [{"filters": [{"modes": ["client", "peer"]}]}]}))
        .open());
    let _z9300 = ztimeout!(Node::new(Client, "12aa9300")
        .multicast("224.1.2.1:9300")
        .endpoints("tcp/[::]:0", &[loc!(_z9000)])
        .gateway(json!({"south": [{"filters": [{"modes": ["client", "peer"]}]}]}))
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
        "12aa9110", "12aa9120", "12aa9130", "12aa9210", "12aa9220", "12aa9230", "12aa9310",
        "12aa9320", "12aa9330",
    ] {
        assert_eq!(
            count!(s, demux{zid=i src="north..."}:declare_subscriber{expr="test/**"}: "()"),
            2
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_regions_scenario1_order2_pubsub() {
    init_tracing_subscriber();

    let _z9110 = ztimeout!(Node::new(Peer, "12ab9110")
        .multicast("224.1.2.2:9100")
        .open());
    let _z9120 = ztimeout!(Node::new(Peer, "12ab9120")
        .multicast("224.1.2.2:9100")
        .open());
    let _z9130 = ztimeout!(Node::new(Peer, "12ab9130")
        .multicast("224.1.2.2:9100")
        .open());

    let _z9210 = ztimeout!(Node::new(Peer, "12ab9210")
        .multicast("224.1.2.2:9200")
        .open());
    let _z9220 = ztimeout!(Node::new(Peer, "12ab9220")
        .multicast("224.1.2.2:9200")
        .open());
    let _z9230 = ztimeout!(Node::new(Peer, "12ab9230")
        .multicast("224.1.2.2:9200")
        .open());

    let _z9310 = ztimeout!(Node::new(Peer, "12ab9310")
        .multicast("224.1.2.2:9300")
        .open());
    let _z9320 = ztimeout!(Node::new(Peer, "12ab9320")
        .multicast("224.1.2.2:9300")
        .open());
    let _z9330 = ztimeout!(Node::new(Peer, "12ab9330")
        .multicast("224.1.2.2:9300")
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

    let _z9000 = ztimeout!(Node::new(Router, "12ab9000").listen("tcp/[::]:0").open());

    let _z9100 = ztimeout!(Node::new(Client, "12ab9100")
        .multicast("224.1.2.2:9100")
        .endpoints("tcp/[::]:0", &[loc!(_z9000)])
        .gateway(json!({"south": [{"filters": [{"modes": ["client", "peer"]}]}]}))
        .open());
    let _z9200 = ztimeout!(Node::new(Client, "12ab9200")
        .multicast("224.1.2.2:9200")
        .endpoints("tcp/[::]:0", &[loc!(_z9000)])
        .gateway(json!({"south": [{"filters": [{"modes": ["client", "peer"]}]}]}))
        .open());
    let _z9300 = ztimeout!(Node::new(Client, "12ab9300")
        .multicast("224.1.2.2:9300")
        .endpoints("tcp/[::]:0", &[loc!(_z9000)])
        .gateway(json!({"south": [{"filters": [{"modes": ["client", "peer"]}]}]}))
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
        "12ab9110", "12ab9120", "12ab9130", "12ab9210", "12ab9220", "12ab9230", "12ab9310",
        "12ab9320", "12ab9330",
    ] {
        assert_eq!(
            count!(s, demux{zid=i src="north..."}:declare_subscriber{expr="test/**"}: "()"),
            3
        );
    }
}
